# Fuse Implementation Plan

> **Status:** normative. This document is the master plan for building the Fuse compiler, runtime, and standard library, from an empty repository to three-generation self-hosting and beyond.
>
> **Companion documents** (same directory):
> - `language-guide.md` — the language specification.
> - `rules.md` — discipline rules. Read on every session.
> - `repository-layout.md` — directory and file placement.
>
> **Task IDs.** Every task in this plan has an ID of the form `W<wave>.<phase>.<task>`. For example, `W03.2.4` is Wave 3, Phase 2, Task 4. Branch names, commit messages, and learning log entries reference these IDs. The format is deliberately compact so the ID fits into a commit subject line.

---

## Table of contents

- [Overview](#overview)
- [Waves at a glance](#waves-at-a-glance)
- [Wave 0 — Project scaffolding](#wave-0--project-scaffolding)
- [Wave 1 — Lexer](#wave-1--lexer)
- [Wave 2 — Parser and AST](#wave-2--parser-and-ast)
- [Wave 3 — Name resolution and module graph](#wave-3--name-resolution-and-module-graph)
- [Wave 4 — TypeTable, HIR infrastructure, pass manifest](#wave-4--typetable-hir-infrastructure-pass-manifest)
- [Wave 5 — Type checker](#wave-5--type-checker)
- [Wave 6 — Liveness and ownership analysis](#wave-6--liveness-and-ownership-analysis)
- [Wave 7 — HIR to MIR lowering](#wave-7--hir-to-mir-lowering)
- [Wave 8 — Runtime library](#wave-8--runtime-library)
- [Wave 9 — C11 code generator](#wave-9--c11-code-generator)
- [Wave 10 — `cc` driver and linking](#wave-10--cc-driver-and-linking)
- [Wave 11 — Top-level driver and CLI](#wave-11--top-level-driver-and-cli)
- [Wave 12 — Core stdlib](#wave-12--core-stdlib)
- [Wave 13 — Full stdlib](#wave-13--full-stdlib)
- [Wave 14 — Stage 2 port](#wave-14--stage-2-port)
- [Wave 15 — Bootstrap gate](#wave-15--bootstrap-gate)
- [Wave 16 — Ext stdlib](#wave-16--ext-stdlib)
- [Wave 17 — Targets and cross-compilation](#wave-17--targets-and-cross-compilation)
- [Wave 18 — Beyond day one](#wave-18--beyond-day-one)

---

## Overview

### Working philosophy

1. **Correctness first.** Every wave produces code that passes its invariant walkers and its tests. A wave does not advance until its exit criteria are met.
2. **Structural bugs over symptomatic fixes.** When a bug is found, the fix is in the pass, not in the test.
3. **No workarounds.** See `rules.md` §4.2.
4. **Single liveness computation, single type table, single pass manifest.** Any of these duplicated is a regression.
5. **Stdlib is the stress test.** Stdlib work in Waves 12 and 13 will expose compiler bugs. Those are compiler fixes, not library workarounds (`rules.md` §4.1).

### How to use this document

- **Plan alignment.** At the start of a session, open this document, locate the wave in progress, and pick a task. Work on **one task at a time**. If the task turns out to span more work than its description suggests, stop, update this document, and split it.
- **Task tracking.** When a task is in progress, the working branch is named `<task-id>/<short-description>` (e.g., `W05.2.3/trait-resolution`). When the task is done, merge with `--no-ff` and delete the branch.
- **Exit criteria.** A wave has an exit criteria section. The wave is complete only when every criterion is satisfied. Partial completion blocks the next wave.
- **Ungrouped fixes.** Bug fixes discovered during a wave belong to that wave. A bug in Wave 5 code found while working Wave 6 gets a Wave 5 task retroactively added.

### Terminology

- **Wave** — a coarse-grained unit of work with a theme (e.g. "lexer", "stdlib core").
- **Phase** — a subset of a wave that forms a cohesive block. Phases are ordered inside a wave; a later phase usually depends on earlier phases.
- **Task** — a single unit of work, committable in one session.
- **Definition of Done (DoD)** — the per-task exit criterion. A task with no DoD is a rejected task.
- **Invariant walker** — a debug-time pass that asserts IR invariants after another pass runs. Owned by `compiler/passmgr`.
- **Golden file** — a checked-in file against which test output is compared byte-for-byte.

### Notation

Every task is listed in the form:

```
W<wave>.<phase>.<task>  <one-line description>
                        DoD: <definition of done>
```

For example:

```
W05.2.3  Implement trait method lookup during check pass.
         DoD: check pass finds method impls for the Core trait set;
              test `check/trait-method-lookup.fuse` passes.
```

The DoD is checkable by a reviewer without requiring the contributor to explain.

---

## Waves at a glance

| Wave | Theme                                  | Phases | Entry criterion              | Exit criterion                              |
|------|----------------------------------------|--------|------------------------------|---------------------------------------------|
| 0    | Project scaffolding                    | 6      | —                            | `make all` runs; CI green                   |
| 1    | Lexer                                  | 6      | Wave 0 done                  | Lexer handles every token kind              |
| 2    | Parser and AST                         | 8      | Wave 1 done                  | Parser handles every construct in the guide |
| 3    | Name resolution & module graph         | 5      | Wave 2 done                  | Module graph built for stdlib core          |
| 4    | TypeTable, HIR, pass manifest          | 5      | Wave 3 done                  | HIR builders enforced, pass mgr running     |
| 5    | Type checker                           | 8      | Wave 4 done                  | Checker passes all type-system tests        |
| 6    | Liveness & ownership                   | 6      | Wave 5 done                  | ASAP inserted, mutref/move enforced         |
| 7    | HIR → MIR lowering                     | 5      | Wave 6 done                  | MIR property test passes                    |
| 8    | Runtime library                        | 12     | Wave 0 done (parallel to 1-7)| All `fuse_rt_*` entries implemented         |
| 9    | C11 code generator                     | 9      | Waves 7 and 8 done           | Hello-world compiles and runs               |
| 10   | `cc` driver and linking                | 6      | Wave 9 done                  | End-to-end `fuse build` produces a binary   |
| 11   | Top-level driver and CLI               | 10     | Wave 10 done                 | All 9 subcommands work                      |
| 12   | Core stdlib                            | 15     | Wave 11 done                 | Core tier complete, all tests pass          |
| 13   | Full stdlib                            | 8      | Wave 12 done                 | Full tier complete, threaded tests pass     |
| 14   | Stage 2 port                           | 11     | Wave 13 done                 | stage1 compiles stage2 successfully         |
| 15   | Bootstrap gate                         | 5      | Wave 14 done                 | Three-generation reproducibility            |
| 16   | Ext stdlib                             | 6      | Wave 15 done                 | Ext modules ship                            |
| 17   | Targets and cross-compilation          | 6      | Wave 15 done (parallel to 16)| All 6 targets produce running binaries      |
| 18   | Beyond day one                         | open   | Wave 17 done                 | —                                           |

Waves 1 through 7 are sequential: later waves depend directly on earlier ones. Wave 8 (runtime) can be developed in parallel with Waves 1–7 because it is pure C11 and has no compiler dependencies. Waves 16 and 17 can run in parallel with each other after Wave 15.

---

## Wave 0 — Project scaffolding

**Goal.** Stand up the repository with the structure described in `repository-layout.md`, a working Go module, a Makefile that runs, a CI skeleton, tooling for docs and the learning log, and a golden test harness. No compiler code yet — this wave produces the soil the compiler grows in.

**Exit criteria.**
- Running `make all` from a clean checkout builds every Go package in `compiler/` and `cmd/` and produces no errors.
- Running `make test` runs the (currently empty) Go tests and returns success.
- Running `tools/checklog` on the empty `docs/learning-log.md` returns success.
- Running `tools/checkdoc` on the empty `stdlib/` returns success.
- CI green on a push to `main`.
- `go.sum` is empty — zero external Go dependencies.
- The four documents under `docs/` are checked in.

### Phase 0 — Repository initialization

```
W00.0.1  Create the git repository with initial branch `main`.
         DoD: empty repo, one commit with `.gitignore`, `README.md`, `LICENSE`.

W00.0.2  Add the four normative documents to `docs/`.
         DoD: `docs/language-guide.md`, `docs/implementation-plan.md`,
              `docs/rules.md`, `docs/repository-layout.md` present and readable.

W00.0.3  Create `docs/learning-log.md` with its header and L000 entry.
         DoD: L000 is a meta-entry documenting the log format.

W00.0.4  Create the top-level directory tree from `repository-layout.md` §1.
         DoD: every directory in §1 exists, each with a `.keep` file if empty.

W00.0.5  Write the `README.md` quickstart.
         DoD: README has 1-paragraph introduction and pointers to the four docs.
```

### Phase 1 — Go module

```
W00.1.1  Initialize the Go module at `github.com/<owner>/fuse`.
         DoD: `go.mod` present; `go build ./...` runs without errors.

W00.1.2  Choose the minimum Go version and pin it in `go.mod`.
         DoD: `go.mod` specifies a concrete Go toolchain version.

W00.1.3  Create empty Go packages for every subdirectory under `compiler/`.
         DoD: each package has a `doc.go` with a one-paragraph description.

W00.1.4  Create the `cmd/fuse/main.go` stub.
         DoD: `go run ./cmd/fuse` prints "not yet implemented" and exits 1.

W00.1.5  Configure `go test` to run the empty test suite.
         DoD: `go test ./...` passes with 0 tests.
```

### Phase 2 — Makefile

```
W00.2.1  Write the top-level Makefile.
         DoD: targets `all`, `stage1`, `runtime`, `clean`, `fmt`, `test` exist.

W00.2.2  Implement the `stage1` target (builds the Go compiler).
         DoD: `make stage1` produces a `fuse` binary in `build/`.

W00.2.3  Implement the `runtime` target (builds the C runtime to an object file).
         DoD: `make runtime` produces `build/runtime/fuse_rt.o`.

W00.2.4  Implement the `test` target.
         DoD: `make test` runs `go test ./...` and any runtime tests.

W00.2.5  Implement the `clean` target.
         DoD: `make clean` removes `build/` and leaves the tree pristine.

W00.2.6  Implement the `fmt` target.
         DoD: `make fmt` runs `gofmt -w` on Go files; `fuse fmt` on `.fuse` files
              (no-op until Wave 11).
```

### Phase 3 — CI skeleton

```
W00.3.1  Choose a CI provider and add `.ci/workflows/ci.yml`.
         DoD: CI runs on push and PR; initial workflow just runs `make all`.

W00.3.2  Add a matrix for Linux, macOS, and Windows runners.
         DoD: CI passes on all three OSes.

W00.3.3  Add a step that installs `cc` if missing.
         DoD: CI OS images have a C compiler available.

W00.3.4  Add the `make test` step.
         DoD: CI runs the empty test suite and reports success.

W00.3.5  Add the `repro` step that runs `tools/repro` (stubbed for now).
         DoD: CI calls `tools/repro --check` and succeeds on the empty compiler.
```

### Phase 4 — Tooling

```
W00.4.1  Implement `tools/checklog`.
         DoD: reads `docs/learning-log.md`, verifies entry numbering is monotonic,
              exits 0 on valid log and non-zero with a diagnostic on invalid.

W00.4.2  Implement `tools/checkdoc` as a stub.
         DoD: walks `stdlib/`, currently returns success on empty tree;
              full implementation in Wave 12.

W00.4.3  Implement `tools/repro` as a stub.
         DoD: prints "not yet wired" and exits 0; full implementation in Wave 10.

W00.4.4  Implement `tools/goldens` skeleton.
         DoD: CLI parses `--update` flag; full implementation delayed until tests
              need it (Wave 1).

W00.4.5  Wire `tools/checklog` into the pre-commit hook.
         DoD: committing an invalid learning log fails before the commit completes.
```

### Phase 5 — Golden test harness

```
W00.5.1  Write the `testutil/golden` Go package.
         DoD: exports `Compare(t *testing.T, got []byte, path string)`,
              with `--update` mode via env var.

W00.5.2  Write a one-test smoke test to verify the harness.
         DoD: a dummy test compares a string against a golden file and passes.

W00.5.3  Document the golden-update workflow in `tools/goldens/README.md`.
         DoD: the workflow is reproducible by another contributor.
```

**Wave 0 exit review.** Before moving to Wave 1, walk the exit criteria list and verify each one. File a Wave 0 completion entry in `docs/learning-log.md` if anything surprising came up.

---

## Wave 1 — Lexer

**Goal.** Produce a lexer that converts a UTF-8 `.fuse` source file into a stream of tokens covering every token kind described in `language-guide.md` §3. The lexer reports errors with spans, never panics on malformed input, and is deterministic.

**Entry criterion.** Wave 0 done.

**Exit criteria.**
- Every token kind in §3 of the guide has a unit test.
- The lexer handles every reserved keyword and active keyword.
- The lexer rejects BOM, rejects `.` alone (needs digit), and handles nested block comments.
- Property test: random token streams roundtrip through lexer → pretty-printer → lexer.
- No Go `map` in lexer data structures (deterministic iteration).

### Phase 0 — Token kinds

```
W01.0.1  Define the `Token` struct in `compiler/lex/token.go`.
         DoD: struct has Kind, Span, Text, Literal payload.

W01.0.2  Enumerate token kinds as Go constants in `compiler/lex/kind.go`.
         DoD: every token in the guide's §3.5 plus keyword tokens is present;
              `String()` method returns stable names.

W01.0.3  Write `kinds_test.go` verifying `Kind.String()` for every kind.
         DoD: passes; golden file of kind names checked in.

W01.0.4  Define `Span` and `Position` in `compiler/diagnostics/span.go`.
         DoD: Span has start/end positions; Position has file/line/column/offset.
```

### Phase 1 — Scanner core

```
W01.1.1  Implement the `Scanner` struct and constructor.
         DoD: `NewScanner(source []byte, filename string) *Scanner` compiles.

W01.1.2  Implement `Next() Token` for whitespace and comments.
         DoD: whitespace skipped; line and block comments consumed;
              nested block comments handled correctly.

W01.1.3  Implement line tracking.
         DoD: positions inside comments and strings have correct line/column.

W01.1.4  Reject BOM and CRLF handling (normalize to LF on input).
         DoD: a file with BOM produces a lex error;
              a file with CRLF is accepted and emits LF-positioned tokens.

W01.1.5  Handle EOF cleanly; emit a synthetic EOF token.
         DoD: calling `Next()` past end returns EOF indefinitely without panic.
```

### Phase 2 — Identifiers and keywords

```
W01.2.1  Lex identifiers according to the rule in guide §3.2.
         DoD: ASCII letters/digits/underscores; unicode rejected.

W01.2.2  Build the keyword table.
         DoD: active keywords of guide §19.1 classified as their token kind;
              reserved keywords of §19.2 classified as `ReservedKeyword`;
              lookup is deterministic and O(1) per identifier.

W01.2.3  Emit `ReservedKeyword` with the identifier text captured.
         DoD: parser can later report "'async' is reserved, not implemented".

W01.2.4  Lex the Never marker `!` as `TOK_BANG` and in `!=` as `TOK_NE`.
         DoD: `a!` lexes as ident + bang;
              `a != b` lexes as ident + ne + ident;
              `a!b` lexes as ident + bang + ident (parser rejects later).
```

### Phase 3 — Literals

```
W01.3.1  Lex integer literals with all bases and all suffixes.
         DoD: decimal, 0x, 0b, 0o; underscores between digits; suffixes i8..i64,
              u8..u64, usize, isize.

W01.3.2  Reject malformed integer literals.
         DoD: `0x` with no digits, trailing underscore, leading underscore after
              prefix → reported as lex error.

W01.3.3  Lex float literals with suffixes.
         DoD: `1.0`, `1.0e3`, `1.0e-3`, `1.0f32`, `1.0f64`.

W01.3.4  Lex string literals with escape sequences.
         DoD: `\n`, `\r`, `\t`, `\\`, `\"`, `\u{...}` all handled; invalid escapes
              produce lex errors.

W01.3.5  Lex raw string literals (`r"..."` and `r#"..."#`).
         DoD: embedded quotes with `#` delimiters work.

W01.3.6  Lex character literals (`'a'`, `'\n'`, `'\u{1F600}'`).
         DoD: single unicode scalar, not a byte; multi-char sequence rejected.

W01.3.7  Lex boolean literals (`true`/`false`) as keyword tokens.
         DoD: token kind distinguishes them from identifiers.
```

### Phase 4 — Operators and punctuation

```
W01.4.1  Lex multi-character operators (==, !=, <=, >=, <<, >>, &&, ||, ->, =>, ..,
         ..=, ::, +=, -=, *=, /=, %=, &=, |=, ^=, <<=, >>=).
         DoD: each has a distinct token kind; longest-match wins.

W01.4.2  Lex single-character operators and punctuation.
         DoD: +, -, *, /, %, &, |, ^, ~, <, >, =, !, ?, ., ,, ;, :, (, ), [, ], {,
              }, @, #.

W01.4.3  Reject `!x` at lex level? — no, that is a parse issue. Just emit `!` + ident.
         DoD: `!x` lexes as TOK_BANG + TOK_IDENT.
```

### Phase 5 — Error recovery and property tests

```
W01.5.1  Implement scanner error reporting that continues after errors.
         DoD: a file with multiple lex errors yields all of them in order.

W01.5.2  Write a token pretty-printer in `compiler/lex/pretty.go`.
         DoD: given a token stream, produces a canonical source-form string.

W01.5.3  Write a property test: for N random token streams, lex(pretty(stream))
         yields the same stream.
         DoD: 10,000 iterations with a fixed seed pass.

W01.5.4  Write a corpus test: lex every `.fuse` file in `tests/fixtures/lex/` and
         compare to the golden.
         DoD: fixture corpus has at least 30 files covering all token kinds;
              goldens checked in.

W01.5.5  Add the invariant walker: "no token has an empty span."
         DoD: runs in debug builds; invariant walker fails tests if violated.
```

**Wave 1 exit review.** Verify lexer coverage: every reserved keyword, every operator, every literal form, every error path. File learning log entries for any surprises.

---

## Wave 2 — Parser and AST

**Goal.** Hand-rolled recursive-descent parser that produces a correct `ast.File` for every construct in `language-guide.md`. The parser reports multiple errors per run (does not stop at the first one), emits structured diagnostics, and supports a pretty-printer for roundtrip testing.

**Entry criterion.** Wave 1 done.

**Exit criteria.**
- Every construct in the grammar summary (guide §21) has at least one positive and one negative test.
- Parser emits structured diagnostics with spans.
- Pretty-printer roundtrip succeeds on the stdlib-core corpus from Wave 12 (ahead-of-time: we use synthetic corpus here).
- No Go `map` in AST nodes.
- Invariant walker: "no AST node has a nil span" passes.

### Phase 0 — AST node kinds

```
W02.0.1  Define the `ast.Node` interface and `ast.Span()` method.
         DoD: every node type implements Node.

W02.0.2  Enumerate AST expression kinds as disjoint Go types.
         DoD: Literal, Ident, Binary, Unary, Call, FieldAccess, Index, If, Match,
              Loop, While, For, Return, Break, Continue, Block, StructLit,
              TupleLit, ArrayLit, Closure, Move, Unsafe present.

W02.0.3  Enumerate AST statement kinds.
         DoD: Let, Var, Assign, ExprStmt, ItemDecl present.

W02.0.4  Enumerate AST declaration kinds.
         DoD: FnDecl, StructDecl, EnumDecl, TraitDecl, ImplBlock, ConstDecl,
              TypeAlias, ExternBlock, Import, ModuleDoc, AttrBlock present.

W02.0.5  Enumerate AST type expression kinds.
         DoD: TypePath, PtrType, ArrayType, TupleType, FnType, NeverType present.

W02.0.6  Enumerate AST pattern kinds.
         DoD: Wildcard, LitPat, IdentPat, VariantPat, TuplePat, StructPat,
              RangePat, OrPat present.

W02.0.7  Invariant: AST nodes are constructible only via builder helpers that
         enforce "span is non-nil".
         DoD: `NewFnDecl(...)` requires span arg; direct struct literal rejected
              by linter.
```

### Phase 1 — File-level grammar

```
W02.1.1  Parse the optional `#![...]` attribute block.
         DoD: `#![forbid(unsafe)]` parses; unknown attributes rejected.

W02.1.2  Parse the optional module doc comment block (consecutive `//!` lines).
         DoD: doc lines attached to the file node; interleaved line comments OK.

W02.1.3  Parse `import` and `pub import` statements.
         DoD: `import a.b.c`, `import a.b.c as x`, `import a.b.{x, y}`,
              `pub import a.b` all parse.

W02.1.4  Enforce the file structure order.
         DoD: an import after a top-level decl is a parse error with a helpful
              message.

W02.1.5  Parse top-level decls into a slice.
         DoD: `fn main() {}` followed by a `pub fn greet()` parses into two decls.
```

### Phase 2 — Declarations

```
W02.2.1  Parse `fn` declarations with every modifier combination.
         DoD: `pub fn`, `unsafe fn`, generics, where clause, parameters with
              ownership keywords, return type.

W02.2.2  Parse `struct` declarations (plain and @value).
         DoD: `struct X { a: I32 }`, `@value struct X { a: I32 }`, trailing comma
              rejected.

W02.2.3  Parse `data class` declarations.
         DoD: `data class User(name: String, age: Int)` parses with positional
              fields.

W02.2.4  Parse `enum` declarations with every variant shape.
         DoD: payloadless, positional payload, struct-style payload.

W02.2.5  Parse `trait` declarations with supertrait bounds.
         DoD: `trait Hashable implements Equatable { ... }` parses.

W02.2.6  Parse `impl` blocks with and without `implements`.
         DoD: `impl X { }` and `impl X implements Printable { }` both parse.

W02.2.7  Parse `const` declarations.
         DoD: `pub const MAX: Int = 100;` parses.

W02.2.8  Parse `type` aliases.
         DoD: `pub type UserId = U64;` parses.

W02.2.9  Parse `extern { fn ... }` blocks.
         DoD: body contains only fn signatures with primitive types.
```

### Phase 3 — Expressions

```
W02.3.1  Implement a Pratt-style expression parser with the precedence table from
         guide §20.
         DoD: precedence is a lookup, not hardcoded in recursive descent;
              the table is tested directly.

W02.3.2  Parse literals (int, float, string, char, bool).
         DoD: each literal kind produces the right AST node; unit `()` is a
              separate TupleLit with zero elements.

W02.3.3  Parse identifiers, field access, index, method calls.
         DoD: `a.b[0].c(x)` parses into the right nested structure.

W02.3.4  Parse unary operators `-` and `not` and `~`.
         DoD: `not x`, `-5`, `~bits` parse correctly;
              `!x` is a parse error ("`!` is not a unary operator; use `not`").

W02.3.5  Parse binary operators with precedence.
         DoD: `a + b * c` yields `a + (b * c)`; `a < b and b < c` yields
              `(a < b) and (b < c)`; `a < b < c` is a parse error.

W02.3.6  Parse the `?` postfix operator.
         DoD: `parse(x)?.field` yields `(parse(x)?).field`.

W02.3.7  Parse `if`, `match`, `loop`, `while`, `for` as expressions.
         DoD: each returns an expression node; the grammar rejects
              `if x { a } without braces.

W02.3.8  Parse `return`, `break`, `continue`, `return expr`, `break expr`.
         DoD: all parse; `break expr` allowed only inside `loop` at check-time,
              not parse-time.

W02.3.9  Parse struct literals, tuple literals, array literals.
         DoD: `Point { x: 1, y: 2 }`, `(1, 2, 3)`, `[1, 2, 3]` parse;
              trailing commas rejected (lex-level check).

W02.3.10 Parse closure literals.
         DoD: `|x| x + 1`, `|x: I32, y: I32| x + y`, `move |x| x + base` parse.

W02.3.11 Parse the `move` expression form.
         DoD: `move x` parses at a use site; `move` followed by non-expr is a
              parse error.

W02.3.12 Parse `unsafe { ... }` blocks as expressions.
         DoD: the block expression has an UnsafeBlock wrapper.
```

### Phase 4 — Statements and blocks

```
W02.4.1  Parse `let` and `var` bindings with optional type ascription.
         DoD: `let x = 1;`, `let x: I32 = 1;`, `var y = 2;` parse.

W02.4.2  Parse assignment statements with all compound forms.
         DoD: `y = 7;`, `y += 1;`, `arr[0] = x;`, `p.field = 5;`.

W02.4.3  Reject chained assignment.
         DoD: `a = b = c` is a parse error.

W02.4.4  Parse block statements including the final-expression rule.
         DoD: `{ a; b; c }` has type-of `c`; `{ a; b; c; }` has type `()`.

W02.4.5  Parse a block inside an expression position.
         DoD: `let x = { let a = 1; a + 2 };` parses.
```

### Phase 5 — Patterns

```
W02.5.1  Parse wildcard, literal, and identifier patterns.
         DoD: `_`, `42`, `x` parse as expected.

W02.5.2  Parse enum variant patterns with positional and named payloads.
         DoD: `Some(x)`, `Point { x, y }` parse.

W02.5.3  Parse tuple patterns and struct patterns with rest.
         DoD: `(a, b)`, `Point { x, .. }` parse.

W02.5.4  Parse range patterns.
         DoD: `1..10`, `1..=10` parse.

W02.5.5  Parse or-patterns.
         DoD: `1 | 2 | 3 => ...` parses in match arms.

W02.5.6  Parse patterns with guards.
         DoD: `case Some(n) if n > 0 => ...` parses.
```

### Phase 6 — Error recovery

```
W02.6.1  Implement expression-level error recovery.
         DoD: a malformed expression in a function body does not stop the parser
              from reporting errors in later functions.

W02.6.2  Implement statement-level error recovery (synchronize on `;`).
         DoD: after a bad statement, parser advances to the next `;` and resumes.

W02.6.3  Implement top-level error recovery (synchronize on `fn`, `struct`, `enum`,
         `trait`, `impl`, `pub`).
         DoD: a bad top-level item is skipped and the next item parses.

W02.6.4  Limit the number of error reports to avoid cascading noise.
         DoD: after 20 errors, parser emits "too many errors" and stops.

W02.6.5  Add the "reserved keyword used as identifier" helpful message.
         DoD: `let async = 1;` produces
              "error: `async` is a reserved keyword for future use".
```

### Phase 7 — Pretty-printer and property tests

```
W02.7.1  Write the AST pretty-printer in `compiler/ast/pretty.go`.
         DoD: given an AST, produces source code that re-parses to an equal AST.

W02.7.2  Write `ast.Equal` for comparing two ASTs ignoring spans.
         DoD: used by property tests.

W02.7.3  Property test: parse(pretty(parse(src))) == parse(src).
         DoD: seeded test covers 10,000 random programs drawn from an AST
              generator.

W02.7.4  Add a corpus test on handwritten fixtures in `tests/fixtures/parse/`.
         DoD: at least 60 fixtures, one per grammar construct.

W02.7.5  Add the invariant walker: "AST nodes have non-empty spans".
         DoD: walker runs after parse in debug builds.
```

**Wave 2 exit review.** Roundtrip test on the synthetic corpus. Confirm parser reports multiple errors per run. Confirm no Go maps in `ast/`.

---

## Wave 3 — Name resolution and module graph

**Goal.** Given an AST per file, build the module graph, resolve imports, detect cycles, bind every identifier to a `SymbolId`, and produce the HIR skeleton (nodes without type metadata).

**Entry criterion.** Wave 2 done.

**Exit criteria.**
- Module graph for a synthetic multi-file package is built and resolved.
- Cyclic imports produce a precise diagnostic.
- Every identifier in the HIR skeleton is bound.
- `pub import` re-exports work correctly.

### Phase 0 — Symbol table

```
W03.0.1  Define `Symbol`, `SymbolId`, `Scope`, and `SymbolTable` types.
         DoD: symbols have kind (fn, struct, const, trait, module, local),
              a defining span, and a parent scope reference.

W03.0.2  Implement `Scope.Define(name, symbol) error`.
         DoD: duplicate definition yields an error with both spans.

W03.0.3  Implement `Scope.Lookup(name) (Symbol, bool)` with parent-scope walking.
         DoD: lookups walk up to module scope then stop.

W03.0.4  Implement the prelude scope.
         DoD: types `Int`, `String`, `Option`, etc. resolvable by default.
```

### Phase 1 — Module graph builder

```
W03.1.1  Walk a package root, discover all `.fuse` files.
         DoD: given `src/`, produces a list of `(module_path, file)` pairs.

W03.1.2  Parse each file to AST.
         DoD: parallel parsing supported; diagnostic ordering still deterministic.

W03.1.3  Build the module graph edges from `import` statements.
         DoD: edges are `(from_module, to_module)`; `pub import` edges also marked.

W03.1.4  Detect cycles via DFS.
         DoD: cycle produces "circular import: a → b → c → a" with spans.

W03.1.5  Topologically sort modules for resolution order.
         DoD: leaves (no imports) resolved first; diagnostics are order-stable.
```

### Phase 2 — Import resolution

```
W03.2.1  Resolve `import a.b.c` to a concrete module.
         DoD: path resolution follows `src/` layout; missing file yields error.

W03.2.2  Resolve `import a as x` and bind `x` in the importing scope.
         DoD: alias overrides module basename.

W03.2.3  Resolve selective imports `import a.{x, y}`.
         DoD: only listed names are visible; unlisted items rejected on use.

W03.2.4  Resolve `pub import` with re-export semantics.
         DoD: a consumer of the re-exporting module sees the re-exported names as
              if defined there.

W03.2.5  Error on importing a non-pub symbol.
         DoD: diagnostic names the symbol and its defining module.
```

### Phase 3 — Identifier binding

```
W03.3.1  Walk the AST of each module and bind identifiers to SymbolIds.
         DoD: every `Ident` AST node has a resolved `SymbolId` in the HIR skeleton.

W03.3.2  Handle scope introduction in blocks, function bodies, match arms.
         DoD: `let x = 1; { let x = 2; x }` yields inner x bound correctly.

W03.3.3  Handle pattern binding.
         DoD: `let (a, b) = pair;` introduces both a and b.

W03.3.4  Reject use-before-definition.
         DoD: `let y = x; let x = 1;` reports "x not found".

W03.3.5  Handle shadowing.
         DoD: `let x = 1; let x = "hello";` works and the second x is a different
              binding.
```

### Phase 4 — HIR skeleton

```
W03.4.1  Define HIR skeleton node types.
         DoD: structurally mirror the AST but carry `SymbolId`s instead of raw
              names; no Type field yet.

W03.4.2  Emit HIR skeleton nodes from the resolver output.
         DoD: resolver produces `hir.File` with symbol-resolved nodes.

W03.4.3  Mark HIR nodes with their source span.
         DoD: every HIR node has a span for diagnostics.

W03.4.4  Invariant walker: "every HIR ident has a bound symbol".
         DoD: walker runs after resolve; unbound ident is a hard error.
```

### Phase 5 — Tests

```
W03.5.1  Write unit tests for the symbol table.
         DoD: define, lookup, shadow, duplicate, not-found all covered.

W03.5.2  Write integration tests for multi-file resolution.
         DoD: tests use `tests/fixtures/resolve/` with multi-file packages.

W03.5.3  Write a cycle-detection test.
         DoD: a 3-module cycle produces the expected diagnostic.

W03.5.4  Write a re-export test.
         DoD: `pub import` chain across 3 modules works.

W03.5.5  Write the invariant walker test.
         DoD: a synthetic HIR with an unbound ident fails the walker.
```

**Wave 3 exit review.** Resolve a synthetic 5-file package. Verify that every HIR identifier is bound.

---

## Wave 4 — TypeTable, HIR infrastructure, pass manifest

**Goal.** Build the global `TypeTable`, the full HIR node set with per-node metadata, and the pass manifest framework. Every pass from Wave 5 onward registers with the manifest and runs under it.

**Entry criterion.** Wave 3 done.

**Exit criteria.**
- `TypeTable` interns types, returns stable `TypeId`s, and supports structural queries.
- HIR node constructors enforce the "metadata present" invariant.
- Pass manifest computes a topological order and runs passes in it.
- Invariant walker framework is in place.

### Phase 0 — TypeTable

```
W04.0.1  Define `TypeId` as `uint32` and `TypeKind` as a Go enum.
         DoD: kinds include Primitive, Struct, Enum, Trait, Fn, Ptr, Array, Tuple,
              Generic, Never, Unit, Unknown.

W04.0.2  Implement `TypeTable.Intern(key TypeKey) TypeId`.
         DoD: calling Intern with the same key returns the same TypeId;
              different keys return different TypeIds.

W04.0.3  Implement `TypeTable.Kind(id)`, `TypeTable.Fields(id)`, etc. accessors.
         DoD: accessor results are stable and deterministic.

W04.0.4  Handle parameterized types (monomorphization keys).
         DoD: `List[I32]` and `List[U32]` intern to different TypeIds.

W04.0.5  Seed the table with primitive types during init.
         DoD: `TypeTable.Int32()` and friends return stable well-known TypeIds.

W04.0.6  Write unit tests for interning and accessors.
         DoD: deterministic across runs; benchmarks tracked.
```

### Phase 1 — HIR node set

```
W04.1.1  Define the full HIR node set as disjoint Go types.
         DoD: HIR expression, statement, and declaration kinds enumerated,
              frozen against the guide's feature list.

W04.1.2  Add per-node metadata fields: `Type: TypeId`, `Ctx: OwnerCtx`,
         `LiveAfter: LiveSet`, `Owning: Bool`, `DivergesHere: Bool`.
         DoD: every HIR node has those fields, not optional.

W04.1.3  Make HIR constructors private to the `hir` package.
         DoD: user of `hir.Expr` cannot construct one with nil metadata.

W04.1.4  Provide builder helpers that accept metadata as required arguments.
         DoD: `hir.NewLet(span, pat, expr, typeId)` — typeId not nullable.

W04.1.5  Implement `hir.Walk` for traversal.
         DoD: a visitor interface visits every node kind once per call.
```

### Phase 2 — Pass manifest framework

```
W04.2.1  Define `Pass` interface with `Reads() []MetadataKey`, `Writes() []MetadataKey`,
         `Run(ctx) error`.
         DoD: compile-time declarations; no reflection.

W04.2.2  Implement `Manifest.Register(pass)`.
         DoD: passes register themselves in `init()`.

W04.2.3  Implement `Manifest.ExecutionOrder()` via topological sort.
         DoD: cycles yield a build-time error; order is deterministic.

W04.2.4  Implement `Manifest.Run(ctx)`.
         DoD: runs passes in order; short-circuits on first error.

W04.2.5  Write a test: pass A writes X, pass B reads X. The manifest must order
         A before B.
         DoD: verification test passes; removing the declared dependency fails.

W04.2.6  Invariant: a pass that reads a metadata key not written by any earlier
         pass is a build error.
         DoD: test enforces this.

W04.2.7  Invariant: a pass that writes a metadata key already written by an
         earlier pass is a build error unless it declares `--mutates`.
         DoD: test enforces this.
```

### Phase 3 — Invariant walker framework

```
W04.3.1  Define `InvariantWalker` interface: `Walk(hir) error`.
         DoD: a walker reports a list of violations with node spans.

W04.3.2  Add a mechanism to run walkers automatically after each pass in debug
         builds.
         DoD: `Manifest.RunWithInvariants()` runs every registered walker after
              its associated pass.

W04.3.3  Write a baseline walker: "every HIR node has non-nil span".
         DoD: passes with a synthetic test.

W04.3.4  Write a walker: "every HIR node has TypeId != Unknown after checker".
         DoD: registered against the checker pass (stubbed; full check in Wave 5).

W04.3.5  Wire invariant walker failures into CI.
         DoD: a violated invariant in a test run is a failing build.
```

### Phase 4 — Tests

```
W04.4.1  Unit tests for TypeTable interning.
         DoD: deterministic; covers all kinds.

W04.4.2  Unit tests for HIR builder invariants.
         DoD: constructing a HIR node without metadata fails at the type level.

W04.4.3  Integration test for the pass manifest framework on a 3-pass pipeline.
         DoD: topological order correct; metadata dependencies enforced.

W04.4.4  Test for the invariant walker framework.
         DoD: a deliberately broken HIR fails the walker.
```

**Wave 4 exit review.** Verify that the pass manifest is in place and that HIR cannot be constructed without metadata. This wave is foundational; do not advance until it is rock-solid.

---

## Wave 5 — Type checker

**Goal.** The check pass populates `Type` metadata on every HIR node, verifies trait bounds, checks pattern exhaustiveness, and enforces the match arm unification rules U1–U7. This is the wave that turns a name-resolved HIR skeleton into a fully typed HIR.

**Entry criterion.** Wave 4 done.

**Exit criteria.**
- Every HIR node has `Type != Unknown` after check.
- Match exhaustiveness works for enums, bools, tuples, and struct patterns.
- Trait resolution finds the right impl and reports a precise error otherwise.
- Return-type consistency is enforced: a `return expr` whose type does not match the function's declared return type is a compile error. This check is load-bearing — a checker that does not enforce it silently admits programs that codegen to invalid C.
- Every U1–U7 case is covered by tests.

### Phase 0 — Type inference skeleton

```
W05.0.1  Implement `check.Infer(hir.Expr) TypeId`.
         DoD: dispatches on HIR expression kind, returns a TypeId.

W05.0.2  Handle literal expressions.
         DoD: int literal with no suffix gets `Int`; suffixed gets the suffix's
              type.

W05.0.3  Handle identifier expressions.
         DoD: looks up the symbol's type in the environment.

W05.0.4  Handle block expressions (type = type of final expression).
         DoD: block ending in `;` has type `()`.

W05.0.5  Handle if-expressions (both branches must unify).
         DoD: unification failure reports both branches' types and spans.
```

### Phase 1 — Binary and unary operators

```
W05.1.1  Type-check arithmetic operators.
         DoD: both operands must be the same numeric type; mismatch is a hard
              error with no implicit conversion.

W05.1.2  Type-check bitwise operators.
         DoD: integer only; same type on both sides.

W05.1.3  Type-check comparison operators.
         DoD: both operands implement `Equatable` (for `==`/`!=`) or `Comparable`
              (for `<`/`<=`/etc.); result is `Bool`.

W05.1.4  Type-check logical operators `and`, `or`, `not`.
         DoD: operands are `Bool`; result is `Bool`.

W05.1.5  Type-check unary `-` and `~`.
         DoD: `-` requires signed numeric; `~` requires integer.

W05.1.6  Type-check the `?` postfix operator.
         DoD: operand must be `Result[T, E]` or `Option[T]`; short-circuit branch
              uses the function's declared return type.
```

### Phase 2 — Functions and calls

```
W05.2.1  Type-check function declarations.
         DoD: parameter types captured into environment; return type registered.

W05.2.2  Type-check call expressions.
         DoD: callee's function type is known; each argument's type must match the
              parameter type; arity mismatch is a hard error.

W05.2.3  Enforce ownership keyword matching at the call site.
         DoD: calling `sort(xs)` where `sort` expects `mutref` is a hard error
              ("annotate call site `mutref xs`"); this is a structural check.

W05.2.4  Type-check method calls via dot notation.
         DoD: receiver's type determines the impl scope; method lookup returns the
              right signature.

W05.2.5  Type-check associated functions via `Type.fn_name(args)`.
         DoD: `Point.new(1.0, 2.0)` works without a receiver.

W05.2.6  Enforce return-type consistency.
         DoD: `return x` where x's type does not match the function's declared
              return type is a hard error with both spans.
         Note: this check is load-bearing. A checker that does not enforce it
              admits programs whose codegen is invalid. Tests MUST cover this
              path explicitly, including the subtle case of a function whose
              declared return type is `()` but whose body returns a non-unit
              expression.

W05.2.7  Handle divergence: functions whose body ends in `panic`, `return`,
         or `loop` with no `break` have type `!`.
         DoD: a function declared `-> !` whose body is non-divergent is a hard
              error.
```

### Phase 3 — Generics and trait resolution

```
W05.3.1  Implement type parameter binding.
         DoD: `fn swap[T](...)` introduces `T` into the local type environment.

W05.3.2  Implement trait bound checking.
         DoD: `fn sort[T: Comparable](...)` enforces `Comparable` on `T` at every
              call site.

W05.3.3  Implement trait impl lookup.
         DoD: given `T` and trait `Printable`, find the `impl T implements
              Printable` block in the current or imported modules.

W05.3.4  Implement monomorphization keys.
         DoD: each distinct instantiation of a generic has a unique TypeId keyed
              by (fn_symbol, type_args).

W05.3.5  Error on missing trait impl.
         DoD: `let s = x.sortedKeys()` where `K` does not implement `Comparable`
              reports "K does not implement `Comparable`; required by sortedKeys".

W05.3.6  Handle multiple trait bounds with `+`.
         DoD: `T: Comparable + Hashable` checked against both.

W05.3.7  Handle `where` clauses.
         DoD: long bound lists via `where` work.
```

### Phase 4 — Pattern matching

```
W05.4.1  Type-check literal patterns.
         DoD: literal type must match the scrutinee type.

W05.4.2  Type-check variant patterns.
         DoD: enum variant lookup finds the variant and binds its payload.

W05.4.3  Type-check struct and tuple patterns.
         DoD: all fields match; rest `..` covers the unlisted fields.

W05.4.4  Type-check or-patterns.
         DoD: all alternatives bind the same names with the same types.

W05.4.5  Check match arm unification rules U1–U7.
         DoD: each rule has a dedicated test file under tests/fixtures/check/u*.

W05.4.6  Check exhaustiveness.
         DoD: a match over an enum that misses a variant is a hard error; the
              diagnostic names the missing variant.

W05.4.7  Check reachability.
         DoD: an unreachable arm is a hard error.

W05.4.8  Guards do not contribute to exhaustiveness.
         DoD: test case: `case Some(n) if n > 0 => ...` alone is not exhaustive
              over `Option[Int]`.
```

### Phase 5 — Special-type handling

```
W05.5.1  Type `!` is a subtype of every other type.
         DoD: `let x: Int = panic(...)` type-checks.

W05.5.2  Unit type `()` unification.
         DoD: block ending in `;` has type `()`; caller must accept `()`.

W05.5.3  Implicit `Some(...)` wrapping for rule U4.
         DoD: `match x { case Some(n) => n, case None => 0 }` has type `Int`,
              not `Option[Int]`, because the bare-Int arm is implicitly wrapped
              — wait, U4 says the other direction. Implement exactly what U4 says.

W05.5.4  Error on ambiguous type inference.
         DoD: `let x = []` (empty list with no context) is a hard error with a
              "annotate the type of x" help.
```

### Phase 6 — Diagnostics

```
W05.6.1  Emit every check-phase error with spans, expected type, actual type, and
         help text.
         DoD: every error kind has a structured rendering.

W05.6.2  Error codes assigned: E0001–E0099 for the checker.
         DoD: each error kind has a unique code; `fuse help error EXXXX` works
              (stubbed until Wave 11).

W05.6.3  Consolidate "cascaded" errors.
         DoD: a type error in one expression does not produce 5 downstream errors;
              follow-on uses of the failed expression are suppressed.

W05.6.4  Write golden files for each error path.
         DoD: tests/fixtures/check/errors/*.fuse with matching
              tests/fixtures/check/errors/*.golden.
```

### Phase 7 — Tests

```
W05.7.1  Per-expression test fixtures covering every HIR expression kind.
         DoD: one positive and one negative fixture per kind.

W05.7.2  Per-operator fixtures.
         DoD: every operator tested; type errors tested.

W05.7.3  U1–U7 fixtures (one positive and one negative per rule).
         DoD: 14 fixtures, each mapped to its rule.

W05.7.4  Return-type consistency fixture.
         DoD: a dedicated regression fixture `check/return-type-mismatch.fuse`
              covers the case where a function's `return` expression disagrees
              with its declared return type.

W05.7.5  Trait-bound-violation fixture.
         DoD: generic call with unsatisfied bound produces the expected
              diagnostic.

W05.7.6  Invariant walker after check: "every HIR node has TypeId != Unknown".
         DoD: walker runs and passes on the full test corpus.
```

**Wave 5 exit review.** Verify that return-type consistency is checked — do not move on until the regression fixture exists and the check fires for it.

---

## Wave 6 — Liveness and ownership analysis

**Goal.** The liveness pass computes `LiveAfter` metadata for every HIR node, identifies the last use of every binding, marks ownership transfers, detects escaping closures, and enforces the `mutref` call-site and `move` closure-capture rules.

**Entry criterion.** Wave 5 done.

**Exit criteria.**
- Every HIR node has a correct `LiveAfter` set.
- ASAP destruction sites identified and attached to the HIR.
- `mutref` required at the call site enforced.
- Escaping closures require explicit `move` captures.
- Single liveness computation — no recomputation anywhere downstream.

### Phase 0 — Liveness analysis

```
W06.0.1  Implement the liveness lattice: a set of `SymbolId` per program point.
         DoD: uses a deterministic sorted slice, not a Go map.

W06.0.2  Implement backward dataflow for liveness.
         DoD: per-function fixpoint; iterations bounded by program size.

W06.0.3  Handle branches (if, match).
         DoD: liveness at a branch join is the union of live-sets per branch.

W06.0.4  Handle loops (while, for, loop).
         DoD: fixpoint converges for loop-carried uses.

W06.0.5  Test liveness on 20 synthetic functions.
         DoD: goldens match hand-computed sets.
```

### Phase 1 — LiveAfter metadata

```
W06.1.1  Populate `LiveAfter` on every HIR node after the liveness pass.
         DoD: `LiveAfter` is the set of symbols live after this node executes.

W06.1.2  Identify last-use positions.
         DoD: for each binding, the last node where it is live is flagged.

W06.1.3  Handle partial moves (field extraction from owned).
         DoD: `let x = obj.field;` marks `obj` as partially moved; subsequent use
              of `obj.other_field` is still allowed; use of `obj` as a whole is
              rejected.

W06.1.4  Invariant walker: "every HIR node has a non-nil LiveAfter set".
         DoD: walker runs after the liveness pass.
```

### Phase 2 — `mutref` enforcement

```
W06.2.1  Check that `mutref` parameters are annotated at the call site.
         DoD: `sort(xs)` on a function `fn sort(xs: mutref ...)` is a hard error
              with the fix suggestion.

W06.2.2  Check that the argument is a place expression.
         DoD: `sort(mutref (foo() or bar()))` is a hard error.

W06.2.3  Check that the argument is actually mutable.
         DoD: `sort(mutref someLet)` — where `someLet` is a `let` not `var` — is
              a hard error.

W06.2.4  Write the test fixtures.
         DoD: tests/fixtures/ownership/mutref/*.fuse cover each case.
```

### Phase 3 — Escape analysis

```
W06.3.1  Identify closures.
         DoD: every closure in the HIR is marked.

W06.3.2  Classify each closure as escaping or non-escaping.
         DoD: a closure that flows into a return, a struct field, a `spawn` call,
              or a channel send is escaping; otherwise non-escaping.

W06.3.3  For escaping closures, verify every capture is explicitly `move`.
         DoD: an implicit capture in an escaping closure is a hard error with the
              fix suggestion "add `move` at the capture site or use `move |args|
              body`".

W06.3.4  Non-escaping closures may capture by `ref` or `mutref` implicitly.
         DoD: `forEach(ref xs, |x| print(x))` works without `move`.

W06.3.5  Handle nested closures.
         DoD: an outer escaping closure with an inner non-escaping closure works
              as expected.

W06.3.6  Tests: tests/fixtures/ownership/closures/*.fuse.
         DoD: 10+ cases covering escaping, non-escaping, nested, partial capture.
```

### Phase 4 — Move insertion

```
W06.4.1  For every `owned` parameter, insert a move at the last use of the
         argument binding.
         DoD: HIR is annotated with an explicit "move here" marker; downstream
              use of the binding is rejected by a later walker.

W06.4.2  For explicit `move x` at a use site, validate the binding still has
         ownership.
         DoD: `move x` after `move x` is a hard error.

W06.4.3  For `owned` return values, mark the return expression as an ownership
         transfer.
         DoD: the callee's local is moved out into the caller's scope.

W06.4.4  Tests: tests/fixtures/ownership/move/*.fuse.
         DoD: 10+ cases covering implicit, explicit, and double-move.
```

### Phase 5 — ASAP destruction

```
W06.5.1  For each owned binding, determine the point at which it is last live.
         DoD: that point is the destruction site.

W06.5.2  Attach destruction markers to the HIR.
         DoD: every owned binding has exactly one destruction marker unless it
              was moved out.

W06.5.3  Handle branching: a binding destroyed in one branch but not another
         is destroyed at the branch join on the branch that did not destroy it.
         DoD: a test case exercises this.

W06.5.4  Destruction order at a single site is reverse of declaration order.
         DoD: tested.

W06.5.5  Invariant walker: "every owned binding is destroyed on every path".
         DoD: walker runs after ASAP; missing destruction is a hard error.
```

### Phase 6 — Tests

```
W06.6.1  Unit tests for the liveness lattice.
         DoD: union, intersection, subset ops covered.

W06.6.2  Fixture corpus tests.
         DoD: every fixture under tests/fixtures/ownership/ passes.

W06.6.3  Property test: random HIR programs have at most one destructor call per
         owned binding on every path.
         DoD: 10,000 iterations with a fixed seed pass.

W06.6.4  Invariant walker is wired into debug builds.
         DoD: CI fails if a walker fires on any test program.
```

**Wave 6 exit review.** Verify that the single liveness computation covers everything downstream. There MUST NOT be a second liveness computation anywhere in the compiler after this wave lands.

---

## Wave 7 — HIR to MIR lowering

**Goal.** Transform typed, ownership-annotated HIR into MIR that is flat, explicit about drops and moves, and close to C. MIR is the IR the codegen consumes.

**Entry criterion.** Wave 6 done.

**Exit criteria.**
- Every HIR construct has a corresponding MIR lowering.
- MIR has explicit `Drop`, `Move`, and `Borrow` instructions.
- Property test: HIR → MIR → interpreter yields semantically equivalent results on a random corpus.
- MIR invariant walker runs after lowering.

### Phase 0 — MIR node set

```
W07.0.1  Define MIR instruction types.
         DoD: disjoint Go types for MIR are present: Assign, Call, Drop, Move,
              Borrow, Branch, Goto, Return, AllocStack, AllocHeap, Load, Store.

W07.0.2  Define `MirBlock` (basic block) and `MirFn`.
         DoD: basic blocks have exactly one terminator; functions are a list of
              blocks.

W07.0.3  Define MIR operand types: constant, local, parameter, temporary.
         DoD: every operand has a TypeId.

W07.0.4  Builder helpers for constructing MIR with invariants.
         DoD: constructing a block without a terminator fails at the type level.
```

### Phase 1 — Expression lowering

```
W07.1.1  Lower literal expressions.
         DoD: produces a constant operand.

W07.1.2  Lower binary operators.
         DoD: produces a temporary with the operation's result.

W07.1.3  Lower calls.
         DoD: argument evaluation order fixed (left-to-right).

W07.1.4  Lower method calls to function calls with self as first arg.
         DoD: `x.foo(y)` → `foo(x, y)` in MIR.

W07.1.5  Lower struct literals.
         DoD: each field assigned in declaration order.

W07.1.6  Lower closure literals.
         DoD: escaping closures lower to a heap-allocated environment struct and
              a function pointer; non-escaping lower to an inline struct.
```

### Phase 2 — Control flow lowering

```
W07.2.1  Lower `if`/`else` to basic blocks with conditional branches.
         DoD: a single-expression if yields 3 blocks (then, else, join).

W07.2.2  Lower `while` to a loop with a back-edge.
         DoD: loop header, body, exit.

W07.2.3  Lower `for` to the iterator protocol.
         DoD: `for x in seq { ... }` lowers to a call to `seq.next()` and a
              match on the result.

W07.2.4  Lower `match` to a decision tree.
         DoD: the decision tree is deterministic and uses dense switches where
              scrutinee type allows.

W07.2.5  Lower `loop` to an infinite loop block.
         DoD: `break expr` materializes the value and jumps to an exit block.

W07.2.6  Lower `return` to a MIR return terminator.
         DoD: the current basic block ends with Return.

W07.2.7  Lower `break` and `continue` to gotos.
         DoD: the target block is the nearest enclosing loop's exit or header.
```

### Phase 3 — Explicit drops and moves

```
W07.3.1  For each ASAP destruction site, emit a `Drop` MIR instruction.
         DoD: every owned binding has exactly one `Drop` in its MIR, unless
              moved out.

W07.3.2  For each ownership transfer, emit a `Move` MIR instruction.
         DoD: source and destination operands recorded; source is no longer
              usable after the move.

W07.3.3  For each borrow, emit a `Borrow` MIR instruction.
         DoD: source operand and borrow kind (ref vs mutref) recorded.

W07.3.4  Handle branched drops.
         DoD: a binding dropped on one branch and not another produces a `Drop`
              on the branch that did not drop it, inserted at the join.

W07.3.5  Invariant walker: every owned operand is either dropped, moved, or
         returned.
         DoD: walker fails on any orphaned owned operand.
```

### Phase 4 — Runtime calls

```
W07.4.1  Emit `fuse_rt_alloc` calls for heap allocations.
         DoD: MIR has an `AllocHeap` instruction; lowering to C (Wave 9) turns
              it into the runtime call.

W07.4.2  Emit `fuse_rt_panic` calls for panics.
         DoD: `panic(msg)` lowers to a runtime call with no return edge
              (type `!`).

W07.4.3  Atomic operations are emitted inline (not through the runtime).
         DoD: `Atomic.store` lowers to a MIR `Atomic` instruction that the
              codegen emits as a C11 `atomic_store_explicit` call.
```

### Phase 5 — Tests

```
W07.5.1  Unit tests for each HIR construct's lowering.
         DoD: a per-construct fixture shows the input HIR and the expected MIR.

W07.5.2  Property test: random HIR programs lower to MIR whose interpretation
         matches the HIR interpretation.
         DoD: a simple HIR interpreter and a simple MIR interpreter are both
              written for the property test; they agree on 10,000 programs.

W07.5.3  Invariant walker after lower: "every block has exactly one terminator".
         DoD: walker runs and passes.

W07.5.4  Invariant walker: "no Move references a moved operand".
         DoD: walker runs and passes.
```

**Wave 7 exit review.** Verify that the property test is meaningful and covers representative programs. This is the single most valuable test in the compiler — do not skip it.

---

## Wave 8 — Runtime library

**Goal.** Implement the ~40 `fuse_rt_*` entries in C11, one file per category, with POSIX and Windows platform branches. This wave can run in parallel with Waves 1–7 because it has no compiler dependency.

**Entry criterion.** Wave 0 done (parallelizable).

**Exit criteria.**
- Every entry in `language-guide.md` §15 implemented.
- `runtime/tests/` exercises each entry.
- POSIX and Windows branches both build and test green.
- Runtime is reachable from a minimal C program linking against it.

### Phase 0 — Header and build

```
W08.0.1  Write `runtime/include/fuse_rt.h` with all declarations.
         DoD: matches guide §15.2 exactly; includes `_Static_assert`s for
              primitive sizes; compiles cleanly under -Wall -Wextra -Werror
              -std=c11.

W08.0.2  Configure the runtime build rule in the Makefile.
         DoD: `make runtime` builds every `.c` file in `runtime/src/` into
              `build/runtime/fuse_rt.o` (or `.lib` on Windows).

W08.0.3  Configure platform-specific includes via `#ifdef _WIN32`.
         DoD: a single conditional picks POSIX or Windows platform.
```

### Phase 1 — Memory

```
W08.1.1  Implement `fuse_rt_alloc` on top of `malloc`.
         DoD: returns NULL on failure; OOM handled by caller.

W08.1.2  Implement `fuse_rt_alloc_aligned` on top of `aligned_alloc` (POSIX)
         or `_aligned_malloc` (Windows).
         DoD: alignment arg is a power of two; size is a multiple of alignment.

W08.1.3  Implement `fuse_rt_realloc`.
         DoD: shrink and grow both work; NULL ptr is equivalent to alloc.

W08.1.4  Implement `fuse_rt_free`.
         DoD: NULL is a no-op; double-free is undefined but the runtime does not
              assert.

W08.1.5  Implement `fuse_rt_oom`.
         DoD: writes to stderr, calls `fuse_rt_abort`.

W08.1.6  Tests under `runtime/tests/mem_test.c`.
         DoD: alloc/free/realloc exercised; leak check via a test harness.
```

### Phase 2 — Panic

```
W08.2.1  Implement `fuse_rt_panic`.
         DoD: writes message to stderr, calls `fuse_rt_abort`.

W08.2.2  Implement `fuse_rt_panic_with_loc`.
         DoD: prepends `file:line: ` to the message.

W08.2.3  Implement `fuse_rt_abort`.
         DoD: calls `abort()`.

W08.2.4  Tests under `runtime/tests/panic_test.c`.
         DoD: runtime panic is caught by a test harness via fork+waitpid
              (POSIX) or equivalent (Windows).
```

### Phase 3 — Raw I/O

```
W08.3.1  Implement `fuse_rt_stdout_write`.
         DoD: writes len bytes from buf to stdout via `write(1, ...)` (POSIX)
              or `WriteFile(GetStdHandle(STD_OUTPUT_HANDLE), ...)` (Windows);
              returns bytes written or -1 on error.

W08.3.2  Implement `fuse_rt_stderr_write`.
         DoD: analogous.

W08.3.3  Implement `fuse_rt_stdin_read` and `fuse_rt_stdin_eof`.
         DoD: blocking read until cap reached or EOF; eof-check uses a peek.

W08.3.4  Tests under `runtime/tests/io_test.c`.
         DoD: stdout/stderr/stdin round-tripped via pipes.
```

### Phase 4 — Process

```
W08.4.1  Implement `fuse_rt_exit`.
         DoD: `_exit(code)` on POSIX, `ExitProcess(code)` on Windows.

W08.4.2  Implement `fuse_rt_argc` and `fuse_rt_argv`.
         DoD: `main` captures argc/argv into globals; the runtime returns them.

W08.4.3  Tests.
         DoD: a C program using these prints argv[0] correctly.
```

### Phase 5 — File I/O

```
W08.5.1  Implement `fuse_rt_file_open`.
         DoD: converts flags to OS flags; returns fd or -1 on error.

W08.5.2  Implement `fuse_rt_file_read`.
         DoD: blocking read up to cap bytes; returns bytes read or -1.

W08.5.3  Implement `fuse_rt_file_write`.
         DoD: blocking write of len bytes; returns bytes written or -1.

W08.5.4  Implement `fuse_rt_file_close`.
         DoD: returns 0 on success.

W08.5.5  Implement `fuse_rt_file_seek`.
         DoD: whence values are SET/CUR/END; returns new offset or -1.

W08.5.6  Tests.
         DoD: write/read/seek round-trip on a temporary file.
```

### Phase 6 — Threads

```
W08.6.1  Implement `fuse_rt_thread_create` on POSIX via `pthread_create`.
         DoD: returns an opaque handle (pthread_t packed into a u64).

W08.6.2  Implement `fuse_rt_thread_create` on Windows via `CreateThread`.
         DoD: HANDLE packed into a u64.

W08.6.3  Implement `fuse_rt_thread_join`.
         DoD: blocks until the thread exits; populates out_result.

W08.6.4  Implement `fuse_rt_thread_detach`.
         DoD: detaches the thread.

W08.6.5  Implement `fuse_rt_thread_yield`.
         DoD: `sched_yield` / `SwitchToThread`.

W08.6.6  Tests.
         DoD: spawn 100 threads, join all, verify a counter.
```

### Phase 7 — Sync primitives

```
W08.7.1  Implement `fuse_rt_mutex_*` on POSIX via `pthread_mutex_*`.
         DoD: init/lock/unlock work; the mutex struct is opaque to callers.

W08.7.2  Implement `fuse_rt_mutex_*` on Windows via `CRITICAL_SECTION` or SRWLOCK.
         DoD: analogous.

W08.7.3  Implement `fuse_rt_rwlock_*` on both platforms.
         DoD: POSIX uses `pthread_rwlock_*`; Windows uses SRWLOCK.

W08.7.4  Implement `fuse_rt_cond_*` on both platforms.
         DoD: POSIX uses `pthread_cond_*`; Windows uses `CONDITION_VARIABLE`.

W08.7.5  Tests.
         DoD: producer-consumer test with mutex+cond works on both platforms.
```

### Phase 8 — TLS

```
W08.8.1  Implement `fuse_rt_tls_get` / `fuse_rt_tls_set`.
         DoD: POSIX uses `pthread_setspecific` / `pthread_getspecific`;
              Windows uses `TlsGetValue` / `TlsSetValue`.

W08.8.2  Tests.
         DoD: per-thread value round-trips across 8 threads.
```

### Phase 9 — Time

```
W08.9.1  Implement `fuse_rt_time_now_nanos` (monotonic).
         DoD: POSIX uses `clock_gettime(CLOCK_MONOTONIC, ...)`;
              Windows uses `QueryPerformanceCounter`.

W08.9.2  Implement `fuse_rt_wall_now_nanos` (wall clock).
         DoD: POSIX uses `clock_gettime(CLOCK_REALTIME, ...)`;
              Windows uses `GetSystemTimeAsFileTime`.

W08.9.3  Implement `fuse_rt_sleep_nanos`.
         DoD: POSIX uses `nanosleep`; Windows uses `Sleep` (with millisecond
              rounding and a comment documenting the limitation).

W08.9.4  Tests.
         DoD: a sleep of 10ms measured by the monotonic clock yields ≥10ms.
```

### Phase 10 — Platform branches

```
W08.10.1 Organize platform-specific code under `runtime/platform/posix/` and
         `runtime/platform/windows/`.
         DoD: each `fuse_rt_*.c` file #includes the appropriate platform helper.

W08.10.2 Add a `runtime_conform.c` test that verifies every declared function in
         the header has a definition.
         DoD: linker error at test time flags missing definitions.
```

### Phase 11 — Runtime test harness

```
W08.11.1 Set up `runtime/tests/` with a CMake-free Makefile rule.
         DoD: `make runtime-test` runs every `*_test.c` and reports results.

W08.11.2 Add a CI step that runs the runtime tests on every OS.
         DoD: CI matrix covers Linux, macOS, Windows.

W08.11.3 Add a sanitizer run in CI (`-fsanitize=address,undefined` where supported).
         DoD: at least Linux CI runs the runtime tests under sanitizers.
```

**Wave 8 exit review.** Verify every declared entry is present, every platform branch compiles, and sanitizer runs are clean.

---

## Wave 9 — C11 code generator

**Goal.** Translate MIR into portable C11 source code that, when compiled by a standards-conforming C11 compiler and linked against the runtime, produces a working Fuse program. Codegen is deterministic and produces byte-identical output given identical input.

**Entry criterion.** Waves 7 and 8 done.

**Exit criteria.**
- A minimal `hello.fuse` compiles to C, which `cc` compiles to a binary, which prints "hello".
- Symbol mangling is stable and deterministic.
- `fuse_rt.h` is #included in every emitted C file.
- Atomic operations are emitted inline with C11 `<stdatomic.h>`.

### Phase 0 — Module header

```
W09.0.1  Emit the `fuse_rt.h` include at the top of every C file.
         DoD: every emitted file starts with `#include "fuse_rt.h"`.

W09.0.2  Emit C type declarations for every Fuse struct and enum used.
         DoD: structs map to `typedef struct { ... } FuseType_<mangled>;`.

W09.0.3  Emit forward declarations for every Fuse function in the module.
         DoD: every function declared before any definition.

W09.0.4  Emit `_Static_assert` size checks for primitive types.
         DoD: asserts `sizeof(int32_t) == 4`, etc.
```

### Phase 1 — Type layout

```
W09.1.1  Map Fuse primitives to C types.
         DoD: `I32` → `int32_t`, `F64` → `double`, `Bool` → `_Bool`, `Char` → `uint32_t`,
              `USize` → `size_t`.

W09.1.2  Map Fuse structs to C structs.
         DoD: field order preserved; explicit padding inserted when needed for
              `@rank` or explicit alignment.

W09.1.3  Map Fuse enums to tagged unions.
         DoD: `struct { tag: uint32_t; union { ... }; }`; tag is deterministic.

W09.1.4  Map generic types via monomorphized type names.
         DoD: `List[I32]` → `FuseType_List_I32`.

W09.1.5  Map slices to a `(ptr, len)` pair.
         DoD: `[I32]` → `struct { int32_t* ptr; size_t len; }`.

W09.1.6  Map closures to a `(fn_ptr, env_ptr)` pair.
         DoD: `FnType_..._` struct type generated.
```

### Phase 2 — Expression codegen

```
W09.2.1  Emit C for literal operands.
         DoD: int literals emitted as `INT32_C(42)`, floats as `42.0`, bools as
              `1` / `0`.

W09.2.2  Emit C for binary operations.
         DoD: uses C operator; overflow rules match §4.1 of the guide (inline
              overflow check in debug builds).

W09.2.3  Emit C for call expressions.
         DoD: `foo(x, y)` → `FuseFn_foo(x, y)` with mangled name.

W09.2.4  Emit C for method calls.
         DoD: `x.foo(y)` → `FuseFn_X_foo(&x, y)` with receiver as first arg.

W09.2.5  Emit C for field access.
         DoD: `x.field` → `x.field_name` after mangling.

W09.2.6  Emit C for index access with bounds check.
         DoD: debug build: `((i) < (len) ? arr[i] : fuse_rt_panic(...))`;
              release build: no check unless `--debug-bounds` was passed.

W09.2.7  Emit C for struct literals.
         DoD: `(FuseType_Point){ .x = 1.0, .y = 2.0 }`.

W09.2.8  Emit C for if-expressions (conditional expression or helper).
         DoD: simple ifs become `cond ? a : b`; complex ones become statement
              blocks with a result temporary.
```

### Phase 3 — Control flow codegen

```
W09.3.1  Emit C for MIR blocks with gotos.
         DoD: every MIR block becomes a labeled C block with a trailing goto or
              return.

W09.3.2  Emit C for match as a switch on the tag.
         DoD: `switch (x.tag) { case 0: ...; break; ... }`.

W09.3.3  Emit C for loops.
         DoD: `while (1) { ... }` with a break on the exit condition.

W09.3.4  Emit C for drops.
         DoD: `FuseFn_X___del__(&x);` at the drop site.

W09.3.5  Emit C for moves.
         DoD: move is emitted as an assignment that clears or poisons the source
              in debug builds (release: just assignment).
```

### Phase 4 — Function emission

```
W09.4.1  Emit C function definitions.
         DoD: `<ret_type> <mangled_name>(<params>) { <body> }`.

W09.4.2  Emit the `main` entry point.
         DoD: `int main(int argc, char** argv) { fuse_rt_argc_init(argc, argv);
              return FuseFn_main(); }`.

W09.4.3  Handle divergent functions (`-> !`).
         DoD: marked with `_Noreturn`.

W09.4.4  Emit parameter ownership as C conventions.
         DoD: `ref` → `const T*`; `mutref` → `T*`; `owned` → by value (`T`).
```

### Phase 5 — Atomics

```
W09.5.1  Emit `#include <stdatomic.h>` on any file that uses atomics.
         DoD: atomic ops generate includes automatically.

W09.5.2  Emit `atomic_load_explicit`, `atomic_store_explicit`, etc.
         DoD: orderings map `Ordering.Relaxed` → `memory_order_relaxed`, etc.

W09.5.3  Emit `atomic_compare_exchange_*` for CAS operations.
         DoD: strong and weak variants supported.
```

### Phase 6 — FFI header generation

```
W09.6.1  For every module with `extern fn` or `@export`, emit a C header.
         DoD: the header is written to `build/include/<module-path>.h`.

W09.6.2  `extern fn` produces an extern declaration in the header.
         DoD: Fuse code uses the `extern fn` as declared; C code sees the same
              signature.

W09.6.3  `@export` produces a declaration for C consumers.
         DoD: `@export("add")` yields `int32_t add(int32_t, int32_t);` in the
              header.

W09.6.4  `_Static_assert` the primitive sizes at the top of every FFI header.
         DoD: failure on non-conforming host aborts compilation.
```

### Phase 7 — Name mangling

```
W09.7.1  Define the mangling scheme.
         DoD: format is `FuseFn_<module>_<name>_<type_args>`; deterministic.

W09.7.2  Implement the mangler.
         DoD: given a `SymbolId` and monomorphization args, produces a unique
              stable name.

W09.7.3  Property test: two different symbols never mangle to the same name.
         DoD: seeded test over a random symbol corpus.

W09.7.4  Property test: the same symbol always mangles to the same name across
         compilation runs.
         DoD: determinism verified.
```

### Phase 8 — End-to-end smoke test

```
W09.8.1  Write `examples/hello/main.fuse`.
         DoD: `fn main() -> Int { print("hello\n"); return 0; }`.

W09.8.2  Compile end-to-end: Fuse → MIR → C → object → binary.
         DoD: each step runs via a Go integration test.

W09.8.3  Run the binary and verify it prints "hello" and exits 0.
         DoD: `tests/e2e/hello/` fixture passes.

W09.8.4  Verify determinism: two builds on the same source produce byte-identical
         binaries.
         DoD: `tools/repro --check examples/hello` passes.
```

**Wave 9 exit review.** Verify hello world end-to-end. This is the "lights turn on" moment; celebrate, then return to grinding.

---

## Wave 10 — `cc` driver and linking

**Goal.** Wrap the C compiler invocation, handle all six target triples, construct the right argument list, forward diagnostics, and produce the final artifact. By the end of this wave, `fuse build` can produce a native binary on the host platform without the user installing anything beyond `cc`.

**Entry criterion.** Wave 9 done.

**Exit criteria.**
- `fuse build examples/hello/` produces `build/hello` (or `build/hello.exe`).
- `fuse build --target=linux-amd64 examples/hello/` cross-compiles (test skipped if host cc does not support the target).
- Diagnostics from the C compiler are forwarded to the user.
- Link errors are readable.

### Phase 0 — cc detection

```
W10.0.1  Implement `compiler/cc/detect.go`.
         DoD: checks `$CC`, then `$PATH` for `cc`, `gcc`, `clang`; returns the
              first one found.

W10.0.2  Implement a version check.
         DoD: runs `cc --version`, parses the output, requires C11 support.

W10.0.3  Fail early with a helpful message when no cc is found.
         DoD: message names the environment variable and suggests installation.
```

### Phase 1 — Argument construction

```
W10.1.1  Build the `cc` command line for a host-target compile.
         DoD: includes `-std=c11`, `-Wall`, `-Werror`, `-O2`, target-specific flags.

W10.1.2  Add include paths for the runtime header.
         DoD: `-I<build>/include -I<runtime>/include`.

W10.1.3  Add the runtime object file to the link command.
         DoD: `<runtime>/build/fuse_rt.o` or `.lib`.

W10.1.4  Add per-target flags.
         DoD: linux-arm64 adds `-target aarch64-linux-gnu` (for clang); macos
              adds `-mmacosx-version-min=...`; etc.

W10.1.5  Handle separate compile and link phases.
         DoD: every Fuse module compiles to a `.o`, then a link step combines
              them.
```

### Phase 2 — Subprocess invocation

```
W10.2.1  Implement `cc.Compile(args []string) (stdout, stderr []byte, err error)`.
         DoD: captures stdout and stderr; returns the exit code.

W10.2.2  Forward cc diagnostics to the user, normalized.
         DoD: cc errors appear in the user's terminal with a note saying they
              come from the C compiler.

W10.2.3  Implement a timeout and cancellation.
         DoD: cc runs subject to a deadline; Ctrl-C cancels.

W10.2.4  Handle the Windows path separator and shell-escaping.
         DoD: invocations with spaces in paths work on Windows.
```

### Phase 3 — Error forwarding

```
W10.3.1  Map common cc errors back to Fuse source locations where possible.
         DoD: if cc says `error: ... in file X.c line Y`, and X.c is a compiler-
              emitted file, the driver looks up the Fuse source/line and reports
              that instead.

W10.3.2  Preserve the cc error when the mapping fails.
         DoD: with a note "this error came from the C compiler; the Fuse compiler
              could not map it back to Fuse source".
```

### Phase 4 — Artifact production

```
W10.4.1  Produce a native executable for `fuse build`.
         DoD: `examples/hello/` produces a runnable binary.

W10.4.2  Produce a static library for `--crate-type=staticlib`.
         DoD: `.a` (Unix) or `.lib` (Windows).

W10.4.3  Produce a shared library for `--crate-type=cdylib`.
         DoD: `.so`, `.dylib`, `.dll` as appropriate.

W10.4.4  Place artifacts under `build/<package>/<target>/`.
         DoD: deterministic path; `fuse clean` removes them.
```

### Phase 5 — Cross-compilation plumbing

```
W10.5.1  Wire `--target=linux-amd64` on a non-Linux host using clang.
         DoD: if host clang supports cross-compilation, produces a linux-amd64
              binary; if not, a helpful "install cross tools" message.

W10.5.2  Wire `--target=windows-amd64` on a non-Windows host using
         `clang --target=x86_64-pc-windows-msvc` and `lld-link`.
         DoD: produces a .exe on a Linux or macOS host.

W10.5.3  Wire `--target=wasm32-wasi` via `clang --target=wasm32-wasi`.
         DoD: requires the WASI SDK; if not present, reports it.

W10.5.4  Wire `--target=macos-amd64` and `--target=macos-arm64`.
         DoD: requires the macOS SDK or cctools-port on non-macOS.

W10.5.5  Tests: each target triple has a smoke test that builds hello.fuse
         (skipped if the host cannot cross-compile).
         DoD: CI on each supported runner at least builds the native target
              and skips the rest.
```

**Wave 10 exit review.** Confirm that `fuse build examples/hello/` produces a working binary on a clean checkout.

---

## Wave 11 — Top-level driver and CLI

**Goal.** The `fuse` CLI with all nine subcommands, hand-rolled argument parsing, `--format=json` on every subcommand, and clean help text.

**Entry criterion.** Wave 10 done.

**Exit criteria.**
- `fuse build`, `fuse run`, `fuse check`, `fuse test`, `fuse fmt`, `fuse doc`, `fuse repl`, `fuse version`, `fuse help` all work.
- Every subcommand accepts `--format=json`.
- Unknown flags produce a diagnostic and a non-zero exit.
- No external flag-parsing library.

### Phase 0 — Argument parser

```
W11.0.1  Implement `cmd/fuse/args.go` — a hand-rolled flag parser.
         DoD: supports long (`--name=value`), short (`-n value`), boolean
              (`--verbose`), and value (`--output path`) forms.

W11.0.2  Reject unknown flags.
         DoD: prints the flag name and suggests the nearest valid flag.

W11.0.3  Print help on `--help` for the subcommand.
         DoD: help text is generated from the flag declarations.

W11.0.4  Parse positional arguments.
         DoD: `fuse build examples/hello/` treats `examples/hello/` as a positional.
```

### Phase 1 — `fuse build`

```
W11.1.1  Implement the `build` subcommand.
         DoD: runs the driver, produces the artifact.

W11.1.2  Support `--target=<triple>`.
         DoD: forwarded to the cc driver.

W11.1.3  Support `--crate-type={bin,staticlib,cdylib,obj}`.
         DoD: artifact type chosen accordingly.

W11.1.4  Support `--release` / `--debug`.
         DoD: sets C compiler optimization flags.

W11.1.5  Support `--format=json` to emit build info as JSON.
         DoD: output includes the artifact path, target, duration.
```

### Phase 2 — `fuse run`

```
W11.2.1  Implement the `run` subcommand.
         DoD: builds then immediately executes the artifact.

W11.2.2  Forward argv from `fuse run -- arg1 arg2` to the program.
         DoD: `--` separates driver args from program args.

W11.2.3  Return the program's exit code.
         DoD: `fuse run` exits with the inner process's code.
```

### Phase 3 — `fuse check`

```
W11.3.1  Implement the `check` subcommand.
         DoD: runs the pipeline up to the checker pass, does not run codegen.

W11.3.2  Supports `--format=json` for LSP and editor integration.
         DoD: diagnostics serialized to JSON.

W11.3.3  Runs in under 1 second on a minimal package.
         DoD: benchmark tracked.
```

### Phase 4 — `fuse test`

```
W11.4.1  Implement the `test` subcommand.
         DoD: discovers tests in `tests/` and in `#[test]`-annotated functions;
              compiles and runs them.

W11.4.2  Supports `--filter=<pattern>`.
         DoD: only tests whose name matches run.

W11.4.3  Reports results in a readable format and as JSON with `--format=json`.
         DoD: pass/fail counts and per-test detail.

W11.4.4  Integrates with `tools/goldens` for golden-diff comparison.
         DoD: failing goldens show a diff in the output.
```

### Phase 5 — `fuse fmt`

```
W11.5.1  Implement the `fmt` subcommand.
         DoD: reads files, formats, writes back in place.

W11.5.2  Support `--check` (exits non-zero if any file would change).
         DoD: used by CI.

W11.5.3  Support `--fix` (rewrites case conventions).
         DoD: warns about what it changed.
```

### Phase 6 — `fuse doc`

```
W11.6.1  Implement the `doc` subcommand.
         DoD: produces HTML under `target/doc/` for the package's public items.

W11.6.2  Uses templates in `compiler/doc/templates/`.
         DoD: templates are checked in.

W11.6.3  Cross-links symbols.
         DoD: a reference to `core.list.List` in a doc comment becomes a clickable
              link.
```

### Phase 7 — `fuse version` and `fuse help`

```
W11.7.1  Implement `fuse version`.
         DoD: prints compiler version, language version, target list.

W11.7.2  Implement `fuse help` (general).
         DoD: lists subcommands and brief descriptions.

W11.7.3  Implement `fuse help <subcommand>`.
         DoD: detailed help for the subcommand.

W11.7.4  Implement `fuse help error <code>` (for `EXXXX` codes).
         DoD: prints a long-form explanation from an embedded database.
```

### Phase 8 — `fuse repl`

```
W11.8.1  Implement the `repl` subcommand with line editing.
         DoD: supports `up arrow` history, Ctrl-C, Ctrl-D; hand-rolled (no
              external library).

W11.8.2  Each expression is compiled to a small module and run via `fuse run`.
         DoD: state persists via a growing accumulator file on disk.

W11.8.3  `:help`, `:quit`, `:reset`, `:type <expr>` meta-commands.
         DoD: each does what it says.
```

### Phase 9 — `--format=json`

```
W11.9.1  Every subcommand accepts `--format=json`.
         DoD: consistency test: a Go integration test runs every subcommand with
              `--format=json` and verifies the output is valid JSON.

W11.9.2  Schema is documented in `compiler/driver/json_schema.md` (non-normative).
         DoD: sample outputs in the file.

W11.9.3  The JSON output is a stable contract once shipped.
         DoD: a schema-evolution rule: fields may be added, never removed; types
              are versioned.
```

**Wave 11 exit review.** Smoke test every subcommand. Verify `--format=json` returns parseable JSON everywhere.

---

## Wave 12 — Core stdlib

**Goal.** Implement the `core` tier of the standard library. This wave is the **first real stress test** of the compiler: every bug found during this wave is fixed in the compiler, not worked around in the library (`rules.md` §4.1).

**Entry criterion.** Wave 11 done.

**Exit criteria.**
- Every module in `stdlib/core/` compiles, tests, and is used by the Stage 2 port in Wave 14.
- Auto-generation of the Core trait set works for `@value struct` and `data class`; fails correctly for plain `struct`.
- `Map[K, V]` preserves insertion order.
- No hardcoded special cases for `Int` or `String` in the library.

### Phase 0 — Prelude

```
W12.0.1  Write `stdlib/core/prelude.fuse`.
         DoD: re-exports Option, Result, Ordering, primitive aliases, Core trait
              set names.

W12.0.2  Wire the prelude into every module automatically.
         DoD: the compiler imports the prelude silently unless
              `#![no_prelude]` is set (feature is reserved, not implemented; the
              compiler errors on attempted use).
```

### Phase 1 — Option, Result, Ordering

```
W12.1.1  `stdlib/core/option.fuse` — the Option enum and its methods.
         DoD: `map`, `unwrap`, `unwrapOr`, `andThen`, `orElse`, `isSome`, `isNone`,
              and the `?` protocol.

W12.1.2  `stdlib/core/result.fuse` — the Result enum and its methods.
         DoD: analogous set plus `mapErr`.

W12.1.3  `stdlib/core/ordering.fuse` — Ordering enum.
         DoD: `Less`, `Equal`, `Greater`; `reverse()` method.
```

### Phase 2 — Core traits

```
W12.2.1  `core/traits/equatable.fuse`.
         DoD: trait defined; compiler's `==` lowers to its method.

W12.2.2  `core/traits/hashable.fuse`.
         DoD: trait defined as implementing `Equatable`.

W12.2.3  `core/traits/comparable.fuse`.
         DoD: trait defined; compiler's ordering operators lower to it.

W12.2.4  `core/traits/printable.fuse` and `core/traits/debuggable.fuse`.
         DoD: both traits defined.

W12.2.5  `core/traits/sequence.fuse`.
         DoD: trait with `type Item` and `next` method; `for` loop lowers to it.

W12.2.6  `core/traits/default.fuse`, `core/traits/from.fuse`, `core/traits/index.fuse`.
         DoD: all three defined.

W12.2.7  Auto-derivation in the checker.
         DoD: `@value struct` and `data class` auto-implement the Core trait set
              if all fields are eligible; plain `struct` does not.
```

### Phase 3 — Primitive methods

```
W12.3.1  `core/primitive/int.fuse` — methods on integer types.
         DoD: `toI32`, `toI64`, `toUSize`, `checkedAdd`, `saturatingAdd`, etc.

W12.3.2  `core/primitive/float.fuse`.
         DoD: `sqrt`, `abs`, `isNan`, `isInfinite`, `floor`, `ceil`, `round`.

W12.3.3  `core/primitive/bool.fuse`.
         DoD: `toInt()` and `not` are compiler primitives; this file adds
              pretty-printing.

W12.3.4  `core/primitive/char.fuse`.
         DoD: `isLetter`, `isDigit`, `toLower`, `toUpper`, `toInt`.

W12.3.5  Implement each primitive's auto-derivation of Core traits.
         DoD: primitives are `Equatable`, `Hashable`, `Comparable`, `Printable`,
              `Debuggable` — their implementations are in these files, not in
              the compiler.
```

### Phase 4 — String

```
W12.4.1  `core/string.fuse` — String type.
         DoD: owns a `Ptr[U8]` + length + capacity in `rt_bridge/alloc.fuse`.

W12.4.2  Construction: `String.new()`, `String.fromBytes(slice)`, literals.
         DoD: literal lowering emits calls to `String.fromStaticBytes(ptr, len)`.

W12.4.3  Accessors: `len()`, `byteAt(i)`, `isEmpty()`.
         DoD: O(1) len and byteAt.

W12.4.4  Iteration: `chars() -> Sequence[Char]`, `bytes() -> Sequence[U8]`.
         DoD: works with `for` loops.

W12.4.5  Search: `contains`, `startsWith`, `endsWith`, `indexOf`.
         DoD: all work on arbitrary substrings.

W12.4.6  Mutation: `append`, `appendChar`, `clear`.
         DoD: uses the alloc bridge for growth.

W12.4.7  Conversion: `toInt`, `toFloat`, `fromInt`, `fromFloat`.
         DoD: uses `core.fmt` for formatting.

W12.4.8  Implements `Equatable`, `Hashable`, `Comparable`, `Printable`,
         `Debuggable`, `Sequence`.
         DoD: hand-implemented because String is a plain struct; no auto-derive.
```

### Phase 5 — List

```
W12.5.1  `core/list.fuse` — List[T].
         DoD: owns an alloc bridge, a length, and a capacity.

W12.5.2  Methods: `new`, `withCapacity`, `push`, `pop`, `get`, `set`, `len`,
         `isEmpty`, `clear`, `insert`, `remove`.
         DoD: all O(1) or O(n) as expected.

W12.5.3  Iteration: `iter` / `iterMut` / `intoIter`.
         DoD: for loops work with ref, mutref, and consumption.

W12.5.4  Traits: `Equatable` and `Comparable` if T implements them; `Sequence`.
         DoD: tests verify.
```

### Phase 6 — Hash and SipHash

```
W12.6.1  `core/hash/hasher.fuse` — the Hasher trait.
         DoD: `writeU64`, `writeU32`, `writeBytes`, `finish` methods.

W12.6.2  `core/hash/siphash.fuse` — a SipHash-2-4 implementation in Fuse.
         DoD: produces the same output as the reference implementation; tested
              against known vectors.

W12.6.3  Default Hasher for Map.
         DoD: `Map.new()` uses SipHash with a deterministic seed (runtime
              generated or compile-time constant; document choice).
```

### Phase 7 — Map (insertion-ordered)

```
W12.7.1  `core/map.fuse` — Map[K: Hashable, V].
         DoD: backed by a parallel array of entries plus a hash index.

W12.7.2  Preserves insertion order on iteration.
         DoD: `for (k, v) in map` yields in insertion order.

W12.7.3  Operations: `new`, `insert`, `get`, `remove`, `contains`, `len`,
         `isEmpty`, `clear`.
         DoD: O(1) amortized insert/get/remove; remove preserves order of
              survivors.

W12.7.4  Iteration views: `keys`, `values`, `entries`, `sortedKeys`, `unorderedKeys`.
         DoD: sorted/unordered are opt-ins per guide.

W12.7.5  No special case for Int or String keys.
         DoD: all key types go through `Hashable`; test: a user-defined type
              implementing `Hashable` works identically.
```

### Phase 8 — Set

```
W12.8.1  `core/set.fuse` — Set[T: Hashable], implemented on top of Map.
         DoD: internal representation is `Map[T, ()]`; preserves insertion order.
```

### Phase 9 — fmt

```
W12.9.1  `core/fmt/builder.fuse` — StringBuilder.
         DoD: `append`, `appendChar`, `appendInt`, `appendFloat`, `toString`.

W12.9.2  `core/fmt/format.fuse` — the `print` function family.
         DoD: `print(x: ref impl Printable)` writes to stdout via the `io`
              bridge (wait — io is in `full`; `print` in core writes via an
              unsafe runtime bridge file).

W12.9.3  Implement `print` in `core/rt_bridge/print.fuse`.
         DoD: uses `fuse_rt_stdout_write` via unsafe; this file is on the
              approved `unsafe` list.
```

### Phase 10 — math

```
W12.10.1 `core/math.fuse` — trig, exp, log, pow.
         DoD: implementations call libm via the FFI (declared in a bridge file).
```

### Phase 11 — iter

```
W12.11.1 `core/iter.fuse` — iterator combinators.
         DoD: `map`, `filter`, `take`, `skip`, `fold`, `collect`, `count`, `sum`,
              `zip`, `enumerate`.

W12.11.2 All combinators preserve the `Sequence` trait.
         DoD: `[1, 2, 3].iter().map(|x| x * 2).collect()` works.
```

### Phase 12 — Atomic

```
W12.12.1 `core/atomic.fuse` — Atomic[T] type and methods.
         DoD: `load`, `store`, `compareExchange`, `fetchAdd`, etc.; methods
              lower to MIR `Atomic` instructions (not to runtime calls).
```

### Phase 13 — rt_bridge

```
W12.13.1 `core/rt_bridge/alloc.fuse` — safe wrappers over `fuse_rt_alloc*`.
         DoD: exposes `allocate[T](n: USize) -> Ptr[T]` and `deallocate[T]`.

W12.13.2 `core/rt_bridge/panic.fuse` — safe wrapper over `fuse_rt_panic`.
         DoD: exposes `panic(msg: String) -> !` and `abort() -> !`.

W12.13.3 `core/rt_bridge/intrinsics.fuse` — integer overflow helpers, etc.
         DoD: each function has a `// SAFETY:` comment.

W12.13.4 `core/rt_bridge/print.fuse` — stdout write bridge for `core.fmt.print`.
         DoD: the only place in `core` that calls into `fuse_rt_stdout_write`.
```

### Phase 14 — Testing and polish

```
W12.14.1 Test each module with unit tests.
         DoD: coverage ≥ 90% of public API surface.

W12.14.2 Run `tools/checkdoc` and fix every missing doc comment.
         DoD: checkdoc reports 0 missing.

W12.14.3 Fix compiler bugs as they surface.
         DoD: every bug gets a learning log entry AND a regression test in the
              relevant wave's test corpus (Waves 1-7).

W12.14.4 Confirm the "no workaround" discipline held throughout.
         DoD: `git log stdlib/core/` shows no commits with "workaround" or
              "temporary" in the subject.
```

**Wave 12 exit review.** Verify no hardcoded special cases, no workarounds, insertion-order Map. The stdlib is now usable for building the compiler in Fuse (Wave 14).

---

## Wave 13 — Full stdlib

**Goal.** Implement the `full` tier: I/O, filesystem, OS, time, threads, sync, and channels. These are the pieces the Stage 2 compiler needs that Core cannot provide.

**Entry criterion.** Wave 12 done.

**Exit criteria.**
- Every module in `stdlib/full/` compiles and is tested.
- `Shared[T]` with `@rank(N)` enforces lock order at compile time.
- `Chan[T]` works across threads with correct memory ordering.
- `spawn` creates an OS thread; `join` retrieves the result.

### Phase 0 — io

```
W13.0.1  `full/io/stdin.fuse` — Stdin reader.
         DoD: `readLine`, `readToEnd`, `bytes()` iterator.

W13.0.2  `full/io/stdout.fuse` / `full/io/stderr.fuse`.
         DoD: `write`, `writeLine`, `flush`; thread-safe via a mutex.
```

### Phase 1 — fs

```
W13.1.1  `full/fs/file.fuse` — File type.
         DoD: `open`, `create`, `read`, `write`, `seek`, `close`; uses the file
              I/O runtime bridge.

W13.1.2  `full/fs/path.fuse` — path manipulation.
         DoD: `join`, `parent`, `filename`, `extension`; platform-aware.

W13.1.3  `full/fs/dir.fuse` — directory enumeration.
         DoD: `listDir`, `createDir`, `removeDir`.
```

### Phase 2 — os

```
W13.2.1  `full/os/env.fuse` — environment variable access.
         DoD: `get`, `set`, `unset`.

W13.2.2  `full/os/process.fuse` — process control.
         DoD: `exit(code)`, `currentPid`, `executablePath`.

W13.2.3  `full/os/args.fuse` — command-line argument access.
         DoD: `args() -> List[String]`.
```

### Phase 3 — time

```
W13.3.1  `full/time/instant.fuse` — monotonic clock.
         DoD: `Instant.now()`, `elapsed()`, arithmetic.

W13.3.2  `full/time/duration.fuse` — Duration.
         DoD: from nanos/millis/seconds; arithmetic.

W13.3.3  `full/time/wallclock.fuse` — wall clock.
         DoD: `WallClock.now()` returns a UTC instant.
```

### Phase 4 — thread

```
W13.4.1  `full/thread/spawn.fuse` — the `spawn` function.
         DoD: takes an owned closure, returns a ThreadHandle.

W13.4.2  `full/thread/handle.fuse` — ThreadHandle type.
         DoD: `join` returns Result[T, JoinError]; `detach` consumes the handle.

W13.4.3  Interaction with the runtime's thread bridge.
         DoD: `spawn` calls `fuse_rt_thread_create` via an unsafe bridge.
```

### Phase 5 — sync

```
W13.5.1  `full/sync/mutex.fuse` — Mutex[T].
         DoD: `lock() -> MutexGuard[T]`; the guard uses ASAP to unlock.

W13.5.2  `full/sync/rwlock.fuse` — RwLock[T].
         DoD: `readLock`, `writeLock`; guards.

W13.5.3  `full/sync/cond.fuse` — CondVar.
         DoD: `wait`, `signal`, `broadcast`.

W13.5.4  `full/sync/once.fuse` — Once.
         DoD: `callOnce(f)` runs f at most once across all threads.

W13.5.5  `full/sync/shared.fuse` — Shared[T] with @rank(N).
         DoD: `with(f)` method; compile-time rank check via a separate compiler
              pass (or integrated into the checker; add a task here if a new
              pass is needed).

W13.5.6  @rank compile-time check.
         DoD: a program that acquires rank 5 while holding rank 3 produces a
              compile-time error with both sites.
```

### Phase 6 — chan

```
W13.6.1  `full/chan/chan.fuse` — Chan[T].
         DoD: bounded MPMC channel backed by a ring buffer, a mutex, and two
              condvars.

W13.6.2  `send`, `recv`, `trySend`, `tryRecv`, `close`, `len`, `capacity`.
         DoD: each has unit tests.

W13.6.3  Memory model: send/recv pairs form synchronization edges.
         DoD: tests use `Atomic[T]` to verify ordering at the edges.

W13.6.4  Integration test: producer-consumer with 1000 messages.
         DoD: no drops, no reorders, no deadlocks.
```

### Phase 7 — Testing

```
W13.7.1  Test each module in isolation.
         DoD: unit tests with high coverage.

W13.7.2  Integration tests across the full tier.
         DoD: multi-thread, multi-channel tests pass.

W13.7.3  Sanitizer runs in CI (tsan and asan).
         DoD: CI runs the full-tier tests under tsan on Linux.
```

**Wave 13 exit review.** Verify that `Shared[T]` deadlock prevention works and that `Chan[T]` has correct memory ordering under sanitizers.

---

## Wave 14 — Stage 2 port

**Goal.** Port the Go compiler to Fuse, one subsystem at a time. By the end of this wave, `fuse build stage2/` (using the Stage 1 Go compiler) produces a Stage 2 binary that is a functionally equivalent Fuse compiler.

**Entry criterion.** Wave 13 done.

**Exit criteria.**
- `stage2/src/main.fuse` and every subsystem under `stage2/src/` compile.
- Stage 2 correctly compiles a test program (`examples/hello/`).
- Stage 2 output matches Stage 1 output on the hello-world test.

### Phase 0 — Port the lexer

```
W14.0.1  Port `compiler/lex/` to Fuse under `stage2/src/lex/`.
         DoD: every token kind present, every literal form handled.

W14.0.2  Port the lexer tests.
         DoD: Fuse-level unit tests pass.

W14.0.3  The port uses only stdlib features already implemented.
         DoD: no compiler workarounds needed.
```

### Phase 1 — Port the parser and AST

```
W14.1.1  Port `compiler/ast/` to `stage2/src/ast/`.
         DoD: every AST node kind present.

W14.1.2  Port `compiler/parse/` to `stage2/src/parse/`.
         DoD: recursive-descent parser ported; Pratt precedence table ported.

W14.1.3  Port parser tests.
         DoD: same fixtures pass against both Stage 1 and Stage 2.
```

### Phase 2 — Port name resolution

```
W14.2.1  Port `compiler/resolve/` to `stage2/src/resolve/`.
         DoD: symbol table and module graph ported.

W14.2.2  Test against the same fixtures.
         DoD: pass.
```

### Phase 3 — Port TypeTable and HIR

```
W14.3.1  Port `compiler/typetable/` to `stage2/src/typetable/`.
         DoD: interning works; TypeId is a u32 in Fuse too.

W14.3.2  Port HIR node set.
         DoD: constructors enforce metadata.

W14.3.3  Port the pass manifest framework.
         DoD: topological order and dependency enforcement work.
```

### Phase 4 — Port the checker

```
W14.4.1  Port `compiler/check/` to `stage2/src/check/`.
         DoD: every check-phase rule ported, including U1–U7 and return-type
              consistency.

W14.4.2  Port checker tests.
         DoD: full test corpus passes.
```

### Phase 5 — Port liveness and ownership

```
W14.5.1  Port `compiler/liveness/` to `stage2/src/liveness/`.
         DoD: single liveness computation; LiveAfter metadata.

W14.5.2  Port ownership tests.
         DoD: mutref, move, escape analysis all tested.
```

### Phase 6 — Port HIR → MIR lowering

```
W14.6.1  Port `compiler/mir/` and `compiler/lower/` to Stage 2.
         DoD: MIR construction and lowering ported.

W14.6.2  Port the property test.
         DoD: random HIR → MIR preservation verified in Fuse too.
```

### Phase 7 — Port codegen

```
W14.7.1  Port `compiler/codegen/` to `stage2/src/codegen/`.
         DoD: every MIR construct has a C emission rule.

W14.7.2  Port name mangling.
         DoD: deterministic; produces the same output as Stage 1 for equivalent
              input.
```

### Phase 8 — Port the cc driver

```
W14.8.1  Port `compiler/cc/` to `stage2/src/cc/`.
         DoD: subprocess invocation works via the full stdlib.

W14.8.2  Port target-triple handling.
         DoD: cross-compilation tests pass.
```

### Phase 9 — Port the top-level driver

```
W14.9.1  Port `compiler/driver/` to `stage2/src/driver/`.
         DoD: end-to-end pipeline.

W14.9.2  Port `cmd/fuse` to `stage2/src/main.fuse`.
         DoD: argument parser; subcommand dispatch.

W14.9.3  Port each subcommand.
         DoD: build, run, check, test, fmt, doc, repl, version, help.
```

### Phase 10 — First stage 2 compile

```
W14.10.1 Build stage2/ with stage1.
         DoD: `fuse build stage2/` produces `build/stage2/fuse`.

W14.10.2 Run the stage 2 compiler on `examples/hello/`.
         DoD: `build/stage2/fuse build examples/hello/` produces a working binary.

W14.10.3 Diff stage 1 and stage 2 outputs on hello.
         DoD: byte-for-byte match of emitted C (and, eventually, machine code).
```

**Wave 14 exit review.** Verify that Stage 2 compiles Stage 1's test corpus successfully. This is the second "lights turn on" moment.

---

## Wave 15 — Bootstrap gate

**Goal.** Achieve three-generation reproducibility: Stage 2 compiles itself, the resulting binary compiles itself again, and the outputs are byte-identical. This is the gate that says "the language is self-hosting".

**Entry criterion.** Wave 14 done.

**Exit criteria.**
- `stage1 → stage2.bin`
- `stage2.bin → stage2'.bin`
- `stage2'.bin → stage2''.bin`
- `stage2'.bin` and `stage2''.bin` are byte-identical.
- The bootstrap test compiles the real `stage2/src/main.fuse`, not a synthetic input.
- CI runs the bootstrap on every merge to `main`.

### Phase 0 — Stage 2 compiles itself

```
W15.0.1  Run Stage 2 against its own source tree.
         DoD: `build/stage2/fuse build stage2/` produces `build/stage2-gen2/fuse`.

W15.0.2  Fix all compiler bugs discovered during self-compile.
         DoD: each bug gets a regression test in the relevant wave and a
              learning log entry.

W15.0.3  Iterate until stage2-gen2 builds without errors.
         DoD: no errors; no warnings.
```

### Phase 1 — Three-generation reproducibility

```
W15.1.1  Run stage2-gen2 against stage2/ to produce stage2-gen3.
         DoD: gen3 exists.

W15.1.2  Diff gen2 and gen3 byte-for-byte.
         DoD: they are identical.

W15.1.3  If the diff is non-empty, fix the determinism bug.
         DoD: the fix has a learning log entry, a regression test, and preserves
              gen2 == gen3.
```

### Phase 2 — Real source, not synthetic

```
W15.2.1  The bootstrap test MUST compile the real stage2/src/main.fuse.
         DoD: test harness invokes the Stage 2 binary on the actual source tree
              (not a trimmed-down fixture).
         Note: a bootstrap test that passes on a trivial program while the real
              self-host is broken is worse than no test. The only acceptable
              bootstrap target is the real Stage 2 source in its current state.

W15.2.2  Test runs in CI on every push to main.
         DoD: CI step fails the build if bootstrap fails.
```

### Phase 3 — CI gate

```
W15.3.1  Wire the bootstrap test into CI.
         DoD: `.ci/scripts/bootstrap.sh` runs the three-generation sequence;
              workflow invokes it.

W15.3.2  Bootstrap fails loudly.
         DoD: on failure, CI produces a readable diff and the failing source line.

W15.3.3  Bootstrap runs on the full matrix (Linux, macOS, Windows).
         DoD: all three platforms green.
```

### Phase 4 — Reproducibility across targets

```
W15.4.1  Stage 2 on host produces bit-identical output to Stage 2 on every other
         host.
         DoD: CI cross-checks this: Linux CI and macOS CI both build stage2 and
              diff the outputs.

W15.4.2  Fix host-dependent determinism bugs.
         DoD: each fix has a regression test and a log entry.
```

**Wave 15 exit review.** The language is now self-hosting and byte-reproducible. This is a project-level milestone; write a learning log entry marking the achievement and summarizing the bugs that had to be fixed along the way.

---

## Wave 16 — Ext stdlib

**Goal.** Build the Ext tier — opt-in modules that go beyond Core/Full. Each module is a separate, small project sharing the repo's discipline.

**Entry criterion.** Wave 15 done (parallelizable with Wave 17).

**Exit criteria.**
- `ext.json`, `ext.regex`, `ext.serde`, `ext.compress`, `ext.crypto`, `ext.net` all ship.
- Each module has tests and docs.
- No module depends on anything outside Core, Full, or other Ext modules.

### Phase 0 — json

```
W16.0.1  `ext/json/parse.fuse` — JSON parser.
         DoD: RFC 8259 compliant; produces a `Value` tree.

W16.0.2  `ext/json/emit.fuse` — JSON emitter.
         DoD: round-trips parse(emit(value)) = value on a test corpus.

W16.0.3  `ext/json/value.fuse` — the tagged Value type.
         DoD: Null, Bool, Num, String, Array, Object variants.

W16.0.4  Tests.
         DoD: passes a subset of JSONTestSuite.
```

### Phase 1 — regex

```
W16.1.1  `ext/regex/compile.fuse` — regex compiler.
         DoD: supports the subset documented in the module's README: literals,
              character classes, quantifiers, alternation, grouping, anchors.

W16.1.2  `ext/regex/engine.fuse` — a linear-time NFA engine.
         DoD: avoids exponential backtracking by construction.

W16.1.3  Tests.
         DoD: a test corpus of pattern/input/expected-match triples.
```

### Phase 2 — serde

```
W16.2.1  `ext/serde/traits.fuse` — Serializable / Encodable / Decodable.
         DoD: three traits and their method signatures.

W16.2.2  Auto-derive for `@value struct` and `data class`.
         DoD: compiler-assisted derive attribute; `@derive(Encodable)` reserved
              but implementation may defer to a future wave if it requires
              compiler changes.

W16.2.3  Tie-in to ext.json.
         DoD: `jsonEncode[T: Encodable](value: ref T) -> String`.
```

### Phase 3 — compress

```
W16.3.1  `ext/compress/gzip.fuse` — gzip compress/decompress.
         DoD: round-trip on a test corpus.

W16.3.2  `ext/compress/zstd.fuse` — zstd (optional; may be deferred).
         DoD: if present, round-trip.
```

### Phase 4 — crypto

```
W16.4.1  `ext/crypto/sha256.fuse`, `ext/crypto/sha512.fuse`.
         DoD: against known vectors.

W16.4.2  `ext/crypto/hmac.fuse`.
         DoD: HMAC-SHA256 / HMAC-SHA512 against known vectors.

W16.4.3  `ext/crypto/random.fuse` — cryptographically secure random.
         DoD: uses OS-specific entropy source via a runtime bridge.
```

### Phase 5 — net

```
W16.5.1  `ext/net/tcp.fuse` — TCP sockets.
         DoD: listen, accept, connect, read, write, close via runtime bridges.

W16.5.2  `ext/net/udp.fuse` — UDP sockets.
         DoD: bind, sendTo, recvFrom.

W16.5.3  `ext/net/http.fuse` — minimal HTTP/1.1 client and server.
         DoD: server handles GET; client sends GET; round-trip works.
```

**Wave 16 exit review.** Each Ext module has tests and docs. No cross-dependencies beyond documented ones.

---

## Wave 17 — Targets and cross-compilation

**Goal.** Validate that every documented target triple actually builds and runs a Fuse program, including cross-compilation from each host.

**Entry criterion.** Wave 15 done (parallelizable with Wave 16).

**Exit criteria.**
- `linux-amd64`, `linux-arm64`, `macos-amd64`, `macos-arm64`, `windows-amd64`,
  `wasm32-wasi` all build the hello-world program.
- Each target has a tested smoke binary.
- Cross-compilation from Linux host works for Windows and WASM.

### Phase 0 — linux-amd64

```
W17.0.1  Verify the native target builds on a Linux-amd64 runner.
         DoD: hello and a larger example build and run.
```

### Phase 1 — linux-arm64

```
W17.1.1  Cross-compile from Linux-amd64 host using `clang --target=aarch64-linux-gnu`.
         DoD: artifact produced; run via qemu-aarch64 in CI.

W17.1.2  Native compile on Linux-arm64 runner (if available).
         DoD: same tests pass.
```

### Phase 2 — macos-amd64 and macos-arm64

```
W17.2.1  Native compile on macOS-amd64 runner.
         DoD: hello builds and runs.

W17.2.2  Native compile on macOS-arm64 runner.
         DoD: hello builds and runs.

W17.2.3  Cross-compile macOS targets from a non-macOS host.
         DoD: either works with cctools-port, or the plan documents that macOS
              cross-builds require a macOS host.
```

### Phase 3 — windows-amd64

```
W17.3.1  Native compile on Windows-amd64 runner.
         DoD: hello builds and runs.

W17.3.2  Cross-compile from Linux-amd64 host using
         `clang --target=x86_64-pc-windows-msvc` and `lld-link`.
         DoD: produces a .exe; runs under Wine in CI.
```

### Phase 4 — wasm32-wasi

```
W17.4.1  Install WASI SDK in CI image.
         DoD: documented installation script.

W17.4.2  Cross-compile hello to wasm32-wasi.
         DoD: produces a .wasm file.

W17.4.3  Run the .wasm via wasmtime or wasmer in CI.
         DoD: program prints "hello" and exits 0.
```

### Phase 5 — Cross-target CI matrix

```
W17.5.1  Add a CI matrix that runs every target on every push to main.
         DoD: failures on any target block the merge.

W17.5.2  Add target-specific skip rules where the host cannot cross-compile.
         DoD: documented and tested.
```

**Wave 17 exit review.** Every target builds and runs hello. Native platforms pass the full test suite; cross platforms pass the minimum smoke test.

---

## Wave 18 — Beyond day one

Work here is permission-gated: tasks can be picked up only after the Day-1 surface is locked in (end of Wave 17) and an ADR authorizes the feature.

### 18.1 `select` on channels

Implement `select` as a statement that waits on multiple channels and fires the first ready arm. Requires an extension of the runtime's condvar usage and careful fairness guarantees.

### 18.2 Green threads

Implement a user-space scheduler with M:N thread multiplexing. Requires stack management, yield points in the compiler, and a cooperative runtime. Do not land this without a dedicated ADR and a consensus decision.

### 18.3 Incremental compilation (per-function)

Replace file-level incremental compilation with function-level caching. Requires a content hash over HIR and a cache directory. No effect on emitted binaries, but a large effect on build speed.

### 18.4 Macros

Add a hygienic macro system (decision between declarative and procedural is itself an ADR). Macros are a big design commitment; do not start until the language is stable enough that the macro surface would not churn.

### 18.5 `dyn Trait`

Add runtime dispatch via trait objects. Additive to monomorphization. Requires a vtable layout and a codegen extension.

### 18.6 SIMD (`Vec[T, N]` implementation)

The type and attribute are reserved from day one; implementation adds the MIR nodes, the codegen intrinsics, and the `@simd` loop-annotation handling. Target-specific: uses the C compiler's SIMD intrinsics where available.

### 18.7 Package management

A dependency resolver and vendor tool. Day one has no package manager. When this happens, the design is done from scratch with explicit attention to the reproducibility and no-runtime-spill rules.

### 18.8 Language server protocol

`cmd/fuse-lsp/` becomes the second `cmd/` binary. Reuses the Stage 2 frontend (lex, parse, resolve, check) to provide hover, go-to-definition, completion, diagnostics. No semantic indexing in the first version.

---

*End of implementation plan.*
