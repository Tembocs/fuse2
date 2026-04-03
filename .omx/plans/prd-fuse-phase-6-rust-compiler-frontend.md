# PRD — Fuse Phase 6 / Stage 1 Rust Compiler Frontend

## Requirements Summary
Implement the Phase 6 Stage 1 frontend described by:
- `docs/fuse-implementation-plan-2.md` as the phase contract
- `docs/fuse-repository-layout-2.md` as the structural target
- `.omx/specs/deep-interview-fuse-phase-6-rust-compiler-frontend.md` as the clarified execution brief
- `.omx/context/fuse-phase-6-rust-compiler-frontend-20260403T105433Z.md` as the grounding snapshot

Delivery creates a new `stage1/` Rust workspace with a `fusec` compiler frontend that lexes, parses, lowers to HIR, and semantically checks Fuse Core source via `--check`.

### Source precedence
When the sources differ, prefer:
1. `.omx/specs/deep-interview-fuse-phase-6-rust-compiler-frontend.md` for clarified scope and non-goals
2. `docs/fuse-implementation-plan-2.md` for Phase 6 deliverables and done-when criteria
3. `docs/fuse-repository-layout-2.md` for file and module shape
4. Existing `stage0/src/*.py` and `tests/fuse/core/**` for behavioral parity

Any material reconciliation should be documented in the PRD/test-spec or nearby implementation notes.

## RALPLAN-DR Summary
### Principles
1. Core-checking fidelity first: preserve Stage 0 observable outcomes for Fuse Core.
2. HIR is mandatory in Phase 6; do not collapse directly from AST into checking.
3. Frontend only: no backend/codegen/native binary work in this task.
4. Source-aware diagnostics are part of the deliverable, not polish.
5. Prefer a small, reviewable Rust workspace with no nonstandard dependencies.

### Decision Drivers
1. The repo already has a working Stage 0 Python reference and a populated test corpus.
2. The deep-interview artifact explicitly narrowed Phase 6 to Fuse Core parity plus a real HIR deliverable.
3. `stage1/` does not exist yet, so the work is a brownfield extension with a new Rust bootstrap.

### Viable Options
#### Option A — Direct Stage 0 translation with Rust-native cleanup (recommended)
Mirror the Stage 0 lexer/parser/checker behavior closely, but use Rust enums/structs, explicit spans, and an AST-to-HIR lowering pass.
- Pros: lowest semantic drift, fastest route to passing Core acceptance tests, naturally supports HIR.
- Cons: can preserve Stage 0 architectural limitations unless deliberately cleaned up.

#### Option B — Fresh Rust frontend designed from docs/tests first
Design a new parser/checker from docs and tests, using Stage 0 only as spot reference.
- Pros: potentially cleaner architecture.
- Cons: much higher drift risk, larger proof burden, slower to reach verified parity.

### Recommendation
Use **Option A**. Translate the proven Stage 0 behavior into Rust, then introduce HIR as the architectural tightening layer required by Phase 6.

## Scope
### In Scope
- Create `stage1/` Cargo workspace and `stage1/fusec` binary crate.
- Implement `src/{lexer,parser,ast,hir,checker}` for Fuse Core syntax and semantics.
- Add source spans and production-grade diagnostics for parse/check failures.
- Support `cargo run --bin fusec -- --check <file.fuse>` as the primary completion path.
- Update repository-side expectations if final Rust diagnostics intentionally differ from current Stage 0 wording.
- Add the minimum Stage 1 test harness or verification scripts needed to prove the acceptance criteria.

### Out of Scope
- Phase 7 backend/codegen/object emission/native binary execution.
- `stage1/fuse-runtime` and `stage1/cranelift-ffi` unless tiny placeholders are strictly required by workspace shape.
- Fuse Full semantics: `@rank`, `spawn` capture rules, async warnings, channels, shared state, async runtime, SIMD, or Full stdlib implementation.
- New dependencies beyond standard Cargo crates already available in the Rust toolchain.

## Acceptance Criteria
1. `stage1/` exists with a valid Cargo workspace and a `stage1/fusec` compiler crate.
2. `stage1/fusec/src/{lexer,parser,ast,hir,checker}` exist and are wired into a functioning `--check` pipeline.
3. `cargo run --bin fusec -- --check <file.fuse>` accepts valid Fuse Core programs and rejects invalid Core programs.
4. HIR exists as a concrete artifact and is used by checking rather than silently deferred.
5. Diagnostics include file/line/column context and are stable enough for repository expectations.
6. The implementation does not claim Phase 7 backend work or Fuse Full semantic coverage.
7. Repository expectations are updated where Rust diagnostics intentionally replace Stage 0-style wording.

## Implementation Steps
1. **Bootstrap Stage 1 workspace**
   - Create `stage1/Cargo.toml`, `stage1/fusec/Cargo.toml`, and `stage1/fusec/src/main.rs`.
   - Keep the workspace intentionally small unless additional crates become necessary.
2. **Port lexical and AST foundations**
   - Define token/span structures and lexer behavior matching Stage 0 tokenization.
   - Define Rust AST nodes for declarations, statements, expressions, and patterns needed by the Core suite.
3. **Port parser**
   - Reproduce the Stage 0 parser coverage for all syntax used in `tests/fuse/core/**`.
   - Preserve module/import parsing because Core tests include module visibility/import cases.
4. **Add HIR lowering**
   - Define HIR nodes with explicit ownership/type annotation slots.
   - Lower AST into HIR before semantic checking.
5. **Port checker behavior**
   - Reproduce Stage 0 Core checks: ownership rules, `val` immutability, `mutref` call-site explicitness, match exhaustiveness, import visibility, and basic type consistency.
   - Emit source-aware Rust diagnostics.
6. **Build verification harness**
   - Add commands/tests to exercise `--check` over valid and invalid Core programs.
   - Update expected diagnostics if wording changes are intentional and stable.
7. **Document explicit non-goals**
   - Keep Full-only functionality absent or clearly unsupported.
   - Avoid adding codegen/backend modules during this phase.

## Risks and Mitigations
- **Stage 0 drift risk:** Python and Rust frontend behavior can diverge subtly.
  Mitigation: verify against `tests/fuse/core/**` rather than docs-only reasoning.
- **HIR overdesign risk:** HIR can become a second AST with no payoff.
  Mitigation: keep HIR narrowly focused on checker-ready representation and spans.
- **Diagnostic churn risk:** richer Rust diagnostics can break existing expectations.
  Mitigation: stabilize the format early and update checked-in expectations once.
- **Scope creep risk:** docs mention Full-facing checker files in the aspirational layout.
  Mitigation: follow the deep-interview scope decision and keep Phase 6 limited to Core parity.

## Verification Steps
1. Build the Stage 1 workspace successfully with Cargo.
2. Run `cargo run --bin fusec -- --check` across valid Core tests and confirm success.
3. Run the same command on invalid Core tests and confirm stable diagnostics with spans.
4. Prove HIR lowering runs on checked inputs.
5. Prove no backend/codegen execution path is required for completion.

## ADR
- **Decision:** Implement Phase 6 as a Rust Core-only frontend with mandatory HIR and explicit source-aware diagnostics.
- **Drivers:** deep-interview scope resolution; Stage 0 reference availability; need to keep Phase 6 separate from Phases 7-8.
- **Alternatives considered:** docs-literal Phase 6 including Full-facing checks; fresh Rust frontend independent of Stage 0.
- **Why chosen:** best balance of correctness, throughput, and phase boundary discipline.
- **Consequences:** some aspirational Stage 1 layout files remain deferred until later phases.
- **Follow-ups:** Phase 7 can extend the workspace into runtime/codegen only after the Core frontend is verified stable.

## Available-Agent-Types Roster
- `executor` — main implementation lane
- `architect` — translation strategy and final architectural sign-off
- `debugger` — parser/checker failure diagnosis
- `test-engineer` — acceptance harness and regression coverage
- `verifier` — completion evidence validation
- `writer` — expectation and docs adjustments if diagnostic wording changes

## Follow-up Staffing Guidance
### Ralph path
- 1x `executor` (high): workspace bootstrap + frontend implementation
- 1x `test-engineer` (medium): Core-suite verification harness and expectation handling
- 1x `architect` (high): checker/HIR boundary review and final sign-off
- 1x `verifier` (high): end-state evidence audit

### Explicit delegation lanes
- Implementation lane: `executor` owns `stage1/` code creation and incremental fixes.
- Evidence lane: `test-engineer` owns Core-suite command shape, invalid-test selection, and expectation updates.
- Final sign-off lane: `architect` then `verifier` review the finished `stage1/` path with fresh evidence.

## Launch Hints
- Ralph: `$ralph .omx/plans/prd-fuse-phase-6-rust-compiler-frontend.md`
- Direct execution should read `.omx/plans/test-spec-fuse-phase-6-rust-compiler-frontend.md` alongside this PRD.

## Team Verification Path
1. Prove the workspace and crate topology exist.
2. Prove lexer/parser/HIR/checker integration through `--check`.
3. Prove valid-vs-invalid Core test behavior with fresh commands.
4. Prove diagnostic stability and scope discipline.
5. Hand the evidence to architect/verifier lanes before claiming completion.
