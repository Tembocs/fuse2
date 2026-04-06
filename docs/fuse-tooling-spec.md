# Fuse — Tooling & Platform Specification

> **Status:** Design complete. Implementation not started.
> **Prerequisite:** Stage 2 (self-hosting) complete for package manager.
>   Cross-compilation complete for mobile support.
> **Scope:** 3 major areas — manifest file, mobile support, master sequencing.
> **Authority:** This document is the authoritative specification for
> Fuse's project manifest (`fuel.toml`), mobile platform strategy, and
> overall project sequencing.
>
> **This document captures decisions that are sequenced AFTER the
> hardening plan and Stage 2.** It is not actionable until those
> prerequisites are met. Its purpose is to prevent re-design and
> ensure alignment when the time comes.

---

## Mandatory Rules

> **Before starting any implementation from this document, re-read:**
>
> 1. The **Language Philosophy** section in `docs/fuse-pre-stage2.md`
> 2. The **Mandatory Rules** section in `docs/fuse-pre-stage2.md`
> 3. `docs/fuse-pre-stage2.md` — unified pre-Stage 2 plan
> 4. `docs/fuse-language-guide-2.md` — the authoritative language specification
>
> All rules from the hardening plan apply here without exception.

---

# Part I — Manifest File: `fuel.toml`

## 1. Design Decision

Fuse uses a project manifest file named `fuel.toml` as the single
source of truth for project configuration. This replaces `@entrypoint`
annotations and provides the foundation for the package manager.

**Name rationale:** Fuse + Fuel — natural pairing. Short, unique, not
used by any major tool.

**Entry point rule:** Every binary has `fn main()` as its entry
function. No `@entrypoint` annotation. The manifest specifies *which
file* contains `main`, not the function name.

---

## 2. Design Principles

| # | Principle | Rationale |
|---|---|---|
| 1 | **TOML only, never executable** | Prevents supply-chain attacks. No `build.rs` / `setup.py` equivalent. |
| 2 | **One file, one format** | Everything in `fuel.toml`, lockfile in `fuel.lock`. No split across multiple config files. |
| 3 | **Lockfile from day one** | `fuel.lock` committed to repo. Exact reproducible builds. |
| 4 | **Namespaced packages** | `@author/package` prevents name squatting. |
| 5 | **Global cache** | Dependencies cached in `~/.fuse/cache/`, shared across projects. |
| 6 | **Semver resolution (newest compatible)** | Not Go's MVS. Pick the newest version satisfying all constraints. |
| 7 | **Fail loudly on conflicts** | Diamond dependency conflicts are errors, not warnings. No silent version resolution. |
| 8 | **`dev-dependencies` from the start** | Test and benchmark deps never leak to consumers. |
| 9 | **No editions unless unavoidable** | One language, one syntax. Deprecation over breakage. |
| 10 | **Built-in CLI** | `fuse init`, `fuse build`, `fuse run`, `fuse test`, `fuse add` — no separate tool install. |

---

## 3. Manifest Format

### 3.1 Minimal Example

```toml
[project]
name = "hello"
version = "0.1.0"

entry = "src/main.fuse"
```

This is enough for `fuse build` to work. Everything else has sensible
defaults.

### 3.2 Full Example

```toml
[project]
name = "my-app"
version = "1.2.0"
description = "A web service written in Fuse"
license = "MIT"
authors = ["Alice <alice@example.com>"]
repository = "https://github.com/alice/my-app"
fuse-edition = "2026"

# Single binary (most projects)
entry = "src/main.fuse"

# Multi-binary (overrides entry)
[[bin]]
name = "server"
entry = "src/server.fuse"

[[bin]]
name = "migrate"
entry = "src/migrate.fuse"

[[bin]]
name = "healthcheck"
entry = "src/healthcheck.fuse"

# Library (no entry point — importable by other projects)
[lib]
entry = "src/lib.fuse"
public = true

# Dependencies
[dependencies]
json = "1.0"
http = "0.2"
log = { version = "0.3", features = ["color"] }

# Dev-only dependencies (tests, benchmarks)
[dev-dependencies]
test = "0.1"

# Build configuration
[build]
target = "native"           # "native" | "wasm" | "ios" | "android"
optimization = "debug"      # "debug" | "release"
output-dir = "build/"

# Packaging (for publishing to registry)
[package]
include = ["src/**", "README.md", "LICENSE"]
exclude = ["tests/**", "examples/**", "benchmarks/**"]
readme = "README.md"
keywords = ["web", "server", "http"]
categories = ["web", "networking"]
registry = "https://registry.fuse.dev"
min-fuse-version = "0.1.0"

# Platform-specific settings
[platform.windows]
link = ["ws2_32"]

[platform.linux]
link = ["pthread"]

[platform.macos]
link = ["Security"]
frameworks = ["Foundation"]

# Mobile platforms (post-Stage 2)
[platform.ios]
min-version = "16.0"
frameworks = ["UIKit", "Foundation", "CoreGraphics"]
bundle-id = "com.alice.myapp"
arch = ["arm64"]

[platform.android]
min-sdk = 26
target-sdk = 34
arch = ["arm64-v8a", "x86_64"]
```

### 3.3 Section Reference

| Section | Required | Purpose |
|---|---|---|
| `[project]` | Yes | Name, version, metadata |
| `entry` | Yes (unless `[[bin]]` or `[lib]`) | Default binary entry point |
| `[[bin]]` | No | Multi-binary definitions |
| `[lib]` | No | Library configuration |
| `[dependencies]` | No | Runtime dependencies |
| `[dev-dependencies]` | No | Test/benchmark dependencies |
| `[build]` | No | Build configuration (target, optimization) |
| `[package]` | No | Publishing configuration |
| `[platform.*]` | No | Platform-specific link/framework settings |

---

## 4. Lessons from Other Languages

### 4.1 Rust (`Cargo.toml`) — Best in class

**Adopted:**
- Single file for everything
- `[dependencies]` with semver
- `Cargo.lock` for reproducibility
- `[[bin]]` for multi-binary
- `[dev-dependencies]`
- Scaffolding commands (`init`/`new`)

**Avoided:**
- `build.rs` escape hatch (arbitrary build code — slow, opaque, supply-chain risk)
- Edition system complexity (multiple syntax rules per edition)
- Feature unification conflicts
- Slow compile times from proc macros
- Registry name squatting (no namespacing)

### 4.2 Go (`go.mod`) — Minimalist

**Adopted:**
- Minimal manifest (~5 lines to start)
- Lockfile with checksums
- Built-in tooling (no separate install)

**Avoided:**
- Minimal Version Selection (picks oldest — everyone pins anyway)
- GOPATH legacy (global workspace confusion)
- No `dev-dependencies` (test imports bloat consumers)
- `replace` directives (leak into consumers)
- No shared dependency cache

### 4.3 Python — Cautionary tale

**Everything avoided:**
- 5 competing standards (`setup.py`, `setup.cfg`, `requirements.txt`, `pyproject.toml`, `Pipfile`)
- Runtime dependency resolution (conflicts after deployment)
- No lockfile until recently
- `setup.py` as executable code (security nightmare)
- Virtual environment hell

### 4.4 C# (`.csproj` / NuGet)

**Adopted:**
- SDK-style minimal project files
- Unified CLI tool (`dotnet new/build/run`)

**Avoided:**
- XML format (verbose, merge-conflict-prone)
- Multiple package sources scattered across config files
- Framework targeting combinatorial explosion
- Silent transitive dependency resolution

### 4.5 Kotlin (`build.gradle.kts`)

**Everything avoided:**
- Build system is a programming language (Turing-complete build config)
- JVM startup cost (5-15 seconds just to start Gradle)
- Third-party plugin ecosystem that breaks between versions

---

## 5. CLI Commands

| Command | What it does |
|---|---|
| `fuse init` | Create `fuel.toml` + `src/main.fuse` scaffold |
| `fuse build` | Compile project per `fuel.toml` |
| `fuse run` | Build + execute (default binary) |
| `fuse run --bin migrate` | Build + execute specific binary |
| `fuse test` | Build + run tests |
| `fuse add json` | Add dependency to `fuel.toml` |
| `fuse remove json` | Remove dependency |
| `fuse update` | Update dependencies within semver constraints |
| `fuse publish` | Publish to registry |
| `fuse check` | Type-check without compiling |

---

## 6. Project Layout Convention

```
my-project/
  fuel.toml               ← manifest
  fuel.lock                ← lockfile (auto-generated, committed)
  src/
    main.fuse              ← default entry (fn main)
    lib.fuse               ← library root (if [lib] section exists)
    server.fuse            ← additional binary
  tests/
    math_test.fuse         ← test files
  examples/
    demo.fuse              ← example programs
```

---

## 7. Open Questions

| # | Question | Options | When to resolve |
|---|---|---|---|
| 1 | Support `[scripts]` for custom commands? | `fuse run-script lint` vs no custom scripts | Before package manager implementation |
| 2 | Library: separate `[lib]` or just omit `entry`? | Explicit `[lib]` (clearer) vs infer from missing entry | Before package manager implementation |
| 3 | Registry name and URL? | `fuse.dev`? `fuel.dev`? | Before registry implementation |
| 4 | Should `fuse init` be interactive? | Interactive prompts vs flags only | Before CLI implementation |
| 5 | Monorepo/workspace structure? | `[workspace]` section (like Cargo) vs flat | Before multi-package support |

---

# Part II — Mobile Support (Post-Stage 2)

## 8. Prerequisites

Mobile work begins only after ALL of these are complete:

| Prerequisite | Where it gets done |
|---|---|
| Structs compiled in codegen | Hardening H1.1 |
| Sized integers (Int8, UInt8, Int32, UInt32, UInt64) | Hardening H2.6–H2.9 |
| Float32 | Hardening H2.1–H2.5 |
| Cross-compilation (Cranelift ARM64 target) | Post-hardening |
| FFI maturity (C struct layout, raw pointers, callbacks) | Post-hardening |
| Raw pointers and unsafe blocks | Language design decision |
| Package manager with `fuel.toml` | Post-Stage 2 |

---

## 9. Mobile Work Sequence

All items are sequenced, not time-bound:

### 9.1 Cross-compilation

- Cranelift ARM64 backend
- Cross-linker support
- `fuse build --target ios` / `--target android`

### 9.2 FFI Maturity

- C struct layout matching
- Raw pointer support
- Callback function pointers
- Opaque type handles

### 9.3 iOS Bridge

- Objective-C interop layer
- Framework linking (UIKit, Foundation)
- Code signing integration
- `.ipa` bundle generation

### 9.4 Android Bridge

- JNI interop layer
- NDK integration
- `.apk` / `.aab` generation
- Gradle integration (for mixed Kotlin/Fuse projects)

### 9.5 UI Strategy Decision

| Option | Description | Risk |
|---|---|---|
| A: Wrap native UI | Fuse bindings for UIKit/Jetpack Compose | High maintenance, platform churn |
| B: Cross-platform renderer | Skia/own engine | Massive engineering effort |
| C: Headless (shared logic) | Business logic in Fuse, UI in Swift/Kotlin | Lowest risk, highest ROI |

**Recommendation:** Option C first. This is what Kotlin Multiplatform does.

### 9.6 Shared Business Logic Model

```
┌─────────────────────────┐
│   iOS App (SwiftUI)     │
│   ┌───────────────────┐ │
│   │ Fuse native lib   │ │  ← compiled from Fuse, linked as .a
│   │ (business logic)  │ │
│   └───────────────────┘ │
└─────────────────────────┘

┌─────────────────────────┐
│ Android App (Compose)   │
│   ┌───────────────────┐ │
│   │ Fuse native lib   │ │  ← compiled from Fuse, linked via JNI
│   │ (business logic)  │ │
│   └───────────────────┘ │
└─────────────────────────┘
```

Manifest support:

```toml
[project]
name = "shared-logic"
version = "1.0.0"

[lib]
entry = "src/lib.fuse"
public = true

[build]
target = "ios"              # or "android"
output = "static-lib"       # produces .a / .so
```

---

# Part III — Master Sequencing

## 10. What Comes After What

> **Principle:** No timelines. Only sequence. Each step must be
> complete before the next begins. The language can be used at
> every stage — Stage 1 is a working compiler.

```
Stage 1 Hardening (current)
│
├── H0: Critical Bug Fixes ✓
├── H0.6: Async/Await/Suspend Removal
├── H1: Language Feature Completion (structs, generics)
├── H2: Numeric Type System (Float32, sized integers)
├── H3: Stdlib Polish
├── H4: Annotation System
├── H5: Evaluator Robustness
├── H6: LSP Foundation
├── H7: WASM Target
│
├── Implement interfaces (docs/fuse-pre-stage2.md Wave 5)
├── Implement @entrypoint → fn main() transition
├── Operator overloading
├── Fixed-size arrays [T; N]
├── Int16 / UInt16 (if needed)
│
Stage 2 — Self-hosting
│
├── Rewrite compiler frontend in Fuse
│   (lexer, parser, AST, HIR, checker)
├── Validate: Fuse compiles itself
│
Post-Stage 2
│
├── MCP Server (written in Fuse)
│   └── Validates concurrency, I/O, protocols
│
├── Package Manager (written in Fuse)
│   └── fuel.toml support
│   └── Registry (fuse.dev)
│   └── fuse init/build/run/test/add/publish
│   └── Validates file I/O, networking, CLI UX
│
├── Cross-compilation (ARM64)
│
├── Mobile support
│   └── iOS bridge
│   └── Android bridge
│   └── Shared business logic model
│
├── Domain-specific features
│   └── Float16 / BFloat16
│   └── GPU access
│   └── Tensor type
│   └── Linear algebra stdlib
```

### 10.1 Key Sequencing Rules

1. **Hardening before Stage 2.** The compiler must be bug-free before
   attempting self-hosting. A buggy compiler cannot compile itself.

2. **Async removal before interfaces.** The concurrency model must
   be settled before interfaces lock in method signatures that might
   reference async types.

3. **Interfaces before Stage 2.** The self-hosted compiler will use
   interfaces extensively. They must be proven first.

4. **Stage 2 before package manager.** The package manager is written
   in Fuse. Stage 2 proves Fuse can build complex tools.

5. **Package manager before mobile.** Mobile projects need dependency
   management. The package manager provides it.

6. **Cross-compilation before mobile.** Cannot target iOS/Android
   without ARM64 compilation.

---

*Document created from design sessions. Source: `interfaces_n_others.md` §8–§10.*
*Last updated: 2026-04-06*
