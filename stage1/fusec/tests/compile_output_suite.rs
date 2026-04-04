use std::fs;
use std::path::{Path, PathBuf};

mod harness;

#[test]
fn core_output_fixtures_compile_and_run() {
    let root = harness::repo_root();
    let mut fixtures = fs::read_dir(root.join("tests").join("fuse").join("core"))
        .expect("read core fixture root")
        .flat_map(|entry| {
            let path = entry.expect("dir entry").path();
            walk_fuse_files(&path)
        })
        .collect::<Vec<_>>();
    fixtures.push(root.join("tests").join("fuse").join("milestone").join("four_functions.fuse"));
    fixtures.sort();
    for fixture in fixtures {
        if fixture.components().any(|component| component.as_os_str() == "src") {
            continue;
        }
        let content = fs::read_to_string(&fixture).expect("read fixture");
        if !content.lines().next().unwrap_or_default().starts_with("// EXPECTED OUTPUT") {
            continue;
        }
        let output = harness::unique_output_path(
            fixture
                .file_stem()
                .and_then(|part| part.to_str())
                .unwrap_or("fixture"),
        );
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
        assert_eq!(actual, expected, "{}", fixture.display());
    }
}

fn walk_fuse_files(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return (path.extension().and_then(|ext| ext.to_str()) == Some("fuse"))
            .then(|| vec![path.to_path_buf()])
            .unwrap_or_default();
    }
    if !path.is_dir() {
        return Vec::new();
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(path).expect("read nested fixture dir") {
        files.extend(walk_fuse_files(&entry.expect("dir entry").path()));
    }
    files
}
