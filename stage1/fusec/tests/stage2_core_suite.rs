//! W7.4 — Core Test Suite Validation for Stage 2.
//!
//! Builds the Stage 2 compiler (fusec2) using Stage 1, then compiles every
//! `tests/fuse/core/` and `tests/fuse/milestone/` fixture with fusec2 and
//! verifies output matches the EXPECTED OUTPUT / EXPECTED ERROR block.

use std::fs;
use std::path::{Path, PathBuf};

mod harness;

// ---------------------------------------------------------------------------
// EXPECTED OUTPUT fixtures — compile with fusec2, run binary, match stdout
// ---------------------------------------------------------------------------

#[test]
fn stage2_core_output_fixtures_compile_and_run() {
    let root = harness::repo_root();
    let fusec2 = harness::build_stage2_compiler();

    let mut fixtures = collect_fuse_files(&root.join("tests").join("fuse").join("core"));
    fixtures.push(
        root.join("tests")
            .join("fuse")
            .join("milestone")
            .join("four_functions.fuse"),
    );
    fixtures.sort();

    let mut passed = 0usize;
    let mut skipped = 0usize;
    let mut failures = Vec::<String>::new();

    for fixture in &fixtures {
        // Skip helper modules (files inside src/ subdirectories).
        if fixture.components().any(|c| c.as_os_str() == "src") {
            skipped += 1;
            continue;
        }
        let (kind, expected) = harness::extract_expected_block(fixture);
        if kind != harness::FixtureExpectation::Output {
            skipped += 1;
            continue;
        }

        let relative = fixture
            .strip_prefix(&root)
            .unwrap_or(fixture)
            .display()
            .to_string();

        let output = harness::unique_output_path(
            fixture
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("fixture"),
        );

        // Compile with Stage 2.
        let compile = harness::compile_fixture_stage2(&fusec2, fixture, &output);
        if !compile.status.success() {
            failures.push(format!(
                "COMPILE FAIL: {relative}\nstderr:\n{}",
                String::from_utf8_lossy(&compile.stderr)
            ));
            continue;
        }

        // Run the binary.
        let run = harness::run_compiled_binary(&output);
        if !run.status.success() {
            failures.push(format!(
                "RUN FAIL: {relative}\nstderr:\n{}",
                String::from_utf8_lossy(&run.stderr)
            ));
            continue;
        }

        let actual = String::from_utf8(run.stdout).expect("utf-8 stdout");
        if actual.trim() != expected.trim() {
            failures.push(format!(
                "OUTPUT MISMATCH: {relative}\n  expected: {:?}\n  actual:   {:?}",
                expected.trim(),
                actual.trim()
            ));
            continue;
        }

        passed += 1;
    }

    // Summary.
    let total = fixtures.len();
    eprintln!(
        "\n=== Stage 2 Core Output Suite ===\n  passed: {passed}\n  skipped: {skipped}\n  failed: {}\n  total:  {total}\n",
        failures.len()
    );

    if !failures.is_empty() {
        for (i, fail) in failures.iter().enumerate() {
            eprintln!("--- failure {} ---\n{fail}\n", i + 1);
        }
        panic!(
            "{} / {} EXPECTED OUTPUT fixtures failed (Stage 2)",
            failures.len(),
            total - skipped
        );
    }
}

// ---------------------------------------------------------------------------
// EXPECTED ERROR fixtures — compile with fusec2 --check, match diagnostics
// ---------------------------------------------------------------------------

#[test]
fn stage2_core_error_fixtures_match_diagnostics() {
    let root = harness::repo_root();
    let fusec2 = harness::build_stage2_compiler();

    let fixtures = collect_fuse_files(&root.join("tests").join("fuse").join("core"));

    let mut passed = 0usize;
    let mut skipped = 0usize;
    let mut failures = Vec::<String>::new();

    for fixture in &fixtures {
        if fixture.components().any(|c| c.as_os_str() == "src") {
            skipped += 1;
            continue;
        }
        let (kind, expected) = harness::extract_expected_block(fixture);
        if kind != harness::FixtureExpectation::Error {
            skipped += 1;
            continue;
        }

        let relative = fixture
            .strip_prefix(&root)
            .unwrap_or(fixture)
            .display()
            .to_string();

        let check = harness::check_fixture_stage2(&fusec2, fixture);
        let actual = String::from_utf8_lossy(&check.stdout);
        let actual_stderr = String::from_utf8_lossy(&check.stderr);
        // Merge stdout and stderr — diagnostics may go to either stream.
        let combined = format!("{actual}{actual_stderr}");

        if combined.trim() != expected.trim() {
            failures.push(format!(
                "ERROR MISMATCH: {relative}\n  expected: {:?}\n  actual:   {:?}",
                expected.trim(),
                combined.trim()
            ));
            continue;
        }
        passed += 1;
    }

    let total = fixtures.len();
    eprintln!(
        "\n=== Stage 2 Core Error Suite ===\n  passed: {passed}\n  skipped: {skipped}\n  failed: {}\n  total:  {total}\n",
        failures.len()
    );

    if !failures.is_empty() {
        for (i, fail) in failures.iter().enumerate() {
            eprintln!("--- failure {} ---\n{fail}\n", i + 1);
        }
        panic!(
            "{} / {} EXPECTED ERROR fixtures failed (Stage 2)",
            failures.len(),
            total - skipped
        );
    }
}

// ---------------------------------------------------------------------------
// EXPECTED WARNING fixtures — compile with fusec2 --check, match warnings
// ---------------------------------------------------------------------------

#[test]
fn stage2_core_warning_fixtures_match_diagnostics() {
    let root = harness::repo_root();
    let fusec2 = harness::build_stage2_compiler();

    let fixtures = collect_fuse_files(&root.join("tests").join("fuse").join("core"));

    let mut passed = 0usize;
    let mut skipped = 0usize;
    let mut failures = Vec::<String>::new();

    for fixture in &fixtures {
        if fixture.components().any(|c| c.as_os_str() == "src") {
            skipped += 1;
            continue;
        }
        let (kind, expected) = harness::extract_expected_block(fixture);
        if kind != harness::FixtureExpectation::Warning {
            skipped += 1;
            continue;
        }

        let relative = fixture
            .strip_prefix(&root)
            .unwrap_or(fixture)
            .display()
            .to_string();

        let check = harness::check_fixture_stage2(&fusec2, fixture);
        let actual = String::from_utf8_lossy(&check.stdout);
        let actual_stderr = String::from_utf8_lossy(&check.stderr);
        let combined = format!("{actual}{actual_stderr}");

        if combined.trim() != expected.trim() {
            failures.push(format!(
                "WARNING MISMATCH: {relative}\n  expected: {:?}\n  actual:   {:?}",
                expected.trim(),
                combined.trim()
            ));
            continue;
        }
        passed += 1;
    }

    let total = fixtures.len();
    eprintln!(
        "\n=== Stage 2 Core Warning Suite ===\n  passed: {passed}\n  skipped: {skipped}\n  failed: {}\n  total:  {total}\n",
        failures.len()
    );

    if !failures.is_empty() {
        for (i, fail) in failures.iter().enumerate() {
            eprintln!("--- failure {} ---\n{fail}\n", i + 1);
        }
        panic!(
            "{} / {} EXPECTED WARNING fixtures failed (Stage 2)",
            failures.len(),
            total - skipped
        );
    }
}

// ---------------------------------------------------------------------------
// Stage 1 vs Stage 2 parity — verify identical output on all OUTPUT fixtures
// ---------------------------------------------------------------------------

#[test]
fn stage1_and_stage2_produce_identical_output() {
    let root = harness::repo_root();
    let fusec2 = harness::build_stage2_compiler();

    let mut fixtures = collect_fuse_files(&root.join("tests").join("fuse").join("core"));
    fixtures.push(
        root.join("tests")
            .join("fuse")
            .join("milestone")
            .join("four_functions.fuse"),
    );
    fixtures.sort();

    let mut compared = 0usize;
    let mut skipped = 0usize;
    let mut failures = Vec::<String>::new();

    for fixture in &fixtures {
        if fixture.components().any(|c| c.as_os_str() == "src") {
            skipped += 1;
            continue;
        }
        let (kind, _) = harness::extract_expected_block(fixture);
        if kind != harness::FixtureExpectation::Output {
            skipped += 1;
            continue;
        }

        let relative = fixture
            .strip_prefix(&root)
            .unwrap_or(fixture)
            .display()
            .to_string();

        let stem = fixture
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("fixture");

        // Compile and run with Stage 1.
        let s1_out = harness::unique_output_path(&format!("s1_{stem}"));
        let s1_compile = harness::compile_fixture(fixture, &s1_out);
        if !s1_compile.status.success() {
            skipped += 1;
            continue; // Skip if Stage 1 itself can't compile this fixture.
        }
        let s1_run = harness::run_compiled_binary(&s1_out);
        let s1_stdout = String::from_utf8_lossy(&s1_run.stdout).to_string();

        // Compile and run with Stage 2.
        let s2_out = harness::unique_output_path(&format!("s2_{stem}"));
        let s2_compile = harness::compile_fixture_stage2(&fusec2, fixture, &s2_out);
        if !s2_compile.status.success() {
            failures.push(format!(
                "STAGE2 COMPILE FAIL: {relative}\nstderr:\n{}",
                String::from_utf8_lossy(&s2_compile.stderr)
            ));
            continue;
        }
        let s2_run = harness::run_compiled_binary(&s2_out);
        let s2_stdout = String::from_utf8_lossy(&s2_run.stdout).to_string();

        if s1_stdout.trim() != s2_stdout.trim() {
            failures.push(format!(
                "PARITY MISMATCH: {relative}\n  stage1: {:?}\n  stage2: {:?}",
                s1_stdout.trim(),
                s2_stdout.trim()
            ));
            continue;
        }
        compared += 1;
    }

    let total = fixtures.len();
    eprintln!(
        "\n=== Stage 1 vs Stage 2 Parity ===\n  matched: {compared}\n  skipped: {skipped}\n  failed: {}\n  total:  {total}\n",
        failures.len()
    );

    if !failures.is_empty() {
        for (i, fail) in failures.iter().enumerate() {
            eprintln!("--- failure {} ---\n{fail}\n", i + 1);
        }
        panic!(
            "{} / {} fixtures produce different output between Stage 1 and Stage 2",
            failures.len(),
            total - skipped
        );
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn collect_fuse_files(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if dir.is_file() {
        if dir.extension().and_then(|e| e.to_str()) == Some("fuse") {
            result.push(dir.to_path_buf());
        }
        return result;
    }
    if !dir.is_dir() {
        return result;
    }
    for entry in fs::read_dir(dir).expect("read fixture directory") {
        let path = entry.expect("dir entry").path();
        result.extend(collect_fuse_files(&path));
    }
    result
}
