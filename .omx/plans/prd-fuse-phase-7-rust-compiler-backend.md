# PRD — Fuse Phase 7 / Stage 1 Rust Compiler Backend

## Requirements Summary

Implement the full documented Phase 7 Stage 1 backend described by:

- `docs/fuse-implementation-plan-2.md` as the phase contract
- `docs/fuse-language-guide-2.md` as the language/phase boundary reference
- `docs/fuse-repository-layout-2.md` as the structural target
- `.omx/specs/deep-interview-fuse-phase-7-rust-compiler-backend.md` as the clarified execution brief
- `.omx/context/fuse-phase-7-rust-compiler-backend-20260403T155807Z.md` as the grounding snapshot

Delivery expands the verified Phase 6 frontend into a Stage 1 Rust workspace that can:

- keep the existing `--check` path for Core compile-time failures
- compile valid Fuse Core programs to native binaries via Cranelift
- provide the runtime support those binaries require
- build a real `cranelift-ffi` crate for later Stage 2 consumption

## Source Precedence

When sources differ, prefer:

1. `.omx/specs/deep-interview-fuse-phase-7-rust-compiler-backend.md` for scope, non-goals, and decision boundaries
2. `tests/fuse/core/**` plus `stage0/tests/run_tests.py` for the actual observable contract of the Core corpus
3. `docs/fuse-implementation-plan-2.md` for Phase 7 deliverables and hazards
4. `docs/fuse-language-guide-2.md` for phase boundaries and backend rationale
5. `docs/fuse-repository-layout-2.md` for target crate/module shape

The key reconciliation is intentional: the docs use “all Core tests compile and pass” as shorthand, but the repo's Core corpus already distinguishes `EXPECTED OUTPUT` tests from `EXPECTED ERROR` tests. Phase 7 must preserve that contract rather than force error fixtures into native binaries.

## RALPLAN-DR Summary

### Principles

1. Preserve the existing Core corpus contract rather than oversimplifying it into “everything runs natively.”
2. Keep Phase 7 about Fuse Core native compilation; do not absorb Fuse Full or Stage 2 implementation work.
3. Reuse the verified Phase 6 frontend and add the thinnest backend/runtime/FFI layers that satisfy the full Phase 7 outcome.
4. Treat the documented hazards (`mutref_cells`, SSA short-circuiting, UTF-8 indexing, stack size) as design constraints, not late bugs.
5. Prove correctness by comparing Stage 1 behavior against Stage 0 across the full Core corpus.

### Decision Drivers

1. The clarified spec requires the full documented Phase 7 outcome, not a milestone-only stop.
2. Current `stage1/` has only the frontend crate, so the backend plan must introduce real workspace boundaries and verification harnesses.
3. The Core corpus already provides a grounded split between runtime-output cases and compile-error cases.

### Viable Options

#### Option A — Direct Cranelift backend extension of the current frontend (recommended)

Extend the existing `fusec` crate with codegen, add `fuse-runtime` and `cranelift-ffi`, and build a Stage 1 runner that mirrors the Stage 0 Core contract.

- Pros: lowest semantic drift, reuses verified HIR/checker pipeline, aligns with docs and clarified scope, gives Stage 2 the required FFI surface now
- Cons: larger initial workspace jump, requires careful ABI/runtime design to avoid rework

#### Option B — Literal repo-layout-first expansion with broader placeholder modules

Create most of the documented Stage 1 workspace shape up front, including placeholders that anticipate later Full/runtime work.

- Pros: closer to the long-term directory blueprint
- Cons: high scope-creep risk, weakens the Phase 7/8 boundary, adds review noise without helping the Core backend milestone

#### Option C — Milestone-first backend, defer the full Core parity burden

Implement only enough codegen/runtime to satisfy `tests/fuse/milestone/four_functions.fuse` and a small representative subset, then document the rest for later.

- Pros: faster first executable
- Cons: explicitly rejected during deep-interview because it would overload Phase 8 and blur completion boundaries

### Recommendation

Use **Option A**.

Architect steelman antithesis: Stage 2's FFI bridge appears again in later self-hosting docs, so `cranelift-ffi` could be deferred and Phase 7 could focus on native codegen only.  
Synthesis: implement `cranelift-ffi` now, but keep it deliberately thin and verification-focused; do not let Stage 2 consumer work leak into this phase.

## Scope

### In Scope

- Expand `stage1/Cargo.toml` into a workspace containing at minimum `fusec`, `fuse-runtime`, and `cranelift-ffi`
- Add `stage1/fusec/src/codegen/{mod,cranelift,layout}.rs`
- Add `stage1/fuse-runtime/src/` for the runtime surface required by compiled Core binaries
- Add `stage1/cranelift-ffi/src/lib.rs` with a C-compatible wrapper surface
- Preserve `--check` for `EXPECTED ERROR` Core cases
- Add compile mode and binary emission for `EXPECTED OUTPUT` Core cases
- Build a Stage 1 test harness that mirrors `stage0/tests/run_tests.py` semantics:
  - `EXPECTED ERROR` cases use checking and compare diagnostics
  - `EXPECTED OUTPUT` cases compile, run the produced binary, and compare stdout
- Address or explicitly guard the known backend pitfalls documented in Phase 7

### Out of Scope

- Phase 8 Fuse Full features: `spawn`, `@rank`, async runtime, channels, shared state, SIMD
- Stage 2 compiler implementation under `stage2/`
- Optional non-Cranelift backend work such as LLVM
- Optimization work beyond what is needed for correct Phase 7 completion

## Crate / Boundary Plan

### `stage1/fusec`

- Owns CLI, frontend reuse, HIR-to-backend lowering, object emission orchestration, and linker invocation
- Continues to own parse/check diagnostics for error cases
- Adds compile path alongside `--check`

### `stage1/fuse-runtime`

- Owns the runtime value contract used by generated code
- Provides builtins and support routines needed by compiled Core programs
- Includes ASAP destruction helpers and character-aware string operations
- Builds as the runtime library linked by Stage 1-generated binaries in this phase; do not over-shape it around later Stage 2 or Fuse Full concerns
- Avoids Phase 8 runtime surfaces (`chan`, `shared`, async executor, SIMD)

### `stage1/cranelift-ffi`

- Owns a thin C ABI around Cranelift operations needed by later Stage 2 work
- Must build now and be smoke-tested from C-compatible entry points
- Must stay low-level and wrapper-oriented; do not duplicate the full `fusec` backend pipeline behind the FFI surface
- Must not pull Stage 2 implementation into this phase

## Critical Design Decisions

1. **Link/runtime boundary**
   - Keep code generation, object emission, and linker orchestration in `fusec`
   - Keep reusable runtime behavior in `fuse-runtime`
   - Isolate platform linker and stack-size handling behind a narrow `fusec` boundary so the local platform path can be verified without scattering toolchain logic across codegen

2. **Runtime packaging**
   - Treat `fuse-runtime` as the library linked into Stage 1-generated binaries now
   - Expose only the runtime support actually required by the current Core corpus in Phase 7
   - Defer Fuse Full runtime surface growth to Phase 8

3. **FFI proof shape**
   - Prove `cranelift-ffi` is callable through exported `extern "C"` symbols in-repo
   - Prefer a smoke test that invokes the exported ABI from a separate caller boundary without depending on Stage 2 implementation
   - Use a minimal native C smoke caller only if the local toolchain is available; otherwise keep the proof at the ABI/symbol-call boundary inside the Rust workspace

## Acceptance Criteria

1. `stage1/` is a valid workspace containing `fusec`, `fuse-runtime`, and `cranelift-ffi`.
2. `fusec` still supports `--check` for error-path verification and adds a compile mode for valid Core programs.
3. Every Core fixture is satisfied under the Stage 0 contract:
   - `EXPECTED ERROR` fixtures fail in checking/compile validation with stable expected diagnostics
   - `EXPECTED OUTPUT` fixtures compile to native binaries whose stdout matches checked-in expectations byte-for-byte
4. `tests/fuse/milestone/four_functions.fuse` compiles to a native binary and runs with the expected output.
5. `fuse-runtime` exists and provides the runtime functions needed by the valid Core corpus, including ASAP-destruction support and character-aware string operations.
6. `cranelift-ffi` builds successfully and exposes a smoke-tested C-compatible surface.
7. The known hazards are accounted for in code and/or targeted verification:
   - `mutref_cells` writeback timing
   - `and`/`or` SSA corruption
   - UTF-8 indexing correctness
   - platform stack-size requirements
8. No Phase 8 Fuse Full or Stage 2 implementation claims are made.

## Implementation Plan

1. **Workspace Expansion**
   - Update `stage1/Cargo.toml` to include `fuse-runtime` and `cranelift-ffi`
   - Add crate manifests and minimal roots
   - Decide the compile CLI shape while preserving `--check`

2. **Runtime Contract First**
   - Define the Stage 1 runtime value/ABI model in `fuse-runtime`
   - Implement only the runtime surface needed by current Core fixtures
   - Add string and destruction helpers with hazard-driven tests

3. **Backend Skeleton**
   - Add `codegen/` in `fusec`
   - Define layout/ABI decisions in `layout.rs`
   - Wire Cranelift module creation, function lowering, object emission, and linker invocation

4. **HIR-to-Native Coverage**
   - Lower valid Core HIR forms to native code
   - Insert runtime calls for builtins, data/value handling, and destruction
   - Handle loops, control flow, `match`, calls, ownership transitions, and `defer`

5. **FFI Surface**
   - Implement the thin `cranelift-ffi` wrapper crate
   - Keep the API minimal and C-compatible
   - Add a smoke test proving the surface is callable

6. **Verification Harness**
   - Add a Stage 1 runner mirroring `stage0/tests/run_tests.py`
   - Route `EXPECTED ERROR` cases through `--check`/compile validation
   - Route `EXPECTED OUTPUT` cases through compile-run parity checks
   - Add targeted hazard regressions for the four documented pitfalls

## Risks and Mitigations

- **Runtime/ABI overdesign**
  - Mitigation: implement only the value/builtin surface required by the Core corpus; defer Fuse Full runtime abstractions
- **Corpus-contract mismatch**
  - Mitigation: mirror the Stage 0 harness semantics exactly instead of inventing a new universal compile contract
- **SSA corruption in short-circuit lowering**
  - Mitigation: add dedicated regression coverage around loops + `mutref` + `and`/`or`; document any temporary restrictions if required
- **`mutref_cells` writeback bugs**
  - Mitigation: make mutref cell allocation a prologue responsibility and verify explicit `return` paths
- **UTF-8 indexing bugs**
  - Mitigation: centralize string operations in runtime helpers and test multi-byte scenarios directly
- **Platform-link variance**
  - Mitigation: isolate linker/stack-size logic behind a small boundary and verify at least the local platform path explicitly
- **`cranelift-ffi` verification drift**
  - Mitigation: define the callable-surface proof up front and keep the wrapper thin so the crate is buildable and testable without depending on Stage 2

## Verification Steps

1. Build the expanded Stage 1 workspace successfully with Cargo.
2. Prove `fusec --check` still satisfies the Core error fixtures.
3. Compile and run every Core output fixture natively, comparing stdout to Stage 0 expectations.
4. Compile and run `tests/fuse/milestone/four_functions.fuse`.
5. Build `cranelift-ffi` and run a callable-surface smoke test.
6. Run targeted hazard regressions for:
   - mutref writeback through explicit returns
   - `and`/`or` lowering in loop-heavy code
   - UTF-8 indexing
   - stack-size-sensitive compiler workload path

## ADR

- **Decision:** Extend the verified Phase 6 frontend directly into a full Phase 7 Cranelift backend with `fuse-runtime` and `cranelift-ffi`, while preserving the Stage 0 Core corpus contract.
- **Drivers:** clarified full-scope requirement; current frontend-only Stage 1 state; need for real runtime/FFI surfaces without leaking into Phase 8 or Stage 2
- **Alternatives considered:** literal repo-layout-first placeholder expansion; milestone-first partial backend
- **Why chosen:** best alignment with the clarified brief, the docs, and the existing test corpus while keeping the phase boundary sharp
- **Consequences:** the initial implementation is larger, but acceptance is unambiguous and Phase 8 remains cleanly scoped
- **Follow-ups:** after Phase 7, Phase 8 can extend runtime/checker behavior for Fuse Full without reopening Core backend completion

## Available-Agent-Types Roster

- `architect`: crate boundaries, ABI choices, linker/runtime interfaces
- `executor`: implementation work across `fusec`, `fuse-runtime`, and `cranelift-ffi`
- `debugger`: backend failures, linker/runtime crashes, SSA/writeback issues
- `test-engineer`: corpus harness, parity checks, pitfall regressions
- `verifier`: completion evidence across compile/check/run/ffi paths

## Staffing Guidance

### For `$ralph`

- Use one persistent owner with `high` reasoning
- Keep the loop ordered:
  1. workspace/runtime scaffolding
  2. backend lowering
  3. FFI surface
  4. corpus harness
  5. hazard hardening
  6. final verification

### For `$team`

- Lane 1: `executor` (`high`) on `stage1/fuse-runtime/**`
- Lane 2: `executor` (`high`) on `stage1/fusec/src/codegen/**` and CLI compile path
- Lane 3: `executor` (`medium` or `high`) on `stage1/cranelift-ffi/**`
- Lane 4: `test-engineer` (`medium`) on corpus harness + hazard regressions
- Lane 5: `verifier` (`high`) on parity evidence and acceptance closure

## Launch Hints

- Ralph: `$ralph .omx/plans/prd-fuse-phase-7-rust-compiler-backend.md`
- Team: `$team .omx/plans/prd-fuse-phase-7-rust-compiler-backend.md`
- Team CLI: `omx team .omx/plans/prd-fuse-phase-7-rust-compiler-backend.md`
- Direct executors should also read `.omx/plans/test-spec-fuse-phase-7-rust-compiler-backend.md`

## Team Verification Path

1. Runtime lane proves the minimal runtime surface for current Core fixtures.
2. Codegen lane proves compile + link on milestone and representative fixtures before full-corpus rollout.
3. FFI lane proves `cranelift-ffi` builds and exposes callable symbols.
4. Test lane runs the Stage 1 corpus harness against every Core fixture.
5. Verifier lane signs off only when all PRD acceptance criteria are met with concrete output evidence.
