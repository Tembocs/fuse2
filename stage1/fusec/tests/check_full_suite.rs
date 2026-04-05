use std::fs;

use fusec::{check_path_output, common, parser};

mod harness;

#[test]
fn full_suite_fixtures_are_classified_for_output_error_and_warning() {
    let mut output = 0usize;
    let mut error = 0usize;
    let mut warning = 0usize;

    for path in common::full_test_files() {
        let relative = path
            .strip_prefix(common::repo_root())
            .unwrap_or(&path)
            .display()
            .to_string();
        let (kind, expected) = harness::extract_expected_block(&path);
        assert!(
            !expected.trim().is_empty(),
            "{relative}: expected block should not be empty"
        );
        match kind {
            harness::FixtureExpectation::Output => output += 1,
            harness::FixtureExpectation::Error => error += 1,
            harness::FixtureExpectation::Warning => warning += 1,
        }
    }

    assert_eq!(output, 25, "unexpected number of full output fixtures");
    assert_eq!(error, 5, "unexpected number of full error fixtures");
    assert_eq!(warning, 3, "unexpected number of full warning fixtures");
}

#[test]
fn current_full_fixtures_parse_as_stage1_inputs() {
    for path in common::full_test_files() {
        let relative = path
            .strip_prefix(common::repo_root())
            .unwrap_or(&path)
            .display()
            .to_string();
        let source = fs::read_to_string(&path).expect("read full fixture");
        parser::parse_source(
            &source,
            path.file_name()
                .and_then(|part| part.to_str())
                .unwrap_or("fixture.fuse"),
        )
        .unwrap_or_else(|error| panic!("{relative}: failed to parse: {}", error.render()));
    }
}

#[test]
fn spawn_mutref_rejected_matches_current_checker_contract() {
    let path = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("spawn_mutref_rejected.fuse");
    let (_, expected) = harness::extract_expected_block(&path);
    let actual = check_path_output(&path);
    assert_eq!(actual.trim(), expected.trim(), "{}", path.display());
}

#[test]
fn shared_no_rank_matches_current_checker_contract() {
    let path = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_no_rank.fuse");
    let (_, expected) = harness::extract_expected_block(&path);
    let actual = check_path_output(&path);
    assert_eq!(actual.trim(), expected.trim(), "{}", path.display());
}

#[test]
fn shared_rank_violation_matches_current_checker_contract() {
    let path = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_rank_violation.fuse");
    let (_, expected) = harness::extract_expected_block(&path);
    let actual = check_path_output(&path);
    assert_eq!(actual.trim(), expected.trim(), "{}", path.display());
}

#[test]
fn shared_rank_ascending_is_checker_clean() {
    let path = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_rank_ascending.fuse");
    let actual = check_path_output(&path);
    assert!(
        actual.trim().is_empty(),
        "{}: expected success, got `{actual}`",
        path.display()
    );
}

#[test]
fn hardening_shared_fixtures_are_checker_clean() {
    let concurrency = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency");
    let fixtures = [
        "shared_read_after_write.fuse",
        "shared_multiple_reads.fuse",
        "shared_write_read_cycles.fuse",
        "shared_nested_data.fuse",
        "shared_destruction.fuse",
        "shared_read_then_write.fuse",
        "shared_identity.fuse",
        "shared_no_aliasing.fuse",
        "shared_value_rendering.fuse",
    ];
    for name in fixtures {
        let path = concurrency.join(name);
        let actual = check_path_output(&path);
        assert!(
            actual.trim().is_empty(),
            "{}: expected checker success, got `{actual}`",
            path.display()
        );
    }
}

#[test]
fn hardening_wave2_output_fixtures_are_checker_clean() {
    let root = harness::repo_root().join("tests").join("fuse").join("full");
    let fixtures = [
        root.join("concurrency").join("shared_try_write_success.fuse"),
        root.join("concurrency").join("shared_try_write_timeout.fuse"),
        root.join("async").join("read_guard_across_await.fuse"),
        root.join("simd").join("simd_sum_float.fuse"),
        root.join("simd").join("simd_sum_empty.fuse"),
        root.join("simd").join("simd_sum_large.fuse"),
    ];
    for path in fixtures {
        let actual = check_path_output(&path);
        assert!(
            actual.trim().is_empty(),
            "{}: expected checker success, got `{actual}`",
            path.display()
        );
    }
}

#[test]
fn nested_await_write_guard_matches_warning_contract() {
    let path = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("async")
        .join("nested_await_write_guard.fuse");
    let (_, expected) = harness::extract_expected_block(&path);
    let actual = check_path_output(&path);
    assert_eq!(actual.trim(), expected.trim(), "{}", path.display());
}

#[test]
fn multiple_shared_ranks_await_matches_warning_contract() {
    let path = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("async")
        .join("multiple_shared_ranks_await.fuse");
    let (_, expected) = harness::extract_expected_block(&path);
    let actual = check_path_output(&path);
    assert_eq!(actual.trim(), expected.trim(), "{}", path.display());
}

#[test]
fn write_guard_across_await_matches_current_warning_contract() {
    let path = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("async")
        .join("write_guard_across_await.fuse");
    let (_, expected) = harness::extract_expected_block(&path);
    let actual = check_path_output(&path);
    assert_eq!(actual.trim(), expected.trim(), "{}", path.display());
}

#[test]
fn full_channel_stdlib_surface_is_present_and_parseable() {
    let stdlib = harness::repo_root().join("stdlib").join("full");
    let importer = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("chan_basic.fuse");

    let expected = [
        ("chan", stdlib.join("chan.fuse")),
        ("shared", stdlib.join("shared.fuse")),
        ("timer", stdlib.join("timer.fuse")),
        ("simd", stdlib.join("simd.fuse")),
        ("http", stdlib.join("http.fuse")),
    ];

    for (module_name, path) in expected {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("read {}", path.display()));
        parser::parse_source(
            &source,
            path.file_name()
                .and_then(|part| part.to_str())
                .unwrap_or("fixture.fuse"),
        )
        .unwrap_or_else(|error| panic!("{}: {}", path.display(), error.render()));

        let resolved = common::resolve_import_path(&importer, &format!("full.{module_name}"));
        assert!(
            resolved.as_deref() == Some(path.as_path()),
            "expected full.{module_name} to resolve to {}",
            path.display()
        );
    }
}
