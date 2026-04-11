# Fuse Repository Layout

> **Status:** normative. This document describes the physical layout of the Fuse repository. If the code disagrees with this document, the code is wrong or the document is stale; in either case, a contributor opens a discussion before moving files.
>
> **Companion documents** (same directory):
> - `language-guide.md` — the language specification.
> - `rules.md` — discipline rules, including the list of allowed `unsafe` stdlib bridge files.
> - `implementation-plan.md` — the wave-by-wave plan.

---

## Table of contents

1. [Top-level tree](#1-top-level-tree)
2. [Root-level files](#2-root-level-files)
3. [`cmd/`](#3-cmd)
4. [`compiler/`](#4-compiler)
5. [`runtime/`](#5-runtime)
6. [`stdlib/`](#6-stdlib)
7. [`stage2/`](#7-stage2)
8. [`tests/`](#8-tests)
9. [`examples/`](#9-examples)
10. [`tools/`](#10-tools)
11. [`docs/`](#11-docs)
12. [`.ci/`](#12-ci)
13. [The `unsafe` bridge file list](#13-the-unsafe-bridge-file-list)
14. [Adding a new top-level directory](#14-adding-a-new-top-level-directory)

---

## 1. Top-level tree

```
fuse/
├── cmd/
│   └── fuse/                  # Stage 1 CLI entry point (Go main package)
├── compiler/                  # Stage 1 compiler packages (Go)
│   ├── lex/
│   ├── parse/
│   ├── ast/
│   ├── resolve/
│   ├── hir/
│   ├── check/
│   ├── liveness/
│   ├── lower/
│   ├── mir/
│   ├── codegen/
│   ├── cc/
│   ├── typetable/
│   ├── passmgr/
│   ├── diagnostics/
│   ├── driver/
│   ├── fmt/
│   ├── doc/
│   ├── repl/
│   └── testrunner/
├── runtime/                   # The ~500-700 LOC C11 runtime
│   ├── include/
│   │   └── fuse_rt.h
│   ├── src/
│   │   ├── fuse_rt_mem.c
│   │   ├── fuse_rt_panic.c
│   │   ├── fuse_rt_io.c
│   │   ├── fuse_rt_process.c
│   │   ├── fuse_rt_file.c
│   │   ├── fuse_rt_thread.c
│   │   ├── fuse_rt_sync.c
│   │   ├── fuse_rt_tls.c
│   │   └── fuse_rt_time.c
│   ├── platform/
│   │   ├── posix/
│   │   └── windows/
│   └── tests/
├── stdlib/
│   ├── core/                  # OS-free, always available (written in Fuse)
│   │   ├── prelude.fuse
│   │   ├── option.fuse
│   │   ├── result.fuse
│   │   ├── ordering.fuse
│   │   ├── traits/
│   │   │   ├── equatable.fuse
│   │   │   ├── hashable.fuse
│   │   │   ├── comparable.fuse
│   │   │   ├── printable.fuse
│   │   │   ├── debuggable.fuse
│   │   │   ├── sequence.fuse
│   │   │   ├── default.fuse
│   │   │   ├── from.fuse
│   │   │   └── index.fuse
│   │   ├── primitive/
│   │   │   ├── int.fuse
│   │   │   ├── float.fuse
│   │   │   ├── bool.fuse
│   │   │   └── char.fuse
│   │   ├── string.fuse
│   │   ├── list.fuse
│   │   ├── map.fuse
│   │   ├── set.fuse
│   │   ├── hash/
│   │   │   ├── hasher.fuse
│   │   │   └── siphash.fuse
│   │   ├── fmt/
│   │   │   ├── builder.fuse
│   │   │   └── format.fuse
│   │   ├── math.fuse
│   │   ├── iter.fuse
│   │   ├── atomic.fuse
│   │   └── rt_bridge/         # The small set of `unsafe` stdlib files (§13)
│   │       ├── alloc.fuse
│   │       ├── panic.fuse
│   │       └── intrinsics.fuse
│   ├── full/                  # Hosted OS surface (written in Fuse)
│   │   ├── io/
│   │   │   ├── stdin.fuse
│   │   │   ├── stdout.fuse
│   │   │   └── stderr.fuse
│   │   ├── fs/
│   │   │   ├── file.fuse
│   │   │   ├── dir.fuse
│   │   │   └── path.fuse
│   │   ├── os/
│   │   │   ├── env.fuse
│   │   │   ├── process.fuse
│   │   │   └── args.fuse
│   │   ├── time/
│   │   │   ├── instant.fuse
│   │   │   ├── duration.fuse
│   │   │   └── wallclock.fuse
│   │   ├── thread/
│   │   │   ├── spawn.fuse
│   │   │   └── handle.fuse
│   │   ├── sync/
│   │   │   ├── mutex.fuse
│   │   │   ├── rwlock.fuse
│   │   │   ├── cond.fuse
│   │   │   ├── once.fuse
│   │   │   └── shared.fuse
│   │   └── chan/
│   │       └── chan.fuse
│   └── ext/                   # Opt-in extras (written in Fuse)
│       ├── json/
│       ├── regex/
│       ├── serde/
│       ├── compress/
│       ├── crypto/
│       └── net/
├── stage2/                    # Self-hosted Fuse compiler sources (Fuse files)
│   └── src/
│       ├── main.fuse
│       ├── lex/
│       ├── parse/
│       ├── ast/
│       ├── resolve/
│       ├── hir/
│       ├── check/
│       ├── lower/
│       ├── mir/
│       ├── codegen/
│       └── driver/
├── tests/
│   ├── e2e/                   # End-to-end: Fuse source → binary → run
│   ├── bootstrap/             # Three-generation reproducibility
│   ├── property/               # Property-based IR tests
│   └── fixtures/              # Shared test corpora
├── examples/                  # Example programs, buildable with `fuse build`
│   ├── hello/
│   ├── echo/
│   ├── wordcount/
│   ├── http_server/           # uses ext.net (post-day-one)
│   └── concurrent_pipeline/
├── tools/
│   ├── checklog/              # verifies learning log format
│   ├── checkdoc/              # verifies doc comment coverage
│   ├── goldens/               # golden-update helper
│   ├── repro/                 # byte-identical build verifier
│   └── bench/                 # microbenchmark runner
├── docs/
│   ├── language-guide.md
│   ├── implementation-plan.md
│   ├── rules.md
│   ├── repository-layout.md
│   ├── learning-log.md
│   └── adr/
├── .ci/
│   ├── workflows/
│   └── scripts/
├── go.mod
├── go.sum                     # empty after Stage 1 wave 0 (zero deps)
├── Makefile
├── README.md
└── .gitignore
```

The tree above is the full picture. Any new top-level directory requires the procedure in §14.

---

## 2. Root-level files

- **`go.mod`** — declares the Go module. Module path is `github.com/<owner>/fuse`. Minimum Go version is specified here.
- **`go.sum`** — present for tooling compatibility but empty. The project has zero external Go dependencies (`rules.md` §8.3).
- **`Makefile`** — the entry point for local builds. Targets: `all`, `stage1`, `stage2`, `runtime`, `test`, `bootstrap`, `clean`, `fmt`, `docs`, `repro`. The `Makefile` is a thin wrapper over `go build`, `fuse build`, and a few shell scripts in `tools/`.
- **`README.md`** — short introduction, quickstart, and pointers to the four documents under `docs/`. This file is the only documentation in the repo that a newcomer sees first; it must remain short.
- **`.gitignore`** — excludes build artifacts, editor temporaries, and `target/`-equivalents. No file under version control is covered by `.gitignore`.

There are no other root-level files. In particular: no `LICENSE` outside this list (place license information in a `LICENSE` file at the root if needed), no `CHANGELOG.md` (release notes go into git tags), no `CONTRIBUTING.md` (rules live in `docs/rules.md`).

---

## 3. `cmd/`

`cmd/` holds Go `main` packages. On day one, it has exactly one subdirectory.

### `cmd/fuse/`

The Stage 1 CLI entry point. This package:

1. Parses the `fuse` command line (hand-rolled, no external flag library).
2. Dispatches to one of the nine subcommands described in §17 of `language-guide.md`.
3. Wires together the compiler pipeline from `compiler/driver`.
4. Owns the process-level concerns: exit codes, signal handling, stdout/stderr.

`cmd/fuse/` is deliberately thin. It does not contain business logic; all business logic lives in `compiler/`.

Files:

- `main.go` — the `main` function.
- `args.go` — the argument parser.
- `dispatch.go` — the subcommand dispatcher.
- `version.go` — the build-stamped version string.

No subdirectories inside `cmd/fuse/`. No second `cmd/*` directory on day one. A second CLI (e.g., a language server) would be `cmd/fuse-lsp/` when it ships.

---

## 4. `compiler/`

The Stage 1 Go packages that implement the compiler. Each subdirectory is a Go package. Imports flow in one direction: later packages may import earlier ones; earlier packages MUST NOT import later ones. The dependency order is:

```
diagnostics < typetable < ast < lex < parse < resolve < hir < check < liveness
             < lower < mir < codegen < cc < driver
                                                    < fmt
                                                    < doc
                                                    < repl
                                                    < testrunner
passmgr spans the middle: used by every pass-hosting package from `resolve` onward.
```

### `compiler/diagnostics/`
The error and warning rendering library. Owns `Diagnostic`, `Span`, `Severity`, and the pretty-printer that formats errors for the terminal. Every other package emits diagnostics through this one.

### `compiler/typetable/`
The global type-interning store. Owns `TypeId` (a `uint32`) and the `TypeTable` struct. Every type in HIR and MIR is referenced through a `TypeId`, and equality is integer comparison (`rules.md` §3.7).

### `compiler/ast/`
The AST node types. Pure syntax: every `ast.Expr` and `ast.Stmt` is a Go interface implemented by concrete structs. No fields on an AST node refer to types, resolved symbols, or liveness — those are HIR-level concerns.

### `compiler/lex/`
The lexer. Converts a `[]byte` source buffer into a stream of tokens. Hand-rolled, table-driven for keyword recognition.

### `compiler/parse/`
The parser. Hand-rolled recursive-descent, produces an `ast.File`. Emits diagnostics through `compiler/diagnostics` for parse errors and attempts recovery where possible so multiple errors can be reported from one run.

### `compiler/resolve/`
Name resolution. Walks the AST, resolves identifiers to symbols (function, variable, type, trait), builds the module graph, detects cycles, and produces the HIR skeleton with every identifier bound. The output is a `hir.File` with identifiers replaced by `SymbolId`s.

### `compiler/hir/`
The HIR node types and their construction helpers. Every HIR node has per-node metadata populated by the time the node is constructed (`rules.md` §3.2). Constructors are **private** outside this package; other packages use builder functions that enforce the invariant.

### `compiler/check/`
The type checker. Infers types, checks trait bounds, checks pattern exhaustiveness, checks match arm unification (U1–U7), and populates `Type` metadata on every HIR node. A HIR node leaving the checker with `Type = Unknown` is a hard error caught by an invariant walker.

### `compiler/liveness/`
The single liveness pass. Computes `LiveAfter` metadata for every HIR node, identifies the last use of every binding, and marks ownership transfers. Also detects the escape of closures (§6.6 of the guide) and enforces the explicit `move` rule. This is the **single** liveness computation in the compiler (`rules.md` §3.9).

### `compiler/lower/`
The HIR → MIR lowering pass. Flattens expressions, makes `Drop` and `Move` explicit, materializes temporaries, and emits MIR that is already close to what codegen will produce.

### `compiler/mir/`
The MIR node types. Like HIR, constructors are private outside the package; builder functions enforce invariants.

### `compiler/codegen/`
The MIR → C11 code generator. Emits a C11 source file per Fuse module plus a single "runtime header include" file. Uses `compiler/typetable` for type layout decisions.

### `compiler/cc/`
The subprocess wrapper around `cc`. Responsibilities: detect the installed C compiler, construct the right argument list per target triple, spawn the compiler, forward diagnostics, and handle link errors.

### `compiler/passmgr/`
The pass manifest framework. Owns the `Pass` interface, the `Manifest` struct, and the topological-sort-based scheduler. Every pass registers itself with the manifest at package init; the driver asks the manifest for the execution order.

### `compiler/driver/`
The end-to-end pipeline orchestrator. Takes a package root, runs the full sequence of passes, and returns either a successful artifact or a list of diagnostics.

### `compiler/fmt/`
The `fuse fmt` implementation. Parses a file to AST, runs a formatter that produces a normalized token stream, re-emits source. Used both by the CLI and by CI to check that the tree is formatted.

### `compiler/doc/`
The `fuse doc` implementation. Walks the HIR, extracts doc comments, and produces HTML output.

### `compiler/repl/`
The `fuse repl` implementation. Drives an interactive session that JIT-compiles each input expression through the Stage 1 pipeline.

### `compiler/testrunner/`
The `fuse test` implementation. Discovers tests, compiles them, runs them, and reports results.

---

## 5. `runtime/`

The C11 runtime library. This directory contains **all** the C code in the repository outside of FFI-generated headers. It implements the ~40 entry points of §15 of the language guide.

### `runtime/include/fuse_rt.h`

The single header file that the Fuse compiler includes in every emitted C file. Declares every `fuse_rt_*` function in one place. The header's contents are frozen in the same sense as the language guide: adding a new entry point requires updating the guide and this directory together.

### `runtime/src/`

The implementation files, one per category:

- **`fuse_rt_mem.c`** — allocation (`fuse_rt_alloc`, `fuse_rt_alloc_aligned`, `fuse_rt_realloc`, `fuse_rt_free`, `fuse_rt_oom`).
- **`fuse_rt_panic.c`** — panic handlers (`fuse_rt_panic`, `fuse_rt_panic_with_loc`, `fuse_rt_abort`).
- **`fuse_rt_io.c`** — raw stdin/stdout/stderr.
- **`fuse_rt_process.c`** — exit, argc, argv.
- **`fuse_rt_file.c`** — raw file I/O wrappers.
- **`fuse_rt_thread.c`** — thread create/join/detach/yield.
- **`fuse_rt_sync.c`** — mutex, rwlock, cond.
- **`fuse_rt_tls.c`** — thread-local storage.
- **`fuse_rt_time.c`** — monotonic and wall clocks.

Each source file is self-contained and declares its internal helpers `static`. The runtime does not expose any symbols other than those in `fuse_rt.h`.

### `runtime/platform/`

Platform-specific implementation fragments:

- **`runtime/platform/posix/`** — POSIX branches for threads, file handles, and time. Uses `pthread`.
- **`runtime/platform/windows/`** — Windows branches. Uses Win32 directly.

The source files in `runtime/src/` `#include` the appropriate platform fragment via a single `#if defined(_WIN32)` conditional. There is no autoconf, no `configure` script, and no build-time code generation.

### `runtime/tests/`

C-level tests for the runtime. These run under a standard C test harness before any Fuse-level testing happens. If `runtime/tests/` fails, nothing downstream can be trusted.

---

## 6. `stdlib/`

The Fuse standard library, written in Fuse. Three tiers as described in §16 of the language guide.

### `stdlib/core/`

OS-free, always available. Core modules may only import other `core` modules and may only call runtime entry points from a `rt_bridge/*` file.

- **`prelude.fuse`** — re-exports the names that appear in every module without explicit import.
- **`option.fuse`** — `Option[T]` type and methods.
- **`result.fuse`** — `Result[T, E]` type and methods.
- **`ordering.fuse`** — `Ordering` enum.
- **`traits/`** — the Core trait set (one file per trait).
- **`primitive/`** — one file per primitive type, holding the methods on `Int`, `Float`, `Bool`, `Char`.
- **`string.fuse`** — `String` type and methods.
- **`list.fuse`** — `List[T]` type and methods.
- **`map.fuse`** — `Map[K, V]` (insertion-ordered) type and methods.
- **`set.fuse`** — `Set[T]` type and methods.
- **`hash/`** — `Hasher` trait and concrete hasher (SipHash).
- **`fmt/`** — `StringBuilder` and the low-level formatting interface.
- **`math.fuse`** — math functions on `F32`/`F64`.
- **`iter.fuse`** — the iterator protocol and combinators.
- **`atomic.fuse`** — `Atomic[T]` wrapper. Operations are emitted inline by the compiler; this file declares the type and its method signatures.
- **`rt_bridge/`** — the small set of `unsafe` files that call runtime entries. See §13.

### `stdlib/full/`

The hosted OS surface. Modules may import from `core` or from other `full` modules.

- **`io/`** — standard input/output/error streams.
- **`fs/`** — file system (`File`, `Dir`, `Path`).
- **`os/`** — environment variables, process control, command-line arguments.
- **`time/`** — monotonic clock, wall clock, durations.
- **`thread/`** — `spawn` and `ThreadHandle`.
- **`sync/`** — mutex, rwlock, cond, once, `Shared[T]`.
- **`chan/`** — `Chan[T]`.

### `stdlib/ext/`

Optional extensions. Each subdirectory is a separate module that a program imports explicitly. None of these is imported by default.

- **`json/`** — JSON parser and emitter.
- **`regex/`** — regular expressions.
- **`serde/`** — serialization traits and derive support.
- **`compress/`** — compression (gzip, zstd).
- **`crypto/`** — cryptography primitives.
- **`net/`** — TCP/UDP sockets, higher-level HTTP.

Ext modules are not required for a minimal conforming distribution; a packager may choose to ship only core and full.

---

## 7. `stage2/`

The Fuse source code for the self-hosted compiler. `stage2/src/main.fuse` is the entry point. The internal structure mirrors `compiler/` but is written in Fuse:

```
stage2/src/
├── main.fuse
├── lex/
├── parse/
├── ast/
├── resolve/
├── hir/
├── check/
├── liveness/
├── lower/
├── mir/
├── codegen/
└── driver/
```

The Stage 2 compiler is built by running `fuse build stage2/` with the Stage 1 compiler. The resulting binary is the Stage 2 compiler. The three-generation reproducibility test (`rules.md` §6.5) runs Stage 2 against itself and verifies the output is byte-identical.

Stage 2 does **not** have its own runtime or stdlib. It uses the same `runtime/` and `stdlib/` trees as any other Fuse program.

There is no `stage0/` and never will be. The language bootstrap begins at Stage 1 (Go) and terminates at Stage 2 (Fuse). Two stages, no more.

---

## 8. `tests/`

The global test tree. Most tests live next to the code they exercise (`rules.md` §6.7); `tests/` is for cross-cutting and end-to-end tests only.

### `tests/e2e/`

End-to-end tests: Fuse source → binary → run. Each test is a directory containing:

- `program.fuse` — the input.
- `expected.stdout` — the expected standard output.
- `expected.stderr` — the expected standard error.
- `expected.exit` — the expected exit code.
- `args` (optional) — command-line arguments.

The test harness compiles `program.fuse`, runs the result with the given `args`, and compares the captured output against the expected files. A mismatch is a test failure with a diff.

### `tests/bootstrap/`

The three-generation reproducibility harness. Builds Stage 2 with Stage 1, then builds Stage 2 with itself, then compares. This is the master bootstrap gate.

### `tests/property/`

Property-based tests for the IR lowering passes. Each file generates random valid input (AST or HIR) and verifies an observable invariant. Seeds are printed and reproducible (`rules.md` §6.4).

### `tests/fixtures/`

Shared corpora that multiple tests consume. Kept minimal. A fixture that is used by exactly one test should live next to that test, not here.

---

## 9. `examples/`

Example programs. Each is a full, buildable package:

- **`hello/`** — the minimum-viable program: print "hello" and exit.
- **`echo/`** — reads from stdin and writes to stdout.
- **`wordcount/`** — counts words in a file. Uses `full.fs`, `core.map`, `core.string`.
- **`http_server/`** — a small HTTP server. Uses `ext.net`. Post-day-one.
- **`concurrent_pipeline/`** — a producer-consumer pipeline using `Chan[T]` and `spawn`.

Examples are buildable with `fuse build <example>/` and run on every commit in CI as a smoke test.

---

## 10. `tools/`

Developer tools that support the build but are not part of the compiler.

### `tools/checklog/`

A Go program that verifies `docs/learning-log.md` is well-formed: entries are numbered sequentially, dates are parseable, no entry has been edited since it was first committed (determined by comparing the content hash to a frozen hash in a companion `learning-log.hashes` file).

### `tools/checkdoc/`

A Go program that walks the stdlib and verifies every public declaration has a `///` doc comment (`rules.md` §5.6).

### `tools/goldens/`

A helper for updating golden test files. Interactive and non-interactive modes. Writes an update explanation to a scratch file that gets incorporated into the commit message.

### `tools/repro/`

The build-determinism verifier. Runs a full `fuse build`, saves the artifact, runs `fuse build` again on the same tree, and diffs the two artifacts byte-for-byte. Used by CI to enforce `rules.md` §7.1.

### `tools/bench/`

A microbenchmark runner. Compiles and runs a set of benchmark programs, captures their timings, and writes a report. Benchmarks live in `tools/bench/benches/`.

---

## 11. `docs/`

The four documents that define the project:

- **`docs/language-guide.md`** — the language specification.
- **`docs/implementation-plan.md`** — the wave-by-wave plan.
- **`docs/rules.md`** — the discipline rules (this document's sibling).
- **`docs/repository-layout.md`** — this document.

And two additional files:

- **`docs/learning-log.md`** — the append-only learning log (`rules.md` §10).
- **`docs/adr/`** — architecture decision records. Each ADR is a file `NNNN-short-title.md`; the number is monotonic and assigned when the ADR is first committed.

No other files in `docs/`. No per-wave plan files; the implementation plan is one document. No per-feature design docs; those become ADRs.

---

## 12. `.ci/`

CI configuration.

- **`.ci/workflows/`** — CI provider-specific workflow files.
- **`.ci/scripts/`** — shell scripts invoked by workflows. Keeping logic in scripts (rather than inline in workflow YAML) means CI logic can be run locally for debugging.

The two workflows at a minimum:

- **`ci.yml`** — runs on every PR: `go build`, `go test`, `fuse check`, `fuse test`, bootstrap test, reproducibility test.
- **`nightly.yml`** — runs nightly: full cross-target build matrix, longer property-test runs.

CI setup must work on the following OSes: Linux, macOS, Windows. Each OS needs a C compiler installed; the workflow scripts handle installation via the OS package manager.

---

## 13. The `unsafe` bridge file list

`rules.md` §6.8 and §12.2 require that `#![forbid(unsafe)]` is the default in the stdlib, with exceptions listed by name here. The complete list of files permitted to use `unsafe { }` in the stdlib:

### In `stdlib/core/rt_bridge/`

- **`alloc.fuse`** — wraps `fuse_rt_alloc` / `fuse_rt_alloc_aligned` / `fuse_rt_realloc` / `fuse_rt_free`. Exports safe `allocate`, `deallocate`, and `reallocate` functions that use `Ptr[T]` internally.
- **`panic.fuse`** — wraps `fuse_rt_panic` and `fuse_rt_abort`. Exports safe `panic(msg: String)` and `abort()` functions.
- **`intrinsics.fuse`** — low-level intrinsics that the compiler emits calls to but that are declared in Fuse for documentation and type-checking purposes. Items such as the integer checked-arithmetic helpers.

### In `stdlib/full/io/`

- **`stdin.fuse`** — wraps `fuse_rt_stdin_read` and `fuse_rt_stdin_eof`.
- **`stdout.fuse`** — wraps `fuse_rt_stdout_write`.
- **`stderr.fuse`** — wraps `fuse_rt_stderr_write`.

### In `stdlib/full/fs/`

- **`file.fuse`** — wraps `fuse_rt_file_*` entries.

### In `stdlib/full/os/`

- **`env.fuse`** — wraps environment variable access (via `fuse_rt_*` extensions as needed).
- **`process.fuse`** — wraps `fuse_rt_exit` and process control.
- **`args.fuse`** — wraps `fuse_rt_argc` / `fuse_rt_argv`.

### In `stdlib/full/time/`

- **`clock.fuse`** — wraps `fuse_rt_time_now_nanos` and `fuse_rt_wall_now_nanos`.

### In `stdlib/full/thread/`

- **`spawn.fuse`** — wraps `fuse_rt_thread_create` / `fuse_rt_thread_join` / `fuse_rt_thread_detach`.

### In `stdlib/full/sync/`

- **`mutex.fuse`** — wraps `fuse_rt_mutex_*`.
- **`rwlock.fuse`** — wraps `fuse_rt_rwlock_*`.
- **`cond.fuse`** — wraps `fuse_rt_cond_*`.

### In `stdlib/full/chan/`

- **`chan.fuse`** — uses `unsafe` internally for the queue implementation, protected by the runtime's mutex primitives.

**No other file in `stdlib/` is permitted to contain `unsafe { }`.** A PR that introduces an `unsafe { }` block in a file not on this list MUST either (a) delete the `unsafe { }`, or (b) add the file to this list with an ADR explaining why.

---

## 14. Adding a new top-level directory

Top-level directories are deliberately few. Adding one requires:

1. **An ADR in `docs/adr/`** naming the directory, its purpose, and why the work does not fit into an existing directory.
2. **A guide update** if the new directory introduces a user-visible concept.
3. **A rules update** if the new directory has different discipline (e.g., different `unsafe` policy).
4. **A layout update** (this document) adding the directory to the tree in §1 and a section describing it.
5. **Review** by at least one maintainer who did not write the ADR.

A contributor SHOULD assume the answer is "fit it into an existing directory" and only pursue a new top-level directory once that has been tried and clearly fails.

---

*End of repository layout.*
