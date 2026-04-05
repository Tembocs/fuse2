# Context Snapshot — Fuse Phase 8 Planning

## Task statement

Clarify how to plan Phase 8 (Stage 1 Fuse Full), especially whether it should be split into smaller, sequential epics with good stop/resume points.

## Desired outcome

Produce a planning-ready requirements artifact for Phase 8 decomposition that is small enough to avoid clogging, easy to debug, and suitable for sequential execution with explicit completion markers.

## Stated solution

Use deep-interview to decide the right planning granularity before moving into a planning workflow.

## Probable intent hypothesis

The user wants a Phase 8 structure that minimizes coordination debt and makes it safe to pause, resume, and verify work incrementally.

## Known facts / evidence

- `docs/fuse-implementation-plan-2.md` defines Phase 8 as Stage 1 Fuse Full with concurrency, async, SIMD, runtime work, stdlib work, and checker wiring.
- `docs/fuse-language-guide-2.md` says Phase 8 deliverables are `chan.rs`, `shared.rs`, `async_rt.rs`, SIMD intrinsics, and rank/spawn/async checker passes wired to runtime.
- `docs/fuse-repository-layout-2.md` documents a concrete `tests/fuse/full/` split:
  - `tests/fuse/full/concurrency/`
  - `tests/fuse/full/async/`
  - `tests/fuse/full/simd/`
- The current repo already contains these Full tests:
  - concurrency: `chan_basic`, `chan_bounded_backpressure`, `shared_rank_ascending`, `shared_rank_violation`, `shared_no_rank`, `spawn_mutref_rejected`
  - async: `await_basic`, `suspend_fn`, `write_guard_across_await`
  - simd: `simd_sum`
- The user prefers more, smaller epics rather than coarse buckets and is explicitly open to going beyond 9 if it reduces roadblocks.

## Constraints

- Stay in deep-interview mode; do not jump directly into implementation.
- The planning split should preserve the documented Phase 8 boundary rather than shrink it.
- The decomposition should support sequential dependency order and explicit stop/resume points.

## Unknowns / open questions

- Whether the user wants Phase 8 split by feature family, by runtime/checker/test surface, or by the smallest practical debug slices.
- Whether they want a lean epic list or a deliberately more granular one even at the cost of extra bookkeeping.

## Decision-boundary unknowns

- How small the planning units should be before the bookkeeping overhead outweighs the debugging benefit.
- Whether the final planning artifact should stop at epics only or already include per-epic done-when/tests/files.

## Likely codebase touchpoints

- `docs/fuse-implementation-plan-2.md`
- `docs/fuse-language-guide-2.md`
- `docs/fuse-repository-layout-2.md`
- `tests/fuse/full/**`
- `stage1/fuse-runtime/src/`
- `stage1/fusec/src/checker/`
- `stdlib/full/`
