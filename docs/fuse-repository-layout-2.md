# Fuse Repository Layout (v2)

> **For AI agents reading this document:** This describes the source repository
> structure for the Fuse programming language as of the Stage D (self-hosting)
> attempt. Each top-level directory maps to one concern. Stage boundaries are
> hard directory boundaries — `stage0/`, `stage1/`, `stage2/`. Shared test cases
> live in `tests/fuse/` and are executed by all stages. The canonical milestone
> program is `tests/fuse/milestone/four_functions.fuse`. The authoritative
> language reference is `docs/guide/fuse-language-guide-2.md`.

---

## Philosophy

One repository, three stages, one test suite. The stages do not share source
code — each is an independent implementation. They do share test cases, the
language guide, and the standard library definitions. A test that passes in
Stage 0 must produce identical output in Stage 1 and Stage 2.

The repository is structured so that each stage can be understood, built, and
tested in isolation. Nothing in `stage1/` depends on `stage0/`. The shared
test suite is the contract between them.

Stage 2 (the self-hosting compiler) currently exists as a working monolith —
three `.fuse` files that compile Fuse Core programs to native executables via
Cranelift FFI. The planned modular layout is documented below alongside the
current state.

---

## Full Tree

```
fuse/
│
├── README.md                          # Project overview and quick start
├── CONTRIBUTING.md                    # Contribution guidelines
├── LICENSE
├── .gitignore                         # Excludes *.exe, *.pdb, *.o, *.lib, *.a, fusec2*
│
├── docs/                              # All human and AI-readable documentation
│   ├── guide/
│   │   ├── fuse-language-guide.md     # Original language guide (v1)
│   │   └── fuse-language-guide-2.md   # Canonical language guide (v2, authoritative)
│   ├── adr/                           # Architecture Decision Records
│   │   ├── ADR-001-ref-not-borrowed.md
│   │   ├── ADR-002-mutref-not-inout.md
│   │   ├── ADR-003-move-not-caret.md
│   │   ├── ADR-004-rank-mandatory.md
│   │   ├── ADR-005-deadlock-three-tiers.md
│   │   ├── ADR-006-stage0-python.md
│   │   ├── ADR-007-cranelift-not-llvm.md
│   │   ├── ADR-008-no-timelines.md
│   │   ├── ADR-009-data-contextual-keyword.md   # data as contextual keyword for value types
│   │   ├── ADR-010-asap-destruction-semantics.md # ASAP destruction at last-use point
│   │   └── ADR-011-extern-fn-ffi.md              # extern fn for C FFI interop
│   ├── spec/                          # Future: formal grammar and type rules
│   │   └── .gitkeep                   # Reserved — populated in Stage 1
│   ├── guide-v2-plan.md               # Plan and rationale for the v2 language guide
│   ├── self_hosting.md                # Self-hosting strategy and Stage D plan
│   ├── fuse-implementation-plan.md    # Overall implementation roadmap
│   └── fuse-repository-layout.md      # Original repository layout (v1)
│
├── tests/                             # Shared test suite — all stages must pass these
│   └── fuse/
│       ├── milestone/
│       │   └── four_functions.fuse    # Stage 0 milestone: the canonical example
│       ├── core/                      # Tests for Fuse Core features only
│       │   ├── ownership/
│       │   │   ├── ref_read_only.fuse
│       │   │   ├── mutref_modifies_caller.fuse
│       │   │   ├── move_transfers_ownership.fuse
│       │   │   └── move_prevents_reuse.fuse
│       │   ├── memory/
│       │   │   ├── asap_destruction.fuse
│       │   │   ├── value_auto_lifecycle.fuse      # value type auto-lifecycle
│       │   │   ├── del_fires_at_last_use.fuse
│       │   │   └── multiple_defers.fuse            # defer ordering and multiple defers
│       │   ├── errors/
│       │   │   ├── result_propagation.fuse
│       │   │   ├── option_chaining.fuse
│       │   │   ├── match_exhaustive.fuse
│       │   │   ├── match_missing_arm.fuse
│       │   │   └── question_mark_shortcircuit.fuse
│       │   └── types/
│       │       ├── val_immutable.fuse
│       │       ├── var_mutable.fuse
│       │       ├── data_class_equality.fuse
│       │       ├── extension_functions.fuse
│       │       ├── type_inference.fuse
│       │       ├── block_expression.fuse           # block expressions returning values
│       │       ├── enum_construction.fuse          # enum variant construction
│       │       ├── expression_body_fn.fuse         # single-expression function bodies
│       │       ├── for_loop.fuse                   # for-in loop iteration
│       │       ├── if_else.fuse                    # if/else as expression
│       │       ├── integer_division.fuse           # integer truncation semantics
│       │       └── when_expression.fuse            # when (pattern-match) expression
│       └── full/                      # Tests for Fuse Full — Stage 1 and beyond
│           ├── concurrency/
│           │   ├── chan_basic.fuse
│           │   ├── chan_bounded_backpressure.fuse
│           │   ├── shared_rank_ascending.fuse
│           │   ├── shared_rank_violation.fuse    # must produce compile error
│           │   ├── shared_no_rank.fuse           # must produce compile error
│           │   └── spawn_mutref_rejected.fuse    # must produce compile error
│           ├── async/
│           │   ├── await_basic.fuse
│           │   ├── suspend_fn.fuse
│           │   └── write_guard_across_await.fuse # must produce compile warning
│           └── simd/
│               └── simd_sum.fuse
│
├── stdlib/                            # Standard library — written in Fuse
│   ├── README.md                      # Which stdlib files are available at each stage
│   ├── core/                          # Available in Fuse Core (Stage 0+)
│   │   ├── result.fuse                # Result<T,E>, Ok, Err
│   │   ├── option.fuse                # Option<T>, Some, None
│   │   ├── list.fuse                  # List<T> — map, filter, sorted, retainWhere, etc.
│   │   ├── map.fuse                   # Map<K, V> — set, get, remove, keys, values, entries
│   │   ├── string.fuse                # String — interpolation, slicing, parsing
│   │   ├── int.fuse                   # Int — arithmetic, conversions
│   │   ├── float.fuse                 # Float — arithmetic, SIMD-ready layout
│   │   └── bool.fuse                  # Bool — and, or, not
│   └── full/                          # Available in Fuse Full (Stage 1+)
│       ├── chan.fuse                   # Chan<T> — bounded, unbounded, send, recv
│       ├── shared.fuse                # Shared<T> — read, write, try_write
│       ├── timer.fuse                 # Timer — sleep, timeout
│       ├── simd.fuse                  # SIMD<T,N> — sum, dot, broadcast
│       └── http.fuse                  # Http — get, post (used in canonical example)
│
├── examples/                          # Standalone Fuse programs for learning and testing
│   ├── README.md
│   ├── hello.fuse                     # Simplest possible Fuse program
│   ├── ownership_tour.fuse            # ref, mutref, owned, move demonstrated
│   ├── error_handling.fuse            # Result, Option, match, ?
│   ├── channels.fuse                  # Tier 1 concurrency — Chan<T>
│   ├── shared_state.fuse              # Tier 2 concurrency — Shared<T> + @rank
│   └── four_functions.fuse            # The full canonical example from the guide
│
├── stage0/                            # Python tree-walking interpreter — Fuse Core only
│   ├── README.md                      # How to install, run, and test Stage 0
│   ├── requirements.txt               # Python dependencies (stdlib only — no third-party)
│   │
│   ├── src/
│   │   ├── main.py                    # CLI entry point: fusec0 <file.fuse>
│   │   ├── repl.py                    # Interactive REPL: fusec0 --repl
│   │   ├── lexer.py                   # Tokeniser — produces token stream from source text
│   │   ├── token.py                   # Token type definitions and literals
│   │   ├── parser.py                  # Recursive descent parser — produces AST
│   │   ├── ast.py                     # AST node dataclasses for every construct
│   │   ├── checker.py                 # Ownership checker and basic type verifier
│   │   ├── evaluator.py               # Tree-walking evaluator — executes AST nodes
│   │   ├── environment.py             # Scope and binding management
│   │   ├── values.py                  # Runtime value representations
│   │   └── errors.py                  # Interpreter error types and formatting
│   │
│   └── tests/
│       ├── run_tests.py               # Test runner — executes tests/fuse/core/ against Stage 0
│       └── snapshots/                 # Expected output for each test case
│           └── ...
│
├── stage1/                            # Rust compiler with Cranelift backend — Fuse Full
│   ├── README.md                      # How to build, run, and test Stage 1
│   ├── Cargo.toml                     # Workspace manifest (fusec, fuse-runtime, cranelift-ffi)
│   ├── Cargo.lock
│   │
│   ├── fusec/                         # The compiler binary crate
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs                # CLI entry point: fusec <file.fuse>
│   │       ├── eval.rs                # Evaluation / interpretation mode
│   │       ├── error.rs               # Compiler error types and diagnostics
│   │       ├── lexer/
│   │       │   ├── mod.rs
│   │       │   ├── token.rs           # Token types — mirrors stage0/token.py
│   │       │   └── lexer.rs           # Tokeniser
│   │       ├── parser/
│   │       │   ├── mod.rs
│   │       │   └── parser.rs          # Recursive descent parser — produces AST
│   │       ├── ast/
│   │       │   ├── mod.rs
│   │       │   └── nodes.rs           # AST node definitions
│   │       ├── hir/                   # High-level intermediate representation
│   │       │   ├── mod.rs
│   │       │   ├── lower.rs           # AST -> HIR lowering
│   │       │   └── nodes.rs           # HIR node definitions
│   │       ├── checker/               # Semantic analysis
│   │       │   ├── mod.rs
│   │       │   ├── types.rs           # Type inference and checking
│   │       │   ├── ownership.rs       # ref/mutref/owned/move enforcement
│   │       │   ├── exhaustiveness.rs  # match exhaustiveness checking
│   │       │   ├── rank.rs            # @rank ordering enforcement
│   │       │   ├── spawn.rs           # spawn capture rule enforcement
│   │       │   └── async_lint.rs      # write-guard-across-await warning
│   │       └── codegen/
│   │           ├── mod.rs
│   │           ├── cranelift.rs       # HIR -> Cranelift IR translation
│   │           └── layout.rs          # Value layout and ABI
│   │
│   ├── fuse-runtime/                  # Runtime support library linked into compiled programs
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                 # Crate root and re-exports
│   │       ├── value.rs               # Tagged value representation for Fuse values
│   │       ├── asap.rs                # ASAP destruction bookkeeping
│   │       ├── builtins.rs            # Built-in function implementations (print, etc.)
│   │       ├── ffi.rs                 # C FFI surface — extern "C" functions for Stage 2
│   │       ├── list_ops.rs            # List operations (create, append, get, length)
│   │       ├── string_ops.rs          # String operations (concat, interpolation, slicing)
│   │       ├── chan.rs                 # Chan<T> implementation
│   │       ├── shared.rs              # Shared<T> + RwLock guard implementation
│   │       └── async_rt.rs            # Async executor (lightweight, no tokio dependency)
│   │
│   ├── cranelift-ffi/                 # C-compatible Cranelift wrappers for Stage 2 FFI
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs                 # extern "C" API: module_new, function_new, emit, etc.
│   │
│   └── tests/
│       └── run_tests.rs               # Test runner — executes tests/fuse/ against Stage 1
│
└── stage2/                            # Self-hosting Fuse compiler — written in Fuse Core
    ├── README.md                      # Entry condition and milestone definition
    └── src/
        ├── main.fuse                  # CLI entry, lexer, parser, AST (monolith)
        ├── codegen.fuse               # HIR lowering and Cranelift IR generation via FFI
        └── backend.fuse               # Object emission, linking, platform abstraction
```

---

## Directory Reference

### `docs/`

All documentation lives here. The language guide (`fuse-language-guide-2.md`)
is the authoritative source for language behaviour. The original guide
(`fuse-language-guide.md`) is retained for historical reference but is no
longer canonical.

ADRs in `docs/adr/` are standalone files — the same decisions recorded at the
end of the guide, each given a dedicated file so they can be linked, cited, or
updated independently. ADR-009 through ADR-011 were added during the Stage C
and Stage D work.

| File | Purpose |
|---|---|
| `guide/fuse-language-guide-2.md` | Canonical language reference (v2) |
| `guide/fuse-language-guide.md` | Original language guide (v1, historical) |
| `guide-v2-plan.md` | Plan and rationale for the v2 guide rewrite |
| `self_hosting.md` | Self-hosting strategy and Stage D bootstrap plan |
| `fuse-implementation-plan.md` | Overall implementation roadmap |
| `fuse-repository-layout.md` | Original repository layout (v1) |
| `adr/ADR-009-data-contextual-keyword.md` | `data` as contextual keyword for value types |
| `adr/ADR-010-asap-destruction-semantics.md` | ASAP destruction at last-use point |
| `adr/ADR-011-extern-fn-ffi.md` | `extern fn` for C FFI interop |

The `spec/` directory is intentionally empty until the language is stable
enough that writing a formal grammar is descriptive rather than speculative.

---

### `tests/fuse/`

The shared test suite. Every `.fuse` file here is a valid (or intentionally
invalid) Fuse program. Test runners in each stage execute these files and
compare output against snapshots.

**Three categories of test:**

- **Core tests** (`tests/fuse/core/`) — Fuse Core features only. Stage 0 must
  pass all of these. Stage 1 and Stage 2 must also pass all of these.
- **Full tests** (`tests/fuse/full/`) — Fuse Full features. Stage 1 and
  Stage 2 only.
- **Milestone** (`tests/fuse/milestone/`) — the canonical four-function program
  from Section 11 of the language guide. This is the single most important test
  in the repository.

**Core test inventory (25 tests):**

| Subdirectory | Tests |
|---|---|
| `ownership/` | `ref_read_only`, `mutref_modifies_caller`, `move_transfers_ownership`, `move_prevents_reuse` |
| `memory/` | `asap_destruction`, `del_fires_at_last_use`, `value_auto_lifecycle`, `multiple_defers` |
| `errors/` | `result_propagation`, `option_chaining`, `match_exhaustive`, `match_missing_arm`, `question_mark_shortcircuit` |
| `types/` | `val_immutable`, `var_mutable`, `data_class_equality`, `extension_functions`, `type_inference`, `block_expression`, `enum_construction`, `expression_body_fn`, `for_loop`, `if_else`, `integer_division`, `when_expression` |

**Error tests:** Files whose names end in `_rejected.fuse` or `_error.fuse`
are expected to produce a compiler error, not execute successfully. The test
runner verifies that the error is produced and that its message matches the
expected snapshot.

---

### `stdlib/`

Standard library definitions written in Fuse. Core stdlib (`stdlib/core/`) is
available to the Stage 0 interpreter. Full stdlib (`stdlib/full/`) is available
from Stage 1 onward.

These files are source of truth for the standard library API. The Stage 0
interpreter may implement them natively in Python for performance; the Stage 1
compiler compiles them as ordinary Fuse source. The API must be identical.

---

### `examples/`

Standalone Fuse programs intended for learning and manual testing. Not part of
the automated test suite. These are the programs a new developer or AI agent
would read first to understand how Fuse feels to write.

`four_functions.fuse` here is the narrative version of the canonical example —
annotated with comments explaining each feature. The copy in
`tests/fuse/milestone/` is the clean version used for automated testing.

---

### `stage0/`

The Python tree-walking interpreter. Implements Fuse Core only.

**Entry point:** `python src/main.py <file.fuse>`

**Key files:**

| File | Responsibility |
|---|---|
| `lexer.py` | Converts source text to a flat token stream |
| `parser.py` | Converts token stream to an AST using recursive descent |
| `ast.py` | Dataclass definitions for every AST node |
| `checker.py` | Enforces ownership rules and match exhaustiveness before evaluation |
| `evaluator.py` | Walks the AST and produces a result value |
| `values.py` | Python representations of every Fuse runtime value |

**Stage 0 milestone:** `python src/main.py ../../tests/fuse/milestone/four_functions.fuse`
executes without error and produces the expected output.

---

### `stage1/`

The Rust compiler. A Cargo workspace with three crates.

**`fusec/`** — the compiler. Takes a `.fuse` source file, runs it through
lexer -> parser -> AST -> HIR -> checker -> codegen, and emits a native binary
via Cranelift.

**`fuse-runtime/`** — the runtime library linked into every compiled Fuse
program. Provides ASAP destruction bookkeeping, built-in functions, value
representation, list and string operations, `Chan<T>`, `Shared<T>`, the async
executor, and a C FFI surface (`ffi.rs`) that Stage 2 calls via `extern fn`.

**`cranelift-ffi/`** — C-compatible wrappers around Cranelift APIs. This crate
exposes `extern "C"` functions (`module_new`, `function_new`, `emit_insn`,
`finalize`, etc.) so that the Stage 2 compiler — written in Fuse — can drive
Cranelift code generation without needing a Rust toolchain at runtime.

| Crate | Type | Purpose |
|---|---|---|
| `fusec` | `[[bin]]` | The Stage 1 compiler binary |
| `fuse-runtime` | `[lib]` | Runtime support linked into compiled programs |
| `cranelift-ffi` | `[lib]` (staticlib) | C-callable Cranelift wrappers for Stage 2 |

**Key runtime files:**

| File | Responsibility |
|---|---|
| `value.rs` | Tagged value representation for Fuse values |
| `builtins.rs` | Built-in function implementations (print, len, etc.) |
| `ffi.rs` | C FFI surface — `extern "C"` entry points for Stage 2 |
| `list_ops.rs` | List operations: create, append, get, length |
| `string_ops.rs` | String operations: concat, interpolation, slicing |
| `asap.rs` | ASAP destruction bookkeeping |
| `chan.rs` | `Chan<T>` implementation |
| `shared.rs` | `Shared<T>` + RwLock guard implementation |
| `async_rt.rs` | Async executor (lightweight, no tokio dependency) |

**Entry point:** `cargo run --bin fusec -- <file.fuse>`

**Stage 1 milestone:** `fusec tests/fuse/milestone/four_functions.fuse` compiles
and runs correctly. The full `tests/fuse/` suite passes. The Stage 0 snapshot
suite passes against Stage 1 output.

---

### `stage2/`

The self-hosting Fuse compiler written in Fuse Core. Stage 2 compiles Fuse
programs to native executables by calling Cranelift and the Fuse runtime through
C FFI (`extern fn` declarations).

#### Current state (monolith)

Stage 2 currently consists of three files that together form a working but
monolithic compiler:

| File | Responsibility |
|---|---|
| `main.fuse` | CLI entry point, lexer, parser, AST definitions, and driver loop |
| `codegen.fuse` | HIR lowering and Cranelift IR generation via FFI calls |
| `backend.fuse` | Object file emission, platform-specific linking, PE/ELF handling |

All three files are compiled together by Stage 1. The resulting `fusec2`
binary can compile Fuse Core programs to native executables.

#### Planned modular structure

The monolith will eventually be split along the same boundaries as Stage 1:

```
stage2/src/
├── main.fuse          # CLI entry point and driver
├── lexer.fuse         # Tokeniser
├── parser.fuse        # Recursive descent parser
├── ast.fuse           # AST node definitions
├── hir.fuse           # HIR nodes and AST -> HIR lowering
├── checker.fuse       # Ownership and type checking
├── codegen.fuse       # HIR -> Cranelift IR via FFI
└── backend.fuse       # Object emission and linking
```

This refactoring is blocked on Stage 2 supporting multi-file compilation
(imports between `.fuse` files).

**Stage 2 milestone:** `fusec2 <file.fuse>` compiles a Fuse program to a
native binary using only the Stage 2 compiler, without invoking the Stage 1
Rust binary. The self-hosting bootstrap is complete when Stage 2 can compile
itself.

---

## Build Commands

### Stage 0

```bash
cd stage0
python src/main.py <file.fuse>
python tests/run_tests.py                     # run Core test suite
```

### Stage 1

```bash
cd stage1
cargo build --release                          # builds fusec, fuse-runtime, cranelift-ffi
cargo run --bin fusec -- <file.fuse>           # compile and run a Fuse program
cargo test                                     # run Stage 1 test suite
```

To build just the FFI libraries for Stage 2:

```bash
cd stage1
cargo build --release -p fuse-runtime          # produces fuse_runtime.lib / libfuse_runtime.a
cargo build --release -p cranelift-ffi         # produces cranelift_ffi.lib / libcranelift_ffi.a
```

### Stage 2

Stage 2 is compiled by Stage 1. The build produces `fusec2`:

```bash
cd stage1
cargo run --release --bin fusec -- ../stage2/src/main.fuse -o fusec2
```

Then use `fusec2` to compile Fuse programs:

```bash
./fusec2 <file.fuse>
```

---

## Conventions

### File naming

| Pattern | Meaning |
|---|---|
| `*_rejected.fuse` | Program that must fail with a compile error |
| `*_warning.fuse` | Program that must produce a specific compile warning |
| `*_test.fuse` | Executable test — checked against a snapshot |
| `*_tour.fuse` | Annotated example for learning — not in automated suite |

### .gitignore

The repository excludes all build artifacts. Key patterns:

| Pattern | Reason |
|---|---|
| `stage1/target/` | Rust/Cargo build output |
| `stage0/src/__pycache__/`, `*.pyc` | Python bytecode |
| `*.exe`, `*.pdb` | Compiled binaries and debug symbols (Windows) |
| `*.o`, `*.obj`, `*.lib`, `*.a` | Object files and static libraries |
| `fusec2*` | Stage 2 compiler output (built from Stage 1) |
| `.claude/`, `.vscode/`, `.idea/` | IDE and editor configuration |

### Snapshot format

Each test file in `tests/fuse/` has a corresponding snapshot in the stage's
`tests/snapshots/` directory. Snapshots are plain text files containing the
exact expected stdout output, or for error tests, the expected error message.

```
tests/fuse/core/ownership/ref_read_only.fuse
stage0/tests/snapshots/core/ownership/ref_read_only.txt
stage1/tests/snapshots/core/ownership/ref_read_only.txt
```

Stage 0 and Stage 1 snapshots must be identical for all Core tests. If they
differ, Stage 1 has a bug.

### Adding a new language feature

1. Update `docs/guide/fuse-language-guide-2.md` with the concept, rationale,
   and code example.
2. Add an ADR to `docs/adr/` if it records a non-obvious design choice.
3. Add test cases to `tests/fuse/core/` or `tests/fuse/full/`.
4. Add snapshots to `stage0/tests/snapshots/` (or `stage1/` if Full-only).
5. Implement in Stage 0 first. Verify tests pass.
6. Implement in Stage 1. Verify the same tests pass.

The guide is updated before the implementation. A feature without a guide
entry does not exist yet.

---

## Relationship Between Repositories and Stages

```
fuse/                          <- this repository
│
├── docs/        <──────────────── read by humans and AI agents at all stages
├── tests/fuse/  <──────────────── executed by stage0, stage1, stage2
├── stdlib/      <──────────────── interpreted by stage0, compiled by stage1+
├── examples/    <──────────────── read by developers and AI agents
│
├── stage0/      <──────────────── Python; runs tests/fuse/core/
├── stage1/      <──────────────── Rust + Cranelift; runs tests/fuse/ (all)
│   └── cranelift-ffi/  <──────── C FFI bridge consumed by stage2
└── stage2/      <──────────────── Fuse + FFI; runs tests/fuse/ (all)
                                   links: fuse_runtime.lib + cranelift_ffi.lib

The stages share no source code with each other.
They share: tests, stdlib definitions, examples, documentation.
Stage 2 links against Stage 1 library artifacts (runtime + cranelift-ffi)
but does not share compiler source code with Stage 1.
```

---

## Quick Reference: Where Things Live

| I want to... | Go to... |
|---|---|
| Understand the language | `docs/guide/fuse-language-guide-2.md` |
| Find the canonical example | `tests/fuse/milestone/four_functions.fuse` |
| Understand a design decision | `docs/adr/ADR-NNN-*.md` |
| Run the interpreter | `stage0/` |
| Build the compiler | `stage1/` |
| Build the self-hosting compiler | `stage2/` (compiled by Stage 1) |
| See stdlib API | `stdlib/core/` or `stdlib/full/` |
| Add a test | `tests/fuse/core/` or `tests/fuse/full/` |
| Read an annotated example | `examples/` |
| Understand the self-hosting plan | `docs/self_hosting.md` |
| See the FFI bridge for Stage 2 | `stage1/cranelift-ffi/` |

---

*End of Fuse Repository Layout (v2)*

---

> **For AI agents:** The single most important file in this repository is
> `tests/fuse/milestone/four_functions.fuse`. Stage 0 is complete when
> that file runs. Stage 1 is complete when that file compiles to a native
> binary. Stage 2 is complete when `fusec2` can compile that file without
> invoking Stage 1. All ownership and concurrency semantics are documented in
> `docs/guide/fuse-language-guide-2.md`. New features always start with
> a guide update before any implementation. Stage 2 calls Cranelift and the
> Fuse runtime through C FFI — see `stage1/cranelift-ffi/` and
> `stage1/fuse-runtime/src/ffi.rs`.
