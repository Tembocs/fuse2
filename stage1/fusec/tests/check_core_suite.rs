use std::fs;

use fusec::{check_path_output, common, hir, parser};

/// Fixtures that are temporarily blocked on Wave B11 of
/// docs/fuse-stage2-parity-plan.md. Each one imports stage2/src/*.fuse
/// modules whose `.concat()`, `.unwrap()`, etc. calls require stdlib
/// imports that have not yet been added (B2 made these errors loud,
/// B11 adds the imports). When B11 lands, remove the corresponding
/// entries from this list and the test will lock the fix in.
///
/// Format: relative path under tests/, exactly as printed by the
/// failure message. Slashes are forward (Unix-style); the comparison
/// normalizes Windows backslashes.
const B11_BLOCKED_FIXTURES: &[&str] = &[
    "tests/fuse/core/types/checker_exhaustiveness.fuse",
    "tests/fuse/core/types/checker_module.fuse",
    "tests/fuse/core/types/checker_ownership.fuse",
];

fn is_blocked_on_b11(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/");
    B11_BLOCKED_FIXTURES.iter().any(|p| normalized == *p)
}

#[test]
fn core_suite_matches_expected_contract() {
    let mut blocked_seen: Vec<String> = Vec::new();
    let mut real_failures: Vec<String> = Vec::new();
    for path in common::core_test_files() {
        let relative = path
            .strip_prefix(common::repo_root())
            .unwrap_or(&path)
            .display()
            .to_string();
        let (kind, expected) =
            common::extract_expected_block(&path).unwrap_or_else(|| panic!("missing expected block: {relative}"));
        let actual = check_path_output(&path);
        let outcome = if kind.contains("ERROR") || kind.contains("WARNING") {
            if actual.trim() == expected.trim() { Ok(()) } else { Err(format!("{relative}: expected `{}`, got `{}`", expected.trim(), actual.trim())) }
        } else if actual.trim().is_empty() {
            Ok(())
        } else {
            Err(format!("{relative}: expected success, got `{actual}`"))
        };
        match outcome {
            Ok(()) => {
                if is_blocked_on_b11(&relative) {
                    panic!(
                        "{relative} is in B11_BLOCKED_FIXTURES but now passes. Remove it from the list — Wave B11 may have landed."
                    );
                }
            }
            Err(message) => {
                if is_blocked_on_b11(&relative) {
                    blocked_seen.push(relative);
                } else {
                    real_failures.push(message);
                }
            }
        }
    }
    if !real_failures.is_empty() {
        panic!(
            "{} real failure(s):\n{}",
            real_failures.len(),
            real_failures.join("\n")
        );
    }
    eprintln!(
        "core_suite: {} fixtures blocked on B11 (expected): {:?}",
        blocked_seen.len(),
        blocked_seen
    );
}

#[test]
fn hir_lowering_groups_top_level_items() {
    let path = common::repo_root()
        .join("tests")
        .join("fuse")
        .join("core")
        .join("modules")
        .join("import_basic.fuse");
    let source = fs::read_to_string(&path).expect("read representative input");
    let program = parser::parse_source(&source, "import_basic.fuse").expect("parse representative input");
    let module = hir::lower::lower_program(&program, path);

    assert_eq!(module.imports.len(), 1);
    assert_eq!(module.functions.len(), 1);
    assert!(module.data_classes.is_empty());
    assert!(module.enums.is_empty());
}
