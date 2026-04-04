use std::fs;

mod harness;

#[test]
fn chan_basic_fixture_compiles_and_runs() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("chan_basic.fuse");
    let output = harness::unique_output_path("chan_basic_full");
    let compile = harness::compile_fixture(&fixture, &output);
    assert!(
        compile.status.success(),
        "compile failed for {}:\nstdout:\n{}\nstderr:\n{}",
        fixture.display(),
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = harness::run_compiled_binary(&output);
    assert!(
        run.status.success(),
        "binary failed for {}:\nstdout:\n{}\nstderr:\n{}",
        fixture.display(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let actual = String::from_utf8(run.stdout).expect("utf-8 stdout");
    let (_, expected) = harness::extract_expected_block(&fixture);
    assert_eq!(actual.trim(), expected.trim(), "{}", fixture.display());
}

#[test]
fn full_fixture_files_are_present() {
    let root = harness::repo_root().join("tests").join("fuse").join("full");
    let count = walk(&root);
    assert_eq!(count, 10, "unexpected full fixture count");
}

fn walk(path: &std::path::Path) -> usize {
    if path.is_file() {
        return usize::from(path.extension().and_then(|ext| ext.to_str()) == Some("fuse"));
    }
    fs::read_dir(path)
        .expect("read full fixture tree")
        .map(|entry| walk(&entry.expect("dir entry").path()))
        .sum()
}
