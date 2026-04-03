# Test Spec — Fuse Stage 0 / Phases 1-5

## Scope Under Test
- Phase 1 artifact completeness for `tests/fuse/milestone`, `tests/fuse/core/**`, and textual `tests/fuse/full/**`.
- Phase 2 parser coverage for all Core syntax needed by the listed tests.
- Phase 3 checker coverage for ownership, immutability, match exhaustiveness, module visibility, and basic type mismatches.
- Phase 4 evaluator coverage for runnable Fuse Core semantics and the milestone program.
- Phase 5 stabilization coverage for any edge cases discovered during implementation.

## Acceptance Matrix
| Phase | Proof |
|---|---|
| 1 | Every required `.fuse` file exists and starts with an expected-output/error block. |
| 2 | Parsing all `tests/fuse/core/**` files succeeds with zero parse errors. |
| 3 | Invalid Core programs emit expected diagnostics; valid Core programs check cleanly. |
| 4 | `four_functions.fuse` output matches exactly; `stage0/tests/run_tests.py` reports zero Core failures. |
| 5 | Guide/ADR/test updates cover discovered ambiguities and the guide marks Fuse Core stable. |

## Planned Test Inventory
### Artifact-completeness checks
- Enumerate the exact file list from `docs/fuse-implementation-plan-2.md:42-92`.
- Assert each file exists.
- Assert each file contains `// EXPECTED OUTPUT:` or `// EXPECTED ERROR:`.
- Allow a warning-style expectation only where the plan explicitly calls for one (`write_guard_across_await.fuse`).

### Suite-boundary checks
- Confirm `tests/fuse/full/**` files exist as Phase 1 artifacts.
- Confirm the Stage 0 runner includes only milestone + Core tests.
- Confirm no Fuse Full file is accidentally executed by the Stage 0 suite.

### Parser checks
- Parser smoke pass for every file under `tests/fuse/core/`.
- AST spot checks for representative constructs:
  - ownership call-site modifiers
  - `while` / `break` / `continue`
  - `match` / `when`
  - imports and `pub`
  - interpolation / optional chaining / Elvis

### Checker checks
- Negative cases: `move_prevents_reuse`, `match_missing_arm`, `val_immutable`, `import_pub_only`, and any additional `_error` / `_rejected` files.
- Positive cases: valid ownership/memory/errors/types/control-flow/modules tests pass check.
- Diagnostic quality: file/line/column present, hint when appropriate.

### Evaluator checks
- Golden-output execution for all runnable Core tests.
- Milestone exact-match execution.
- Focused semantic checks for:
  - mutation through `mutref`
  - move/use-after-move behavior (through checker + runtime integration)
  - `Result` / `Option` propagation via `?`
  - `defer` ordering
  - ASAP destruction hooks
  - map operations
  - imports/module resolution

### Stabilization checks
- Diff-driven review of guide updates against uncovered semantics.
- ADR presence for decisions not already covered by existing ADRs.
- Final audit that no unresolved ambiguity remains in implementation notes.
- Verify the guide explicitly marks Fuse Core stable.

## Verification Commands (planned)
- `python stage0/src/parser.py <file.fuse>` for parser proof.
- `python stage0/src/main.py --check <file.fuse>` for checker proof.
- `python stage0/src/main.py ../../tests/fuse/milestone/four_functions.fuse` from `stage0/` for milestone proof.
- `python stage0/tests/run_tests.py` for Core-suite proof.

## Risks to Watch in Testing
- Full-test textual artifacts could accidentally be added to runnable Stage 0 suites.
- Error formatting may drift from expected comment blocks.
- Ownership/use-after-move semantics may require checker + evaluator coordination.
- Guide stabilization may lag behind actual interpreter behavior.
- REPL work could expand beyond the agreed boundary if treated as mandatory rather than optional/stubbed.

## Exit Criteria
- Every acceptance matrix row has concrete evidence.
- Zero known parser/checker/evaluator failures remain for Core scope.
- No Stage 1/Fuse Full implementation work has crept into the diff.
- The runnable Stage 0 suite excludes `tests/fuse/full/**`.
