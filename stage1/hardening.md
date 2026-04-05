# Stage 1 Hardening Plan

> This document tracks the Phase 8 hardening pass for the Stage 1 Fuse Full compiler.
> It is a pre-Phase-9 gate — its purpose is to close edge-case gaps and build trust
> before self-hosting begins.
>
> **Philosophy reminder:** Fuse is not a research language. It is designed to be
> implemented, self-hosted, and used to build production systems. Every feature has
> been proven in production at scale. Fuse does not experiment — it integrates.
> The three non-negotiable properties are: memory safety without GC, concurrency
> safety without a borrow checker, and developer experience as a first-class concern.
>
> The authoritative spec is `docs/fuse-language-guide-2.md`. All work must conform
> to the language guide. If the guide says it, we implement it. If the guide does not
> say it, we do not invent it.

---

## Status Key

- `[ ]` — not started
- `[~]` — in progress
- `[x]` — done
- `[!]` — blocked

---

## Wave 1 — Shared<T> Foundation (H1 + H2)

> **Before starting Wave 1:** Re-read sections 1.17 (Concurrency) and 1.10 (Memory Model)
> of `docs/fuse-language-guide-2.md`. Re-read ADR-004 (rank mandatory) and ADR-005
> (deadlock three-tiers). Read the current implementations:
> `stage1/fuse-runtime/src/shared.rs`, `stage1/fuse-runtime/src/value.rs`,
> `stage1/fusec/src/codegen/object_backend.rs`, and all existing Shared tests in
> `tests/fuse/full/concurrency/`.
>
> **Do not present half-cooked work and give excuses. Every task must be complete,
> tested, and green before it is marked done. After this wave completes, stop and
> let the user decide before proceeding.**

### H1 — Shared Runtime Contract Freeze

Goal: Explicitly define what `Shared.read()`, `Shared.write()`, and `Shared` value
identity mean in Stage 1, and prove it with tests.

- [x] **H1.1** Audit current `Shared<T>` runtime: document that `read()` and `write()`
      are currently identical (both return inner handle). Identify what must change
      so that `write()` is semantically distinct.

- [x] **H1.2** Make `write()` semantically distinct from `read()` in the runtime.
      `read()` returns an immutable view. `write()` returns a mutable view. The
      distinction must be observable (the checker already enforces ref vs mutref at
      call sites — the runtime should not contradict that).

- [x] **H1.3** Add test: `shared_read_after_write.fuse` — write a value into Shared,
      then read it back and verify the value is correct.

- [x] **H1.4** Add test: `shared_multiple_reads.fuse` — call `.read()` multiple times
      on the same Shared value and verify each read returns the same value.

- [x] **H1.5** Add test: `shared_write_read_cycles.fuse` — write, read, write again,
      read again. Verify mutation visibility across cycles.

- [x] **H1.6** Add test: `shared_nested_data.fuse` ��� wrap a data class (not just Int)
      inside `Shared<T>`. Read and verify field access works through shared storage.

- [x] **H1.7** Add test: `shared_destruction.fuse` — verify ASAP destruction of values
      inside Shared. The `__del__` of the inner value must fire when the Shared
      wrapper is destroyed.

- [x] **H1.8** Run `cargo test` — all existing + new tests green.

### H2 — Shared Guard / Ownership Boundary

Goal: Clarify whether Stage 1 models read/write as plain exposed values or as
guard-like handles, and make that model consistent and tested.

- [x] **H2.1** Decide and document the Stage 1 model: plain-value-with-rank-checked-access
      (no real RwLock) is the expected answer for single-threaded Stage 1. Document
      this explicitly in a comment block at the top of `shared.rs`.

- [x] **H2.2** Add test: `shared_read_then_write.fuse` — interleaved read then write
      on the same Shared value. Verify the read value is not corrupted by the
      subsequent write.

- [x] **H2.3** Add test: `shared_identity.fuse` — verify that values read from Shared
      can be compared, printed, and used in expressions like any other value.

- [x] **H2.4** Verify no accidental aliasing: if a value is read from Shared and then
      the Shared is written to, the previously read value must not change (copy
      semantics, not reference aliasing). Add test: `shared_no_aliasing.fuse`.

- [x] **H2.5** Add test: `shared_value_rendering.fuse` — print a value read from
      Shared. Verify the output matches what was written.

- [x] **H2.6** Run `cargo test` — all existing + new tests green.

---

## Wave 2 — Deepen Specific Features (H3, H4, H5)

> **Before starting Wave 2:** Re-read sections 1.17 (Tier 3 — try_write), 1.18
> (Async), and 1.19 (SIMD) of `docs/fuse-language-guide-2.md`. Re-read the current
> implementations: `stage1/fusec/src/checker/mod.rs` (async/shared warnings),
> `stage1/fuse-runtime/src/simd.rs`, `stage1/fuse-runtime/src/value.rs` (SIMD parts),
> `stage1/fusec/src/codegen/object_backend.rs` (SIMD codegen).
>
> **Do not present half-cooked work and give excuses. Every task must be complete,
> tested, and green before it is marked done. After this wave completes, stop and
> let the user decide before proceeding.**

### H3 — Shared Dynamic Locking / `try_write`

Goal: Implement and test the guide's dynamic-lock-order escape hatch (`try_write`
with timeout).

- [x] **H3.1** Implement `fuse_shared_try_write()` in the runtime (`value.rs` /
      `shared.rs`). It takes a timeout value and returns `Result<T, String>`.
      In single-threaded Stage 1, the lock is always available — the positive case
      succeeds immediately. The timeout/error path must also be exercisable.

- [x] **H3.2** Wire `try_write` codegen in `object_backend.rs`. The method call
      `shared.try_write(timeout)` must lower to the runtime function.

- [x] **H3.3** Update the checker to track `try_write` in rank analysis if needed
      (the checker already mentions `try_write` in `held_rank_from_expr` — verify
      it works correctly).

- [x] **H3.4** Add test: `shared_try_write_success.fuse` — positive case where
      `try_write` succeeds and the value is accessible.

- [x] **H3.5** Add test: `shared_try_write_timeout.fuse` — simulate or force the
      timeout/error path. Verify `Err(...)` is returned and the caller handles it.

- [x] **H3.6** Verify interaction with rank model: `try_write` on a ranked Shared
      must still respect rank ordering or explicitly bypass it (per guide: Tier 3
      is the dynamic escape hatch). Document the decision.

- [x] **H3.7** Run `cargo test` — all existing + new tests green.

### H4 — Async + Shared Warning Hardening

Goal: Make the `write_guard_across_await` warning rest on a stronger semantic base
than "any held rank in scope fires the warning."

- [x] **H4.1** Audit current warning logic in `checker/mod.rs`. Document exactly when
      it fires: currently fires for ANY held rank (read or write) across await.

- [x] **H4.2** Fix the warning to distinguish read vs write guards. Per the language
      guide (1.17 Rules): "Write guard held across `await` produces a compile
      warning." Read guards across await should NOT produce a warning (the task is
      single-threaded; read guards are safe to hold).

- [x] **H4.3** Update existing test: `write_guard_across_await.fuse` — verify it still
      warns for write guards.

- [x] **H4.4** Add test: `read_guard_across_await.fuse` — hold a `.read()` result
      across an `await`. Verify NO warning is emitted.

- [x] **H4.5** Add test: `nested_await_write_guard.fuse` — write guard held across
      a nested await (await inside a block or conditional). Verify warning fires.

- [x] **H4.6** Add test: `multiple_shared_ranks_await.fuse` — multiple Shared values
      with different ranks, only one held as write across await. Verify warning
      fires for the write guard only.

- [x] **H4.7** Run `cargo test` — all existing + new tests green.

### H5 — SIMD Surface Hardening

Goal: Move from one scalar-backed happy path to a clearer Stage 1 SIMD contract
with type/lane validation and broader test coverage.

- [x] **H5.1** Add type parameter validation in the checker or codegen: `T` must be
      one of `Float32`, `Float64`, `Int32`, `Int64`. Other types (e.g., `String`)
      must produce a compile error.

- [x] **H5.2** Add lane count validation: `N` must be a power of 2 in {2, 4, 8, 16}.
      Other values must produce a compile error.

- [x] **H5.3** Fix return type inference: `SIMD::<Float32, N>.sum(...)` must return
      `Float`, not `Int`. The codegen currently hardcodes `Int` — fix it to respect
      the type parameter.

- [x] **H5.4** Add test: `simd_sum_float.fuse` — sum using `SIMD::<Float64, 4>.sum(...)`.
      Verifies Float64 type parameter is accepted. (Note: float literals are not yet
      supported in the backend; test uses integer values with Float64 type param.)

- [x] **H5.5** Add test: `simd_sum_empty.fuse` — sum an empty list. Verify the result
      is 0 (or 0.0 for float).

- [x] **H5.6** Add test: `simd_sum_large.fuse` — sum a list larger than the lane count.
      Verify correct result including tail handling.

- [x] **H5.7** Add test: `simd_invalid_type.fuse` — use `SIMD::<String, 4>`. Verify
      compile error.

- [x] **H5.8** Add test: `simd_invalid_lane.fuse` — use `SIMD::<Int32, 3>`. Verify
      compile error.

- [x] **H5.9** Run `cargo test` — all existing + new tests green.

---

## Wave 3 — Integration Confidence (H6, H7, H8)

> **Before starting Wave 3:** Re-read the full test suite in `tests/fuse/full/`.
> Re-read `stdlib/full/*.fuse` to understand what surfaces are claimed. Re-read
> `stage1/fuse-runtime/src/chan.rs` for channel internals. Review the overall
> project philosophy in section 1.1 of the language guide.
>
> **Do not present half-cooked work and give excuses. Every task must be complete,
> tested, and green before it is marked done. After this wave completes, stop and
> let the user decide before proceeding.**

### H6 — Full Stdlib Exercise Pass

Goal: Ensure important `stdlib/full/*` modules are exercised where they materially
affect trust — not just parseable surfaces.

- [x] **H6.1** Audit each `stdlib/full/*.fuse` module. For each, determine: is it
      already exercised by a runtime-backed test, or is it just a parseable stub?

- [x] **H6.2** `chan.fuse` — already exercised by `chan_basic.fuse` and
      `chan_bounded_backpressure.fuse`. Verify tests are sufficient. Add edge case
      test if gaps found.

- [x] **H6.3** `shared.fuse` — already exercised by Wave 1 + Wave 2 tests. Verify
      the stdlib surface matches what the runtime actually provides. Update the
      stub signatures if they have drifted from the implementation.

- [x] **H6.4** `simd.fuse` — already exercised by Wave 2 SIMD tests. Verify the
      stdlib surface matches the implementation. Update stub signatures.

- [x] **H6.5** `timer.fuse` — currently not exercised. Add test:
      `timer_basic.fuse` — create a `Timeout.ms(N)` value and verify it is usable
      (at minimum: construct and print). If `Timer.sleep` is runtime-backed, test
      it. If it is a stub only, document that explicitly.

- [x] **H6.6** `http.fuse` — currently returns `Err("not implemented")`. This is
      acceptable for pre-self-hosting. Document it as an intentional stub. No
      runtime test needed — self-hosting does not require HTTP.

- [x] **H6.7** Run `cargo test` — all existing + new tests green.

### H7 — Compiler Stress / Repetition Sanity

Goal: Add narrow repeated-operation tests that stress runtime semantics harder than
the current minimal fixtures, without becoming a full benchmark suite.

- [x] **H7.1** Add test: `shared_repeated_mutation.fuse` — write to a Shared value
      in a loop (e.g., 100 iterations). Read the final value. Verify correctness
      and no destructor-order regressions.

- [x] **H7.2** Add test: `chan_repeated_send_recv.fuse` — send and receive N values
      through a channel in a loop. Verify all values arrive in order and none are
      lost.

- [x] **H7.3** Add test: `simd_repeated_sum.fuse` — call `SIMD.sum` in a loop on
      different input lists. Verify each result is correct.

- [x] **H7.4** Add test: `stress_destructor_order.fuse` — create multiple data class
      instances with `__del__`, use them through Shared/channel/SIMD paths, and
      verify destruction order is deterministic and correct.

- [x] **H7.5** Run `cargo test` — all existing + new tests green.

### H8 — Phase 9 Readiness Gate

Goal: Force an explicit go/no-go decision for self-hosting.

- [x] **H8.1** Run `cargo check -p fusec` — clean, no warnings.

- [x] **H8.2** Run `cargo test` in `stage1/` — all tests green.

- [x] **H8.3** Review all new hardening tests — verify each one exercises the
      behavior it claims to exercise, not a trivial pass-through.

- [x] **H8.4** Review the language guide sections 1.17, 1.18, 1.19 one final time.
      List any remaining gaps between the guide and the implementation.

- [x] **H8.5** Write the Phase 9 readiness verdict:
      - `Ready to start Phase 9` — with confidence notes
      - or `Do not start Phase 9 yet because of X` — with specific blockers

- [x] **H8.6** Place the verdict in `.omx/plans/phase-9-readiness.md`.

---

## Completion Summary

| Wave | Units | Tasks | Status |
|------|-------|-------|--------|
| 1    | H1, H2 | 14 | **done** |
| 2    | H3, H4, H5 | 23 | **done** |
| 3    | H6, H7, H8 | 17 | **done** |
| **Total** | **8** | **54** | **done** |
