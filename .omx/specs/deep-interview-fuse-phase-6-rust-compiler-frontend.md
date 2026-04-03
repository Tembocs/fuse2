# Deep Interview Spec — Fuse Phase 6 Rust Compiler Frontend

## Metadata
- Profile: standard
- Rounds: 4
- Final ambiguity: 11%
- Threshold: 20%
- Context type: brownfield
- Context snapshot: `.omx/context/fuse-phase-6-rust-compiler-frontend-20260403T105433Z.md`
- Transcript: `.omx/interviews/fuse-phase-6-rust-compiler-frontend-20260403T105433Z.md`

## Clarity breakdown
| Dimension | Score |
|---|---:|
| Intent | 76% |
| Outcome | 90% |
| Scope | 98% |
| Constraints | 86% |
| Success | 92% |
| Context | 95% |

## Intent
Advance the repository from completed Stage 0 / Fuse Core work into the documented Stage 1 frontend by reproducing the Core lexer, parser, AST/HIR pipeline, and semantic checking in Rust without drifting into backend/codegen or Fuse Full feature implementation.

## Desired Outcome
Create `stage1/` as the Rust compiler frontend described by `docs/fuse-implementation-plan-2.md`, with a `fusec` CLI that supports `--check`, parses and checks Fuse Core programs correctly, includes the documented HIR layer, and accepts/rejects the relevant test inputs with production-grade source-aware diagnostics.

## In Scope
- Create the Rust Stage 1 workspace and `stage1/fusec` compiler crate.
- Implement Rust lexer, parser, AST, HIR, and checker for Fuse Core semantics.
- Mirror Stage 0 Core checking behavior closely enough to preserve language outcome fidelity.
- Add a `fusec -- --check <file.fuse>` path as the primary completion interface.
- Update checked-in test expectations when the final Rust diagnostics intentionally differ from current Stage 0-oriented wording/format.
- Use HIR in Phase 6 because it is a documented architectural deliverable.

## Out-of-Scope / Non-goals
- Phase 7 backend/codegen work.
- Phase 8 self-hosting work.
- Fuse Full semantic enforcement in Phase 6, including `@rank`, `spawn` capture checks, async warnings, channels, shared state, async/await execution, or SIMD implementation.
- Any dependency growth beyond normal Rust/Cargo requirements for the Stage 1 crate(s).

## Decision Boundaries
OMX may decide without further confirmation:
- exact `stage1/` crate/module split,
- AST and HIR Rust representations,
- checker organization inside Phase 6 scope,
- final diagnostic wording and formatting,

provided that it:
- stays within Core-checking parity,
- preserves the intended Phase 6 outcome,
- includes a real HIR deliverable,
- updates repository test expectations to the emitted diagnostic contract,
- does not pull in extra nonstandard dependencies.

## Constraints
- `docs/fuse-implementation-plan-2.md` is the phase contract.
- `docs/fuse-repository-layout-2.md` is the intended structural map.
- Existing Stage 0 Python sources are the main semantic reference implementation.
- Existing `tests/fuse/core/**` files are the main observable behavior contract for acceptance/rejection.
- Current workspace has no `stage1/` tree yet, so Phase 6 is a brownfield extension with a new Stage 1 bootstrap.

## Testable acceptance criteria
1. `stage1/` exists with a Rust workspace and a `fusec` compiler crate.
2. `stage1/fusec/src/{lexer,parser,ast,hir,checker}` exist and are wired into a functioning `--check` pipeline.
3. The frontend handles Fuse Core inputs only and does not claim Phase 7/8 semantics.
4. `cargo run --bin fusec -- --check <file.fuse>` accepts valid Core programs and rejects invalid Core programs.
5. HIR exists as a concrete Phase 6 deliverable rather than being silently deferred.
6. Diagnostics are source-aware and repository expectations are updated to match the final emitted Rust contract, avoiding false positives from stale Stage 0 strings.
7. No backend/codegen or Full-facing checker functionality is introduced as part of Phase 6 completion.

## Assumptions exposed + resolutions
- Assumption: The Phase 6 deliverables might force Full-facing checker modules now.
  - Resolution: No. Phase 6 stops at Core-checking parity only; Full-facing checks move to Phases 7-8.
- Assumption: HIR could be postponed if `--check` worked without it.
  - Resolution: No. HIR should be created in Phase 6 because the docs rely on it and it may otherwise be forgotten.
- Assumption: Existing error comments must remain verbatim.
  - Resolution: No. Richer Rust diagnostics are acceptable, but checked-in expectations must be updated to the final diagnostic shape.

## Pressure-pass findings
The earlier Core-only scope decision was revisited with a deeper architectural tradeoff: whether HIR could be skipped temporarily. That follow-up changed the implementation boundary from “minimum external parity” to “external parity plus the documented HIR layer,” tightening the Phase 6 deliverable set.

## Brownfield evidence vs inference
### Evidence
- `docs/fuse-implementation-plan-2.md` defines Phase 6 as the Rust compiler frontend and explicitly lists `ast/`, `hir/`, and `checker/`.
- `docs/fuse-repository-layout-2.md` repeats the intended `stage1/fusec/src/` structure.
- `stage0/src/{lexer.py,parser.py,fuse_ast.py,checker.py}` already exist and provide a semantic baseline.
- `tests/fuse/core/**` and `tests/fuse/full/**` already exist in the repo.
- `stage1/` does not currently exist.

### Inference
- The most reliable path is to bootstrap Stage 1 from the Stage 0 semantics and the existing test corpus rather than invent a fresh contract.

## Technical context findings
- Brownfield repository with completed Stage 0 artifacts and no Stage 1 implementation yet.
- The main unresolved implementation work is architectural and translational, not requirements discovery.
- Recommended next lane: `$ralplan` to produce a Phase 6 PRD + test-spec before execution.

## Condensed transcript
- Q1: Core-only parity or Full-facing checker scope now?
  - A: Core-only for Phase 6.
- Q2: May OMX choose internal Rust architecture and diagnostic shape?
  - A: Yes, if it preserves the outcome.
- Q3: Is HIR mandatory now, even if a thinner checker could work?
  - A: Yes, create it now.
- Q4: Must diagnostics match existing comments verbatim?
  - A: No; richer diagnostics are fine if test files are updated to match.
