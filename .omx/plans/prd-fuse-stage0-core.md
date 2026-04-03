# PRD — Fuse Stage 0 / Phases 1-5

## Requirements Summary
- Implement only phases 1-5 from the revised plan: Test Suite, Lexer & Parser, Ownership Checker, Evaluator, and Language Stabilization (`docs/fuse-implementation-plan-2.md:14-18`, `28-125`, `129-218`, `222-356`, `360-389`).
- Treat `docs/fuse-language-guide-2.md` as the language source of truth and `docs/fuse-repository-layout-2.md` as the intended repository structure.
- Build from scratch in this repo because the current workspace lacks the planned `tests/` and `stage0/` trees.
- Stop after complete Stage 0 / Fuse Core delivery. Do not begin Stage 1 or Fuse Full implementation.
- Still author the Phase 1 `tests/fuse/full/**` files as textual test-contract artifacts even though their implementation/execution is deferred.
- Keep Stage 0 execution scoped to `tests/fuse/core/**` plus the milestone; `tests/fuse/full/**` must exist but must not be included in the runnable Stage 0 suite.
- Preserve `tests/fuse/milestone/four_functions.fuse` as the canonical milestone (`docs/fuse-implementation-plan-2.md:5`, `17`, `42-45`, `346-352`).
- Do not add new dependencies.

## RALPLAN-DR Summary
### Principles
1. Tests before implementation: write the repository contract before coding (`docs/fuse-implementation-plan-2.md:30-38`, `123-125`).
2. Core-only delivery: implement just enough Fuse Core to satisfy phases 1-5 and freeze behavior before any Stage 1 work (`docs/fuse-implementation-plan-2.md:18`, `360-389`).
3. Minimize moving parts: prefer a small, dependency-free Python interpreter and only the runtime/stdlib surface needed by tests.
4. Documentation follows discovered behavior: Phase 5 should codify evaluator-revealed semantics in guide/tests/ADRs before expansion (`docs/fuse-implementation-plan-2.md:369-389`).

### Decision Drivers
1. Meet the explicit phase done-when criteria with evidence.
2. Keep scope bounded to Fuse Core / Stage 0 only.
3. Preserve flexibility by bootstrapping from docs with the smallest coherent runtime.

### Viable Options
#### Option A — Monolithic first pass
- Approach: implement parser/checker/evaluator in a single vertical slice and backfill tests/docs later.
- Pros: potentially faster first milestone.
- Cons: violates the plan's tests-first and stabilization sequencing; raises rework risk when semantics drift.

#### Option B — Phase-faithful staged build **(recommended)**
- Approach: author all required test artifacts first, then implement parser, checker, evaluator, then stabilize docs/ADRs.
- Pros: aligns to plan order; keeps semantics pinned; gives clear verification gates.
- Cons: more upfront authoring before the first runnable interpreter.

#### Option C — Minimal milestone-first subset
- Approach: target only the milestone program first, then widen toward full Core tests.
- Pros: fast milestone demo.
- Cons: under-specifies Phase 1 and Phase 2/3 done-when; likely causes parser/checker rewrites.

### Recommendation
Choose Option B. It is the only option that fully respects the revised plan's sequencing and keeps the Phase 5 freeze meaningful.

## Acceptance Criteria
1. Every Phase 1 file listed in `docs/fuse-implementation-plan-2.md:42-92` exists under `tests/fuse/` with `// EXPECTED OUTPUT:` or `// EXPECTED ERROR:` / warning blocks as appropriate.
2. `stage0/src/token.py`, `lexer.py`, `ast.py`, and `parser.py` exist and `python src/parser.py <file.fuse>` parses every Core test without parse errors (`docs/fuse-implementation-plan-2.md:139-217`).
3. `stage0/src/checker.py` rejects invalid ownership/module/type/match programs and valid Core tests pass checking (`docs/fuse-implementation-plan-2.md:232-280`).
4. `stage0/src/values.py`, `environment.py`, `evaluator.py`, `main.py`, and `stage0/tests/run_tests.py` exist and the milestone plus Core tests pass (`docs/fuse-implementation-plan-2.md:294-356`).
5. The Stage 0 runner excludes `tests/fuse/full/**` from execution while still validating those files as Phase 1 artifacts.
6. The guide and ADR set are updated so Phase 4-discovered behavior is explicit and Fuse Core is marked stable (`docs/fuse-implementation-plan-2.md:375-389`; `docs/fuse-language-guide-2.md:104`).
7. No Stage 1 or Fuse Full implementation code is introduced.

## Non-goals
- Stage 1 Rust compiler work.
- Fuse Full implementation/execution (`spawn`, channels, `Shared`, async/await, SIMD).
- A full interactive REPL, extra examples, performance tuning, nonessential stdlib surface, and unrelated docs cleanup.

## Architecture Notes
- **Doc reconciliation rule:** when the revised docs disagree, use `docs/fuse-implementation-plan-2.md` for phase gates and deliverables, `docs/fuse-language-guide-2.md` for language semantics, and record any reconciliation in ADRs/notes during Phase 5.
- **Runtime surface rule:** prefer interpreter-owned builtins/runtime types for `Result`, `Option`, `Map`, lists, strings, and printing until a test explicitly requires module-backed stdlib files. Do not grow the stdlib beyond what Core tests need.
- **Execution boundary:** `main.py` must support file execution and `--check`; a full REPL is not a required completion criterion for this delivery and may be left as a documented stub or deferred note unless implementation becomes trivial.

## Implementation Steps
1. **Bootstrap repo skeleton**
   - Create `tests/fuse/{milestone,core,full}` tree, `stage0/src`, `stage0/tests`, and any minimal docs/ADR directories implied by the revised layout.
   - Add a small shared harness utility for reading expected comment blocks.
2. **Phase 1 authoring pass**
   - Write `tests/fuse/milestone/four_functions.fuse` first.
   - Add all listed Core and Full test files with expected outputs/errors based on the guide.
   - Keep the Full tests textual only; they are artifacts, not runnable targets in Stage 0.
3. **Phase 2 parser pass**
   - Implement token definitions, lexer, AST nodes, and recursive-descent parser.
   - Add parser smoke checks and a CLI pathway for AST printing.
4. **Phase 3 checker pass**
   - Implement semantic passes for ownership, move tracking, `val` immutability, match exhaustiveness, import visibility, and obvious type mismatches.
   - Normalize diagnostic formatting and ensure error snapshots line up with test comments.
5. **Phase 4 evaluator pass**
   - Implement runtime values, environments, control flow, pattern matching, ownership-aware evaluation, `defer`, interpolation, `?.`, `?:`, maps, and imports.
   - Prefer a minimal builtin-backed Core runtime before adding any module-backed stdlib surface.
   - Build a test runner that executes milestone + Core tests and compares output with the comment contract, while excluding `tests/fuse/full/**`.
6. **Phase 5 stabilization pass**
   - Run the full Core suite and milestone repeatedly.
   - Patch guide gaps, add/adjust tests for newly exposed edge cases, and write ADRs for decisions uncovered during implementation.
   - Add a clear "Fuse Core is stable" statement to the guide.

## Risks and Mitigations
- **Risk:** The v2 docs contain mismatches between lists across documents.  
  **Mitigation:** Prefer `fuse-implementation-plan-2.md` for phase gating; record reconciliations in ADRs/notes.
- **Risk:** The Phase 4 deliverables mention `--repl`, but the agreed scope minimizes anything not required by the Phase 1-5 done-when criteria.  
  **Mitigation:** Treat run/check support as mandatory; implement REPL only if it is nearly free, otherwise leave a documented stub/deferred note.
- **Risk:** Writing all tests first may consume time before code runs.  
  **Mitigation:** Keep tests concise and derive them directly from guide examples and plan deliverables.
- **Risk:** Stage 0 semantics such as ASAP destruction or ownership checking may be hard to model exactly.  
  **Mitigation:** Use the milestone/Core tests as executable truth and codify any newly exposed rules in Phase 5.
- **Risk:** Sparse repo means no prior utilities or harnesses exist.  
  **Mitigation:** Keep infrastructure minimal and colocated inside `stage0/`.

## Verification Steps
- Verify test tree completeness against the plan list.
- Run parser over all Core tests.
- Run checker over valid + invalid Core tests and compare diagnostics.
- Run `python src/main.py ../../tests/fuse/milestone/four_functions.fuse` from `stage0/` and compare output exactly.
- Run `python stage0/tests/run_tests.py` (or equivalent package invocation) to confirm zero Core failures.
- Review guide/ADR updates for every semantic gap discovered during execution.

## ADR
- **Decision:** Build Fuse phases 1-5 with a phase-faithful, tests-first Stage 0 Python interpreter from scratch in this repo.
- **Drivers:** explicit revised-plan sequencing; bounded Core-only scope; need for a stable contract before any Stage 1 work.
- **Alternatives considered:** monolithic vertical slice; milestone-only subset first.
- **Why chosen:** only the phase-faithful path preserves the required test contract and stabilization firewall.
- **Consequences:** more upfront authoring, but less semantic churn and clearer verification.
- **Follow-ups:** after completion, Stage 1 planning can target a frozen Core spec.

## Available-Agent-Types Roster
- `explore`: fast file/symbol lookup
- `planner`: planning/revision
- `architect`: design/tradeoff review
- `critic`: quality gate
- `executor`: implementation
- `test-engineer`: test strategy/hardening
- `verifier`: completion evidence
- `writer`: docs/ADR updates

## Follow-up Staffing Guidance
### Ralph path
- 1x `executor` (high) for repo bootstrap + parser/checker/evaluator
- 1x `test-engineer` (medium) for test harness and acceptance-criteria auditing
- 1x `writer` (high) late in Phase 5 for guide/ADR stabilization
- 1x `verifier` (high) before completion claims

### Team path
- Lane 1: `executor` on tests + repo skeleton
- Lane 2: `executor` on parser/AST/tokenizer
- Lane 3: `executor` on checker/evaluator/runtime
- Lane 4: `test-engineer` / `writer` on harness, docs, ADRs, and final evidence

## Launch Hints
- Ralph: `$ralph .omx/plans/prd-fuse-stage0-core.md`
- Team: `$team .omx/plans/prd-fuse-stage0-core.md`

## Team Verification Path
Before shutdown, the team should prove: test tree completeness, parser success across Core tests, checker correctness for invalid cases, milestone runtime success, and captured docs/ADR updates. A later Ralph/verifier pass should confirm no known errors remain and all acceptance criteria are evidenced.

## Changelog
- Initial planning draft generated from the deep-interview spec and revised Fuse docs.
- Revised after architect/critic-style review to clarify runnable-suite boundaries, doc reconciliation, runtime surface minimization, and REPL scope handling.
