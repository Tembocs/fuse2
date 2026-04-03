# PRD — Fuse Phase 1 to 5 / Stage 0 Bootstrap

## Requirements Summary
Build the missing Fuse repository implementation from scratch in this repo using:
- `docs/fuse-implementation-plan-2.md` as the phase contract
- `docs/fuse-language-guide-2.md` as the language source of truth
- `docs/fuse-repository-layout-2.md` as the target repository structure
- `.omx/specs/deep-interview-fuse-phase-1-to-5.md` as the clarified execution brief

Delivery stops at a complete **Stage 0 / Fuse Core** implementation. Do **not** start Stage 1 Rust work.

### Source precedence
When the documents differ, prefer:
1. `docs/fuse-language-guide-2.md` for language semantics
2. `docs/fuse-implementation-plan-2.md` for phase deliverables and done-when criteria
3. `docs/fuse-repository-layout-2.md` for directory/file-shape intent
Any material reconciliation should be documented in an ADR or nearby note.

## RALPLAN-DR Summary
### Principles
1. Docs-first fidelity: implement only what the revised v2 docs define.
2. Tests define behavior before runtime polish.
3. Minimal complete Stage 0 beats partial multi-stage scaffolding.
4. Preserve clear boundaries: Fuse Core in, Fuse Full implementation out.
5. Prefer simple Python stdlib solutions over abstraction or dependency growth.

### Decision Drivers
1. The repo currently lacks `tests/` and `stage0/`; the work is a bootstrap from documentation.
2. `docs/fuse-implementation-plan-2.md` makes phases 1-5 sequential and test-driven.
3. The user explicitly scoped the delivery to Stage 0/Core only, while still requiring Phase 1 textual Fuse Full tests.

### Viable Options
#### Option A — Strict phased bootstrap (recommended)
Build the repo in plan order: Phase 1 tests -> Phase 2 parser -> Phase 3 checker -> Phase 4 evaluator -> Phase 5 stabilization.
- Pros: matches the implementation plan; keeps verification aligned with each phase; reduces rework.
- Cons: phase boundaries can slow early end-to-end feedback.

#### Option B — Thin vertical slice first, then backfill
Implement just enough lexer/parser/checker/evaluator to run `four_functions.fuse`, then backfill the full test suite and stabilization.
- Pros: faster milestone feedback.
- Cons: conflicts with the documented tests-first contract; higher risk of semantic drift and rework.

### Recommendation
Use **Option A**. The repo has no implementation to preserve, and the revised plan explicitly makes the test suite and phase sequencing the contract.

## Scope
### In Scope
- Create `tests/fuse/` tree and all Phase 1 test files listed in `docs/fuse-implementation-plan-2.md`.
- Include `tests/fuse/full/**` as text-only Phase 1 artifacts with expected output/error blocks.
- Create `stage0/src/` tree and implement `token.py`, `lexer.py`, `ast.py`, `parser.py`, `checker.py`, `environment.py`, `values.py`, `evaluator.py`, `main.py`, and any minimal helper/error modules required.
- Create `stage0/tests/run_tests.py` and any minimal snapshots/helpers needed by the agreed format.
- Update/finalize docs/ADRs required to freeze Fuse Core for Phase 5.

### Out of Scope
- Stage 1+ work.
- Fuse Full runtime/compiler implementation.
- REPL beyond a minimal placeholder or explicit defer note, extra examples, performance optimization, nonessential stdlib surface, unrelated docs cleanup.
- New dependencies.

## Acceptance Criteria
1. Every Phase 1 test file named in `docs/fuse-implementation-plan-2.md` exists under `tests/fuse/` with an `EXPECTED OUTPUT` or `EXPECTED ERROR` block.
2. `stage0/src/parser.py` can parse every Core test file without parse errors and print a human-readable AST.
3. `stage0/src/checker.py` rejects invalid Core programs with location-rich errors and accepts valid ones.
4. `stage0/src/main.py` can run `tests/fuse/milestone/four_functions.fuse` with output matching its expected block.
5. `stage0/tests/run_tests.py` executes the Core suite and exits successfully when all Core expectations match.
6. Phase 5 stabilization leaves Fuse Core explicitly marked stable in the guide and records any new decisions in ADRs.
7. No Stage 1/Fuse Full implementation files are introduced.

## Implementation Steps
1. **Bootstrap repo skeleton**
   - Create `tests/fuse/{milestone,core,full}` and `stage0/src`, `stage0/tests`, `docs/adr` as needed.
   - Add README/runner placeholders only where needed for verification.
2. **Author Phase 1 test contract**
   - Write `tests/fuse/milestone/four_functions.fuse` first.
   - Add all listed Core and Full `.fuse` files with expected blocks.
   - Prefer self-contained test intent in-file; avoid separate golden files unless needed for runner internals.
3. **Implement Phase 2 parsing stack**
   - Create tokens, lexer, AST dataclasses, parser, and parse CLI.
   - Verify parser across all Core tests.
4. **Implement Phase 3 checking stack**
   - Add ownership/immutability/import visibility/match exhaustiveness/basic type checks.
   - Emit stable error messages with file/line/column and hints.
5. **Implement Phase 4 evaluator/runtime**
   - Add runtime values, scope tracking, evaluator, import resolution, `?`, `?.`, `?:`, `defer`, loops, maps, basic extension dispatch, and milestone execution path.
   - Build `stage0/tests/run_tests.py` and validate the Core suite.
   - Treat REPL support as optional unless needed to keep the Stage 0 CLI contract coherent; a stub or defer note is acceptable if Phase 4 done-when is otherwise satisfied.
6. **Execute Phase 5 stabilization**
   - Patch `docs/fuse-language-guide-2.md` where implementation exposed ambiguities.
   - Add ADRs for decisions not already covered.
   - Mark Fuse Core stable for this scope.

## Risks and Mitigations
- **Spec volume risk:** `docs/fuse-language-guide-2.md` is large. Mitigation: implement only constructs exercised by the Phase 1 Core tests first, then fill gaps required by milestone behavior.
- **Bootstrap mismatch risk:** repo-layout doc is aspirational. Mitigation: prefer implementation-plan deliverables when layout and current repo differ; document deviations.
- **Semantic drift risk:** ownership and ASAP destruction are subtle. Mitigation: encode them first in tests/checker/evaluator rather than ad hoc runtime behavior.
- **Phase 5 creep risk:** stabilization can expand endlessly. Mitigation: limit Phase 5 to ambiguities encountered during Phases 1-4 delivery.

## Verification Steps
1. File-existence audit for every required Phase 1 test and Stage 0 source file.
2. Parser pass over `tests/fuse/core/**`.
3. Checker validation on valid vs invalid Core tests.
4. Core test runner pass.
5. Direct milestone run for `tests/fuse/milestone/four_functions.fuse`.
6. Documentation/ADR review for every new semantic decision.

## ADR
- **Decision:** Build Stage 0 using a strict phase-ordered, tests-first bootstrap.
- **Drivers:** implementation plan sequencing; docs-first repo state; user-scoped Stage 0-only delivery.
- **Alternatives considered:** vertical-slice milestone-first bootstrap; partial scaffolding with deferred tests.
- **Why chosen:** lowest risk of spec drift and best match for the plan's done-when criteria.
- **Consequences:** slower first execution signal but cleaner verification and lower rework.
- **Follow-ups:** after successful Phase 5, Stage 1 planning can begin from a frozen Core contract.

## Available-Agent-Types Roster
- `executor` — main implementation lane
- `architect` — design review / semantic tradeoffs
- `critic` — plan and scope challenge
- `debugger` — parser/checker/evaluator failure diagnosis
- `test-engineer` — runner and acceptance coverage
- `verifier` — final claim validation
- `writer` — ADR / guide stabilization edits

## Follow-up Staffing Guidance
### Ralph path
- 1x `executor` (high): repo bootstrap + implementation
- 1x `test-engineer` (medium): test-runner and acceptance harness checks
- 1x `writer` (medium): Phase 5 docs/ADR cleanup near the end
- 1x `verifier` (high): final evidence pass

### Team path
- Lane A: `executor` — tests + parser
- Lane B: `executor` — checker + diagnostics
- Lane C: `executor` — evaluator/runtime + runner
- Lane D: `writer` / `verifier` — stabilization + final evidence
Keep shared-file ownership explicit around `stage0/src/parser.py`, `stage0/src/checker.py`, `stage0/src/evaluator.py`, and `docs/fuse-language-guide-2.md`.

## Launch Hints
- Ralph: `$ralph .omx/plans/prd-fuse-phase-1-to-5.md`
- Team: `$team .omx/plans/prd-fuse-phase-1-to-5.md`
- Direct execution should read `.omx/plans/test-spec-fuse-phase-1-to-5.md` alongside this PRD.

## Team Verification Path
1. Prove all required files exist.
2. Prove parser/checker/evaluator behaviors against the test-spec.
3. Prove milestone output exactness.
4. Prove Phase 5 docs/ADR updates correspond to actual implementation decisions.
5. Hand final evidence to a verifier/Ralph lane before claiming completion.

## Changelog
- Added explicit source-precedence guidance across the three v2 docs.
- Clarified that REPL support is not required for completion unless needed to preserve a coherent Stage 0 CLI contract.
