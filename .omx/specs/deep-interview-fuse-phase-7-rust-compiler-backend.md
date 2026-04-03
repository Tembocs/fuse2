# Deep Interview Spec — Fuse Phase 7 Rust Compiler Backend

## Metadata

- Profile: `standard`
- Context type: `brownfield`
- Rounds: `5`
- Final ambiguity: `12%`
- Threshold: `20%`
- Context snapshot: `.omx/context/fuse-phase-7-rust-compiler-backend-20260403T155807Z.md`
- Transcript: `.omx/interviews/fuse-phase-7-rust-compiler-backend-20260403T160739Z.md`

## Clarity Breakdown

| Dimension | Score |
| --- | --- |
| Intent | `80%` |
| Outcome | `92%` |
| Scope | `96%` |
| Constraints | `82%` |
| Success | `90%` |
| Context | `94%` |

## Intent

Advance the repository from the completed Phase 6 Rust frontend into a full Stage 1 Fuse Core native compiler backend, while preserving clean phase boundaries so Phase 8 remains about Fuse Full features rather than unfinished Core backend work.

## Desired Outcome

Implement full documented Phase 7 completion:

- `stage1/fusec` grows from `--check`-only frontend into a native compiler using Cranelift
- `stage1/fuse-runtime/` exists and provides the documented runtime support needed by compiled Fuse Core programs
- `stage1/cranelift-ffi/` exists, builds, and exposes the documented C-callable Cranelift wrapper surface
- every `tests/fuse/core/` program compiles to a native binary and produces output byte-for-byte identical to Stage 0 snapshots
- the documented known pitfalls for Phase 7 are handled or explicitly accounted for in the implementation and verification

## In Scope

- Extend `stage1/Cargo.toml` from a single-crate workspace into the documented multi-crate Stage 1 workspace
- Add `stage1/fusec/src/codegen/` for HIR-to-Cranelift lowering
- Add `stage1/fuse-runtime/` with the runtime support required for compiled Fuse Core programs, including ASAP destruction support and character-aware string operations
- Add `stage1/cranelift-ffi/` with a C-compatible surface for Stage 2 consumers
- Keep `--check` support and add the compile path/CLI needed to satisfy Phase 7
- Compile all `tests/fuse/core/**` programs to native binaries and verify output parity against Stage 0
- Build and verify the callable C surface of `cranelift-ffi`
- Document and test around the known Phase 7 pitfalls called out in the docs

## Out of Scope / Non-goals

- Phase 8 Fuse Full features, including `spawn`, `@rank`, async runtime, channels, shared state, and SIMD
- Stage 2 self-hosting compiler implementation under `stage2/`
- Optional backend experiments such as LLVM or broader optimization work beyond the documented Cranelift backend

## Decision Boundaries

OMX may decide without further confirmation:

- the concrete CLI shape, provided the final result still satisfies full documented Phase 7 behavior
- the internal crate/module layout for `codegen/`, `fuse-runtime/`, and `cranelift-ffi`
- intermediate execution staging and checkpoints, provided the work does not stop short of full documented Phase 7 completion

OMX may not decide without violating the clarified brief:

- to defer unfinished Core backend parity into Phase 8
- to shrink verification below the documented Phase 7 done-when
- to absorb Fuse Full or Stage 2 implementation work into this phase

## Constraints

- Phase 6 is already complete and should remain the verified frontend base
- Cranelift is the intended backend for this phase
- Phase boundaries must stay sharp: Phase 7 completes Fuse Core native compilation; Phase 8 remains Fuse Full
- Verification must include native binary output parity against Stage 0 for the Core corpus
- `cranelift-ffi` must be real and buildable now, even though Stage 2 implementation itself is out of scope

## Testable Acceptance Criteria

1. `stage1/` is a valid Rust workspace containing at minimum `fusec`, `fuse-runtime`, and `cranelift-ffi`.
2. `fusec` can compile checked Fuse Core programs to native binaries, not only run `--check`.
3. Every `tests/fuse/core/` case compiles to a native binary and the produced output matches Stage 0 snapshots byte-for-byte.
4. `tests/fuse/milestone/four_functions.fuse` compiles to a native binary and runs with the expected output as part of the broader proof, not as a substitute for it.
5. `stage1/fusec/src/codegen/` exists and lowers HIR into Cranelift IR with support for the documented Core constructs.
6. `stage1/fuse-runtime/` provides the runtime support required by compiled Core binaries, including ASAP destruction support and character-aware string operations.
7. `stage1/cranelift-ffi/` builds successfully and exposes a callable C-compatible surface for the documented Cranelift operations.
8. The implementation explicitly addresses, guards, or documents the known Phase 7 pitfalls: `mutref_cells` writeback timing, `and`/`or` SSA corruption, UTF-8 indexing, and stack-size requirements.
9. No Phase 8 Fuse Full features or Stage 2 compiler implementation are claimed as completed by this work.

## Assumptions Exposed + Resolutions

- Assumption: Phase 7 might only need minimal native codegen in `fusec`.
  - Resolution: No. Include `fuse-runtime` and `cranelift-ffi` as documented.
- Assumption: A smaller milestone could be enough if the remainder were documented for later.
  - Resolution: No. That would overload and blur Phase 8. Phase 7 should meet its full documented done-when.
- Assumption: CLI and module shape might require repeated confirmation.
  - Resolution: No. OMX may choose CLI shape and internal layout as long as the final outcome still matches full documented Phase 7.

## Pressure-pass Findings

The weaker “milestone-first only” outcome was challenged against the documented phase boundary. That challenge exposed that deferring unfinished Core backend parity into Phase 8 would make the next phase too large and conceptually muddy. The accepted outcome changed from partial Phase 7 delivery to full documented Phase 7 completion.

## Brownfield Evidence vs Inference

### Evidence

- `docs/fuse-implementation-plan-2.md` defines Phase 7 as native Fuse Core code generation via Cranelift and includes deliverables in `fusec/src/codegen/`, `fuse-runtime/`, and `cranelift-ffi/`.
- `docs/fuse-language-guide-2.md` describes Phase 7 as Stage 1 backend work and Phase 8 as Fuse Full.
- `docs/fuse-repository-layout-2.md` documents the broader Stage 1 workspace shape.
- Current `stage1/` contains only the Phase 6 frontend crate and has no `codegen/`, `fuse-runtime/`, or `cranelift-ffi/` tree yet.

### Inference

- The exact CLI flags and internal code organization are not fixed by the docs and can be chosen pragmatically, provided the acceptance boundary is preserved.
- Some runtime and FFI surfaces may need to be introduced incrementally, but they must land within this phase because they are part of the clarified scope.

## Technical Context Findings

- Current `stage1/` is frontend-only:
  - `stage1/Cargo.toml`
  - `stage1/fusec/**`
- No current runtime or Cranelift FFI crates exist.
- Phase 6 artifacts already established that backend/codegen was intentionally deferred until Phase 7.
- The docs call out four implementation hazards that should be treated as design-time requirements, not afterthoughts:
  - `mutref_cells` timing
  - `and`/`or` SSA corruption
  - UTF-8 indexing correctness
  - platform stack-size requirements for compiler workloads

## Condensed Transcript

1. Scope widened to include `fuse-runtime` and `cranelift-ffi` as documented.
2. Success was briefly narrowed to a milestone-first interpretation.
3. A pressure pass revisited that choice and reversed it to full documented Phase 7 completion to avoid overloading Phase 8.
4. OMX was authorized to choose CLI shape, module layout, and internal staging.
5. Fuse Full, Stage 2 self-hosting, and non-Cranelift backend experimentation were made explicit non-goals.

## Execution Bridge

### `$ralplan` (Recommended)

- Input artifact: `.omx/specs/deep-interview-fuse-phase-7-rust-compiler-backend.md`
- Invocation: `$plan --consensus --direct .omx/specs/deep-interview-fuse-phase-7-rust-compiler-backend.md`
- Reason: requirements are clarified, but the execution surface is large enough that architecture, test strategy, and decomposition should be locked before implementation
- Expected output: `prd-*.md` and `test-spec-*.md` for full Phase 7 delivery

### `$autopilot`

- Best when: you want direct planning and execution from this clarified brief
- Constraint: preserve the full documented Phase 7 acceptance boundary

### `$ralph`

- Best when: you want one owner to persist through the full implementation and verification loop until the clarified Phase 7 criteria are met

### `$team`

- Best when: you want coordinated parallel execution, likely split across `fusec` codegen, `fuse-runtime`, `cranelift-ffi`, and verification lanes

### Refine Further

- Re-enter only if you want to tighten acceptance details further before planning
