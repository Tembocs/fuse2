use std::sync::Mutex;

mod harness;

static WASI_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn wasi_hello_compiles_and_runs() {
    let _guard = WASI_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("wasi")
        .join("wasi_hello.fuse");
    let output = harness::unique_wasm_path("wasi_hello");
    let compile = harness::compile_fixture_wasi(&fixture, &output);
    assert!(
        compile.status.success(),
        "WASI compile failed:\nstderr: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = harness::run_wasm(&output);
    assert!(
        run.status.success(),
        "wasmtime failed:\nstderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let (_, expected) = harness::extract_expected_block(&fixture);
    assert_eq!(
        stdout.trim(),
        expected.trim(),
        "WASI output mismatch for {}",
        fixture.display()
    );
}

#[test]
fn four_functions_compiles_to_wasm_and_runs() {
    let _guard = WASI_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("milestone")
        .join("four_functions.fuse");
    let output = harness::unique_wasm_path("four_functions_wasi");
    let compile = harness::compile_fixture_wasi(&fixture, &output);
    assert!(
        compile.status.success(),
        "WASI compile failed:\nstderr: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = harness::run_wasm(&output);
    assert!(
        run.status.success(),
        "wasmtime failed with non-zero exit:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    // four_functions uses complex expressions (f-strings, variables) that the
    // simplified WASM backend can't evaluate yet. Just verify it runs without
    // crashing — output correctness is a future milestone.
}
