# Test Spec — Fuse Phase 8 / Stage 1 Fuse Full

## Purpose

Define the verification evidence required to prove Stage 1 Fuse Full is complete under the current Phase 8 contract.

## Sources of Truth

- `.omx/specs/deep-interview-fuse-phase-8-planning.md`
- `.omx/plans/prd-fuse-phase-8-stage1-full.md`
- `docs/fuse-implementation-plan-2.md:511-560`
- `docs/fuse-language-guide-2.md:1142-1305`
- `docs/fuse-language-guide-2.md:1606-1614`
- `docs/fuse-repository-layout-2.md:98-105`
- `docs/fuse-repository-layout-2.md:187-214`
- `tests/fuse/full/**`

## Contract Reconciliation

The current Full-test corpus contains:

- output behavior tests
- compile-error tests
- compile-warning tests

Phase 8 verification must preserve all three shapes. “All tests/fuse/full/ pass” therefore means:

- expected-output fixtures compile and run correctly
- expected-error fixtures are rejected with the expected diagnostics
- expected-warning fixtures surface the expected warning behavior

## Verification Matrix

### Harness / Execution Layer

- prove Stage 1 can run `tests/fuse/full/**` as a first-class suite
- keep Full output/error/warning handling distinct

### Concurrency

- `tests/fuse/full/concurrency/chan_basic.fuse`
- `tests/fuse/full/concurrency/chan_bounded_backpressure.fuse`
- `tests/fuse/full/concurrency/shared_rank_ascending.fuse`
- `tests/fuse/full/concurrency/shared_rank_violation.fuse`
- `tests/fuse/full/concurrency/shared_no_rank.fuse`
- `tests/fuse/full/concurrency/spawn_mutref_rejected.fuse`

Expected proof:

- channels execute correctly
- Shared positive path works
- rank-order and missing-rank diagnostics match the test contract
- spawn mutref capture rejection matches the test contract

### Async

- `tests/fuse/full/async/await_basic.fuse`
- `tests/fuse/full/async/suspend_fn.fuse`
- `tests/fuse/full/async/write_guard_across_await.fuse`

Expected proof:

- basic async execution works
- suspend behavior works where required by the test contract
- write-guard-across-await warning is surfaced correctly

### SIMD

- `tests/fuse/full/simd/simd_sum.fuse`

Expected proof:

- SIMD behavior required by the current fixture works
- fallback/scalar semantics do not change observable output

### Full Closure

- after the targeted Full suite passes, run the complete `tests/fuse/**` closure

Expected proof:

- Full support did not regress the existing Core/milestone behavior

## Unit-to-Test Mapping

1. Unit 1 — no runtime tests; document proof only
2. Unit 2 — harness smoke proof
3. Unit 3 — minimal spawn runtime smoke
4. Unit 4 — `spawn_mutref_rejected`
5. Unit 5 — `chan_basic`, `chan_bounded_backpressure`
6. Unit 6 — channel tests still green through stdlib surface
7. Unit 7 — positive Shared runtime smoke or `shared_rank_ascending` once available
8. Unit 8 — `shared_rank_ascending`, `shared_rank_violation`, `shared_no_rank`
9. Unit 9 — `await_basic`, `suspend_fn`
10. Unit 10 — `write_guard_across_await`
11. Unit 11 — `simd_sum`
12. Unit 12 — all `tests/fuse/full/**`, then full `tests/fuse/**`

## Evidence Checklist

- [ ] Full harness exists and classifies output/error/warning cases
- [ ] Concurrency Full fixtures pass
- [ ] Async Full fixtures pass
- [ ] SIMD Full fixture passes
- [ ] Full stdlib surface required by the Full fixtures is usable from Fuse code
- [ ] Complete `tests/fuse/full/**` passes
- [ ] Complete `tests/fuse/**` passes after Full work lands
- [ ] No Phase 9/self-hosting work is required to claim Phase 8 completion

## Exit Criteria

Phase 8 is complete only when every current Full fixture passes under Stage 1 and the complete `tests/fuse/**` suite remains green.
