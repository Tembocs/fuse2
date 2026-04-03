# Test Spec — Fuse Phase 1 to 5 / Stage 0 Bootstrap

## Purpose
Define the evidence required to prove phases 1-5 are complete for the Stage 0 / Fuse Core delivery described in `.omx/plans/prd-fuse-phase-1-to-5.md`.

## Sources of Truth
- `docs/fuse-implementation-plan-2.md`
- `docs/fuse-language-guide-2.md`
- `docs/fuse-repository-layout-2.md`
- `.omx/specs/deep-interview-fuse-phase-1-to-5.md`

## Verification Matrix

### Phase 1 — Test Suite
- Verify every file named in the implementation plan exists under `tests/fuse/`.
- Verify each `.fuse` file begins with either `// EXPECTED OUTPUT:` or `// EXPECTED ERROR:`.
- Verify `tests/fuse/milestone/four_functions.fuse` is authored first and acts as the canonical milestone program.
- Verify `tests/fuse/full/**` files exist as textual artifacts only; do not require execution.

### Phase 2 — Lexer & Parser
- Run `python stage0/src/parser.py <file>` for every file in `tests/fuse/core/`.
- Expected result: no parse errors; printed AST is human-readable.
- Parser coverage must include constructs listed in the Phase 2 deliverables, at minimum for syntax exercised by the Core suite.

### Phase 3 — Ownership Checker
- For valid Core tests: checker returns success.
- For invalid tests (`*_error.fuse`, `*_rejected.fuse`, and other explicit compile-error cases): checker returns failure with file/line/column and an actionable hint where applicable.
- Specifically verify: `val` immutability, move-after-use rejection, `mutref` rules, match exhaustiveness, import visibility, obvious type mismatches.

### Phase 4 — Evaluator
- Run `python stage0/src/main.py tests/fuse/milestone/four_functions.fuse`.
- Expected result: stdout matches the expected block exactly.
- Run `python stage0/tests/run_tests.py`.
- Expected result: all `tests/fuse/core/**` cases pass; runner exits with zero failures.
- Verify evaluator semantics exercised by tests: `?`, `?.`, `?:`, loops, `break`, `continue`, `defer`, map operations, imports, extension functions, value lifecycle behaviors required by the suite.

### Phase 5 — Language Stabilization
- For every semantic rule clarified during implementation, verify one of:
  - guide text updated in `docs/fuse-language-guide-2.md`, or
  - an ADR added under `docs/adr/`.
- Verify the guide explicitly states Fuse Core is stable for this scope.
- Verify no unresolved implementation-discovered ambiguity remains in tracked notes/final summary.

## Negative Tests
- Invalid Core programs must fail before evaluation.
- Fuse Full implementation is not required; attempting to execute Full tests should not be part of completion evidence.
- No Stage 1 files should be added as part of this task.

## Evidence Checklist
- [ ] Tree listing of created test files
- [ ] Tree listing of Stage 0 source files
- [ ] Parser run evidence across Core tests
- [ ] Checker failure examples with locations
- [ ] Milestone stdout evidence
- [ ] Core test runner summary
- [ ] Guide/ADR stabilization evidence
- [ ] Explicit statement that Stage 1/Fuse Full implementation was not started

## Exit Criteria
The task is complete only when the PRD acceptance criteria and every checklist item above are satisfied with concrete evidence.
