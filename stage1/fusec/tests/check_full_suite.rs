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

    assert_eq!(output, 6, "unexpected number of full output fixtures");
    assert_eq!(error, 3, "unexpected number of full error fixtures");
    assert_eq!(warning, 1, "unexpected number of full warning fixtures");
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
