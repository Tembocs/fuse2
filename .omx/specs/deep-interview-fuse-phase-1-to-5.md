# Deep Interview Spec — Fuse Phase 1 to 5

## Metadata
- Profile: standard
- Rounds: 6
- Final ambiguity: 14%
- Threshold: 20%
- Context type: brownfield bootstrap
- Context snapshot: .omx/context/fuse-phase-1-to-5-20260403T095950Z.md
- Transcript: latest file under .omx/interviews/ for slug use-phase-1-to-5

## Clarity breakdown
| Dimension | Score |
|---|---:|
| Intent | 75% |
| Outcome | 88% |
| Scope | 96% |
| Constraints | 82% |
| Success | 80% |
| Context | 92% |

## Intent
Build the missing Fuse repository implementation from the revised v2 docs so the project reaches a complete, working Stage 0 / Fuse Core state without drifting into Stage 1 or Fuse Full implementation.

## Desired Outcome
Deliver phases 1-5 from docs/fuse-implementation-plan-2.md in this repo from scratch: all required Phase 1 test artifacts exist, the Stage 0 lexer/parser/checker/evaluator exist and work for Fuse Core, the milestone/Core tests pass, and Phase 5 stabilization artifacts are finalized for Fuse Core only.

## In Scope
- Use docs/fuse-implementation-plan-2.md, docs/fuse-language-guide-2.md, and docs/fuse-repository-layout-2.md as the authoritative basis.
- Create the missing repository structure needed for phases 1-5.
- Author all Phase 1 .fuse test files with expected output/error blocks, including the textual 	ests/fuse/full/** files required by the plan.
- Implement Stage 0 / Fuse Core lexer, parser, ownership checker, evaluator, and the minimal supporting runtime/files needed to satisfy phases 2-4.
- Stabilize Fuse Core for Phase 5: finalize the guide/tests/ADRs needed by the done-when criteria.
- Preserve the canonical milestone behavior around 	ests/fuse/milestone/four_functions.fuse.
- Keep the implementation dependency-free beyond Python stdlib / existing repo expectations.

## Out of Scope / Non-goals
- Stage 1 Rust compiler work and all later phases.
- Fuse Full feature implementation/execution (spawn, channels, Shared, async/await, SIMD).
- REPL, extra examples, performance optimization, nonessential stdlib surface, and docs cleanup beyond what Phase 5 requires.
- Any work not required to satisfy the Phase 1-5 done-when criteria.

## Decision Boundaries
OMX may make local implementation decisions without further confirmation when the docs have minor gaps or contradictions, provided that it:
- stays within Fuse Core,
- preserves milestone/Core test intent and behavior,
- documents material choices in ADRs or nearby notes,
- adds no new dependencies.

## Constraints
- Revised v2 docs are the source of truth.
- Current repo appears to lack the planned implementation tree; work must bootstrap from scratch.
- Fuse Full implementation remains deferred even though Phase 1 must include textual Fuse Full test artifacts.
- No new dependencies.

## Testable acceptance criteria
1. The repo contains the Phase 1 test tree required by the revised plan, and every listed file has an expected output or expected error block.
2. Stage 0 source files exist for lexer, parser, AST, checker, evaluator, and supporting runtime as needed for Fuse Core.
3. Core programs parse without error per Phase 2 done-when.
4. Invalid ownership/core error programs are rejected with correct errors per Phase 3 done-when.
5. The canonical milestone program 	ests/fuse/milestone/four_functions.fuse runs correctly per Phase 4 done-when.
6. Fuse Core is frozen for Phase 5: relevant guide/tests/ADRs are final for this scope.
7. No Stage 1/Fuse Full implementation work is introduced.

## Assumptions exposed + resolutions
- Assumption: "Implement phases 1-5" might mean continuing into Stage 1. Resolution: stop after complete Stage 0/Core delivery only.
- Assumption: "Fuse Full out of scope" might remove all 	ests/fuse/full/** work. Resolution: still write those Phase 1 textual test artifacts, but do not implement/execute Fuse Full.
- Assumption: docs ambiguity might require repeated user confirmation. Resolution: OMX may decide locally within the approved boundary constraints.

## Pressure-pass findings
The earlier non-goal "Fuse Full is out of scope" was revisited against the plan's explicit Phase 1 deliverables. The refined boundary is: Fuse Full implementation is out of scope, but Fuse Full test files are still in scope as documentation/test-contract artifacts.

## Brownfield evidence vs inference
### Evidence
- docs/fuse-implementation-plan-2.md defines phases 1-5 and their deliverables.
- docs/fuse-language-guide-2.md is authoritative.
- docs/fuse-repository-layout-2.md describes intended directories and Stage 0 files.
- Current workspace inspection showed docs and .omx, but no 	ests/ or stage0/ implementation tree.
### Inference
- The repo must be bootstrapped from docs into a working Stage 0/Core structure.

## Technical context findings
- Brownfield bootstrap with sparse codebase and rich documentation.
- Recommended next lane: $ralplan to convert this clarified spec into PRD + test-spec planning artifacts before implementation.

## Condensed transcript
See .omx/interviews/ transcript for the round-by-round Q&A.
