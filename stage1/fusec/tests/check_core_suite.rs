use std::fs;

use fusec::{check_path_output, common, hir, parser};

#[test]
fn core_suite_matches_expected_contract() {
    for path in common::core_test_files() {
        let relative = path
            .strip_prefix(common::repo_root())
            .unwrap_or(&path)
            .display()
            .to_string();
        let (kind, expected) =
            common::extract_expected_block(&path).unwrap_or_else(|| panic!("missing expected block: {relative}"));
        let actual = check_path_output(&path);
        if kind.contains("ERROR") {
            assert_eq!(actual.trim(), expected.trim(), "{relative}");
        } else {
            assert!(actual.trim().is_empty(), "{relative}: expected success, got `{actual}`");
        }
    }
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
