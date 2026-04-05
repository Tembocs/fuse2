use std::process::Command;

mod harness;

fn fusec() -> Command {
    Command::new(
        harness::repo_root()
            .join("stage1")
            .join("target")
            .join("debug")
            .join(format!("fusec{}", std::env::consts::EXE_SUFFIX)),
    )
}

// ---------------------------------------------------------------------------
// Meta commands
// ---------------------------------------------------------------------------

#[test]
fn help_flag_prints_to_stdout_and_exits_0() {
    let out = fusec().arg("--help").output().expect("run fusec");
    assert!(out.status.success(), "expected exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("fusec — the Fuse compiler"), "help header missing");
    assert!(stdout.contains("USAGE:"), "USAGE section missing");
    assert!(stdout.contains("EXAMPLES:"), "EXAMPLES section missing");
    assert!(out.stderr.is_empty(), "stderr should be empty for --help");
}

#[test]
fn help_short_flag_is_alias() {
    let out = fusec().arg("-h").output().expect("run fusec");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("fusec — the Fuse compiler"));
}

#[test]
fn version_flag_prints_to_stdout_and_exits_0() {
    let out = fusec().arg("--version").output().expect("run fusec");
    assert!(out.status.success(), "expected exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("fusec "), "should start with 'fusec '");
    assert!(out.stderr.is_empty(), "stderr should be empty for --version");
}

#[test]
fn version_short_flag_is_alias() {
    let out = fusec().arg("-V").output().expect("run fusec");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("fusec "));
}

// ---------------------------------------------------------------------------
// Usage errors (exit 2)
// ---------------------------------------------------------------------------

#[test]
fn no_args_exits_2() {
    let out = fusec().output().expect("run fusec");
    assert_eq!(out.status.code(), Some(2), "expected exit 2");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error:"), "error message missing");
    assert!(stderr.contains("--help"), "usage hint missing");
}

#[test]
fn unknown_flag_exits_2() {
    let out = fusec().arg("--foo").output().expect("run fusec");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unexpected argument `--foo`"));
}

#[test]
fn check_without_file_exits_2() {
    let out = fusec().arg("--check").output().expect("run fusec");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("requires a file argument"));
}

#[test]
fn emit_without_stage_exits_2() {
    let out = fusec().arg("--emit").output().expect("run fusec");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("missing emit stage"));
}

#[test]
fn emit_bad_stage_exits_2() {
    let out = fusec()
        .args(["--emit", "llvm", "foo.fuse"])
        .output()
        .expect("run fusec");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown emit stage `llvm`"));
}

#[test]
fn file_without_output_exits_2() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("milestone")
        .join("four_functions.fuse");
    let out = fusec().arg(&fixture).output().expect("run fusec");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("missing output path"));
}

#[test]
fn color_bad_value_exits_2() {
    let out = fusec()
        .args(["--check", "foo.fuse", "--color", "rainbow"])
        .output()
        .expect("run fusec");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid color mode"));
}

#[test]
fn repl_with_file_exits_2() {
    let out = fusec()
        .args(["--repl", "foo.fuse"])
        .output()
        .expect("run fusec");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("does not accept a file argument"));
}

// ---------------------------------------------------------------------------
// --check mode
// ---------------------------------------------------------------------------

#[test]
fn check_clean_file_exits_0_silent() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("milestone")
        .join("four_functions.fuse");
    let out = fusec().args(["--check"]).arg(&fixture).output().expect("run fusec");
    assert!(out.status.success(), "expected exit 0 for clean file");
    assert!(out.stdout.is_empty(), "stdout should be empty on success");
    assert!(out.stderr.is_empty(), "stderr should be empty on success");
}

#[test]
fn check_error_file_exits_1_with_diagnostics_on_stderr() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_no_rank.fuse");
    let out = fusec().args(["--check"]).arg(&fixture).output().expect("run fusec");
    assert_eq!(out.status.code(), Some(1), "expected exit 1 for error file");
    assert!(out.stdout.is_empty(), "diagnostics should go to stderr, not stdout");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Shared<T> requires @rank annotation"));
}

#[test]
fn check_short_format_produces_parseable_output() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_no_rank.fuse");
    let out = fusec()
        .args(["--check", "--error-format", "short"])
        .arg(&fixture)
        .output()
        .expect("run fusec");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Short format: file:line:col: severity: message
    assert!(
        stderr.contains("shared_no_rank.fuse:7:3: error:"),
        "short format should be file:line:col: error: ..., got:\n{stderr}"
    );
}

#[test]
fn check_warning_exits_0_without_deny() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("async")
        .join("write_guard_across_await.fuse");
    let out = fusec().args(["--check"]).arg(&fixture).output().expect("run fusec");
    assert!(out.status.success(), "warnings alone should exit 0");
}

#[test]
fn check_warning_exits_1_with_deny_warnings() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("async")
        .join("write_guard_across_await.fuse");
    let out = fusec()
        .args(["--check", "--deny-warnings"])
        .arg(&fixture)
        .output()
        .expect("run fusec");
    assert_eq!(
        out.status.code(),
        Some(1),
        "warnings with --deny-warnings should exit 1"
    );
}

// ---------------------------------------------------------------------------
// Compile mode
// ---------------------------------------------------------------------------

#[test]
fn compile_clean_file_produces_binary_and_exits_0() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("milestone")
        .join("four_functions.fuse");
    let output = harness::unique_output_path("cli_compile_test");
    let out = fusec()
        .arg(&fixture)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("run fusec");
    assert!(out.status.success(), "expected exit 0 for clean compile");
    assert!(out.stdout.is_empty(), "stdout should be empty on success");
    assert!(output.exists(), "binary should be produced");
}

#[test]
fn compile_error_file_exits_1_no_binary() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_no_rank.fuse");
    let output = harness::unique_output_path("cli_compile_error_test");
    let out = fusec()
        .arg(&fixture)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("run fusec");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Shared<T> requires @rank annotation"));
    assert!(!output.exists(), "no binary should be produced on error");
}

// ---------------------------------------------------------------------------
// --run mode
// ---------------------------------------------------------------------------

#[test]
fn run_hello_world_prints_to_stdout_and_exits_0() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("milestone")
        .join("four_functions.fuse");
    let out = fusec().args(["--run"]).arg(&fixture).output().expect("run fusec");
    assert!(out.status.success(), "expected exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.is_empty(),
        "program should produce output on stdout"
    );
    assert!(
        stdout.contains("Hello, Amara"),
        "expected milestone output, got:\n{stdout}"
    );
}

#[test]
fn run_error_file_exits_1_no_program_output() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_no_rank.fuse");
    let out = fusec().args(["--run"]).arg(&fixture).output().expect("run fusec");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "no program output when check fails");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Shared<T> requires @rank annotation"));
}

// ---------------------------------------------------------------------------
// --emit modes
// ---------------------------------------------------------------------------

#[test]
fn emit_tokens_produces_output_and_exits_0() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("milestone")
        .join("four_functions.fuse");
    let out = fusec()
        .args(["--emit", "tokens"])
        .arg(&fixture)
        .output()
        .expect("run fusec");
    assert!(out.status.success(), "expected exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Fn"), "should contain Fn token");
    assert!(stdout.contains("Identifier"), "should contain Identifier tokens");
}

#[test]
fn emit_ast_produces_output_and_exits_0() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("milestone")
        .join("four_functions.fuse");
    let out = fusec()
        .args(["--emit", "ast"])
        .arg(&fixture)
        .output()
        .expect("run fusec");
    assert!(out.status.success(), "expected exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Module"), "should contain Module node");
    assert!(stdout.contains("FnDecl"), "should contain FnDecl nodes");
}

#[test]
fn emit_hir_produces_output_and_exits_0() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("milestone")
        .join("four_functions.fuse");
    let out = fusec()
        .args(["--emit", "hir"])
        .arg(&fixture)
        .output()
        .expect("run fusec");
    assert!(out.status.success(), "expected exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Module"), "should contain Module node");
    assert!(stdout.contains("FnDecl"), "should contain FnDecl nodes");
}

#[test]
fn emit_ir_produces_output_and_exits_0() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("milestone")
        .join("four_functions.fuse");
    let out = fusec()
        .args(["--emit", "ir"])
        .arg(&fixture)
        .output()
        .expect("run fusec");
    assert!(out.status.success(), "expected exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("function"), "IR should contain function definitions");
    assert!(stdout.contains("block"), "IR should contain block labels");
}

// ---------------------------------------------------------------------------
// --repl
// ---------------------------------------------------------------------------

#[test]
fn repl_evaluates_expression_and_prints_result() {
    let out = fusec()
        .arg("--repl")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(b"2 + 2\nexit\n").ok();
            }
            child.wait_with_output()
        })
        .expect("run fusec --repl");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("4"), "REPL should print expression result 4, got: {stdout}");
}

#[test]
fn repl_val_declaration_is_silent() {
    let out = fusec()
        .arg("--repl")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(b"val x = 42\nexit\n").ok();
            }
            child.wait_with_output()
        })
        .expect("run fusec --repl");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Val declaration should only produce prompt output, not "42"
    assert!(!stdout.contains("42"), "val decl should be silent, got: {stdout}");
}

#[test]
fn repl_error_does_not_crash_session() {
    let out = fusec()
        .arg("--repl")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(b"bad!!!\n3 + 3\nexit\n").ok();
            }
            child.wait_with_output()
        })
        .expect("run fusec --repl");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("6"), "REPL should recover and eval 3+3=6, got: {stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error"), "error should be reported on stderr");
}

#[test]
fn repl_persists_state_across_lines() {
    let out = fusec()
        .arg("--repl")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(b"val x = 10\nprintln(x)\nexit\n").ok();
            }
            child.wait_with_output()
        })
        .expect("run fusec --repl");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("10"), "REPL should persist val x across lines, got: {stdout}");
}

// ---------------------------------------------------------------------------
// --color never strips ANSI codes
// ---------------------------------------------------------------------------

#[test]
fn color_never_strips_ansi_codes() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_no_rank.fuse");
    let out = fusec()
        .args(["--check", "--color", "never"])
        .arg(&fixture)
        .output()
        .expect("run fusec");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("\x1b["),
        "should not contain ANSI escape codes with --color never"
    );
}

#[test]
fn color_always_includes_ansi_codes() {
    let fixture = harness::repo_root()
        .join("tests")
        .join("fuse")
        .join("full")
        .join("concurrency")
        .join("shared_no_rank.fuse");
    let out = fusec()
        .args(["--check", "--color", "always"])
        .arg(&fixture)
        .output()
        .expect("run fusec");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("\x1b["),
        "should contain ANSI escape codes with --color always"
    );
}
