# PRD — Fuse Phase 8 / Stage 1 Fuse Full

## Requirements Summary

Complete Phase 8 as the Stage 1 Fuse Full expansion immediately after the verified Phase 7 Core backend. The plan must stay inside the documented Phase 8 boundary, break the work into single-session checkpoints, and function as the stop/resume ledger for sequential execution.

Primary sources:

- `.omx/specs/deep-interview-fuse-phase-8-planning.md`
- `docs/fuse-implementation-plan-2.md:511-556`
- `docs/fuse-implementation-plan-2.md:558-560`
- `docs/fuse-language-guide-2.md:1142-1305`
- `docs/fuse-language-guide-2.md:1606-1614`
- `docs/fuse-repository-layout-2.md:98-105`
- `docs/fuse-repository-layout-2.md:187-214`
- `tests/fuse/full/**`

## Source Precedence

When sources differ, prefer:

1. `.omx/specs/deep-interview-fuse-phase-8-planning.md` for planning granularity, non-goals, and decision boundaries
2. `tests/fuse/full/**` for the current executable/diagnostic contract already present in the repo
3. `docs/fuse-implementation-plan-2.md` for Phase 8 deliverables and done-when
4. `docs/fuse-language-guide-2.md` for Fuse Full semantics and edge rules
5. `docs/fuse-repository-layout-2.md` for target file/module shape

## RALPLAN-DR Summary

### Principles

1. Every planning unit must be a realistic single-session checkpoint.
2. Runtime support, checker enforcement, stdlib surface, and verification should be split whenever combining them would create debugging ambiguity.
3. The plan must track the actual Full-test surface already in the repository, not an imagined later test set.
4. Phase 8 remains Stage 1 Fuse Full only; no Phase 9/self-hosting work may leak into the plan.
5. Favor dependency-ordered vertical slices that can be paused and resumed with minimal state recovery.

### Decision Drivers

1. The user explicitly wants stop/resume-friendly planning units rather than broad epics.
2. The repo already has Full tests split across concurrency, async, and SIMD, including compile-error and warning cases (`tests/fuse/full/concurrency/*.fuse`, `tests/fuse/full/async/*.fuse`, `tests/fuse/full/simd/*.fuse`).
3. The documented Phase 8 deliverables mix runtime, checker, and stdlib concerns (`docs/fuse-implementation-plan-2.md:519-555`, `docs/fuse-language-guide-2.md:1608-1614`), which is a clogging risk unless decomposed further.

### Viable Options

#### Option A — Coarse feature epics (3-4 units)

Group work by concurrency, async, SIMD, and final integration.

- Pros: fewer plan items, less bookkeeping
- Cons: violates the single-session checkpoint goal; high risk of mixed runtime/checker/debug churn inside each unit

#### Option B — Mid-grain feature buckets (8-9 units)

Split by feature family plus one final integration bucket.

- Pros: better than coarse epics; acceptable for many teams
- Cons: still bundles runtime/checker/stdlib work too tightly for reliable stop/resume debugging

#### Option C — Fine-grain single-session checkpoints (recommended, 12 units)

Split Phase 8 into explicit runtime/checker/stdlib/test-closure units sized to one normal session.

- Pros: strongest stop/resume behavior; narrowest debugging surface; easiest to mark complete honestly
- Cons: more bookkeeping and more status updates required

### Recommendation

Use **Option C**.

Architect steelman antithesis: the repo only has ten existing Full fixtures, so 12 checkpoints may feel heavier than the current test count justifies.  
Synthesis: keep the unit count high because the risk is not raw test count but mixed runtime/checker/stdlib responsibilities; a fine-grain ledger is the safer execution shape for sequential resumable work.

## Brownfield Grounding

Current repo evidence:

- Full tests already exist under `tests/fuse/full/concurrency/`, `tests/fuse/full/async/`, and `tests/fuse/full/simd/` (`docs/fuse-repository-layout-2.md:98-105`).
- Current Stage 1 checker only contains `mod.rs`, `types.rs`, `ownership.rs`, and `exhaustiveness.rs`; the planned `rank.rs`, `spawn.rs`, and `async_lint.rs` surfaces are not present in the current tree (`docs/fuse-repository-layout-2.md:187-194` plus current repo inspection).
- Current Stage 1 runtime only contains Core-era files such as `lib.rs`, `value.rs`, `asap.rs`, `builtins.rs`, and `string_ops.rs`; the planned `chan.rs`, `shared.rs`, and `async_rt.rs` surfaces are not present yet (`docs/fuse-repository-layout-2.md:200-212` plus current repo inspection).
- Full-test contracts already include:
  - spawn compile error: `tests/fuse/full/concurrency/spawn_mutref_rejected.fuse:1-7`
  - missing-rank compile error: `tests/fuse/full/concurrency/shared_no_rank.fuse:1-7`
  - rank-order compile error: `tests/fuse/full/concurrency/shared_rank_violation.fuse:1-7`
  - write-guard-across-await warning: `tests/fuse/full/async/write_guard_across_await.fuse:1-7`
  - async output: `tests/fuse/full/async/await_basic.fuse:1-6`
  - SIMD output: `tests/fuse/full/simd/simd_sum.fuse:1-6`

## Scope

### In Scope

- Phase 8 planning for Stage 1 Fuse Full only
- Runtime support for `spawn`, `Chan<T>`, `Shared<T>`, `async`/`await`/`suspend`, and SIMD
- Checker enforcement for spawn capture rules, `@rank`, and async warnings/placement rules
- Full stdlib work explicitly required by the docs and/or current Full tests
- Verification coverage for the current `tests/fuse/full/**` corpus plus final `tests/fuse/**` closure

### Out of Scope

- Phase 9 / Stage 2 self-hosting work (`docs/fuse-implementation-plan-2.md:558-690`)
- Performance optimization except where the Phase 8 milestone directly requires correctness under load
- Broad cleanup unrelated to Phase 8 delivery
- Replanning the overall phase sequence

## Acceptance Criteria

1. The plan is split into single-session checkpoints with explicit dependency order.
2. Every checkpoint includes:
   - status
   - dependencies
   - likely files touched
   - tests to run
   - done-when
3. The plan covers all documented Phase 8 deliverables (`docs/fuse-language-guide-2.md:1608-1614`).
4. The plan is grounded in the existing Full-test surface (`tests/fuse/full/**`).
5. The plan explicitly excludes all Phase 9/self-hosting work.

## Execution Units

### Unit 1 — Phase 8 Ledger Bootstrap

- Status: `complete`
- Dependencies: none
- Likely files:
  - `.omx/plans/prd-fuse-phase-8-stage1-full.md`
  - `.omx/plans/test-spec-fuse-phase-8-stage1-full.md`
  - optional progress ledger under `.omx/state/`
- Tests to run:
  - none required; document-only checkpoint
- Done when:
  - the Phase 8 PRD and test spec exist
  - unit status fields and dependency order are explicit
  - stop/resume conventions are recorded

### Unit 2 — Full Harness Plumbing

- Status: `complete`
- Dependencies: Unit 1
- Likely files:
  - `stage1/fusec/tests/`
  - `stage1/fusec/src/common.rs`
  - any Stage 1 Full harness helpers
- Tests to run:
  - a new or updated Full harness smoke pass
  - existing Core checks remain green
- Done when:
  - Stage 1 can classify and run Full output/error/warning fixtures cleanly
  - Full tests no longer require ad hoc manual invocation

### Unit 3 — `spawn` Runtime/Codegen Support

- Status: `complete`
- Dependencies: Unit 2
- Likely files:
  - `stage1/fuse-runtime/src/async_rt.rs`
  - `stage1/fusec/src/codegen/`
  - `stage1/fusec/src/parser/` if syntax support is incomplete
- Tests to run:
  - `tests/fuse/full/concurrency/chan_basic.fuse` once channels are stubbed enough for spawn paths
  - a minimal spawn output smoke test if added
- Done when:
  - Stage 1 can lower and execute basic spawned task behavior required by Full fixtures
  - no checker-only rules are assumed solved yet

### Unit 4 — Spawn Checker Enforcement

- Status: `complete`
- Dependencies: Unit 3
- Likely files:
  - `stage1/fusec/src/checker/mod.rs`
  - `stage1/fusec/src/checker/spawn.rs`
- Tests to run:
  - `tests/fuse/full/concurrency/spawn_mutref_rejected.fuse`
- Done when:
  - spawn capture violations produce the expected compile errors
  - spawn rule enforcement is isolated enough to debug without channel/shared-state noise

### Unit 5 — `Chan<T>` Runtime Core

- Status: `complete`
- Dependencies: Unit 3
- Likely files:
  - `stage1/fuse-runtime/src/chan.rs`
  - `stage1/fuse-runtime/src/lib.rs`
  - `stage1/fusec/src/codegen/`
- Tests to run:
  - `tests/fuse/full/concurrency/chan_basic.fuse`
  - `tests/fuse/full/concurrency/chan_bounded_backpressure.fuse`
- Done when:
  - bounded and unbounded channel behavior required by the current Full tests works
  - channel runtime can be exercised independently of Shared/rank work

### Unit 6 — Channel Stdlib Exposure

- Status: `complete`
- Dependencies: Unit 5
- Likely files:
  - `stdlib/full/chan.fuse`
  - any Full stdlib plumbing required by Stage 1 imports
- Tests to run:
  - `tests/fuse/full/concurrency/chan_basic.fuse`
  - `tests/fuse/full/concurrency/chan_bounded_backpressure.fuse`
- Done when:
  - the channel API exposed to Fuse code matches the current Full tests
  - channel behavior is not only runtime-present but language-surface usable

### Unit 7 — `Shared<T>` Runtime Core

- Status: `complete`
- Dependencies: Unit 3
- Likely files:
  - `stage1/fuse-runtime/src/shared.rs`
  - `stage1/fuse-runtime/src/lib.rs`
  - `stage1/fuse-runtime/src/asap.rs`
- Tests to run:
  - at least one positive Shared runtime smoke path
  - `tests/fuse/full/concurrency/shared_rank_ascending.fuse` once checker support exists
- Done when:
  - Shared read/write guard behavior exists
  - guard destruction/unlock behavior is compatible with ASAP rules

### Unit 8 — `@rank` Checker Enforcement

- Status: `complete`
- Dependencies: Unit 7
- Likely files:
  - `stage1/fusec/src/checker/rank.rs`
  - `stage1/fusec/src/checker/mod.rs`
- Tests to run:
  - `tests/fuse/full/concurrency/shared_no_rank.fuse`
  - `tests/fuse/full/concurrency/shared_rank_violation.fuse`
  - `tests/fuse/full/concurrency/shared_rank_ascending.fuse`
- Done when:
  - missing-rank and rank-order errors match the current test contract
  - a positive ascending-rank case is accepted cleanly

### Unit 9 — Async Runtime Support

- Status: `complete`
- Dependencies: Unit 3
- Likely files:
  - `stage1/fuse-runtime/src/async_rt.rs`
  - `stage1/fuse-runtime/src/lib.rs`
  - `stage1/fusec/src/codegen/`
- Tests to run:
  - `tests/fuse/full/async/await_basic.fuse`
  - `tests/fuse/full/async/suspend_fn.fuse`
- Done when:
  - basic `async`/`await`/`suspend` execution works for the current Full output fixtures

### Unit 10 — Async Checker and Warning Surface

- Status: `complete`
- Dependencies: Unit 9, Unit 7
- Likely files:
  - `stage1/fusec/src/checker/async_lint.rs`
  - `stage1/fusec/src/checker/mod.rs`
- Tests to run:
  - `tests/fuse/full/async/write_guard_across_await.fuse`
  - any explicit `await`-placement diagnostics if added
- Done when:
  - write-guard-across-await warnings match the current Full test contract
  - async rule checking is wired to the runtime/shared-state model

### Unit 11 — SIMD Runtime/Codegen Support

- Status: `not started`
- Dependencies: Unit 2
- Likely files:
  - `stage1/fuse-runtime/src/simd.rs`
  - `stage1/fuse-runtime/src/lib.rs`
  - `stage1/fusec/src/codegen/`
- Tests to run:
  - `tests/fuse/full/simd/simd_sum.fuse`
- Done when:
  - SIMD operations required by the current Full test corpus work
  - fallback/scalar behavior is handled where the platform or lane shape requires it

### Unit 12 — Full Stdlib Completion and Final Integration

- Status: `not started`
- Dependencies: Units 4, 6, 8, 10, 11
- Likely files:
  - `stdlib/full/shared.fuse`
  - `stdlib/full/timer.fuse`
  - `stdlib/full/simd.fuse`
  - `stdlib/full/http.fuse`
  - any remaining Stage 1 Full integration paths
- Tests to run:
  - all `tests/fuse/full/**`
  - full `tests/fuse/**` closure
- Done when:
  - the Stage 1 Full milestone is met
  - all current Full tests pass
  - final documentation/status is updated for resume safety

## Risks and Mitigations

- **Mixed-feature clogging risk**
  - Mitigation: keep runtime, checker, stdlib, and integration as separate checkpoints
- **False progress from placeholder tests**
  - Mitigation: keep every unit tied to explicit Full fixtures and require real output/error/warning evidence
- **Checker/runtime ordering tension**
  - Mitigation: where the current Full fixture is checker-only (for example placeholder error/warning contracts), execution may pull the checker checkpoint slightly earlier than its runtime counterpart if that creates a narrower debug surface, but only without widening the unit scope
- **Async/shared-state coupling risk**
  - Mitigation: land Shared runtime first, then async warning enforcement second
- **SIMD scope creep**
  - Mitigation: constrain SIMD work to the current Full fixture and documented rules before broadening
- **Resume ambiguity**
  - Mitigation: every unit has status, dependencies, files, tests, and done-when fields in one document

## Verification Strategy

- Unit-level verification must stay narrow: only the tests relevant to that checkpoint plus no-regression checks where required
- Integration closure happens only in Unit 12
- Final Phase 8 proof requires:
  - all `tests/fuse/full/**` pass
  - complete `tests/fuse/**` suite passes
  - documented Full milestone behavior is reproduced

## ADR

- **Decision:** Plan Phase 8 as 12 single-session checkpoints rather than a small number of broad feature epics.
- **Drivers:** stop/resume safety; current Full-test split; runtime/checker/stdlib coupling risk
- **Alternatives considered:** 4 coarse epics; 8-9 medium epics
- **Why chosen:** best match for the user’s requirement that checkpoints be small enough to pause/resume cleanly
- **Consequences:** more bookkeeping, but less debugging clog and cleaner incremental delivery
- **Follow-ups:** produce the aligned test spec and later hand off to Ralph or Team using this ledger

## Available-Agent-Types Roster

- `planner` — checkpoint ordering, dependency hygiene, risk shaping
- `architect` — runtime/checker boundary review, async/shared/SIMD design review
- `critic` — plan quality gate and stop/resume rigor
- `executor` — implementation lanes for runtime/checker/stdlib work
- `test-engineer` — harness and fixture verification
- `verifier` — acceptance closure and evidence review
- `debugger` — narrow failure diagnosis when a checkpoint goes red

## Follow-up Staffing Guidance

### For `$ralph`

- Use one persistent owner with `high` reasoning.
- Order the work strictly by the 12 checkpoints.
- Treat each checkpoint as complete only when its listed tests and done-when are satisfied.

### For `$team`

- Runtime lane: `executor` (`high`) for `stage1/fuse-runtime/**`
- Checker lane: `executor` or `debugger` (`high`) for `stage1/fusec/src/checker/**`
- Stdlib lane: `executor` (`medium`) for `stdlib/full/**`
- Harness lane: `test-engineer` (`medium`) for Full-test plumbing and targeted fixture runs
- Verification lane: `verifier` (`high`) for milestone/full-suite sign-off

## Launch Hints

- Ralph:
  - `$ralph .omx/plans/prd-fuse-phase-8-stage1-full.md`
- Team:
  - `$team .omx/plans/prd-fuse-phase-8-stage1-full.md`
  - `omx team .omx/plans/prd-fuse-phase-8-stage1-full.md`

## Team Verification Path

1. Prove the harness can execute Full fixtures distinctly from Core fixtures.
2. Prove each checkpoint’s narrow tests before moving to the next checkpoint.
3. Keep runtime/checker/stdlib evidence separate until Unit 12.
4. Only mark Phase 8 complete when all Full fixtures and then the complete `tests/fuse/**` suite pass.

## Changelog

- Initial planning draft created from the deep-interview planning spec, the Phase 8 docs, the current Full-test surface, and the current Stage 1 runtime/checker tree.
- Architect review synthesis applied: keep the 12-unit fine-grain split, but explicitly note that checker-only placeholder fixtures may justify a narrower checker-first execution order during implementation when that reduces clogging.

## Progress Update

Units 1 through 4 are complete: the Phase 8 ledger exists, the Full harness now classifies and parses current Full fixtures, minimal `spawn` runtime/codegen support is in place, and `spawn_mutref_rejected` is enforced as a real checker contract. Unit 5 is in progress, with `chan_basic` already compiling and running through Stage 1 while bounded-channel behavior remains to be implemented.

The latest checkpoint advanced Unit 5 further: `Chan::<T>.bounded(n)` now exists, the first real Full channel fixture is no longer a placeholder, and the compile-run smoke harness was stabilized to avoid wrapper-build races. Bounded backpressure behavior is still the remaining gap before Unit 5 can be marked complete.

The current checkpoint also closes the checker side of `@rank`: `shared_no_rank` and `shared_rank_violation` are now real compile-error fixtures, `shared_rank_ascending` is checker-clean, and a minimal Shared runtime hook is sufficient for the positive smoke path. At this point Units 5 and 7 remain open only for deeper runtime semantics, while Unit 8 is complete.

The newest groundwork keeps the frontend coherent for later async work: the AST/parser/evaluator/checker/object backend now recognize `await` as a first-class expression shape, and the positive Shared fixture remains real code rather than placeholder text. This does not complete the async runtime path yet, but it removes a syntax-level blocker before the async checkpoints begin.

The current green state is stable again after threading `await` through the remaining frontend/backend match arms. Units 5 and 7 still have unfinished runtime semantics, but the checker contracts and positive smoke paths for channels and the initial Shared flow are now holding together under the fast verification set.

The latest async checkpoint promotes `await_basic` to a real executable fixture and proves it in the smoke suite, while explicitly deferring `suspend_fn` execution to the later async-runtime checkpoint. This keeps Unit 9 moving without pretending the full async runtime is already complete, and it preserves a green fast-verification surface for the currently supported behavior.

The warning side of the async checker is now also real: `write_guard_across_await` is no longer a placeholder and the checker emits the expected warning contract while the smoke suite remains green for the currently supported executable Full fixtures. This closes the warning proof without claiming the remaining suspend/runtime semantics are done.

The latest runtime checkpoint converts `chan_bounded_backpressure` into a real executable Full fixture and adds deferred-send behavior for bounded channels: extra sends are now queued until `recv()` frees capacity. With that, the smoke suite covers both unbounded and bounded channel behavior against real Full fixtures rather than synthetic-only proofs.

Units 5, 9, and 10 can now be treated as complete from the current repo contract: the real channel fixtures pass, `await_basic` and `suspend_fn` execute through Stage 1, and the `write_guard_across_await` warning contract is enforced. The next open runtime slice is Unit 7, where `Shared<T>` still needs deeper semantics beyond the minimal positive hook already proven by `shared_rank_ascending`.

Units 6 and 7 are now also complete from the current repo contract: `stdlib/full/chan.fuse` exists as a real parseable stdlib artifact and import target, while the Shared path has progressed beyond a no-op hook into a positive mutation/readback runtime proof. With that, the remaining open units are Unit 11 (SIMD) and Unit 12 (remaining Full stdlib plus final integration).

Unit 11 is now complete from the current repo contract as well: `simd_sum` is a real executable Full fixture, and Stage 1 has a scalar-backed `SIMD.sum` path that satisfies the current SIMD test surface. That leaves Unit 12 as the final open integration unit.
