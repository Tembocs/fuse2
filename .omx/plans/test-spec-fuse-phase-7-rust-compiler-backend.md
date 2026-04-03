# Test Spec — Fuse Phase 7 / Stage 1 Rust Compiler Backend

## Purpose

Define the evidence required to prove the Stage 1 Rust backend is complete for the full clarified Phase 7 scope.

## Sources of Truth

- `.omx/specs/deep-interview-fuse-phase-7-rust-compiler-backend.md`
- `.omx/plans/prd-fuse-phase-7-rust-compiler-backend.md`
- `docs/fuse-implementation-plan-2.md`
- `docs/fuse-language-guide-2.md`
- `stage0/tests/run_tests.py`
- `tests/fuse/core/**`
- `tests/fuse/milestone/four_functions.fuse`

## Contract Reconciliation

The existing Core corpus is bifurcated:

- `EXPECTED ERROR` fixtures validate checker/compile-time failure behavior
- `EXPECTED OUTPUT` fixtures validate successful execution behavior

Phase 7 verification must preserve that contract. The phrase “all Core tests compile and pass” is satisfied by full-corpus parity under the existing harness semantics, not by forcing known-error fixtures into native binaries.

## Verification Matrix

### Workspace Bootstrap

- Verify `stage1/` workspace includes `fusec`, `fuse-runtime`, and `cranelift-ffi`
- Verify each crate builds as part of the workspace

### CLI Surface

- Verify `fusec --check <file.fuse>` remains available for error-path validation
- Verify `fusec <file.fuse> -o <artifact>` or equivalent compile path exists for valid Core programs
- Verify compile mode emits a runnable native artifact on the local platform
- Verify successful compile-run cases exit `0` and error-path checks exit non-zero

### Core Error Corpus

- Run the Stage 1 harness against every Core fixture with `EXPECTED ERROR`
- Expected result: no binary emission is required; diagnostics match the checked-in contract or the intentional updated contract
- Minimum named checks:
  - `tests/fuse/core/errors/match_missing_arm.fuse`
  - `tests/fuse/core/modules/import_pub_only.fuse`
  - `tests/fuse/core/ownership/move_prevents_reuse.fuse`
  - `tests/fuse/core/types/val_immutable.fuse`
  - `tests/fuse/core/ownership/ref_read_only.fuse`

### Core Output Corpus

- Run the Stage 1 harness against every Core fixture with `EXPECTED OUTPUT`
- Expected result: each valid fixture compiles to a native binary, the binary runs successfully, and stdout matches the checked-in expected block byte-for-byte, including intentionally empty output
- Minimum named checks:
  - `tests/fuse/milestone/four_functions.fuse`
  - `tests/fuse/core/memory/asap_destruction.fuse`
  - `tests/fuse/core/memory/del_fires_at_last_use.fuse`
  - `tests/fuse/core/ownership/mutref_modifies_caller.fuse`
  - `tests/fuse/core/control_flow/while_nested_break.fuse`
  - `tests/fuse/core/modules/import_multiple.fuse`
  - `tests/fuse/core/types/extension_functions.fuse`

### Runtime Coverage

- Verify compiled output exercises the runtime helpers required by the valid Core corpus
- Specifically prove:
  - print/output path works
  - data/value handling works for current Core fixtures
  - ASAP destruction hooks run in the correct order
  - character-aware string operations do not use invalid byte indexing

### Codegen Coverage

- Verify codegen handles:
  - function calls and returns
  - ownership-sensitive argument passing
  - loops with `break` and `continue`
  - `match` lowering
  - `defer` cleanup blocks
  - extension function dispatch used by current Core fixtures

### `cranelift-ffi` Coverage

- Verify `stage1/cranelift-ffi` builds as part of the workspace
- Verify a smoke test calls the exported C-compatible API through a true caller boundary rather than direct internal Rust function calls
- Preferred proof: a separate integration test crate or harness that binds the exported ABI via `extern "C"` declarations and exercises the documented minimal wrapper surface
- Optional stronger proof: a minimal native C smoke caller, if the local toolchain is available
- Expected result: callable symbols exist for the documented minimal wrapper surface without depending on Stage 2 implementation

### Hazard Regressions

- `mutref_cells` writeback timing:
  - add a regression where explicit `return` occurs in a loop/body that mutates a `mutref` parameter
  - expected result: caller observes the written-back value
- `and` / `or` SSA corruption:
  - add a regression combining loop/control-flow pressure with short-circuit conditions
  - expected result: no stale SSA behavior or crash
- UTF-8 indexing:
  - add a regression with multi-byte string content
  - expected result: no byte-boundary panic or invalid indexing behavior
- Stack size:
  - verify the chosen linker/runtime path encodes the documented stack-size requirement or an equivalent platform-safe configuration for the local platform

## Evidence Checklist

- [ ] Stage 1 workspace includes all three crates
- [ ] Compile CLI exists in addition to `--check`
- [ ] All Core `EXPECTED ERROR` fixtures satisfy the error contract
- [ ] All Core `EXPECTED OUTPUT` fixtures compile, run, and match expected stdout
- [ ] `tests/fuse/milestone/four_functions.fuse` compiles and runs successfully
- [ ] `fuse-runtime` support is exercised by real compiled fixtures
- [ ] `cranelift-ffi` builds and passes a callable-surface smoke test
- [ ] Linker/stack-size handling is verified on the local platform path
- [ ] Hazard regressions exist for `mutref_cells`, SSA short-circuiting, UTF-8 indexing, and stack-size handling
- [ ] No Phase 8 or Stage 2 implementation work is required to claim completion

## Exit Criteria

Phase 7 is complete only when every PRD acceptance criterion is backed by concrete build/check/run evidence under the reconciled Core corpus contract.
