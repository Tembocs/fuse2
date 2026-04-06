# Context Snapshot — Fuse Phase 8 Hardening

- Task statement: Review `stage1/` deeply enough to replace the existing Phase 8 hardening draft with an execution-ready plan.
- Desired outcome: Concrete hardening PRD and verification plan grounded in the live `stage1/` compiler, runtime, stdlib, and test surface.
- Known facts/evidence:
  - Escalated `cargo test` in `stage1/` does not fully pass: `fusec/tests/compile_output_suite.rs` fails on `tests/fuse/core/control_flow/for_with_break.fuse`, returning `recv: 1` instead of `A\nB`.
  - `stage1/fusec/tests/check_full_suite.rs` passes and proves the current Full checker/warning contracts plus stdlib/full parseability/import resolution.
  - `stage1/fusec/tests/full_smoke_suite.rs` passes and proves compile/run for six Full output paths plus two synthetic smoke fixtures.
  - `spawn` is lowered inline in codegen, not to a real concurrent runtime.
  - `Shared<T>` currently stores a single raw `FuseHandle`; `read()` and `write()` both return that same handle.
  - `Chan<T>` is an in-memory queue with `items`, `pending`, and optional `capacity`; bounded behavior is queue promotion, not a true blocking primitive.
  - `SIMD.sum` is currently scalar-backed in runtime while frontend typing and stdlib surface imply a narrower `Int`-only contract.
  - `stdlib/full/{chan,shared,timer,simd,http}.fuse` are present and parseable, but several are still truthfulness stubs rather than deeply exercised runtime-backed modules.
- Constraints:
  - Hardening must stay pre-Phase-9 and must not drift into Stage 2 implementation.
  - The plan should prefer bounded, trust-increasing work over redesign.
  - The plan must start from the real repo state, not the earlier draft assumptions.
- Unknowns/open questions:
  - Whether `try_write` should be implemented in Stage 1 or explicitly demoted from the claimed executable contract.
  - Whether `timer/http` should remain parse-only surfaces or gain bounded executable proof before Phase 9.
- Likely codebase touchpoints:
  - `stage1/fusec/src/codegen/object_backend.rs`
  - `stage1/fusec/src/checker/mod.rs`
  - `stage1/fuse-runtime/src/value.rs`
  - `stage1/fusec/tests/{compile_output_suite.rs,check_full_suite.rs,full_smoke_suite.rs,harness.rs}`
  - `tests/fuse/full/**`
  - `stdlib/full/**`

