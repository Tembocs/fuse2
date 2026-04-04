#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

MIN_RUSTC = (1, 94, 1)


def run(cmd: list[str], cwd: Path, check: bool = True, capture: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=str(cwd),
        check=check,
        text=True,
        capture_output=capture,
    )


def parse_rustc_version(text: str) -> tuple[int, int, int]:
    match = re.search(r"rustc (\d+)\.(\d+)\.(\d+)", text)
    if not match:
        raise RuntimeError(f"could not parse rustc version from `{text.strip()}`")
    return tuple(int(part) for part in match.groups())


def extract_expected_output(path: Path) -> str:
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines or not lines[0].startswith("// EXPECTED OUTPUT"):
        raise RuntimeError(f"{path} does not start with an EXPECTED OUTPUT block")
    expected: list[str] = []
    for line in lines[1:]:
        if line.startswith("// "):
            expected.append(line[3:])
        elif line.startswith("//"):
            expected.append(line[2:])
        else:
            break
    return "\n".join(expected)


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def copy_tree(src: Path, dst: Path) -> None:
    if dst.exists():
        shutil.rmtree(dst)
    shutil.copytree(src, dst)


def build_package(repo_root: Path, output_dir: Path) -> tuple[Path, Path]:
    stage1_root = repo_root / "stage1"

    rustc_version = run(["rustc", "--version"], cwd=stage1_root, capture=True).stdout.strip()
    cargo_version = run(["cargo", "--version"], cwd=stage1_root, capture=True).stdout.strip()
    parsed_version = parse_rustc_version(rustc_version)
    if parsed_version < MIN_RUSTC:
        required = ".".join(map(str, MIN_RUSTC))
        found = ".".join(map(str, parsed_version))
        raise RuntimeError(f"rustc {required}+ required, found {found}")

    run(
        ["cargo", "build", "--release", "-p", "fusec", "-p", "fuse-runtime", "-p", "cranelift-ffi"],
        cwd=stage1_root,
    )

    package_root = output_dir.resolve()
    if package_root.exists():
        shutil.rmtree(package_root)
    package_root.mkdir(parents=True)

    bin_dir = package_root / "bin"
    lib_dir = package_root / "lib"
    stage1_pkg_dir = package_root / "stage1"
    bin_dir.mkdir()
    lib_dir.mkdir()
    stage1_pkg_dir.mkdir()

    exe_suffix = ".exe" if os.name == "nt" else ""
    source_exe = stage1_root / "target" / "release" / f"fusec{exe_suffix}"
    if not source_exe.exists():
        raise RuntimeError(f"missing release compiler at {source_exe}")
    packaged_exe = bin_dir / source_exe.name
    shutil.copy2(source_exe, packaged_exe)

    write_text(
        bin_dir / ("fusec.cmd" if os.name == "nt" else "fusec"),
        (
            "@echo off\r\n"
            "setlocal\r\n"
            "set FUSE_STAGE1_ROOT=%~dp0..\r\n"
            "\"%~dp0fusec.exe\" %*\r\n"
            if os.name == "nt"
            else "#!/usr/bin/env bash\n"
            'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
            'export FUSE_STAGE1_ROOT="${SCRIPT_DIR}/.."\n'
            '"${SCRIPT_DIR}/fusec" "$@"\n'
        ),
    )

    write_text(
        bin_dir / "fusec.ps1",
        "$env:FUSE_STAGE1_ROOT = Join-Path $PSScriptRoot '..'\n& (Join-Path $PSScriptRoot 'fusec.exe') @args\n",
    )

    copy_tree(stage1_root / "fuse-runtime", stage1_pkg_dir / "fuse-runtime")
    copy_tree(stage1_root / "cranelift-ffi", stage1_pkg_dir / "cranelift-ffi")
    shutil.copy2(stage1_root / "Cargo.lock", stage1_pkg_dir / "Cargo.lock")
    shutil.copy2(stage1_root / "Cargo.toml", stage1_pkg_dir / "Cargo.toml")

    copy_tree(repo_root / "stdlib", package_root / "stdlib")

    for pattern in ("fuse_runtime*", "cranelift_ffi*", "libfuse_runtime*", "libcranelift_ffi*"):
        for artifact in (stage1_root / "target" / "release").glob(pattern):
            if artifact.is_file():
                shutil.copy2(artifact, lib_dir / artifact.name)

    write_text(
        package_root / "README.txt",
        "\n".join(
            [
                "Fuse Stage 1 package",
                "",
                "Contents:",
                "- bin/fusec(.exe): packaged Stage 1 compiler",
                "- stage1/fuse-runtime: runtime crate used by emitted wrapper builds",
                "- stage1/cranelift-ffi: companion FFI crate artifact/source bundle",
                "- stdlib/: Fuse standard library sources",
                "",
                "Usage:",
                "  bin/fusec.exe <file.fuse> -o <output>",
                "",
                "Requirements:",
                f"- rustc >= {'.'.join(map(str, MIN_RUSTC))}",
                "- cargo available on PATH",
            ]
        )
        + "\n",
    )

    manifest = {
        "package_root": str(package_root),
        "compiler": str(packaged_exe),
        "rustc": rustc_version,
        "cargo": cargo_version,
        "included": {
            "runtime_source": str(stage1_pkg_dir / "fuse-runtime"),
            "cranelift_ffi_source": str(stage1_pkg_dir / "cranelift-ffi"),
            "stdlib": str(package_root / "stdlib"),
            "lib_dir": str(lib_dir),
        },
    }
    git_head = run(["git", "rev-parse", "HEAD"], cwd=repo_root, capture=True)
    manifest["git_head"] = git_head.stdout.strip()
    write_text(package_root / "package.json", json.dumps(manifest, indent=2) + "\n")

    archive_base = output_dir.parent / output_dir.name
    archive_path = Path(shutil.make_archive(str(archive_base), "zip", root_dir=output_dir.parent, base_dir=output_dir.name))

    verify_packaged_compiler(repo_root, packaged_exe, package_root)

    return package_root, archive_path


def verify_packaged_compiler(repo_root: Path, packaged_exe: Path, package_root: Path) -> None:
    smoke_src = repo_root / "tests" / "fuse" / "core" / "types" / "type_inference.fuse"
    smoke_expected = extract_expected_output(smoke_src)
    smoke_dir = package_root / "smoke"
    smoke_dir.mkdir(exist_ok=True)
    output = smoke_dir / ("type_inference.exe" if os.name == "nt" else "type_inference")

    env = os.environ.copy()
    env["FUSE_STAGE1_ROOT"] = str(package_root)
    subprocess.run(
        [str(packaged_exe), str(smoke_src), "-o", str(output)],
        cwd=str(smoke_dir),
        check=True,
        text=True,
        env=env,
    )
    run_result = subprocess.run(
        [str(output)],
        cwd=str(smoke_dir),
        check=True,
        text=True,
        capture_output=True,
        env=env,
    )
    if run_result.stdout.strip() != smoke_expected.strip():
        raise RuntimeError(
            "packaged compiler smoke output mismatch:\n"
            f"expected:\n{smoke_expected}\n"
            f"actual:\n{run_result.stdout}"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description="Build a reusable package for the current Stage 1 compiler.")
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("dist") / "stage1-package",
        help="Output package directory (default: dist/stage1-package)",
    )
    args = parser.parse_args()

    script_path = Path(__file__).resolve()
    repo_root = script_path.parent.parent
    package_root, archive_path = build_package(repo_root, repo_root / args.out)
    print(f"package_dir={package_root}")
    print(f"package_zip={archive_path}")
    print(f"compiler={package_root / 'bin' / ('fusec.exe' if os.name == 'nt' else 'fusec')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
