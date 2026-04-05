# Deep Interview Transcript — Fuse Phase 8 Planning

## Summary

This interview clarified how Phase 8 should be planned before execution. The user wants a Phase 8 planning artifact that functions as a live stop/resume ledger, not just a high-level epic list. Planning units should be small enough to serve as single-session checkpoints, OMX may choose the exact number of planning units freely, and the plan must explicitly exclude Phase 9/self-hosting work.

## Brownfield evidence gathered before questioning

- `docs/fuse-implementation-plan-2.md` defines Phase 8 as Stage 1 Fuse Full, covering concurrency, async, SIMD, runtime work, stdlib work, and checker wiring.
- `docs/fuse-language-guide-2.md` states the Phase 8 deliverables explicitly: `chan.rs`, `shared.rs`, `async_rt.rs`, SIMD intrinsics, and rank/spawn/async-lint checker passes wired to runtime.
- `docs/fuse-repository-layout-2.md` documents the expected Full-test split under `tests/fuse/full/`.
- The repo already contains concrete Full tests in:
  - `tests/fuse/full/concurrency/`
  - `tests/fuse/full/async/`
  - `tests/fuse/full/simd/`
- The current Full fixtures include both runtime-output tests and compile-error/warning tests, which makes coarse “feature-only” planning more likely to clog.

## Transcript

### Round 1

- Target: Outcome
- Question: Should the Phase 8 plan stop at a sequential epic list only, or should each unit already include dependencies, likely files, tests, and done-when criteria so it doubles as stop/resume documentation?
- Answer: Include those for stop/resume tracking documentation.
- Effect: The planning artifact must be execution-adjacent and resumable, not just descriptive.

### Round 2

- Target: Scope
- Challenge mode: Simplifier
- Question: Should each planning unit be small enough to act as a single-session checkpoint, or can some span multiple sessions if they still have stop/resume fields?
- Answer: Single-session checkpoint is safest for stop/resume.
- Effect: The planning granularity must be smaller than normal “epic” buckets if needed.

### Round 3

- Target: Decision Boundaries
- Challenge mode: Contrarian
- Question: May OMX choose the exact number and split of Phase 8 planning units freely, as long as they stay small, dependency-ordered, and stop/resume-friendly?
- Answer: Let OMX choose freely.
- Effect: The planning workflow does not need pre-approval on the final unit count.

### Round 4

- Target: Non-goals
- Question: Should the Phase 8 planning document explicitly exclude Phase 9/self-hosting work?
- Answer: Yes, exclude all Phase 9 work.
- Effect: The boundary is explicit: Phase 8 planning remains inside Stage 1 Fuse Full only.
