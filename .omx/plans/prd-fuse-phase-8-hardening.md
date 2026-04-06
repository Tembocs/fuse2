# PRD — Fuse Phase 8 Hardening

## Requirements Summary

Turn the existing hardening draft into an execution-ready plan grounded in the real `stage1/` state. This lane hardens trust in the Stage 1 Fuse Full compiler before Phase 9 by:

- restoring a trustworthy baseline
- freezing the actual Stage 1 execution model
- reconciling claimed Full surfaces with implemented semantics
- strengthening tests around the weakest runtime/checker boundaries
- ending with an explicit Phase 9 readiness decision

## Source Precedence

When sources differ, prefer:

1. live `stage1/` code and tests
2. this PRD
3. `.omx/plans/prd-fuse-phase-8-hardening-draft.md`
4. `.omx/plans/prd-fuse-phase-8-stage1-full.md`
5. `.omx/plans/test-spec-fuse-phase-8-stage1-full.md`
6. `docs/fuse-language-guide-2.md`

## Brownfield Grounding

Current repo evidence changes the shape of the hardening lane:

1. The baseline is not fully trustworthy yet: escalated `cargo test` in `stage1/` fails in `fusec/tests/compile_output_suite.rs` because `tests/fuse/core/control_flow/for_with_break.fuse` unexpectedly runs with `chan_basic` output. Hardening must therefore begin with baseline integrity, not assume a clean green workspace.
2. `spawn` is currently a deterministic inline lowering, not a real concurrent runtime. `stage1/fusec/src/codegen/object_backend.rs` snapshots locals and compiles spawn bodies inline.
3. `Shared<T>` is currently a thin aliasing wrapper over one stored handle. `read()` and `write()` return the same underlying value; rank and await safety are enforced mostly in the checker.
4. `Chan<T>` is currently an in-memory queue with `items`, `pending`, and optional `capacity`. The bounded path is promotion-based, not a real scheduler/blocking primitive.
5. `await` is largely semantically transparent today. The main extra behavior is the checker warning for holding a write guard across `await`.
6. `SIMD.sum` is scalar-backed in runtime and already broader than its frontend contract: runtime can sum floats, while the lowered surface and stdlib still imply `Int`.
7. `stdlib/full/*` is only partly truthful. `chan` and `shared` are backend/runtime special cases, while `timer` and `http` are parseable stubs with no execution proof.

## RALPLAN-DR Summary

### Principles

1. Restore baseline trust before expanding semantics.
2. Freeze and harden the actual Stage 1 model; do not silently redesign toward real concurrency.
3. Prefer repo-backed fixtures over synthetic smoke tests whenever the same contract can be expressed as a checked-in Fuse program.
4. Every claimed Full surface must be either executable and tested or explicitly documented as parse-only/stubbed.
5. Finish with a binary readiness decision for Phase 9, not an implied “probably good enough”.

### Decision Drivers

1. The current workspace test baseline is red, so hardening work on top of it would not be trustworthy.
2. The weakest trust gaps are semantic mismatches, not missing syntax: wrapper/output isolation, Shared aliasing ambiguity, scalar-vs-claimed SIMD behavior, and stubby stdlib/full surfaces.
3. Stage 1 currently models Full features in a bounded, mostly sequential way. The hardening lane must strengthen that model rather than accidentally promising a more advanced runtime than the repo actually has.

### Options Considered

#### Option A — Minimal patch-up

Fix the failing baseline test, add a few extra smokes, and keep the rest of the draft qualitative.

- Pros: fastest path
- Cons: leaves Shared/SIMD/stdlib truthfulness ambiguous; weak Phase 9 gate

#### Option B — Bounded contract hardening with verification upgrade

Repair the baseline, freeze the actual execution model, align surface contracts with implementation, and add focused hardening tests.

- Pros: strongest trust gain without redesign
- Cons: more upfront planning and contract decisions

#### Option C — Runtime redesign toward “real” concurrency

Push Stage 1 closer to a true async/locking runtime before Phase 9.

- Pros: richer semantics
- Cons: too broad for pre-Phase-9 hardening; risks reopening Phase 8 instead of hardening it

### Decision

Choose `Option B`.

## Scope

### In Scope

- Restoring a fully trustworthy `stage1` test baseline
- Hardening the generated-wrapper/native compile path
- Freezing and testing the current Stage 1 execution model for `spawn`, `Chan<T>`, `Shared<T>`, `await`, and SIMD
- Reconciling runtime, checker, codegen, stdlib/full surface files, and tests where they currently disagree
- Strengthening full-suite verification and replacing synthetic tests where a repo fixture is better
- Producing a Phase 9 readiness note

### Out of Scope

- Stage 2 / Phase 9 compiler implementation
- Real multithreaded scheduling, true blocking channels, or a full async executor redesign
- Broad performance work
- Tooling/platform work outside `stage1`

## Acceptance Criteria

1. Escalated `cargo test` in `stage1/` passes cleanly.
2. The generated wrapper/native compile path is isolated enough that fixture output cannot bleed between compile/run cases.
3. The current Stage 1 execution model is explicit and tested:
   - `spawn` deterministic inline semantics
   - `Chan<T>` queue semantics
   - `Shared<T>` aliasing/rank semantics
   - `await` warning semantics
   - bounded SIMD contract
4. `stdlib/full/*` surfaces are truthful:
   - executable surfaces are exercised
   - stub surfaces are explicitly tested/documented as stubs
5. The hardening pass ends with a short written Phase 9 readiness verdict.

## Execution Units

### Unit 0 — Baseline Integrity Restore

- Status: `pending`
- Dependencies: none
- Why first:
  - hardening on top of a red or flaky baseline is low-trust work
- Likely files:
  - [object_backend.rs](/D:/fuse/fuse2/stage1/fusec/src/codegen/object_backend.rs)
  - [compile_output_suite.rs](/D:/fuse/fuse2/stage1/fusec/tests/compile_output_suite.rs)
  - [harness.rs](/D:/fuse/fuse2/stage1/fusec/tests/harness.rs)
- Work:
  - fix generated-wrapper reuse/output contamination in the native compile path
  - add a regression that specifically proves fixture A cannot run fixture B’s generated program
  - verify the failing `for_with_break` case stays isolated
- Tests:
  - `cargo test -p fusec --test compile_output_suite`
  - `cargo test` in `stage1`
- Done when:
  - the full escalated `stage1` workspace test run is green
  - wrapper output reuse cannot reproduce the `recv: 1` cross-fixture failure

### Unit 1 — Verification Surface Upgrade

- Status: `pending`
- Dependencies: Unit 0
- Likely files:
  - [full_smoke_suite.rs](/D:/fuse/fuse2/stage1/fusec/tests/full_smoke_suite.rs)
  - [check_full_suite.rs](/D:/fuse/fuse2/stage1/fusec/tests/check_full_suite.rs)
  - [common.rs](/D:/fuse/fuse2/stage1/fusec/src/common.rs)
- Work:
  - add a first-class repo-backed Full output sweep analogous to the core output suite
  - reduce reliance on synthetic temporary fixtures where an existing repo fixture can prove the same thing
  - keep checker-only Full fixtures explicit and separate from compile/run fixtures
- Tests:
  - targeted Full-suite tests
  - `cargo test -p fusec --test full_smoke_suite`
  - `cargo test -p fusec --test check_full_suite`
- Done when:
  - the Full suite has an explicit, trustworthy verification shape rather than a collection of ad hoc smokes

### Unit 2 — Execution Model Freeze

- Status: `pending`
- Dependencies: Unit 1
- Likely files:
  - [object_backend.rs](/D:/fuse/fuse2/stage1/fusec/src/codegen/object_backend.rs)
  - [check_full_suite.rs](/D:/fuse/fuse2/stage1/fusec/tests/check_full_suite.rs)
  - new repo-backed Full fixture(s) under [tests/fuse/full](/D:/fuse/fuse2/tests/fuse/full)
- Work:
  - explicitly freeze Stage 1 as a bounded deterministic execution model, not a real concurrent runtime
  - add proof around current `spawn` ordering/visibility semantics so later hardening work cannot accidentally drift the model
  - freeze the existing `spawn` checker boundary that rejects `mutref` capture across spawn
  - freeze executable `await` transparency and `suspend` behavior as they exist today, separate from warning hardening
- Tests:
  - new repo-backed fixture(s) for `spawn` sequencing/visibility
  - full targeted suite
- Done when:
  - the repo has executable proof of the actual Stage 1 execution model it is hardening
  - `spawn` checker boundaries and `await` transparency are explicit rather than incidental

### Unit 3 — Shared Contract Reconciliation

- Status: `pending`
- Dependencies: Unit 2
- Likely files:
  - [value.rs](/D:/fuse/fuse2/stage1/fuse-runtime/src/value.rs)
  - [shared.rs](/D:/fuse/fuse2/stage1/fuse-runtime/src/shared.rs)
  - [object_backend.rs](/D:/fuse/fuse2/stage1/fusec/src/codegen/object_backend.rs)
  - [mod.rs](/D:/fuse/fuse2/stage1/fusec/src/checker/mod.rs)
  - [shared.fuse](/D:/fuse/fuse2/stdlib/full/shared.fuse)
- Work:
  - reconcile the current Shared API contract across runtime, backend, checker, fixtures, and stdlib/full
  - match surface claims to current observable Stage 1 behavior unless a mismatch is unsafe enough to justify a bounded fix
  - specifically reconcile:
    - constructor arity/default value policy
    - alias-through-shared semantics for `read()` and `write()`
    - whether `try_write` remains executable or is explicitly demoted from the truthful Stage 1 contract
- Tests:
  - existing Shared/rank fixtures
  - new repo-backed positive Shared contract fixtures
- Done when:
  - there is one coherent Shared contract, not five slightly different ones

### Unit 4 — Shared Behavior Proof

- Status: `pending`
- Dependencies: Unit 3
- Likely files:
  - [full_smoke_suite.rs](/D:/fuse/fuse2/stage1/fusec/tests/full_smoke_suite.rs)
  - new repo-backed fixtures under [tests/fuse/full/concurrency](/D:/fuse/fuse2/tests/fuse/full/concurrency)
  - [value.rs](/D:/fuse/fuse2/stage1/fuse-runtime/src/value.rs)
- Work:
  - replace the synthetic shared roundtrip smoke with repo-backed fixtures
  - prove repeated read/write cycles, nested-data mutation visibility, and release behavior do not regress
  - if `try_write` remains in-scope, prove both success and failure shape explicitly
- Tests:
  - full Shared fixture family
  - `cargo test -p fusec --test full_smoke_suite`
- Done when:
  - Shared behavior is proven by checked-in fixtures rather than inferred from one synthetic happy path

### Unit 5 — Channel Contract Hardening

- Status: `pending`
- Dependencies: Unit 2
- Likely files:
  - [value.rs](/D:/fuse/fuse2/stage1/fuse-runtime/src/value.rs)
  - [chan.rs](/D:/fuse/fuse2/stage1/fuse-runtime/src/chan.rs)
  - [object_backend.rs](/D:/fuse/fuse2/stage1/fusec/src/codegen/object_backend.rs)
  - [chan.fuse](/D:/fuse/fuse2/stdlib/full/chan.fuse)
  - new fixtures under [tests/fuse/full/concurrency](/D:/fuse/fuse2/tests/fuse/full/concurrency)
- Work:
  - freeze the actual sequential queue contract for `Chan<T>`
  - add proof for repeated send/recv cycles, bounded promotion behavior, and empty-receive policy
  - ensure stdlib/full channel signatures truthfully match what Stage 1 supports
- Tests:
  - existing channel fixtures
  - new repo-backed repetition/edge fixtures
- Done when:
  - channel behavior is explicit, repeatable, and no longer inferred from only two happy-path fixtures

### Unit 6 — Async/Shared Warning Hardening

- Status: `pending`
- Dependencies: Unit 3
- Likely files:
  - [mod.rs](/D:/fuse/fuse2/stage1/fusec/src/checker/mod.rs)
  - new fixtures under [tests/fuse/full/async](/D:/fuse/fuse2/tests/fuse/full/async)
- Work:
  - tighten the warning contract around `write` access across `await`
  - add at least one safe no-warning case and one sharper warning case
  - keep the warning aligned with the explicitly frozen Stage 1 Shared model
- Tests:
  - existing `write_guard_across_await`
  - new positive/negative warning fixtures
- Done when:
  - the warning is no longer a one-case heuristic with no nearby counterexample coverage

### Unit 7 — SIMD Contract Alignment

- Status: `pending`
- Dependencies: Unit 1
- Likely files:
  - [value.rs](/D:/fuse/fuse2/stage1/fuse-runtime/src/value.rs)
  - [simd.rs](/D:/fuse/fuse2/stage1/fuse-runtime/src/simd.rs)
  - [object_backend.rs](/D:/fuse/fuse2/stage1/fusec/src/codegen/object_backend.rs)
  - [simd.fuse](/D:/fuse/fuse2/stdlib/full/simd.fuse)
  - new fixtures under [tests/fuse/full/simd](/D:/fuse/fuse2/tests/fuse/full/simd)
- Work:
  - make runtime, lowering, typing, stdlib, and tests agree on the truthful current Stage 1 SIMD contract
  - preferred direction: preserve the currently intended narrow Stage 1 surface unless broader runtime behavior is already intentionally part of the observable contract
  - add edge tests for empty lists and tail handling within that chosen contract
- Tests:
  - existing `simd_sum`
  - new repo-backed SIMD edge fixtures
- Done when:
  - SIMD behavior is explicit and truthful rather than broader in runtime than in the language surface

### Unit 8 — Stdlib Full Truthfulness Pass

- Status: `pending`
- Dependencies: Units 3, 5, 7
- Likely files:
  - [shared.fuse](/D:/fuse/fuse2/stdlib/full/shared.fuse)
  - [chan.fuse](/D:/fuse/fuse2/stdlib/full/chan.fuse)
  - [timer.fuse](/D:/fuse/fuse2/stdlib/full/timer.fuse)
  - [http.fuse](/D:/fuse/fuse2/stdlib/full/http.fuse)
  - [check_full_suite.rs](/D:/fuse/fuse2/stage1/fusec/tests/check_full_suite.rs)
- Work:
  - classify each Full stdlib module as one of:
    - executable and exercised
    - intentionally parse-only/stubbed
  - remove “present therefore trusted” ambiguity
  - add tests that assert stub behavior where a stub remains intentional
- Tests:
  - full stdlib parse/import suite
  - any new stub-contract tests
- Done when:
  - every stdlib/full module is either genuinely exercised or honestly marked/tested as a stub

### Unit 9 — Phase 9 Readiness Gate

- Status: `pending`
- Dependencies: Units 0 through 8
- Likely files:
  - `.omx/plans/`
  - optional readiness note under `.omx/plans/`
- Work:
  - summarize what is now trustworthy
  - record any remaining blockers with exact evidence
  - make a hard call:
    - `Ready to start Phase 9`
    - or `Do not start Phase 9 yet`
- Tests:
  - escalated `cargo test` in `stage1`
  - targeted hardening suites
- Done when:
  - the repo contains an evidence-backed Phase 9 readiness verdict instead of an implied assumption

## Risks

- The biggest risk is accidental redesign: if this lane drifts toward real concurrency/runtime architecture, it stops being hardening.
- The second risk is false confidence from parse-only or checker-only coverage being mistaken for executable proof.
- The third risk is wrapper/build artifact reuse causing misleading green results even after semantics change.

## Follow-up Staffing Guidance

- Best sequential lane: `$ralph .omx/plans/prd-fuse-phase-8-hardening.md`
- Best coordinated lane: `$team .omx/plans/prd-fuse-phase-8-hardening.md`
- Suggested staffing if using team mode:
  - `debugger` or `build-fixer` for Unit 0
  - `test-engineer` for Units 1 and 8
  - `executor` for Units 2 through 7
  - `verifier` for Unit 9
