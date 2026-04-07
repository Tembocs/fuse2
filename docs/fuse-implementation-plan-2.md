# Fuse Implementation Plan (Revised)

> **For AI agents reading this document:**
> This is the revised phased implementation plan for the Fuse programming language. It supersedes `fuse-implementation-plan.md` and incorporates lessons learned from the Stage D (self-hosting) attempt. Nine phases, no timelines. Each phase has one job. The entry condition, deliverables, and done-when criterion are explicit for every phase. Milestones are marked clearly.
> The canonical test program throughout is `tests/fuse/milestone/four_functions.fuse`.
> The authoritative language reference is `fuse-language-guide-2.md`.

---

## Overview

| Phase | Name | Stage | Milestone |
|---|---|---|---|
| 1 | Test Suite | Pre-implementation | All `.fuse` test files written with expected outputs |
| 2 | Lexer & Parser | Stage 0 | All Core test files parse without error |
| 3 | Ownership Checker | Stage 0 | Invalid programs are rejected with correct errors |
| 4 | Evaluator | Stage 0 | ⭐ `four_functions.fuse` runs correctly |
| 5 | Language Stabilization | Between stages | Fuse Core is frozen — guide, tests, and ADRs are final |
| 6 | Rust Compiler Frontend | Stage 1 | All Core programs parse, check, and are rejected correctly |
| 7 | Rust Compiler Backend | Stage 1 | ⭐ Fuse Core compiles to native binaries |
| 8 | Fuse Full | Stage 1 | ⭐ Full test suite passes — concurrency and async work |
| 9 | Self-Hosting | Stage 2 | ⭐ Fuse compiles itself — Rust compiler no longer required |

⭐ = major milestone

---

## Phase 1 — Test Suite

**One job:** Write every `.fuse` test file and its expected output before any implementation exists.

### Entry condition

Language design is complete. The language guide (`fuse-language-guide-2.md`) is written. The repository layout is established.

### Why tests come first

The test files are plain text — they require no tooling, no interpreter, no compiler. Writing them now forces every remaining ambiguity in the language design to be resolved on paper rather than discovered mid-implementation. The lexer cannot be written until you know what it must tokenize. The evaluator cannot be written until you know what it must produce. The tests define both.

### Deliverables

```
tests/fuse/milestone/
  four_functions.fuse          <- the canonical program — write this first

tests/fuse/core/
  ownership/
    ref_read_only.fuse         <- ref cannot be assigned through
    mutref_modifies_caller.fuse
    move_transfers_ownership.fuse
    move_prevents_reuse.fuse   <- must produce compile error
  memory/
    asap_destruction.fuse      <- destroy order verified via __del__ output
    value_auto_lifecycle.fuse
    del_fires_at_last_use.fuse
  errors/
    result_propagation.fuse
    option_chaining.fuse
    match_exhaustive.fuse
    match_missing_arm.fuse     <- must produce compile error
    question_mark_shortcircuit.fuse
  types/
    val_immutable.fuse         <- reassign to val -> compile error
    var_mutable.fuse
    data_class_equality.fuse
    extension_functions.fuse
    type_inference.fuse
  control_flow/
    while_basic.fuse           <- while loop with counter
    while_break.fuse           <- break exits loop early
    while_continue.fuse        <- continue skips to next iteration
    while_nested_break.fuse    <- break from inner loop only
    for_with_break.fuse        <- break inside a for loop
  modules/
    import_basic.fuse          <- import a function from another module
    import_pub_only.fuse       <- non-pub items are not visible — compile error
    import_multiple.fuse       <- import several items from one module

tests/fuse/full/
  concurrency/
    chan_basic.fuse
    chan_bounded_backpressure.fuse
    shared_rank_ascending.fuse
    shared_rank_violation.fuse  <- must produce compile error
    shared_no_rank.fuse         <- must produce compile error
    spawn_mutref_rejected.fuse  <- must produce compile error
  async/
    await_basic.fuse
    suspend_fn.fuse
    write_guard_across_await.fuse  <- must produce compile warning
  simd/
    simd_sum.fuse
```

### Expected output format

Write the expected output as a comment block at the top of every test file. This makes the intent readable without a separate snapshot file, and gives AI agents a self-contained description of what the program must do.

```fuse
// EXPECTED OUTPUT:
// Hello, Amara.
// Flushed 3 ops
// Shutdown: clean exit
// Shutdown sequence complete

@entrypoint
fn main() {
  ...
}
```

Error test files state the expected error:

```fuse
// EXPECTED ERROR:
// error: cannot acquire @rank(2) while holding @rank(3)
//        acquire `db` before `metrics`, or release `metrics` first
//   --> shared_rank_violation.fuse:12:3

...
```

### Done when

Every file listed above exists. Every file has an `// EXPECTED OUTPUT:` or `// EXPECTED ERROR:` block. The expected outputs have been manually verified by reading the program logic — not by running it.

---

## Phase 2 — Lexer & Parser (Stage 0)

**One job:** Turn Fuse Core source text into an AST that represents it faithfully.

### Entry condition

Phase 1 is complete. Test files exist and expected outputs are written.

### Deliverables

**`stage0/src/token.py`** — Token type enum covering every terminal in Fuse Core:

```
Keywords:    fn, val, var, ref, mutref, owned, move, struct, data,
             class, match, when, if, else, for, in, loop, return,
             defer, spawn, async, await, suspend, and, or, not,
             while, break, continue, extern, pub, import
Operators:   => -> ?. ?: ? @ . .. : :: = == != < > <= >= + - * / %
Delimiters:  ( ) { } [ ] , ;
Literals:    Int, Float, String (including f"..." interpolation), Bool
Identifiers: names, type names (capitalized)
Annotations: @value, @entrypoint, @rank
```

**`stage0/src/lexer.py`** — Converts source text to a flat token stream.
Tracks line and column for every token — error messages need this.

**`stage0/src/ast.py`** — Dataclass definitions for every AST node:

```python
# Examples of the node types needed
@dataclass
class FnDecl:
    name: str
    params: list[Param]
    return_type: TypeExpr
    body: Block
    is_pub: bool          # pub visibility for module exports

@dataclass
class Param:
    convention: Literal["ref", "mutref", "owned"] | None
    name: str
    type_expr: TypeExpr

@dataclass
class MatchExpr:
    subject: Expr
    arms: list[MatchArm]

@dataclass
class MoveExpr:          # `move value` at call site
    value: Expr

@dataclass
class WhileStmt:
    condition: Expr
    body: Block

@dataclass
class BreakStmt:
    pass

@dataclass
class ContinueStmt:
    pass

@dataclass
class ImportDecl:
    module_path: str
    items: list[str]     # imported names
```

**`stage0/src/parser.py`** — Recursive descent parser. Produces an AST from the token stream. Covers all Fuse Core constructs:

- Function declarations, extension functions, `pub` visibility
- `val`/`var` bindings with type inference
- `struct`, `data class`, `@value`
- `match`, `when`, `if`, `for`, `loop`, `while`
- `break`, `continue`
- `defer`
- `import` declarations
- All expression forms: calls, field access, `?.`, `?:`, `?`, `f"..."`
- All four ownership conventions at declaration and call sites

### Done when

`python src/parser.py <file.fuse>` prints the AST for every file in
`tests/fuse/core/` without a parse error. The AST is human-readable.
No evaluation — parsing only.

---

## Phase 3 — Ownership Checker (Stage 0)

**One job:** Enforce the ownership model and reject invalid programs with clear error messages before any evaluation occurs.

### Entry condition

Phase 2 is complete. The parser produces an AST for all Core test files.

### What the checker must enforce

**Ownership conventions:**
- `ref` parameters cannot be assigned through or moved from inside the function
- `mutref` parameters can be modified but not moved or consumed
- `owned` parameters give full rights — the function may move or destroy the value
- A `move val` at a call site marks `val` as consumed — any subsequent use of `val` in the same scope is an error
- `mutref` must be explicitly written at the call site — implicit mutation
  is not permitted

**Match exhaustiveness:**
- Every `match` expression must cover all variants of the subject type
- A missing arm is a compile error, not a warning
- The wildcard `_` arm satisfies exhaustiveness for any remaining cases

**`val` immutability:**
- Assigning to a `val` binding after declaration is a compile error

**Module visibility:**
- Only items marked `pub` are visible to importing modules
- Importing a non-`pub` item is a compile error

**Basic type consistency:**
- A `Result<T,E>` returned where a `String` is expected is an error
- Full Hindley-Milner inference is not required for Stage 0 — catch obvious
  mismatches; deep inference comes in Stage 1

### Deliverables

**`stage0/src/checker.py`** — walks the AST and produces a list of
`CheckError` objects. If the list is non-empty, the program is rejected.

Error messages must include:
- The error description
- The file, line, and column
- A hint where one exists (e.g., "did you mean `mutref`?")

```
error: cannot use `conn` after `move`
       ownership was transferred on line 18
  --> four_functions.fuse:22:3
   |
18 |   shutdown(move conn)
   |            ^^^^^^^^^ moved here
22 |   println(conn.dsn)
   |   ^^^^^^^^^^^^^^^^ use after move
```

### Done when

All `_rejected.fuse` and `_error.fuse` test files produce the expected error. All valid Core test files pass the checker without error. Error messages match expected snapshots.

---

## Phase 4 — Evaluator (Stage 0)

**One job:** Execute Fuse Core programs. Make the milestone program run.

### Entry condition

Phase 3 is complete. The checker accepts valid programs and rejects invalid ones.

### Deliverables

**`stage0/src/values.py`** — Python representations of every Fuse runtime value:

```python
@dataclass
class FuseResult:
    is_ok: bool
    value: Any          # the Ok value or the Err value

@dataclass
class FuseOption:
    is_some: bool
    value: Any | None

@dataclass
class FuseStruct:
    type_name: str
    fields: dict[str, Any]
    on_del: Callable | None   # __del__ hook for ASAP simulation

@dataclass
class FuseMap:
    entries: dict[Any, Any]   # Map<K, V> backed by Python dict
```

**`stage0/src/environment.py`** — Scope chain and binding management.
Tracks which bindings are live, which have been moved, and which are `val` vs `var`.

**`stage0/src/evaluator.py`** — Tree-walking evaluation of every AST node:

- All expression forms
- Pattern matching with destructuring
- `?` operator — unwrap `Ok`/`Some` or return early
- `defer` — register callbacks that fire when the scope exits
- ASAP destruction simulation — track last use of each value, call `__del__`
  at that point
- Extension function dispatch
- `f"..."` string interpolation
- `?.` and `?:` short-circuit evaluation
- `while` loops with `break` and `continue`
- `Map<K, V>` — creation, `.set()`, `.get()` returning `Option<V>`, `.remove()`, iteration
- Module resolution and `import`

**`stage0/src/main.py`** — CLI entry point:

```bash
python src/main.py <file.fuse>       # run a file
python src/main.py --repl            # interactive REPL
python src/main.py --check <file>    # check without running
```

**`stage0/tests/run_tests.py`** — automated test runner. Executes every `.fuse` file in `tests/fuse/core/`, compares stdout to the expected output comment, reports pass/fail.

### ⭐ Stage 0 milestone

```bash
python src/main.py ../../tests/fuse/milestone/four_functions.fuse
```

Output matches `// EXPECTED OUTPUT:` exactly. All tests in `tests/fuse/core/` pass. The Python interpreter stays permanently as the reference implementation.

### Done when

The Stage 0 milestone is met. All `tests/fuse/core/` tests pass. The test runner exits with zero failures.

---

## Phase 5 — Language Stabilization

**One job:** Fix every gap, inconsistency, and ambiguity that Phase 4
revealed. Freeze Fuse Core. Do not begin Stage 1 until this phase is complete.

### Entry condition

Phase 4 is complete. `four_functions.fuse` runs. The test suite passes.

### Why this phase exists

Implementing the interpreter exposes things the design could not. Edge cases in ownership semantics become visible when you try to evaluate them. Error message quality reveals where the language is ambiguous. Missing test cases appear when programs fail in unexpected ways.

Stage 1 is a Rust compiler. Rust has a much higher cost of change than Python. Every mistake that survives into Stage 1 is significantly more expensive to fix than a mistake caught in Stage 0. This phase is the firewall between the two.

### Deliverables

**Language guide updates** — every gap found in Phase 4 is documented in `fuse-language-guide-2.md` with a clear rule and a code example. No gap is left as implicit.

**New or corrected test files** — any behaviour that was ambiguous now has
a test that pins it down.

**New ADRs** — any decision made during Phase 4 that was not covered by an
existing ADR gets its own entry.

**Fuse Core definition frozen** — a written statement in the language guide that Fuse Core is stable. This is the contract that Stage 1 implements.

### Done when

The language guide accurately describes every behaviour of the Stage 0 interpreter. No known ambiguity remains. The Fuse Core definition is marked stable in the guide. Any developer or AI agent reading the guide could implement a second correct interpreter independently.

---

## Phase 6 — Rust Compiler Frontend (Stage 1)

**One job:** Reproduce the lexer, parser, and checker from Stage 0 in Rust, with production-grade error messages.

### Entry condition

Phase 5 is complete. Fuse Core is frozen and documented.

### Why Rust for Stage 1

Rust is the right host for a compiler targeting systems-level code. Its pattern matching over enum types maps naturally to AST walking. Its type system catches logic errors in the compiler itself. Its performance makes a fast compiler possible without effort. The team's familiarity with Rust means the codebase is maintainable.

### Deliverables

**`stage1/fusec/src/lexer/`** — Rust tokenizer. Same token set as Stage 0.
Same line/column tracking.

**`stage1/fusec/src/parser/`** — Rust recursive descent parser. Produces the same AST structure as Stage 0, defined as Rust enums.

**`stage1/fusec/src/ast/`** — AST node definitions. Every construct in Fuse Core as a Rust enum or struct.

**`stage1/fusec/src/hir/`** — High-level intermediate representation. The AST is lowered to HIR after parsing. HIR makes type information and ownership annotations explicit — the checker operates on HIR, not AST.

**`stage1/fusec/src/checker/`** — All semantic checks in Rust:

| File | Responsibility |
|---|---|
| `types.rs` | Type inference and consistency checking |
| `ownership.rs` | `ref`/`mutref`/`owned`/`move` enforcement |
| `exhaustiveness.rs` | `match` arm coverage |
| `rank.rs` | `@rank` ordering on `Shared<T>` |
| `spawn.rs` | `mutref` capture across `spawn` — compile error |
| `async_lint.rs` | Write guard held across `await` — compile warning |

### Done when

`cargo run --bin fusec -- --check <file.fuse>` accepts all valid Core test files and rejects all error test files with messages matching their `// EXPECTED ERROR:` blocks. No code generation yet — checking only.

---

## Phase 7 — Rust Compiler Backend (Stage 1)

**One job:** Generate native binaries from checked Fuse Core programs using Cranelift.

### Entry condition

Phase 6 is complete. The checker accepts and rejects programs correctly.

### Why Cranelift

Cranelift is a code generation backend — it is designed for exactly this use. Its API is simpler than LLVM's. Its compilation speed is faster. Its output quality is sufficient for Stage 1. LLVM can be added as an optional backend later for maximum optimisation; Cranelift is the right choice to get native code working first.

### Known pitfalls (discovered during Stage D)

The following issues were discovered during the self-hosting attempt and must be addressed in this phase. They are documented here so they are not rediscovered the hard way.

**mutref_cells writeback timing.** The `mutref_cells` map, which tracks which Cranelift variables back `mutref` parameters, must be populated *before* the function body is compiled. If writeback entries are set after body compilation, the codegen emits stores to variables that do not yet exist. The fix is: allocate the writeback cell and insert into `mutref_cells` during function prologue generation, before `compile_block` is called on the body.

**and/or short-circuit SSA corruption (fixed).** The short-circuit lowering of `and` and `or` expressions previously created SSA merge blocks without threading live mutable variables through as explicit block parameters. In large functions with loops and `mutref` writebacks, the merge block could inherit stale SSA values from the wrong predecessor. Fixed by explicitly passing all live SSA variables as block params to the merge block from both predecessors, then rebinding them after the merge. The same fix was applied to the elvis (`?:`) operator. Regression tests: `and_or_loop_mutref.fuse`, `or_loop_mutref.fuse`.

**UTF-8 string API (resolved).** The runtime provides a dual API: `len()` returns byte length (O(1)), `charCount()` returns character count (O(n)), `charAt(i)` uses character indexing (O(n), safe for multi-byte), `byteAt(i)` returns byte at byte offset (O(1)). All stdlib modules use `charCount()`/`charAt()` for character iteration. `byteAt()` is available for performance-critical byte-level access (e.g., Stage 2 lexer).

**Stack size for compiler workloads.** The default stack size (1MB on Windows) is insufficient for compiling large programs — the compiler itself is a large program. The linker invocation must set the stack size to at least 8MB. On Windows this means passing `/STACK:8388608` to `link.exe`. On Linux, `ulimit -s 8192` or an equivalent linker flag.

### Deliverables

**`stage1/fusec/src/codegen/`** — HIR to Cranelift IR translation:

- All Fuse Core value types mapped to Cranelift types
- Function calls, closures, and recursion
- `while` loops with `break` and `continue`
- `match` lowered to Cranelift branch/switch
- `defer` lowered to cleanup blocks
- ASAP destruction — destructor calls inserted at last-use points during
  HIR lowering

**`stage1/cranelift-ffi/`** — A thin Rust crate that exposes Cranelift's code generation API as `extern "C"` functions callable from Fuse via FFI. This crate is essential for Stage 2 — the self-hosted compiler cannot link against Cranelift's Rust API directly, so it calls through this C-compatible wrapper layer.

```
stage1/cranelift-ffi/
  Cargo.toml
  src/lib.rs          <- extern "C" functions wrapping Cranelift operations
```

The FFI surface covers: module creation, function declaration, block creation, instruction emission, variable declaration and definition, finalizing and emitting object code. Every function takes and returns only C-compatible types (pointers, integers, byte arrays).

**`stage1/fuse-runtime/src/asap.rs`** — Runtime support for ASAP destruction.
The compiler inserts calls into this at last-use points. Handles the cases where last-use is conditional (inside a `match` arm, early `return`, etc.).

**`stage1/fuse-runtime/src/string_ops.rs`** — String operations. Dual API: character-aware indexing (`charAt`, `charCount`, `slice`) and byte-level access (`len`, `byteAt`) for performance-critical code.

**`stage1/fusec/src/codegen/layout.rs`** — Value layout and ABI. How Fuse structs are laid out in memory. How arguments and return values are passed.

### ⭐ Stage 1 Core milestone

```bash
cargo run --bin fusec -- ../../tests/fuse/milestone/four_functions.fuse
./four_functions
```

Output matches expected. The program is a native binary. All `tests/fuse/core/` programs compile and produce correct output. The Stage 0 Python interpreter and the Stage 1 compiled binary produce identical output for every Core test.

### Done when

The Stage 1 Core milestone is met. Every `tests/fuse/core/` test compiles and passes. Output is byte-for-byte identical to Stage 0 snapshots. The `cranelift-ffi` crate builds and its functions are callable from C.

---

## Phase 8 — Fuse Full (Stage 1)

**One job:** Add every feature that distinguishes Fuse Full from Fuse Core: the concurrency model, async runtime, SIMD, and the complete standard library.

### Entry condition

Phase 7 is complete. Fuse Core compiles to native binaries correctly.

### Deliverables

**Concurrency — `Chan<T>`:**

```fuse
val (tx, rx) = Chan::<User>.bounded(4)
spawn worker(move tx)
val result = await rx.recv()?
```

`stage1/fuse-runtime/src/chan.rs` — bounded and unbounded channels.
Thread-safe, integrated with the async executor.

**Concurrency — `Shared<T>` and `@rank`:**

```fuse
@rank(1) val config  = Shared::new(Config.load())
@rank(2) val metrics = Shared::new(Vec<Metric>.new())
```

`stage1/fuse-runtime/src/shared.rs` — `RwLock`-backed `Shared<T>`.
Read guards and write guards that call ASAP destructors on drop.
`stage1/fusec/src/checker/rank.rs` already written in Phase 6 — wire it to the runtime in this phase.

**Async runtime:**

`stage1/fuse-runtime/src/async_rt.rs` — lightweight async executor.
No tokio dependency. Supports `spawn`, `await`, `suspend`. Designed to be small and understandable — the goal is correctness, not maximum throughput.

**SIMD:**

```fuse
val avg = SIMD<Float32, 8>.sum(values) / values.len().toFloat()
```

`stage1/fuse-runtime/src/simd.rs` — mapped to platform SIMD intrinsics
via Cranelift's vector operations.

**Standard library — Fuse Full:**

```
stdlib/full/
  chan.fuse     <- Chan<T> API written in Fuse
  shared.fuse   <- Shared<T> API written in Fuse
  timer.fuse    <- Timer.sleep, Timeout
  simd.fuse     <- SIMD<T,N> API written in Fuse
  http.fuse     <- Http.get, Http.post (used in canonical example)
```

### ⭐ Stage 1 Full milestone

```bash
cargo run --bin fusec -- ../../tests/fuse/milestone/four_functions.fuse
./four_functions
```

The full version of `four_functions.fuse` — including `spawn`, channels, `Shared<T>`, and `async`/`await` — compiles and runs correctly. Every test in `tests/fuse/full/` passes. `@rank` violations, spawn capture violations, and missing rank annotations all produce correct compile errors.

### Done when

The Stage 1 Full milestone is met. The complete `tests/fuse/` suite passes. The compiler is usable for writing real Fuse programs.

---

## Phase 9 — Self-Hosting (Stage 2)

**One job:** Write the Fuse compiler in Fuse Core. Make Fuse compile itself.

### Entry condition

Phase 8 is complete. The Rust compiler handles all of Fuse Full. The language is stable enough that writing a compiler in it is practical — every feature the compiler needs exists in the language.

### Why self-hosting matters

Self-hosting is not a vanity milestone. It is the proof that the language is complete and expressive enough to build real production software. It also means every future improvement to Fuse is immediately available to the compiler itself. The compiler becomes the largest, most real-world test of the language.

### Constraints (learned from the Stage D attempt)

The first self-hosting attempt revealed hard constraints on how the compiler must be written. These are not suggestions — violating them produces broken binaries or crashes.

**Use `while`/`break`/`continue` for all iteration.** Do not use recursion to simulate loops. Recursive iteration blows the stack on compiler-sized workloads and makes the control flow harder for the codegen to lower correctly.

**Use modules to split across files.** A monolithic single-file compiler is unmaintainable and exceeds the practical limits of what the Stage 1 compiler can handle in a single compilation unit. Split the compiler into modules using `import` and `pub`.

**~~Avoid `and`/`or` in functions that combine loops with `mutref` parameters.~~** This limitation has been resolved. The SSA corruption bug in short-circuit lowering was fixed by explicitly threading all live SSA variables through the merge block as block parameters.

**Set stack size to 8MB.** The linker invocation for the self-hosted compiler binary must request 8MB of stack. The compiler compiling itself is a deep workload.

### Bootstrap sub-stages

Self-hosting proceeds in four sub-stages. Each sub-stage has a concrete goal and depends on the previous one.

**Stage A — Foundation.** Write the lexer, parser, and AST in Fuse. Compile them with the Stage 1 Rust compiler. The goal is a program that can tokenize and parse `.fuse` files and print the AST. This validates that the language's string handling, data structures, and control flow are sufficient for compiler infrastructure.

**Stage B — Code generation.** Write the Cranelift codegen in Fuse, calling through the `cranelift-ffi` crate via `extern` function declarations. Compile with Stage 1. The goal is a program that can take a parsed AST and emit a working native binary for a trivial program (e.g., a program that prints a number).

**Stage C — Complete compiler.** Combine Stages A and B into a complete compiler (`fusec2`) that can compile all of Fuse Core. Compile with Stage 1. The goal is: `fusec2` compiles every `tests/fuse/core/` test and produces correct output.

**Stage D — Bootstrap and verify.** Use `fusec2` (compiled by Stage 1) to compile `fusec2` itself. Then use that output to compile `fusec2` a second time. Compare the two self-compiled binaries.

### Bootstrap sequence (exact commands)

```bash
# Stage A-C: Build fusec2 using the Rust compiler
cd stage1
cargo run --bin fusec -- ../stage2/src/main.fuse -o ../fusec2-bootstrap.exe

# Stage D step 1: fusec2 compiles itself
./fusec2-bootstrap.exe stage2/src/main.fuse -o fusec2-stage2.exe

# Stage D step 2: the self-compiled compiler compiles itself again
./fusec2-stage2.exe stage2/src/main.fuse -o fusec2-verified.exe

# Stage D step 3: reproducibility check
sha256sum fusec2-stage2.exe fusec2-verified.exe
# hashes must match — if they do, bootstrap is correct
```

On Windows, replace `sha256sum` with `certutil -hashfile <file> SHA256`.

### Deliverables

**`stage2/src/`** — the Fuse compiler written in Fuse Core, split into modules:

```
stage2/src/
  main.fuse
  lexer.fuse
  token.fuse
  parser.fuse
  ast.fuse
  codegen.fuse         <- calls cranelift-ffi via extern declarations
  string_util.fuse
```

Each module uses `pub` to export its public API. `main.fuse` imports from the other modules. No file exceeds a manageable size.

### ⭐ Stage 2 milestone

```bash
# Compile the Fuse compiler using itself
./fusec2-stage2.exe stage2/src/main.fuse -o fusec2-verified.exe

# Verify reproducibility
# fusec2-stage2.exe and fusec2-verified.exe have identical hashes
```

The Rust compiler is no longer required to build Fuse. The project is self-sufficient.

### Done when

The Stage 2 milestone is met. The reproducibility check passes. The Stage 1 Rust compiler is archived as a bootstrap tool, not retired — it remains the fastest way to rebuild from scratch on a new platform.

---

## Progression summary

```
Phase 1  --  Write tests (including control flow + module tests)
Phase 2  --  Lex + parse (Stage 0, with while/break/continue/import/pub)
Phase 3  --  Check ownership (Stage 0)
Phase 4  --  Evaluate  -----------------------------------  ⭐ four_functions.fuse runs
Phase 5  --  Stabilize — freeze Fuse Core
Phase 6  --  Rust frontend (Stage 1)
Phase 7  --  Cranelift backend + cranelift-ffi  ----------  ⭐ native binaries
Phase 8  --  Fuse Full  ----------------------------------  ⭐ full test suite passes
Phase 9  --  Self-hosting (Stages A-B-C-D)  --------------  ⭐ Fuse compiles itself
```

No phase begins until the previous phase's done-when condition is met. No phase has a deadline. Each phase is complete when it is correct.

---

## Key principle across all phases

**The guide precedes the implementation.**

If a behaviour is not in `fuse-language-guide-2.md`, it does not exist yet. If implementation reveals that the guide is wrong, fix the guide first, then the implementation. The guide is the contract. The tests are the verification. The implementation is the proof.

---

*End of Fuse Implementation Plan (Revised)*

---

> **For AI agents:**
> Phase entry conditions are explicit — check them before beginning any phase. The canonical test program is `tests/fuse/milestone/four_functions.fuse`. The authoritative language reference is `fuse-language-guide-2.md`. Stage boundaries are at phases 4->5 (stabilization) and 8->9 (self-hosting). Phase 9 has four sub-stages (A through D) with hard constraints on iteration style, module usage, and `and`/`or` avoidance. The guide must be updated before any implementation in any phase.
