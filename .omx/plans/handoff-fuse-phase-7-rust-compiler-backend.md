# Handoff — Fuse Phase 7 Rust Compiler Backend

## Current State

This branch contains a substantial Phase 7 work-in-progress, but **Phase 7 is not complete yet**.

What is working now:

- `stage1/` is a multi-crate workspace with:
  - `fusec`
  - `fuse-runtime`
  - `cranelift-ffi`
- `fusec --check <file.fuse>` still works through the verified frontend path.
- `fusec <file.fuse> -o <output>` now emits a real executable.
- The emitted executable runs embedded Fuse source through the new runner in `stage1/fusec/src/evaluator.rs`.
- Representative compile-run coverage is codified in `stage1/fusec/tests/compile_output_suite.rs`.
- The full valid Core output corpus passes through that compile-run integration test.
- `cranelift-ffi` has a callable smoke test in `stage1/cranelift-ffi/tests/smoke.rs`.

## Verified Evidence

The following commands passed during this session:

```powershell
cd D:\fuse\fuse2\stage1
cargo test
cargo test -p cranelift-ffi --test smoke
cargo test -p fusec --test compile_output_suite
```

The compile-run integration test now covers the full valid Core output corpus plus the milestone binary path.

## Critical Truth

The current compile path is **not** a real Cranelift backend yet.

What it does today:

- `fusec` generates a Rust launcher crate in `stage1/target/generated/...`
- that launcher depends on `fusec`
- the launcher executes embedded Fuse source through `fusec::run_embedded_source(...)`
- the actual runtime semantics are currently supplied by the interpreter-style evaluator in:
  - `stage1/fusec/src/evaluator.rs`

What is still missing:

- actual HIR-to-Cranelift lowering in `stage1/fusec/src/codegen/**`
- real backend-driven object emission and linking as the primary execution path
- honest completion against the approved Phase 7 PRD, which explicitly calls for a real Cranelift backend

## Resume Target

When returning, start here:

1. Read:
   - `.omx/plans/prd-fuse-phase-7-rust-compiler-backend.md`
   - `.omx/plans/test-spec-fuse-phase-7-rust-compiler-backend.md`
   - this handoff file
2. Inspect:
   - `stage1/fusec/src/main.rs`
   - `stage1/fusec/src/lib.rs`
   - `stage1/fusec/src/evaluator.rs`
   - `stage1/fusec/src/codegen/mod.rs`
   - `stage1/fusec/src/codegen/cranelift.rs`
   - `stage1/fusec/src/codegen/layout.rs`
3. Preserve the current green evidence:
   - keep `cargo test`
   - keep `cargo test -p cranelift-ffi --test smoke`
   - keep `cargo test -p fusec --test compile_output_suite`
4. Replace the embedded-evaluator compile path with a real Cranelift-backed path while keeping the same tests green.

## Next Concrete Task

The next honest milestone is:

- implement a real backend path in `stage1/fusec/src/codegen/**`
- route `fusec <file> -o <output>` through that backend instead of `run_embedded_source`
- keep the current compile-run suite as the regression harness

## Files Added Or Changed In This WIP

- `stage1/Cargo.toml`
- `stage1/Cargo.lock`
- `stage1/fusec/Cargo.toml`
- `stage1/fusec/src/lib.rs`
- `stage1/fusec/src/main.rs`
- `stage1/fusec/src/evaluator.rs`
- `stage1/fusec/src/codegen/**`
- `stage1/fusec/tests/compile_output_suite.rs`
- `stage1/fuse-runtime/**`
- `stage1/cranelift-ffi/**`
- Phase 7 planning artifacts under `.omx/{context,interviews,plans,specs}/`

## Do Not Forget

- Do **not** claim Phase 7 complete while `fusec` still relies on `run_embedded_source(...)` for runtime behavior.
- The current state is a verified execution scaffold and regression harness, not the final backend implementation.
