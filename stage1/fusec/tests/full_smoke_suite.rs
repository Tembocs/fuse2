use std::fs;
use std::sync::Mutex;

mod harness;

static COMPILE_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn chan_basic_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
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
fn bounded_chan_smoke_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("stage1")
        .join("target")
        .join("phase8_bounded_chan_smoke.fuse");
    fs::write(
        &fixture,
        "@entrypoint\nfn main() {\n  val ch = Chan::<Int>.bounded(1)\n  ch.send(1)\n  println(ch.recv())\n}\n",
    )
    .expect("write bounded smoke fixture");
    let output = harness::unique_output_path("bounded_chan_full");
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
    assert_eq!(actual.trim(), "1", "{}", fixture.display());
}

#[test]
fn shared_rank_ascending_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_rank_ascending.fuse");
    let output = harness::unique_output_path("shared_rank_ascending_full");
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
fn await_basic_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("async")
        .join("await_basic.fuse");
    let output = harness::unique_output_path("await_basic_full");
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
#[ignore = "suspend execution remains part of the later async-runtime checkpoint"]
fn suspend_fn_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().expect("compile lock");
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("async")
        .join("suspend_fn.fuse");
    let output = harness::unique_output_path("suspend_fn_full");
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
