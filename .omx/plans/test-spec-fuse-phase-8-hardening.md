# Test Spec — Fuse Phase 8 Hardening

## Purpose

Define the verification evidence required to prove the hardening pass increased trust in the current Stage 1 compiler rather than merely reshuffling implementation details.

## Source of Truth

- [prd-fuse-phase-8-hardening.md](/D:/fuse/fuse2/.omx/plans/prd-fuse-phase-8-hardening.md)
- live `stage1/` code and tests

## Initial Baseline

At plan creation time:

- `cargo test` in `stage1` passes:
  - `check_core_suite`
  - `check_full_suite`
  - `full_smoke_suite`
  - `cranelift-ffi` smoke
- but fails overall in:
  - [compile_output_suite.rs](/D:/fuse/fuse2/stage1/fusec/tests/compile_output_suite.rs)
- with the observed mismatch:
  - `tests/fuse/core/control_flow/for_with_break.fuse`
  - expected `A\nB`
  - actual `recv: 1`

Hardening is not complete until that baseline is green.

## Verification Matrix

### 1. Baseline Integrity

- prove the generated wrapper/native compile path is isolated per fixture
- prove `cargo test` in `stage1` is green under escalated execution

Required evidence:

- `cargo test -p fusec --test compile_output_suite`
- `cargo test` in `stage1`

### 2. Full Verification Shape

- prove Full output fixtures are run through a repo-backed compile/run sweep
- keep checker-only and warning-only contracts separate and explicit

Required evidence:

- `cargo test -p fusec --test full_smoke_suite`
- `cargo test -p fusec --test check_full_suite`

### 3. Execution Model Freeze

- prove the chosen Stage 1 `spawn` execution model directly
- prove the existing `spawn` checker boundary remains intact
- prove `await` and `suspend` keep their current transparent executable behavior
- ensure hardening tests do not quietly assume true parallelism

Required evidence:

- repo-backed `spawn` sequencing fixture(s)
- `spawn_mutref_rejected`
- `await_basic`
- `suspend_fn`
- targeted Full compile/run proof

### 4. Shared Contract

- prove one coherent contract for:
  - constructor policy
  - `read()`
  - `write()`
  - optional `try_write`
  - rank interaction

Required evidence:

- existing:
  - `shared_no_rank`
  - `shared_rank_violation`
  - `shared_rank_ascending`
  - `write_guard_across_await`
- new:
  - repeated Shared mutation fixture
  - nested-data mutation visibility fixture
  - any `try_write` success/failure fixture if `try_write` remains executable

### 5. Channel Contract

- prove repeated send/recv behavior
- prove bounded promotion behavior
- prove empty-receive policy or explicit rejection policy

Required evidence:

- existing:
  - `chan_basic`
  - `chan_bounded_backpressure`
- new:
  - repeated cycle fixture
  - empty/edge fixture

### 6. Async Warning Contract

- prove the existing warning still fires where intended
- prove at least one nearby safe case does not warn

Required evidence:

- existing:
  - `write_guard_across_await`
- new:
  - safe no-warning fixture
  - sharper warning fixture if behavior is refined

### 7. SIMD Contract

- prove runtime and frontend agree on the chosen bounded SIMD contract
- cover at least one edge case beyond the current happy path

Required evidence:

- existing:
  - `simd_sum`
- new:
  - empty-list or tail-handling fixture
  - any diagnostic fixture required by the chosen contract

### 8. Stdlib Full Truthfulness

- prove each `stdlib/full/*` module is either:
  - executable and exercised
  - or intentionally stubbed with an explicit tested contract

Required evidence:

- parse/import resolution remains green
- any new stub-contract tests for `timer/http`
- execution proofs for `chan/shared/simd` if claimed executable

## Exit Criteria

Hardening is complete only when:

1. escalated `cargo test` in `stage1` passes
2. the baseline wrapper/output isolation issue is fixed and regression-tested
3. new repo-backed hardening fixtures for Shared, Chan, and SIMD are green
4. the Full stdlib surface is truthful
5. a written Phase 9 readiness verdict exists
