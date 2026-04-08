#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureExpectation {
    Output,
    Error,
    Warning,
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("stage1/fusec must sit under the repo root")
        .to_path_buf()
}

pub fn extract_expected_block(path: &Path) -> (FixtureExpectation, String) {
    let content = fs::read_to_string(path).expect("read fixture");
    let mut lines = content.lines();
    let first = lines.next().expect("fixture starts with expected block");
    let kind = if first.starts_with("// EXPECTED OUTPUT") {
        FixtureExpectation::Output
    } else if first.starts_with("// EXPECTED ERROR") {
        FixtureExpectation::Error
    } else if first.starts_with("// EXPECTED WARNING") {
        FixtureExpectation::Warning
    } else {
        panic!("missing expected block: {}", path.display());
    };

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

    (kind, expected.join("\n"))
}

pub fn unique_output_path(stem: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis();
    repo_root()
        .join("stage1")
        .join("target")
        .join(format!("{stem}-{stamp}.exe"))
}

pub fn compile_fixture(fixture: &Path, output: &Path) -> std::process::Output {
    Command::new(
        repo_root()
            .join("stage1")
            .join("target")
            .join("debug")
            .join(format!("fusec{}", std::env::consts::EXE_SUFFIX)),
    )
        .arg(fixture)
        .arg("-o")
        .arg(output)
        .output()
        .expect("run fusec compile")
}

pub fn run_compiled_binary(binary: &Path) -> std::process::Output {
    Command::new(binary)
        .output()
        .expect("run compiled binary")
}

pub fn compile_fixture_wasi(fixture: &Path, output: &Path) -> std::process::Output {
    Command::new(
        repo_root()
            .join("stage1")
            .join("target")
            .join("debug")
            .join(format!("fusec{}", std::env::consts::EXE_SUFFIX)),
    )
        .arg(fixture)
        .arg("-o")
        .arg(output)
        .arg("--target")
        .arg("wasi")
        .output()
        .expect("run fusec compile --target wasi")
}

pub fn run_wasm(wasm_file: &Path) -> std::process::Output {
    Command::new("wasmtime")
        .arg("run")
        .arg(wasm_file)
        .output()
        .expect("run wasmtime")
}

pub fn unique_wasm_path(stem: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis();
    repo_root()
        .join("stage1")
        .join("target")
        .join(format!("{stem}-{stamp}.wasm"))
}

// ---------------------------------------------------------------------------
// Stage 2 bootstrap helpers (W7.4)
// ---------------------------------------------------------------------------

/// Build the Stage 2 compiler by compiling stage2/src/main.fuse with Stage 1.
/// Returns the path to the resulting fusec2 binary. Cached per test run via
/// a well-known output path.
pub fn build_stage2_compiler() -> PathBuf {
    let root = repo_root();
    let stage2_main = root.join("stage2").join("src").join("main.fuse");
    let exe_suffix = std::env::consts::EXE_SUFFIX;
    let fusec2 = root
        .join("stage1")
        .join("target")
        .join(format!("fusec2{exe_suffix}"));

    // Only rebuild if the binary doesn't exist (fast path for repeat runs).
    // The test runner can delete the binary to force a rebuild.
    if fusec2.exists() {
        return fusec2;
    }

    let compile = compile_fixture(&stage2_main, &fusec2);
    assert!(
        compile.status.success(),
        "failed to build Stage 2 compiler:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );
    fusec2
}

/// Compile a fixture using the Stage 2 compiler (fusec2).
pub fn compile_fixture_stage2(
    fusec2: &Path,
    fixture: &Path,
    output: &Path,
) -> std::process::Output {
    Command::new(fusec2)
        .arg(fixture)
        .arg("-o")
        .arg(output)
        .output()
        .expect("run fusec2 compile")
}

/// Run fusec2 --check on a fixture.
pub fn check_fixture_stage2(
    fusec2: &Path,
    fixture: &Path,
) -> std::process::Output {
    Command::new(fusec2)
        .arg("--check")
        .arg(fixture)
        .output()
        .expect("run fusec2 --check")
}
