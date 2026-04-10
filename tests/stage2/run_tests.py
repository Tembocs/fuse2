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

Exit codes:
  0  — every test in the requested mode passed
  1  — at least one real test failure (the compiler ran and produced wrong output)
  2  — the requested mode is BLOCKED by missing prerequisites (e.g.
       --parity or --bootstrap requested but fusec2 has not been built
       yet). Exit 2 is intentionally distinguishable from exit 1 so CI
       can tell "Stage 2 not yet built" apart from "Stage 2 produces
       wrong output." See docs/fuse-stage2-parity-plan.md, Phase B0.2.
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

EXE_SUFFIX = ".exe" if sys.platform == "win32" else ""
DEFAULT_COMPILER = REPO_ROOT / "stage1" / "target" / "release" / f"fusec{EXE_SUFFIX}"
STAGE1_COMPILER = DEFAULT_COMPILER  # for parity mode
DEFAULT_FUSEC2 = REPO_ROOT / "stage1" / "target" / f"fusec2{EXE_SUFFIX}"
KNOWN_FAILURES_FILE = STAGE2_TESTS / "known_failures.txt"

# Exit code 2 means "blocked by missing prerequisite," distinct from
# exit 1 ("real test failure"). See module docstring and Phase B0.2.
EXIT_BLOCKED = 2


def fusec2_exists_or_exit(path: Path, mode_name: str) -> None:
    """Precondition gate for modes that require a built fusec2 binary.

    If `path` does not exist, print a clear "blocked" message and exit
    with code 2 (EXIT_BLOCKED). This is intentionally NOT a silent skip:
    a CI run that requested --parity or --bootstrap and gets exit 2 has
    a definite signal that the work cannot proceed until Stage 2 is
    built. Once fusec2 exists, this gate becomes a no-op and the real
    test runs to completion (success exits 0, failure exits 1).
    """
    if path.exists():
        return
    msg = (
        f"\n{mode_name} cannot run: fusec2 binary not built.\n"
        f"  Expected at: {path}\n"
        f"  Reason:      Stage 2 self-compile is gated by\n"
        f"               docs/fuse-stage2-parity-plan.md (Wave B12).\n"
        f"  Exit code:   {EXIT_BLOCKED} (distinct from test-failure exit 1).\n"
    )
    print(msg)
    sys.exit(EXIT_BLOCKED)

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

    stem = fixture.relative_to(STAGE2_TESTS).as_posix().replace("/", "_").replace(".fuse", "")
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


def load_known_failures(path: Path):
    """Load known_failures.txt → dict of {fixture_posix_path: bug_id}."""
    known = {}
    if not path.exists():
        return known
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        # Format: fixture_path  # bug_id: description
        if "#" in line:
            fixture_part, comment = line.split("#", 1)
            fixture_part = fixture_part.strip()
            bug_id = comment.strip()
        else:
            fixture_part = line
            bug_id = "unknown"
        if fixture_part:
            known[fixture_part] = bug_id
    return known

# ---------------------------------------------------------------------------
# Main runner
# ---------------------------------------------------------------------------

def run_suite(compiler: Path, fixtures: list, parallel: int = 1,
              known_failures: dict = None):
    """Run all fixtures and print results. Returns exit code."""
    if known_failures is None:
        known_failures = {}

    passed = 0
    failed = 0
    skipped = 0
    known_failed = 0
    known_passed = 0  # known failure that now passes — bug may be fixed
    failures = []
    unexpected_passes = []
    known_fail_list = []

    start = time.time()

    def classify(name, result, detail):
        nonlocal passed, failed, skipped, known_failed, known_passed
        is_known = name in known_failures
        bug_id = known_failures.get(name, "")

        if result is None:
            skipped += 1
            print(f"  SKIP  {name}")
        elif result:
            if is_known:
                known_passed += 1
                unexpected_passes.append((name, bug_id))
                print(f"  PASS* {name}  (was known failure: {bug_id})")
            else:
                passed += 1
                print(f"  PASS  {name}")
        else:
            if is_known:
                known_failed += 1
                known_fail_list.append((name, bug_id))
                print(f"  XFAIL {name}  ({bug_id})")
            else:
                failed += 1
                failures.append((name, detail))
                print(f"  FAIL  {name}")

    with tempfile.TemporaryDirectory(prefix="fuse_stage2_") as tmp_dir:
        if parallel > 1:
            with ThreadPoolExecutor(max_workers=parallel) as pool:
                futures = {
                    pool.submit(run_test, compiler, f, tmp_dir): f
                    for f in fixtures
                }
                for future in as_completed(futures):
                    name, result, detail = future.result()
                    classify(name, result, detail)
        else:
            for fixture in fixtures:
                name, result, detail = run_test(compiler, fixture, tmp_dir)
                classify(name, result, detail)

    elapsed = time.time() - start

    # Print unexpected failure details
    if failures:
        print(f"\n{'='*60}")
        print(f"UNEXPECTED FAILURES ({len(failures)}):")
        print(f"{'='*60}")
        for name, detail in failures:
            print(f"\n--- {name} ---")
            print(detail)

    # Flag known failures that now pass
    if unexpected_passes:
        print(f"\n{'='*60}")
        print(f"KNOWN FAILURES NOW PASSING ({len(unexpected_passes)}):")
        print(f"  Remove these from known_failures.txt:")
        print(f"{'='*60}")
        for name, bug_id in unexpected_passes:
            print(f"  {name}  ({bug_id})")

    # Summary
    print(f"\n{'='*60}")
    parts = [f"{passed} passed"]
    if known_passed:
        parts.append(f"{known_passed} fixed (remove from known_failures.txt)")
    if known_failed:
        parts.append(f"{known_failed} known failures")
    if failed:
        parts.append(f"{failed} FAILED")
    if skipped:
        parts.append(f"{skipped} skipped")
    print(f"  {', '.join(parts)}")
    print(f"  {elapsed:.1f}s elapsed")
    print(f"{'='*60}")

    # Exit code: only unexpected failures cause non-zero
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

            s1_out = Path(tmp_dir) / (stem + "_s1" + EXE_SUFFIX)
            s2_out = Path(tmp_dir) / (stem + "_s2" + EXE_SUFFIX)

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
        # Bootstrap mode rebuilds fusec2 from scratch via cargo. The
        # gate here checks for a previously-built fusec2 as evidence
        # that Stage 2 self-compile has worked at least once. If it has
        # never built, exit 2 with a clear blocked message instead of
        # letting the cargo test produce a cryptic Gen 0 error.
        fusec2_exists_or_exit(DEFAULT_FUSEC2, "T5 Bootstrap")
        print("Bootstrap: delegating to cargo test stage2_bootstrap...")
        result = subprocess.run(
            ["cargo", "test", "-p", "fusec", "--test", "stage2_bootstrap", "--", "--nocapture"],
            cwd=str(REPO_ROOT / "stage1"),
        )
        return result.returncode

    if args.parity:
        s2 = args.stage2_compiler or DEFAULT_FUSEC2
        fusec2_exists_or_exit(s2, "T4 Parity")
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

    known = load_known_failures(KNOWN_FAILURES_FILE)

    print(f"Stage 2 tests: {len(fixtures)} fixtures, {len(known)} known failures")
    print(f"Compiler: {args.compiler}")
    print()

    return run_suite(args.compiler, fixtures, args.parallel, known)


if __name__ == "__main__":
    sys.exit(main())
