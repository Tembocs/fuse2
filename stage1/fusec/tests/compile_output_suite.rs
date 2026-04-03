use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("stage1/fusec must sit under the repo root")
        .to_path_buf()
}

fn extract_expected_output(path: &Path) -> String {
    let content = fs::read_to_string(path).expect("read fixture");
    let mut lines = content.lines();
    let first = lines.next().expect("fixture starts with expected block");
    assert!(first.starts_with("// EXPECTED OUTPUT"), "{path:?}");
    let mut expected = Vec::new();
    for line in lines {
        if let Some(rest) = line.strip_prefix("// ") {
            expected.push(rest.to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("//") {
            expected.push(rest.to_string());
            continue;
        }
        break;
    }
    expected.join("\n")
}

fn unique_output_path(stem: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis();
    repo_root()
        .join("stage1")
        .join("target")
        .join(format!("{stem}-{stamp}.exe"))
}

fn compile_and_run(fixture: &Path) -> String {
    let output = unique_output_path(
        fixture
            .file_stem()
            .and_then(|part| part.to_str())
            .unwrap_or("fixture"),
    );
    let fusec = env!("CARGO_BIN_EXE_fusec");
    let compile = Command::new(fusec)
        .arg(fixture)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("run fusec compile");
    assert!(
        compile.status.success(),
        "compile failed for {}:\nstdout:\n{}\nstderr:\n{}",
        fixture.display(),
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&output).output().expect("run compiled binary");
    assert!(
        run.status.success(),
        "binary failed for {}:\nstdout:\n{}\nstderr:\n{}",
        fixture.display(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8(run.stdout).expect("utf-8 stdout")
}

#[test]
fn core_output_fixtures_compile_and_run() {
    let root = repo_root();
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
        let actual = compile_and_run(&fixture);
        let expected = extract_expected_output(&fixture);
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
