# Draft PRD — Stage 3 Tooling Platform

## Purpose

Define the post-self-hosting tooling stage for Fuse. Stage 3 exists **after** Stage 2/self-hosting and turns the compiler into a stable platform for editors, agents, and machine-to-machine tooling.

## Stage Placement

Recommended ordering:

1. Stage 1 — Rust compiler
2. Stage 2 — self-hosting compiler
3. Stage 3 — tooling platform

Within Stage 3:

1. MCP
2. LSP

## Why MCP Before LSP

- MCP forces stable compiler/query/service boundaries.
- LSP naturally wants those same diagnostics/symbol/query APIs.
- If LSP is built first, it tends to hardcode logic that should have been shared tooling services.

So MCP first is the cleaner dependency order.

## In Scope

- stable machine-usable compiler services
- stable editor-facing language services
- structured diagnostics, symbol, semantic, and build queries
- tooling packaging/release boundaries where needed

## Out of Scope

- language redesign
- self-hosting implementation itself
- package manager / registry design
- unrelated UX ideas not tied to concrete tooling capability

## Stage 3A — MCP

### Goal

Expose structured compiler and project capabilities through a stable machine-consumable interface suitable for automation, agents, and external tools.

### MCP Units

#### M1 — Compiler Service Boundary

- Goal:
  - define the internal service boundaries tooling can call
- Required service areas:
  - parse
  - check
  - compile
  - symbol inventory
  - module/project graph
- Edge cases:
  - broken files
  - warnings + errors together
  - out-of-workspace paths
- Done when:
  - the internal service inventory is explicit and bounded

#### M2 — Structured Diagnostics API

- Goal:
  - return diagnostics as machine-readable structured data, not only rendered strings
- Edge cases:
  - multiple diagnostics per file
  - mixed severity
  - no-diagnostic success case
- Done when:
  - external tooling can request diagnostics reliably

#### M3 — Symbol / Definition / Reference API

- Goal:
  - expose symbols and basic semantic navigation through stable queries
- Edge cases:
  - imported symbols
  - extension methods
  - stdlib/builtin surfaces
- Done when:
  - tooling can request symbol/navigation data without shell scraping

#### M4 — Project Graph / Module Resolution API

- Goal:
  - expose the import/module graph and resolution behavior
- Edge cases:
  - stdlib imports
  - missing imports
  - ambiguous module paths
- Done when:
  - tooling can inspect workspace/module structure directly

#### M5 — Build / Package / Run API

- Goal:
  - expose bounded build/package/run surfaces to tooling
- Edge cases:
  - output paths
  - packaged compiler root discovery
  - sandbox/destructive path control
- Done when:
  - tooling can drive bounded compiler operations through a stable API

#### M6 — MCP Stability / Release Gate

- Goal:
  - freeze and version the supported MCP capability surface
- Done when:
  - the MCP contract is explicit enough for downstream integration

## Stage 3B — LSP

### Goal

Expose Fuse through the Language Server Protocol for diagnostics, navigation, hover, rename, completion, and semantic tooling in editors/IDEs.

### LSP Units

#### L1 — LSP Server Skeleton

- Goal:
  - implement transport/session/document lifecycle
- Capabilities:
  - initialize
  - shutdown
  - open/change/close document
- Edge cases:
  - unsaved buffers
  - invalid syntax
  - invalid UTF-8
- Done when:
  - an editor can maintain a Fuse document session

#### L2 — Real-Time Diagnostics

- Goal:
  - map compiler diagnostics to LSP diagnostics
- Edge cases:
  - warnings vs errors
  - repeated edits
  - multi-file diagnostics
- Done when:
  - editor diagnostics match compiler diagnostics reliably

#### L3 — Hover / Definition / References

- Goal:
  - provide semantic navigation backed by stable compiler services
- Edge cases:
  - imported symbols
  - extension methods
  - stdlib/builtin names
- Done when:
  - common navigation operations work on real Fuse projects

#### L4 — Rename / Semantic Tokens / Document Symbols

- Goal:
  - provide richer semantic editing support
- Edge cases:
  - cross-file rename safety
  - generated/builtin names
  - partial parse failure
- Done when:
  - the LSP is useful for real editing, not just diagnostics

#### L5 — Completion / Signature Help

- Goal:
  - provide authoring support with real context awareness
- Edge cases:
  - incomplete expressions
  - generic syntax
  - extension members
- Done when:
  - completions/signatures are stable enough to trust

#### L6 — Workspace Scale / Indexing Hardening

- Goal:
  - ensure the LSP stays responsive and correct on larger projects
- Edge cases:
  - whole-workspace indexing
  - invalidation after edits
  - memory/performance ceilings
- Done when:
  - the LSP is stable enough for sustained real-world use

## Acceptance Criteria

### MCP Acceptance

1. Tooling can request structured diagnostics.
2. Tooling can request symbols/definitions/references/project structure.
3. Tooling can invoke bounded compile/package/run actions.
4. The supported MCP capability surface is documented and versionable.

### LSP Acceptance

1. Editors can maintain Fuse document sessions.
2. Diagnostics are mapped accurately and in real time.
3. Navigation works on real Fuse projects.
4. Richer editor features are stable enough to be useful, not merely demonstrative.

## Risks

- If Stage 2 internals churn too much, both MCP and LSP become expensive to maintain.
- If MCP is skipped and LSP goes first, the editor server will likely hardcode logic that should have been shared compiler services.
- If Stage 3 starts before post-self-hosting stabilization, tooling churn may outpace compiler maturity.

## Recommendation

Reserve Stage 3 now as:

- **Stage 3A — MCP**
- **Stage 3B — LSP**

but start it only after:

1. Stage 2/self-hosting is complete
2. any necessary post-bootstrap stabilization is complete
3. the compiler/query surfaces are stable enough to expose without constant churn
