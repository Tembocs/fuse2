# Deep Interview Spec — Fuse Phase 8 Planning

## Metadata

- Profile: standard
- Rounds: 4
- Final ambiguity: 0.07
- Threshold: 0.20
- Context type: brownfield
- Context snapshot: `.omx/context/fuse-phase-8-planning-20260404T082022Z.md`

## Clarity Breakdown

| Dimension | Score |
| --- | --- |
| Intent | 0.90 |
| Outcome | 0.95 |
| Scope | 0.96 |
| Constraints | 0.96 |
| Success Criteria | 0.94 |
| Context | 0.96 |

## Intent

Create a Phase 8 planning artifact that minimizes clogging, supports reliable stop/resume behavior, and lets execution proceed sequentially through small, dependency-ordered checkpoints.

## Desired Outcome

Produce a planning document for Phase 8 that:

- breaks Phase 8 into small, sequential planning units
- makes each unit a realistic single-session checkpoint
- includes for each unit:
  - dependencies
  - likely files touched
  - tests to run
  - done-when criteria
  - status fields suitable for stop/resume tracking
- stays strictly within the documented Phase 8 / Stage 1 Fuse Full boundary

## In Scope

- Phase 8 decomposition for Stage 1 Fuse Full
- Planning granularity decisions
- Dependency ordering between planning units
- Resumable planning-document structure
- Full-test-driven planning that respects:
  - `tests/fuse/full/concurrency/**`
  - `tests/fuse/full/async/**`
  - `tests/fuse/full/simd/**`
- Planning coverage for:
  - `spawn`
  - `Chan<T>`
  - `Shared<T>`
  - `@rank`
  - async runtime / `await` / `suspend`
  - SIMD
  - remaining Full stdlib work required by the docs/tests

## Out-of-Scope / Non-goals

- Phase 9 / self-hosting work
- Stage 2 planning
- Performance tuning unless a Phase 8 test/milestone directly requires it
- Unrelated cleanup outside what Phase 8 delivery requires
- Reopening whether Phase 8 should exist as a separate phase

## Decision Boundaries

OMX may decide without further confirmation:

- the exact number of Phase 8 planning units
- the exact split between runtime/checker/stdlib/test-driven planning units
- the dependency order, provided it remains sequential and stop/resume-friendly
- whether the final count lands above 9 if that better preserves single-session checkpoints

OMX may not decide without further confirmation:

- to merge planning units into larger multi-session chunks for convenience
- to include Phase 9/self-hosting work in the Phase 8 planning artifact
- to shrink the documented Phase 8 acceptance boundary below the Full-test milestone

## Constraints

- Each planning unit should be small enough to serve as a single-session checkpoint in normal use
- The planning artifact must be usable as stop/resume documentation, not just a conceptual outline
- The plan must remain faithful to the documented Phase 8 deliverables and Full-test surface
- The plan should reduce debugging/clogging risk, even if that means more units and more bookkeeping

## Testable Acceptance Criteria

1. The resulting Phase 8 plan is sequential and dependency-ordered.
2. Each planning unit includes dependencies, likely files touched, tests to run, done-when, and status.
3. Each planning unit is scoped as a realistic single-session checkpoint.
4. The plan explicitly excludes Phase 9/self-hosting work.
5. The plan is grounded in the actual Full-test surface already present in the repository.

## Assumptions Exposed + Resolutions

- Assumption: A simple epic list might be enough.
  - Resolution: No. The plan must also serve as the stop/resume ledger.
- Assumption: Normal multi-session epics might be acceptable.
  - Resolution: No. Single-session checkpoints are preferred as the safer boundary.
- Assumption: The exact count of planning units must be user-approved first.
  - Resolution: No. OMX may choose the count freely as long as the units stay small and sequential.
- Assumption: Phase 8 planning might absorb some self-hosting preparation.
  - Resolution: No. Exclude all Phase 9 work.

## Pressure-pass Findings

The key pressure pass was on granularity. The initial discussion could have settled around “epics with metadata,” but the follow-up clarified that the real requirement is single-session stop/resume checkpoints. That pushes the plan smaller than typical epic sizing and justifies a higher final unit count if needed.

## Brownfield Evidence vs Inference Notes

- Evidence:
  - `docs/fuse-implementation-plan-2.md` and `docs/fuse-language-guide-2.md` define the Phase 8 scope.
  - `docs/fuse-repository-layout-2.md` documents the Full-test structure.
  - The repo already contains concrete Full fixtures under `tests/fuse/full/{concurrency,async,simd}/`.
- Inference:
  - Because the test surface includes both runtime behavior and checker diagnostics, a planning split that separates runtime/checker/stdlib/test closure work will likely be more robust than broad feature-only buckets.

## Technical Context Findings

- Current Full-test areas already provide a natural anchor for planning:
  - concurrency
  - async
  - simd
- The docs also imply internal split points that may deserve separate planning units:
  - runtime implementation
  - checker/diagnostic wiring
  - stdlib exposure
  - final integration/closure

## Condensed Transcript

1. The plan should be a stop/resume ledger, not just an epic list.
2. Each planning unit should be a realistic single-session checkpoint.
3. OMX may choose the final unit count freely.
4. Phase 9/self-hosting is explicitly out of scope.

## Execution Bridge

### Recommended: `$ralplan`

- Input artifact: `.omx/specs/deep-interview-fuse-phase-8-planning.md`
- Invocation: `$plan --consensus --direct .omx/specs/deep-interview-fuse-phase-8-planning.md`
- Consumer behavior:
  - produce the concrete Phase 8 planning document
  - choose the unit count freely
  - keep each unit single-session in scope
  - include dependency/status/test/done-when fields for each unit
- Best when:
  - you want a planning artifact ready for sequential execution and pause/resume use

### Alternative: Refine further

- Best when:
  - you want to constrain the unit count further or prescribe a specific documentation template before planning starts

## Residual Risk

- Low residual ambiguity remains.
- The main remaining choice is planning craft, not requirements uncertainty.
