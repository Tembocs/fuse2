#!/usr/bin/env python3
"""
Fuse Stage 2 — Test Runner

Discovers .fuse fixtures under tests/stage2/, compiles them with the Stage 2
compiler (fusec2), and compares output against EXPECTED blocks.

Modes:
  // EXPECTED OUTPUT   — compile + run, match stdout line-by-line
  // EXPECTED ERROR    — compile only, expect failure, match stderr substrings
  // EXPECTED WARNING  — compile --check, expect success, match stderr substrings

Usage:
  python tests/stage2/run_tests.py
  python tests/stage2/run_tests.py --filter t0_
  python tests/stage2/run_tests.py --compiler path/to/fusec2
  python tests/stage2/run_tests.py --parallel 8
  python tests/stage2/run_tests.py --parity
  python tests/stage2/run_tests.py --bootstrap
  python tests/stage2/run_tests.py --lsp
"""

import argparse
import os
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
STAGE2_TESTS = REPO_ROOT / "tests" / "stage2"
STAGE1_TESTS = REPO_ROOT / "tests" / "fuse"

DEFAULT_COMPILER = REPO_ROOT / "stage1" / "target" / "release" / "fusec"
STAGE1_COMPILER = DEFAULT_COMPILER  # for parity mode

# ---------------------------------------------------------------------------
# Fixture parsing
# ---------------------------------------------------------------------------

def parse_fixture(path: Path):
    """Return (mode, expected_lines) from the EXPECTED block."""
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines:
        return None, []

    first = lines[0]
    if first.startswith("// EXPECTED OUTPUT"):
        mode = "output"
    elif first.startswith("// EXPECTED ERROR"):
        mode = "error"
    elif first.startswith("// EXPECTED WARNING"):
        mode = "warning"
    else:
        return None, []

    expected = []
    for line in lines[1:]:
        if line.startswith("// "):
            expected.append(line[3:])
        elif line.startswith("//"):
            # Handle "//\n" (empty expected line)
            expected.append(line[2:])
        else:
            break

    return mode, expected

# ---------------------------------------------------------------------------
# Compilation and execution helpers
# ---------------------------------------------------------------------------

def compile_fuse(compiler: Path, source: Path, output: Path, check_only=False):
    """Compile a .fuse file. Returns (returncode, stdout, stderr)."""
    cmd = [str(compiler)]
    if check_only:
        cmd.append("--check")
    cmd.append(str(source))
    if not check_only:
        cmd.extend(["-o", str(output)])
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    return result.returncode, result.stdout, result.stderr


def run_binary(binary: Path):
    """Run a compiled binary. Returns (returncode, stdout, stderr)."""
    result = subprocess.run(
        [str(binary)], capture_output=True, text=True, timeout=30
    )
    return result.returncode, result.stdout, result.stderr

# ---------------------------------------------------------------------------
# Single test execution
# ---------------------------------------------------------------------------

def run_test(compiler: Path, fixture: Path, tmp_dir: str):
    """Run a single test fixture. Returns (name, passed, detail)."""
    name = fixture.relative_to(STAGE2_TESTS).as_posix()
    mode, expected = parse_fixture(fixture)

    if mode is None:
        return name, None, "SKIP: no EXPECTED block"

    stem = fixture.stem + "_" + str(hash(str(fixture)) % 100000)
    exe_suffix = ".exe" if sys.platform == "win32" else ""
    output_path = Path(tmp_dir) / (stem + exe_suffix)

    try:
        if mode == "output":
            rc, stdout, stderr = compile_fuse(compiler, fixture, output_path)
            if rc != 0:
                return name, False, f"compilation failed (rc={rc}):\n{stderr}"
            if not output_path.exists():
                return name, False, f"binary not produced at {output_path}"
            rc, stdout, stderr = run_binary(output_path)
            actual = stdout.rstrip("\n").splitlines() if stdout.strip() else []
            if actual == expected:
                return name, True, ""
            else:
                return name, False, diff_lines(expected, actual)

        elif mode == "error":
            rc, stdout, stderr = compile_fuse(compiler, fixture, output_path)
            if rc == 0:
                return name, False, "expected compilation to fail, but it succeeded"
            return check_substrings(name, expected, stderr)

        elif mode == "warning":
            rc, stdout, stderr = compile_fuse(
                compiler, fixture, output_path, check_only=True
            )
            if rc != 0:
                return name, False, f"expected success with warnings, but compilation failed (rc={rc}):\n{stderr}"
            return check_substrings(name, expected, stderr)

    except subprocess.TimeoutExpired:
        return name, False, "TIMEOUT"
    except Exception as e:
        return name, False, f"EXCEPTION: {e}"


def check_substrings(name, expected, output):
    """Check that each expected line appears as a substring in output."""
    missing = []
    for exp in expected:
        if exp not in output:
            missing.append(exp)
    if not missing:
        return name, True, ""
    detail = "missing in output:\n"
    for m in missing:
        detail += f"  - {m}\n"
    detail += f"actual output:\n{output}"
    return name, False, detail


def diff_lines(expected, actual):
    """Simple diff between expected and actual line lists."""
    lines = []
    max_len = max(len(expected), len(actual))
    for i in range(max_len):
        exp = expected[i] if i < len(expected) else "<missing>"
        act = actual[i] if i < len(actual) else "<missing>"
        marker = "  " if exp == act else "! "
        if exp != act:
            lines.append(f"{marker}expected: {exp!r}")
            lines.append(f"{marker}  actual: {act!r}")
        else:
            lines.append(f"  {exp}")
    return "\n".join(lines)

# ---------------------------------------------------------------------------
# Discovery
# ---------------------------------------------------------------------------

def discover_fixtures(root: Path, filter_pattern: str = None):
    """Find all .fuse files under root, optionally filtered."""
    fixtures = sorted(root.rglob("*.fuse"))
    if filter_pattern:
        fixtures = [f for f in fixtures if filter_pattern in f.as_posix()]
    return fixtures

# ---------------------------------------------------------------------------
# Main runner
# ---------------------------------------------------------------------------

def run_suite(compiler: Path, fixtures: list, parallel: int = 1):
    """Run all fixtures and print results. Returns exit code."""
    passed = 0
    failed = 0
    skipped = 0
    failures = []

    start = time.time()

    with tempfile.TemporaryDirectory(prefix="fuse_stage2_") as tmp_dir:
        if parallel > 1:
            with ThreadPoolExecutor(max_workers=parallel) as pool:
                futures = {
                    pool.submit(run_test, compiler, f, tmp_dir): f
                    for f in fixtures
                }
                for future in as_completed(futures):
                    name, result, detail = future.result()
                    if result is None:
                        skipped += 1
                        print(f"  SKIP  {name}")
                    elif result:
                        passed += 1
                        print(f"  PASS  {name}")
                    else:
                        failed += 1
                        failures.append((name, detail))
                        print(f"  FAIL  {name}")
        else:
            for fixture in fixtures:
                name, result, detail = run_test(compiler, fixture, tmp_dir)
                if result is None:
                    skipped += 1
                    print(f"  SKIP  {name}")
                elif result:
                    passed += 1
                    print(f"  PASS  {name}")
                else:
                    failed += 1
                    failures.append((name, detail))
                    print(f"  FAIL  {name}")

    elapsed = time.time() - start

    # Print failure details
    if failures:
        print(f"\n{'='*60}")
        print(f"FAILURES ({len(failures)}):")
        print(f"{'='*60}")
        for name, detail in failures:
            print(f"\n--- {name} ---")
            print(detail)

    # Summary
    print(f"\n{'='*60}")
    print(f"  {passed} passed, {failed} failed, {skipped} skipped")
    print(f"  {elapsed:.1f}s elapsed")
    print(f"{'='*60}")

    return 0 if failed == 0 else 1

# ---------------------------------------------------------------------------
# Parity mode
# ---------------------------------------------------------------------------

def run_parity(stage1_compiler: Path, stage2_compiler: Path, parallel: int = 1):
    """Compare Stage 1 and Stage 2 output on shared core fixtures."""
    core_fixtures = sorted((STAGE1_TESTS / "core").rglob("*.fuse"))
    milestone_fixtures = sorted((STAGE1_TESTS / "milestone").rglob("*.fuse"))
    fixtures = core_fixtures + milestone_fixtures

    # Only include OUTPUT fixtures
    output_fixtures = []
    for f in fixtures:
        mode, _ = parse_fixture(f)
        if mode == "output":
            output_fixtures.append(f)

    print(f"Parity: {len(output_fixtures)} output fixtures")
    passed = 0
    failed = 0
    failures = []

    with tempfile.TemporaryDirectory(prefix="fuse_parity_") as tmp_dir:
        for fixture in output_fixtures:
            name = fixture.relative_to(STAGE1_TESTS).as_posix()
            stem = fixture.stem
            exe_suffix = ".exe" if sys.platform == "win32" else ""

            s1_out = Path(tmp_dir) / (stem + "_s1" + exe_suffix)
            s2_out = Path(tmp_dir) / (stem + "_s2" + exe_suffix)

            try:
                rc1, _, err1 = compile_fuse(stage1_compiler, fixture, s1_out)
                rc2, _, err2 = compile_fuse(stage2_compiler, fixture, s2_out)

                if rc1 != 0:
                    # Stage 1 can't compile it — skip
                    continue
                if rc2 != 0:
                    failed += 1
                    failures.append((name, f"Stage 2 failed to compile:\n{err2}"))
                    print(f"  FAIL  {name}")
                    continue

                _, stdout1, _ = run_binary(s1_out)
                _, stdout2, _ = run_binary(s2_out)

                if stdout1 == stdout2:
                    passed += 1
                    print(f"  PASS  {name}")
                else:
                    failed += 1
                    failures.append((name, f"Stage 1:\n{stdout1}\nStage 2:\n{stdout2}"))
                    print(f"  FAIL  {name}")

            except Exception as e:
                failed += 1
                failures.append((name, f"EXCEPTION: {e}"))
                print(f"  FAIL  {name}")

    if failures:
        print(f"\nPARITY FAILURES ({len(failures)}):")
        for name, detail in failures:
            print(f"\n--- {name} ---")
            print(detail)

    print(f"\nParity: {passed} passed, {failed} failed")
    return 0 if failed == 0 else 1

# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="Fuse Stage 2 Test Runner")
    parser.add_argument(
        "--compiler", type=Path, default=DEFAULT_COMPILER,
        help="Path to the compiler binary (default: stage1/target/release/fusec)"
    )
    parser.add_argument(
        "--filter", type=str, default=None,
        help="Only run fixtures whose path contains this substring"
    )
    parser.add_argument(
        "--parallel", type=int, default=1,
        help="Number of parallel test workers"
    )
    parser.add_argument(
        "--parity", action="store_true",
        help="Run parity mode: compare Stage 1 vs Stage 2 on core fixtures"
    )
    parser.add_argument(
        "--stage2-compiler", type=Path, default=None,
        help="Stage 2 compiler for parity mode (default: stage1/target/fusec2)"
    )
    parser.add_argument(
        "--bootstrap", action="store_true",
        help="Run bootstrap verification (delegates to cargo test)"
    )
    parser.add_argument(
        "--lsp", action="store_true",
        help="Run LSP tests"
    )

    args = parser.parse_args()

    if args.bootstrap:
        print("Bootstrap: delegating to cargo test stage2_bootstrap...")
        result = subprocess.run(
            ["cargo", "test", "-p", "fusec", "--test", "stage2_bootstrap", "--", "--nocapture"],
            cwd=str(REPO_ROOT / "stage1"),
        )
        return result.returncode

    if args.parity:
        s2 = args.stage2_compiler or (REPO_ROOT / "stage1" / "target" / "fusec2")
        return run_parity(STAGE1_COMPILER, s2, args.parallel)

    if args.lsp:
        lsp_dir = STAGE2_TESTS / "lsp"
        if not lsp_dir.exists():
            print("No LSP tests found yet.")
            return 0
        lsp_tests = sorted(lsp_dir.rglob("*.py"))
        if not lsp_tests:
            print("No LSP test scripts found.")
            return 0
        failed = 0
        for test in lsp_tests:
            if args.filter and args.filter not in test.as_posix():
                continue
            print(f"  RUN   {test.relative_to(STAGE2_TESTS)}")
            result = subprocess.run(
                [sys.executable, str(test)], capture_output=True, text=True, timeout=30
            )
            if result.returncode == 0:
                print(f"  PASS  {test.relative_to(STAGE2_TESTS)}")
            else:
                failed += 1
                print(f"  FAIL  {test.relative_to(STAGE2_TESTS)}")
                print(result.stdout)
                print(result.stderr)
        return 0 if failed == 0 else 1

    # Default: run Stage 2 fixtures
    if not args.compiler.exists():
        print(f"Compiler not found: {args.compiler}")
        print("Build with: cargo build -p fusec --release")
        return 1

    fixtures = discover_fixtures(STAGE2_TESTS, args.filter)
    if not fixtures:
        print(f"No fixtures found (filter={args.filter!r})")
        return 1

    print(f"Stage 2 tests: {len(fixtures)} fixtures")
    print(f"Compiler: {args.compiler}")
    print()

    return run_suite(args.compiler, fixtures, args.parallel)


if __name__ == "__main__":
    sys.exit(main())
