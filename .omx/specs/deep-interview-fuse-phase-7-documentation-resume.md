# Deep Interview Spec — Fuse Phase 7 Documentation Resume

## Metadata

- Profile: standard
- Rounds: 3
- Final ambiguity: 0.08
- Threshold: 0.20
- Context type: brownfield
- Context snapshot: `.omx/context/fuse-phase-7-documentation-resume-20260404T051621Z.md`
- Prior Phase 7 anchors:
  - `.omx/plans/handoff-fuse-phase-7-rust-compiler-backend.md`
  - `.omx/plans/prd-fuse-phase-7-rust-compiler-backend.md`
  - `.omx/plans/test-spec-fuse-phase-7-rust-compiler-backend.md`
  - `.omx/specs/deep-interview-fuse-phase-7-rust-compiler-backend.md`

## Clarity Breakdown

| Dimension | Score |
| --- | --- |
| Intent | 0.80 |
| Outcome | 0.95 |
| Scope | 0.95 |
| Constraints | 0.95 |
| Success Criteria | 0.97 |
| Context | 0.97 |

## Intent

Resume Phase 7 from the real stop point using the existing documentation trail, and avoid wasting further work on a compile scaffold that does not satisfy the documented Stage 1 backend boundary.

## Desired Outcome

Use the existing Phase 7 PRD and test spec as the active execution brief, with an explicit resume target:

- replace the current generated-launcher / embedded-evaluator compile path with a real Cranelift backend path
- route `fusec <file.fuse> -o <output>` through actual backend lowering and native emission
- verify completion against honest Phase 7 evidence rather than scaffold-era tests
- remove `run_embedded_source(...)` from the Phase 7 compile story entirely once the real backend works

## In Scope

- Treat `.omx/plans/handoff-fuse-phase-7-rust-compiler-backend.md` as the factual stop point
- Reuse `.omx/plans/prd-fuse-phase-7-rust-compiler-backend.md` and `.omx/plans/test-spec-fuse-phase-7-rust-compiler-backend.md` as the primary execution contract
- Replace the interpreter-backed compile path in:
  - `stage1/fusec/src/main.rs`
  - `stage1/fusec/src/lib.rs`
  - `stage1/fusec/src/evaluator.rs`
  - `stage1/fusec/src/codegen/**`
- Replace scaffold-era verification with backend-honest verification that satisfies the Phase 7 PRD/test-spec contract

## Out of Scope / Non-goals

- Revisiting whether Phase 7 should stop at a smaller milestone
- Preserving the current generated-launcher scaffold as a long-term compatibility target
- Claiming Phase 7 complete while runtime behavior still depends on `run_embedded_source(...)`
- Pulling Phase 8 Fuse Full work or Stage 2 implementation into this resume

## Decision Boundaries

OMX may decide without further confirmation:

- the concrete internal backend design and sequencing needed to replace the scaffold
- how to rewrite the existing compile-run verification path, provided the final proof is stronger and aligned with the Phase 7 PRD and test spec
- how to remove `run_embedded_source(...)` from the Phase 7 compile path once the real backend is functional

OMX may not decide without further confirmation:

- to keep the embedded evaluator path as part of the claimed Phase 7 completion boundary
- to narrow the documented Phase 7 acceptance criteria below the existing PRD/test-spec contract
- to expand into Phase 8 or Stage 2 implementation work

## Constraints

- The current branch still matches the old handoff: compile mode is a generated Rust launcher around `run_embedded_source(...)`
- The real completion contract remains the existing Phase 7 PRD and test spec
- The scaffold may be replaced; it is not a required invariant
- `run_embedded_source(...)` should be removed from the Phase 7 compile story entirely once the real backend path lands and is verified

## Testable Acceptance Criteria

1. The next execution lane starts from `.omx/plans/handoff-fuse-phase-7-rust-compiler-backend.md` and treats it as incomplete work, not as proof of completion.
2. `fusec <file.fuse> -o <output>` no longer works by generating a launcher that calls `run_embedded_source(...)`.
3. The backend path is rooted in real codegen under `stage1/fusec/src/codegen/**`.
4. Verification is rewritten as needed to prove the Phase 7 PRD and test-spec criteria honestly.
5. Once the real backend works, `run_embedded_source(...)` is no longer part of the claimed Phase 7 compile path.

## Assumptions Exposed + Resolutions

- Assumption: The missing piece might be documentation rather than implementation.
  - Resolution: No. The repo already contains the needed planning/handoff docs; the missing piece is the real backend implementation.
- Assumption: The current green scaffold tests might need to stay green throughout the transition.
  - Resolution: No. They may be replaced if that is what it takes to put execution on the correct backend path.
- Assumption: `run_embedded_source(...)` might remain as an acceptable helper after backend work lands.
  - Resolution: No. Remove it from the Phase 7 compile story entirely once the real backend works.

## Pressure-pass Findings

The crucial pressure pass revisited the transition strategy rather than the broad scope. The user explicitly chose correctness of direction over preserving existing scaffold-era green checks. That changed the execution posture from “swap carefully under the old tests” to “replace the old path if necessary so the final Phase 7 evidence is honest.”

## Brownfield Evidence vs Inference Notes

- Evidence:
  - `stage1/fusec/src/main.rs` currently generates a Cargo launcher crate that calls `fusec::run_embedded_source(...)`.
  - `stage1/fusec/src/lib.rs` still exports `run_embedded_source(...)`.
  - `stage1/fusec/src/codegen/cranelift.rs` currently implements a host thunk around an imported entry function rather than real lowering.
  - `.omx/plans/handoff-fuse-phase-7-rust-compiler-backend.md` explicitly says Phase 7 is not complete and names the real backend as the next task.
- Inference:
  - Rewriting `stage1/fusec/tests/compile_output_suite.rs` or replacing it with stronger backend-honest verification will likely be part of the execution lane because the current suite proves the scaffold path.

## Technical Context Findings

- Existing Phase 7 planning artifacts are sufficient and should be reused rather than recreated.
- The immediate execution focal points are:
  - `stage1/fusec/src/main.rs`
  - `stage1/fusec/src/lib.rs`
  - `stage1/fusec/src/evaluator.rs`
  - `stage1/fusec/src/codegen/mod.rs`
  - `stage1/fusec/src/codegen/cranelift.rs`
  - `stage1/fusec/src/codegen/layout.rs`
  - `stage1/fusec/tests/compile_output_suite.rs`

## Condensed Transcript

1. The next step should be execution handoff, not status recap only.
2. The scaffold and its current verification path may be replaced if that is what honest backend completion requires.
3. `run_embedded_source(...)` should leave the Phase 7 compile story entirely once the real Cranelift backend works.

## Execution Bridge

### Recommended: `$ralph`

- Input artifact: `.omx/plans/prd-fuse-phase-7-rust-compiler-backend.md`
- Supporting artifacts:
  - `.omx/plans/test-spec-fuse-phase-7-rust-compiler-backend.md`
  - `.omx/plans/handoff-fuse-phase-7-rust-compiler-backend.md`
  - `.omx/specs/deep-interview-fuse-phase-7-documentation-resume.md`
- Consumer behavior:
  - treat the handoff as incomplete
  - replace the scaffold with the real backend
  - preserve the Phase 7 acceptance boundary
  - remove `run_embedded_source(...)` from the claimed compile path once the backend works
- Best when:
  - one persistent owner should drive the backend replacement through verification

### Alternative: `$team`

- Same input/supporting artifacts
- Best when:
  - you want runtime/codegen/test lanes in parallel

### Residual Risk

- Low residual ambiguity remains about execution boundaries.
- The primary remaining risk is implementation complexity, not requirement clarity.
