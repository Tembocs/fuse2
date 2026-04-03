# ADR-012 Stage 0 doc reconciliation and test-runner boundary

## Status
Accepted

## Context
The revised Fuse docs are authoritative, but they mix three different concerns:
- phase gates and deliverables (`docs/fuse-implementation-plan-2.md`)
- language semantics (`docs/fuse-language-guide-2.md`)
- intended repository shape (`docs/fuse-repository-layout-2.md`)

During the Stage 0 bootstrap, two practical gaps had to be resolved:
1. Phase 1 explicitly requires `tests/fuse/full/**` files even though Fuse Full implementation is out of scope for phases 1-5.
2. The Phase 4 `main.py` snippet mentions `--repl`, but the agreed completion boundary is limited to the explicit Phase 1-5 done-when criteria.

## Decision
- Use `docs/fuse-implementation-plan-2.md` as the source of truth for phase gates and required artifacts.
- Use `docs/fuse-language-guide-2.md` as the source of truth for Fuse Core semantics.
- Treat `tests/fuse/full/**` as Phase 1 text artifacts only during Stage 0; they must exist with expected blocks, but the Stage 0 runner executes only `tests/fuse/core/**` plus the milestone.
- Require `main.py` file execution and `--check` support for Stage 0 completion. A full REPL is not a completion gate for phases 1-5 and may remain deferred unless it becomes necessary later.

## Consequences
- Stage 0 verification stays tightly aligned to the revised phase done-when criteria.
- Fuse Full tests remain available as design-contract files without forcing premature implementation.
- The Stage 0 interpreter stays minimal and dependency-free.
