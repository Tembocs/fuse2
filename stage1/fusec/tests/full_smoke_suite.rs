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
fn chan_bounded_backpressure_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("chan_bounded_backpressure.fuse");
    let output = harness::unique_output_path("chan_bounded_backpressure_full");
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
fn shared_write_roundtrip_smoke_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("stage1")
        .join("target")
        .join("phase8_shared_roundtrip_smoke.fuse");
    fs::write(
        &fixture,
        "@value\ndata class Box(var value: Int)\n\n@entrypoint\nfn main() {\n  @rank(1) val shared: Shared<Box> = Shared::<Box>.new(Box(1))\n  val item = shared.write()\n  item.value = 2\n  println(shared.read().value)\n}\n",
    )
    .expect("write shared roundtrip smoke fixture");
    let output = harness::unique_output_path("shared_roundtrip_full");
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
    assert_eq!(actual.trim(), "2", "{}", fixture.display());
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
fn simd_sum_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("simd")
        .join("simd_sum.fuse");
    let output = harness::unique_output_path("simd_sum_full");
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
fn shared_read_after_write_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_read_after_write.fuse");
    let output = harness::unique_output_path("shared_read_after_write_full");
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
fn shared_multiple_reads_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_multiple_reads.fuse");
    let output = harness::unique_output_path("shared_multiple_reads_full");
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
fn shared_write_read_cycles_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_write_read_cycles.fuse");
    let output = harness::unique_output_path("shared_write_read_cycles_full");
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
fn shared_nested_data_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_nested_data.fuse");
    let output = harness::unique_output_path("shared_nested_data_full");
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
fn shared_destruction_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_destruction.fuse");
    let output = harness::unique_output_path("shared_destruction_full");
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
fn shared_read_then_write_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_read_then_write.fuse");
    let output = harness::unique_output_path("shared_read_then_write_full");
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
fn shared_identity_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_identity.fuse");
    let output = harness::unique_output_path("shared_identity_full");
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
fn shared_no_aliasing_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_no_aliasing.fuse");
    let output = harness::unique_output_path("shared_no_aliasing_full");
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
fn shared_value_rendering_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_value_rendering.fuse");
    let output = harness::unique_output_path("shared_value_rendering_full");
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
fn shared_try_write_success_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_try_write_success.fuse");
    let output = harness::unique_output_path("shared_try_write_success_full");
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
fn shared_try_write_timeout_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_try_write_timeout.fuse");
    let output = harness::unique_output_path("shared_try_write_timeout_full");
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
fn read_guard_across_await_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("async")
        .join("read_guard_across_await.fuse");
    let output = harness::unique_output_path("read_guard_across_await_full");
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
fn simd_sum_float_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("simd")
        .join("simd_sum_float.fuse");
    let output = harness::unique_output_path("simd_sum_float_full");
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
fn simd_sum_empty_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("simd")
        .join("simd_sum_empty.fuse");
    let output = harness::unique_output_path("simd_sum_empty_full");
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
fn simd_sum_large_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("simd")
        .join("simd_sum_large.fuse");
    let output = harness::unique_output_path("simd_sum_large_full");
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
fn simd_invalid_type_fixture_fails_to_compile() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("simd")
        .join("simd_invalid_type.fuse");
    let output = harness::unique_output_path("simd_invalid_type_full");
    let compile = harness::compile_fixture(&fixture, &output);
    assert!(
        !compile.status.success(),
        "expected compile failure for {}",
        fixture.display()
    );
    let stderr = String::from_utf8_lossy(&compile.stderr);
    assert!(
        stderr.contains("unsupported SIMD element type"),
        "{}: expected SIMD type error in stderr, got:\n{stderr}",
        fixture.display()
    );
}

#[test]
fn simd_invalid_lane_fixture_fails_to_compile() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("simd")
        .join("simd_invalid_lane.fuse");
    let output = harness::unique_output_path("simd_invalid_lane_full");
    let compile = harness::compile_fixture(&fixture, &output);
    assert!(
        !compile.status.success(),
        "expected compile failure for {}",
        fixture.display()
    );
    let stderr = String::from_utf8_lossy(&compile.stderr);
    assert!(
        stderr.contains("unsupported SIMD lane count"),
        "{}: expected SIMD lane error in stderr, got:\n{stderr}",
        fixture.display()
    );
}

#[test]
fn shared_repeated_mutation_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_repeated_mutation.fuse");
    let output = harness::unique_output_path("shared_repeated_mutation_full");
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
fn chan_repeated_send_recv_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("chan_repeated_send_recv.fuse");
    let output = harness::unique_output_path("chan_repeated_send_recv_full");
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
fn simd_repeated_sum_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("simd")
        .join("simd_repeated_sum.fuse");
    let output = harness::unique_output_path("simd_repeated_sum_full");
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
fn stress_destructor_order_fixture_compiles_and_runs() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("stress_destructor_order.fuse");
    let output = harness::unique_output_path("stress_destructor_order_full");
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
    assert_eq!(count, 33, "unexpected full fixture count");
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
