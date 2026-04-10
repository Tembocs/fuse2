//! B0.3 — Determinism Regression Harness
//!
//! Compiles a multi-module fixture ten times in a row with `--emit ir`
//! and asserts every IR output is byte-for-byte identical.
//!
//! Why this exists:
//!   `BuildSession.modules` in `stage1/fusec/src/codegen/object_backend.rs`
//!   is a `HashMap<PathBuf, LoadedModule>` whose iteration order is
//!   randomized per process. The codegen iterates this map when emitting
//!   functions, which means the same source compiled twice in different
//!   processes can produce different function IDs and different module
//!   ordering in the output. The investigation in
//!   `docs/t4-parity-investigation.md` documents this as Issue 6.
//!
//! Status:
//!   - This test is currently `#[ignore]`d because the bug is real and
//!     the test fails reproducibly on the current commit.
//!   - Phase B1.1 of `docs/fuse-stage2-parity-plan.md` replaces the
//!     HashMap with a BTreeMap. After that lands, Phase B1.2 unignores
//!     this test. The expectation is 10/10 identical builds.
//!
//! Run:
//!   cargo test --test determinism_suite -- --ignored --nocapture

use std::process::Command;

mod harness;

const TRIALS: usize = 10;

#[test]
#[ignore = "B0.3: enabled by Phase B1.2 after BuildSession.modules: HashMap -> BTreeMap"]
fn ir_emission_is_deterministic_for_multi_module_fixture() {
    let root = harness::repo_root();
    let fixture = root
        .join("tests")
        .join("fuse")
        .join("core")
        .join("modules")
        .join("import_multiple.fuse");
    assert!(
        fixture.exists(),
        "fixture missing: {} — pick a different multi-module fixture",
        fixture.display()
    );

    let fusec = root
        .join("stage1")
        .join("target")
        .join("debug")
        .join(format!("fusec{}", std::env::consts::EXE_SUFFIX));
    assert!(
        fusec.exists(),
        "fusec debug binary missing: {} — run `cargo build -p fusec` first",
        fusec.display()
    );

    let mut outputs: Vec<String> = Vec::with_capacity(TRIALS);
    for trial in 0..TRIALS {
        let result = Command::new(&fusec)
            .arg("--emit")
            .arg("ir")
            .arg(&fixture)
            .output()
            .unwrap_or_else(|e| panic!("trial {trial}: failed to spawn fusec: {e}"));

        assert!(
            result.status.success(),
            "trial {trial}: fusec --emit ir exited non-zero\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );

        outputs.push(String::from_utf8_lossy(&result.stdout).into_owned());
    }

    let baseline = &outputs[0];
    let mut mismatches: Vec<(usize, &String)> = Vec::new();
    for (i, out) in outputs.iter().enumerate().skip(1) {
        if out != baseline {
            mismatches.push((i, out));
        }
    }

    if !mismatches.is_empty() {
        let mut report = format!(
            "Determinism violation: {}/{} runs differ from trial 0.\n\
             Fixture: {}\n\
             Compiler: {}\n\n",
            mismatches.len(),
            TRIALS - 1,
            fixture.display(),
            fusec.display(),
        );
        report.push_str("Trial 0 IR output (baseline) — first 40 lines:\n");
        for line in baseline.lines().take(40) {
            report.push_str("  ");
            report.push_str(line);
            report.push('\n');
        }
        for (i, _) in &mismatches {
            report.push_str(&format!("\nTrial {i} IR output — first 40 lines:\n"));
            for line in outputs[*i].lines().take(40) {
                report.push_str("  ");
                report.push_str(line);
                report.push('\n');
            }
        }
        report.push_str(
            "\nThis test guards Phase B1 of docs/fuse-stage2-parity-plan.md.\n\
             Root cause: BuildSession.modules HashMap iteration order.\n",
        );
        panic!("{report}");
    }

    // Sanity: produce some visible output when the test passes under
    // --ignored --nocapture so the engineer running B1.2 can see the
    // unignore landed cleanly.
    eprintln!(
        "determinism: {} trials of `fusec --emit ir {}` all byte-identical ({} bytes each)",
        TRIALS,
        fixture.file_name().unwrap().to_string_lossy(),
        baseline.len()
    );
}
