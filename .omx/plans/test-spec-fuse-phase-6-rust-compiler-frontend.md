# Test Spec — Fuse Phase 6 / Stage 1 Rust Compiler Frontend

## Purpose
Define the evidence required to prove the Stage 1 frontend is complete for the Phase 6 delivery described in `.omx/plans/prd-fuse-phase-6-rust-compiler-frontend.md`.

## Sources of Truth
- `.omx/specs/deep-interview-fuse-phase-6-rust-compiler-frontend.md`
- `docs/fuse-implementation-plan-2.md`
- `docs/fuse-repository-layout-2.md`
- `stage0/src/*.py`
- `tests/fuse/core/**`

## Verification Matrix

### Workspace Bootstrap
- Verify `stage1/` exists with a Cargo workspace manifest.
- Verify `stage1/fusec/` exists with a binary crate manifest.
- Verify `stage1/fusec/src/{lexer,parser,ast,hir,checker}` exist and are compiled into the binary.

### Lexer + Parser
- Run `cargo run --bin fusec -- --check <file>` on representative valid Core files from:
  - `tests/fuse/core/control_flow/`
  - `tests/fuse/core/errors/`
  - `tests/fuse/core/memory/`
  - `tests/fuse/core/modules/`
  - `tests/fuse/core/ownership/`
  - `tests/fuse/core/types/`
- Expected result: parse succeeds and the command exits successfully for valid files.
- Parser coverage must include imports/modules, control flow, match/when, ownership syntax, and extension-function syntax exercised by the Core suite.

### HIR
- Verify the AST is lowered into HIR before semantic analysis.
- Expected result: at least one direct test or command proves HIR lowering runs on valid input.
- HIR must preserve source spans used by diagnostics.

### Checker
- Run `cargo run --bin fusec -- --check <file>` on invalid Core programs, including at minimum:
  - `tests/fuse/core/ownership/move_prevents_reuse.fuse`
  - `tests/fuse/core/errors/match_missing_arm.fuse`
  - `tests/fuse/core/modules/import_pub_only.fuse`
  - `tests/fuse/core/types/val_immutable.fuse` if it is an invalid test in the current corpus, otherwise another invalid immutability case
- Expected result: command fails with source-aware diagnostics containing file, line, and column.
- Specifically verify: move-after-use rejection, `mutref` call-site explicitness, `ref` assignment restrictions, match exhaustiveness, import visibility, and basic type mismatch behavior.

### Diagnostics Contract
- Verify emitted diagnostics are stable and intentional.
- If Rust diagnostics differ from current Stage 0-oriented wording, update checked-in expectations or test comments that serve as the repository contract.
- Expected result: no known mismatch remains between the accepted diagnostic contract and the emitted output.

### Negative Scope Checks
- Verify completion does not rely on backend/codegen/native binary output.
- Verify Full-only tests under `tests/fuse/full/**` are not claimed as passing evidence for this task.
- Verify the implementation does not present `@rank`, `spawn`, async warnings, channels, shared state, or SIMD as completed semantics.

## Evidence Checklist
- [ ] Tree listing or file existence proof for `stage1/` workspace and `fusec` crate
- [ ] Successful Cargo build output
- [ ] Successful `--check` runs for representative valid Core tests
- [ ] Failing `--check` runs for representative invalid Core tests with spans
- [ ] Proof that HIR lowering executes before checking
- [ ] Diagnostics contract evidence or expectation updates
- [ ] Explicit statement that backend/codegen and Full-only semantics remain out of scope

## Exit Criteria
The task is complete only when every PRD acceptance criterion and every checklist item above is satisfied with concrete verification evidence.
