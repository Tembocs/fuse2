# Draft PRD — Phase 8 Hardening Pass

## Purpose

Harden the finished Phase 8 / Stage 1 Fuse Full compiler so it is a stronger, less ambiguous foundation for self-hosting. This pass is about trust, edge-case closure, and explicit readiness criteria, not about reopening whether Phase 8 was completed under the current repo contract.

This draft assumes:

- Phase 8 is complete under the current tested/documented repository contract
- hardening is a distinct pre-Phase-9 lane
- hardening should stay bounded to compiler/runtime/stdlib trust work, not Stage 2 implementation

## Why This Exists

The current Stage 1 compiler is not a toy. It compiles and passes the current Stage 1 verification surface. But the current acceptance surface is still narrower than the language guide’s broader intent in three important areas:

1. **SIMD depth**  
   The exercised SIMD behavior is currently satisfied by a scalar-backed path for the current fixture. That is acceptable for the current contract, but weak as a long-term confidence base.

2. **`Shared<T>` depth**  
   The current implementation proves the exercised positive read/write and mutation-readback paths, but not a richer or more realistic guard/locking model.

3. **`stdlib/full/*` depth**  
   Several Full stdlib modules now exist as parseable repository artifacts, but not all are deeply exercised as runtime-backed modules.

These are exactly the kinds of gaps that can make self-hosting harder than it should be. Hardening reduces that risk.

## Positioning

- This is **after** Phase 8 and **before** Phase 9
- This is **not** Stage 2 work
- This is **not** MCP/LSP/tooling platform work
- This is a compiler-hardening lane intended to decide whether Stage 1 is truly ready to bootstrap Stage 2

## In Scope

- Deepen `Shared<T>` runtime semantics and tests
- Deepen SIMD semantics and tests
- Turn the important `stdlib/full/*` surfaces into exercised modules where that matters
- Add stronger verification around edge cases the current repo contract does not stress deeply enough
- End with a clear “ready / not ready for Phase 9” conclusion

## Out of Scope

- Phase 9 implementation
- Stage 2 compiler modules
- MCP/LSP/tooling platform work
- Package manager design
- Broad language redesign
- Large performance programs that are not required for confidence in Stage 1 semantics

## Acceptance Criteria

1. `Shared<T>` has a stronger, explicitly defined runtime model than the current thin exercised path.
2. SIMD has stronger and broader behavioral proof than the current scalar-backed `simd_sum` happy path.
3. The important `stdlib/full/*` modules are not just parseable surfaces; they are exercised where the project intends them to matter.
4. The full Stage 1 verification suite remains green throughout.
5. The pass ends with a written Phase 9 readiness gate:
   - `Ready to start Phase 9`
   - or `Do not start Phase 9 yet because of X`

## Hardening Units

### H1 — Shared Runtime Contract Freeze

- Goal:
  - explicitly define what `Shared.read()`, `Shared.write()`, and `Shared` value identity mean in Stage 1
- Why this exists:
  - right now the behavior is exercised but still too implicit
- Likely files:
  - `stage1/fuse-runtime/src/shared.rs`
  - `stage1/fuse-runtime/src/value.rs`
  - `stage1/fusec/src/codegen/object_backend.rs`
- Tests to add/run:
  - positive read-after-write
  - multiple read calls on same shared value
  - repeated write-read cycles
- Edge cases:
  - nested data inside `Shared<T>`
  - equality / rendering of values read from shared storage
  - destruction behavior of values still referenced by Shared
- Done when:
  - the Stage 1 `Shared<T>` runtime model is explicit and stable
  - the added tests are green

### H2 — Shared Guard / Ownership Boundary

- Goal:
  - clarify whether Stage 1 models read/write as plain exposed values or as guard-like handles, and make that model consistent
- Why this exists:
  - self-hosting will stress this boundary harder than current fixtures
- Likely files:
  - `stage1/fuse-runtime/src/shared.rs`
  - `stage1/fuse-runtime/src/value.rs`
  - `stage1/fusec/src/codegen/object_backend.rs`
- Tests to add/run:
  - multiple reads on same shared value
  - interleaved read then write
  - mutation visibility after write
- Edge cases:
  - accidental aliasing
  - hidden copies vs shared identity
  - double-release / stale-handle behavior
- Done when:
  - the chosen model is coherent and exercised

### H3 — Shared Dynamic Locking / `try_write`

- Goal:
  - implement and test the guide’s dynamic-lock-order escape hatch
- Why this exists:
  - the guide documents it, but current repo proof does not
- Likely files:
  - `stage1/fuse-runtime/src/shared.rs`
  - `stdlib/full/shared.fuse`
  - checker files only if new diagnostics are required
- Tests to add/run:
  - positive `try_write(timeout)` case
  - timeout/error case
- Edge cases:
  - repeated retries
  - zero/negative timeout policy
  - interaction with existing rank model
- Done when:
  - `try_write` exists, is exercised, and its failure shape is explicit

### H4 — Async + Shared Warning Hardening

- Goal:
  - make the `write_guard_across_await` warning rest on a stronger semantic base than “syntactic pattern found”
- Why this exists:
  - warning quality matters before self-hosting
- Likely files:
  - `stage1/fusec/src/checker/mod.rs`
  - optional `stage1/fusec/src/checker/async_lint.rs`
  - `tests/fuse/full/async/`
- Tests to add/run:
  - current warning contract
  - at least one safe case with no warning
  - optionally a case where read-only access across await is accepted cleanly
- Edge cases:
  - nested `await`
  - multiple shared values with different ranks
  - write-after-await vs write-before-await
- Done when:
  - the warning is both present where expected and absent where it should not fire

### H5 — SIMD Surface Hardening

- Goal:
  - move from one scalar-backed happy path to a clearer Stage 1 SIMD contract
- Why this exists:
  - the guide’s SIMD surface is broader than the current proof
- Likely files:
  - `stage1/fuse-runtime/src/simd.rs`
  - `stage1/fuse-runtime/src/value.rs`
  - `stage1/fusec/src/codegen/object_backend.rs`
  - `stdlib/full/simd.fuse`
- Tests to add/run:
  - current `simd_sum`
  - additional numeric-lane variants if kept in Stage 1 scope
  - unsupported-lane / unsupported-type diagnostics if checker support is added
- Edge cases:
  - empty input
  - non-multiple-of-lane tail handling
  - mixed int/float policy
  - scalar fallback correctness
- Done when:
  - SIMD behavior is broader and explicitly bounded, not merely implied

### H6 — Full Stdlib Exercise Pass

- Goal:
  - ensure important `stdlib/full/*` modules are exercised where they materially affect trust
- Why this exists:
  - parseable artifacts are good, but not enough for confidence
- Likely files:
  - `stdlib/full/shared.fuse`
  - `stdlib/full/timer.fuse`
  - `stdlib/full/simd.fuse`
  - `stdlib/full/http.fuse`
  - `tests/fuse/full/**`
- Tests to add/run:
  - at least one exercised path per important stdlib surface that is claimed usable
- Edge cases:
  - import resolution from user code
  - parseability vs runtime-backed usage
  - failure shape for intentionally shallow stubs
- Done when:
  - the most important Full stdlib surfaces are exercised intentionally, not just present

### H7 — Compiler Stress / Repetition Sanity

- Goal:
  - add a few narrow repeated-operation/compiler-sized sanity tests that are still cheaper than full self-hosting
- Why this exists:
  - self-hosting will amplify any unstable runtime semantics
- Likely files:
  - `tests/fuse/full/**`
  - maybe `tests/fuse/milestone/**`
  - Stage 1 harness tests
- Tests to add/run:
  - repeated Shared mutation
  - repeated channel send/recv cycles
  - repeated SIMD operations on larger input
- Edge cases:
  - growth in temporary values
  - unexpected destructor order regressions
  - stack-size sensitivity
- Done when:
  - the implementation survives repeated-use patterns better than the current minimal fixtures

### H8 — Phase 9 Readiness Gate

- Goal:
  - force an explicit go/no-go decision for self-hosting instead of drifting into it
- Likely files:
  - `.omx/plans/`
  - optional readiness note or ADR
- Tests to run:
  - `cargo check -p fusec`
  - `cargo test` in `stage1`
  - any new hardening-specific tests
- Done when:
  - there is a short, explicit recommendation:
    - `Ready to start Phase 9`
    - or `Do not start Phase 9 yet because of X`

## Risks

- Hardening can sprawl into redesign if each unit is not kept tied to a concrete trust gap.
- SIMD can become an open-ended performance project unless the contract stays explicit.
- Shared/async interaction can turn into a second concurrency phase if the guard model is not bounded carefully.

## Recommendation

Do this hardening pass before Phase 9 if your bar is:

- “usable for serious work”
- “strong enough to bootstrap the compiler with fewer surprises”
- “ready for self-hosting with fewer semantic unknowns”

Skip or shrink it only if your bar is merely:

- “the current documented/tested Stage 1 contract passes”

That is a valid choice, but it is a lower confidence choice, not a stronger one.
