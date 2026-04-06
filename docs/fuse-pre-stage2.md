# Fuse Stage 1 — Pre-Stage 2 Plan

> **Status:** In progress. Wave 0 complete.
> **Scope:** 9 waves, 43 phases, ~300 tasks.
> **Gate:** This is the final gate before Stage 2 self-hosting begins.
> Its purpose is to eliminate every known compiler limitation, add the
> interface system, close all workaround patterns in the stdlib, and
> build absolute trust in the Stage 1 compiler.
>
> **This plan does not tolerate half measures.** Every phase must be
> complete, tested, and green. Every fix must have a regression test.
> Every existing test must remain green. No workarounds. No deferrals.
> No excuses.

---

## Language Philosophy (Non-Negotiable)

Fuse is not a research language. It is designed to be implemented,
self-hosted, and used to build production systems. Every feature has
been proven in production at scale. Fuse does not experiment — it
integrates.

**The three non-negotiable properties are:**

1. **Memory safety without garbage collection.** ASAP (As Soon As
   Possible) deterministic destruction. Values are destroyed at their
   last use point. No GC pauses. No manual free. No dangling pointers.

2. **Concurrency safety without a borrow checker.** Ownership
   conventions (`ref`, `mutref`, `owned`, `move`) are declared at
   function signatures. `Shared<T>` with ranked locking prevents data
   races and deadlocks at compile time. No lifetime annotations. No
   borrow wars.

3. **Developer experience as a first-class concern.** Clean syntax.
   Helpful error messages. Fast compilation. The language serves the
   developer, not the other way around.

The authoritative spec is `docs/fuse-language-guide-2.md`. All work
must conform to the language guide. If the guide says it, we implement
it. If the guide does not say it, we do not invent it.

**Every decision, every fix, every line of code written during this
plan must serve these three properties. If a change undermines memory
safety, concurrency safety, or developer experience, it is wrong —
regardless of how clever or expedient it may be.**

---

## Mandatory Rules

> **These rules apply to every wave and every phase in this document.**
> **Before starting any wave, re-read this entire section.**
> **Before starting any phase, re-read this entire section.**
> **No exceptions. No shortcuts.**

### Rule 1: Read Before You Build

Before starting **any wave**, you must read:

1. The **Language Philosophy** section above (every time, no skipping)
2. This **Mandatory Rules** section (every time, no skipping)
3. The specific files listed in the wave's `Before starting` block
4. The `docs/fuse-language-guide-2.md` sections referenced in the wave

Before starting **any phase**, you must re-read:

1. The **Language Philosophy** section
2. This **Mandatory Rules** section

Do not begin implementation until you have read and understood all
required material. Reading is not optional. It is the first task of
every wave and every phase. If you skip reading, you will introduce
bugs that violate the language spec. That is not acceptable.

### Rule 2: No TODO, No Defer, No Workaround

Every item in this plan is scoped to be completed within this plan.
There are no items marked "TODO", "defer", "later", or "workaround".

- If a task is in a phase, it must be completed in that phase.
- If a task cannot be completed due to a blocker, the blocker is
  fixed first — not deferred.
- Workarounds that were applied during stdlib implementation must be
  replaced with proper fixes. The stdlib must not retain workaround
  patterns after this plan completes.
- Work that is genuinely out of scope is listed in
  `docs/fuse-post-stage2.md` with an explicit rationale for deferral
  and conditions under which it becomes actionable.

After completing each phase, scan all modified files for `TODO`,
`FIXME`, `HACK`, and `WORKAROUND`. If any are found, resolve them
before marking the phase done.

### Rule 3: Zero Regressions

Every fix must have at least one new regression test. Every existing
test must remain green after every phase. If a phase introduces a
test failure in any existing test, the phase is not complete.

- After each phase: `cargo test` in `stage1/` — all tests green.
- After each wave: full test suite run including `tests/fuse/` fixtures.
- After Wave 3 (stdlib polish): every `tests/fuse/stdlib/` test must
  pass unchanged (behavior preserved, only code style updated).

### Rule 4: Vigilance, Robustness, Professionalism

This is a compiler. Compilers do not get to be "mostly correct." A
single codegen bug can produce silent data corruption in every program
compiled by this compiler. Treat every line of compiler code with the
gravity it deserves.

- **Vigilance:** Read error messages. Read test output. Read the code
  you are modifying. Do not assume — verify. When a test passes, ask
  yourself: does it pass for the right reason? When a test fails, read
  the actual vs expected output before changing anything.
- **Robustness:** Handle edge cases. A fix that works for the common
  case but crashes on an edge case is not a fix. Think about: empty
  inputs, nested expressions, recursive structures, mixed types, the
  absence of optional branches.
- **Professionalism:** Clean code. Clear commit messages. No
  commented-out code. No debugging prints left behind. No "I'll fix
  this later." Every commit should be something you would proudly show
  to a peer reviewer.

### Rule 5: Completion Standard

A phase is done when:

1. All checkbox tasks in the phase are complete
2. All new tests pass
3. All existing tests pass (`cargo test` green)
4. The code is clean (no debugging artifacts, no temporary hacks)
5. No `TODO`, `FIXME`, `HACK`, or `WORKAROUND` in modified files
6. The phase can be demonstrated: the fix works, the test proves it,
   and the old broken behavior no longer occurs

A wave is done when all phases in the wave meet the completion
standard. **After a wave completes, stop and report before proceeding
to the next wave.** Do not silently continue.

---

## Status Key

- `[ ]` — not started
- `[~]` — in progress
- `[x]` — done
- `[!]` — blocked (must state what blocks it)

---

## Task Summary

| Wave | Name | Phases | Tasks | Depends On | Status |
|------|------|--------|-------|------------|--------|
| W0 | Critical Bug Fixes | 7 | 73 | — | **Done** |
| W1 | Language Feature Completion | 4 | 30 | W0 | Not started |
| W2 | Numeric Type System | 9 | 52 | W0.2, W0.5 | Not started |
| W3 | Stdlib Polish | 3 | 19 | W0 + W1 | Not started |
| W4 | Annotation System | 3 | 18 | W0 + W1 | Not started |
| W5 | Interface System | 6 | 52 | W1 (structs, generics) | Not started |
| W6 | Evaluator Robustness | 2 | 9 | — | Not started |
| W7 | LSP Foundation | 4 | 20 | W0 + W1 | Not started |
| W8 | WASM Target | 4 | 16 | W0 + W1 + W2 | Not started |
| **Total** | | **42** | **~289** | | |

---

## Resolved Design Decisions

> These questions were raised during design and resolved before
> implementation. Decisions are final. Do not re-open without new
> evidence that was not available at decision time.

### Interface System

| # | Question | Decision | Rationale |
|---|----------|----------|-----------|
| 1 | Should `enum` types implement interfaces? | **Yes** | Enums are first-class types. Not allowing `enum Color implements Printable` would prevent enums from being printable, hashable, or serializable. Implementation: add `implements: Vec<String>` to `EnumDecl`. |
| 2 | Can module-level `pub fn` satisfy an interface method? | **No** | Interface methods must be extension methods with explicit `self`. Consistency: all implementations use the same `fn Type.method(ref self)` mechanism. Free functions serve a different purpose. |
| 3 | Runtime interface check (`is Printable`)? | **Defer** | Requires RTTI. Static dispatch sufficient for self-hosting. See `docs/fuse-post-stage2.md`. |
| 4 | Marker interfaces (no methods)? | **Yes** | Implementation cost is near-zero. Useful for future `Send`/`Sync` equivalents. Checker validates `implements` even with no methods to verify. |
| 5 | Dynamic dispatch (`dyn Interface`)? | **Defer** | Static dispatch only. No vtables. See `docs/fuse-post-stage2.md`. |

### Concurrency Model

| # | Question | Decision | Rationale |
|---|----------|----------|-----------|
| 6 | `select` syntax? | **Defer** | Requires runtime scheduler integration that does not exist. See `docs/fuse-post-stage2.md`. |
| 7 | Should `spawn` return a joinable handle? | **Defer** | Fire-and-forget + channels is sufficient. See `docs/fuse-post-stage2.md`. |
| 8 | Spawn pool size limit? | **Unbounded** | Developer's responsibility. Matches Go's goroutine model. |
| 9 | Runtime scheduling model? | **OS threads** | Simple, correct, proven. Green threads are a post-Stage 2 optimization. |

---

## Wave 0 — Critical Bug Fixes [DONE]

> All 7 phases complete. 73 tasks done. All tests green.

---

### Phase W0.1 — if/else as Return Expression [DONE]

- [x] **W0.1.1** Add helper `split_block_final_expr(stmts) -> (prefix, Option<Expr>)`
- [x] **W0.1.2** Update then-branch in `compile_if` to compile final expr
- [x] **W0.1.3** Update `ElseBranch::Block` — extract and compile final expr
- [x] **W0.1.4** Update `ElseBranch::IfExpr` recursive case — propagate value
- [x] **W0.1.5** Update return `TypedValue` type via `infer_expr_type()`
- [x] **W0.1.6** Preserve existing behavior when no else branch (type = Unit)
- [x] **W0.1.7** Test: `if_else_return_value.fuse`
- [x] **W0.1.8** Test: `if_elif_else_value.fuse`
- [x] **W0.1.9** Test: `if_else_in_function.fuse`
- [x] **W0.1.10** Full test suite green

---

### Phase W0.2 — Float Literal Codegen [DONE]

- [x] **W0.2.1** Add `fuse_rt_float_from_f64` to runtime
- [x] **W0.2.2** Register in codegen runtime function table
- [x] **W0.2.3** Implement `LiteralValue::Float(f)` in `compile_literal`
- [x] **W0.2.4** Test: `float_literal_basic.fuse`
- [x] **W0.2.5** Test: `float_literal_arithmetic.fuse`
- [x] **W0.2.6** Test: `float_literal_negative.fuse`
- [x] **W0.2.7** Verify `float_test.fuse` stdlib test passes
- [x] **W0.2.8** Full test suite green

---

### Phase W0.3 — F-String Nested Quote Support [DONE]

- [x] **W0.3.1** Add `brace_depth` counter in `read_string`
- [x] **W0.3.2** Increment on `{` when formatted
- [x] **W0.3.3** Decrement on `}` when formatted
- [x] **W0.3.4** Only treat `"` as terminator when `brace_depth == 0`
- [x] **W0.3.5** Handle escaped braces
- [x] **W0.3.6** Test: `fstring_nested_quotes.fuse`
- [x] **W0.3.7** Test: `fstring_method_in_braces.fuse`
- [x] **W0.3.8** Verify existing f-string tests pass
- [x] **W0.3.9** Full test suite green

---

### Phase W0.4 — Builder Method `mutref Self` Return Type [DONE]

- [x] **W0.4.1** Resolve `Self` in return type in `emit_object()`
- [x] **W0.4.2** Handle both `"Self"` and `"mutref Self"` patterns
- [x] **W0.4.3** Apply in `collect_ir_text()` extension function loop
- [x] **W0.4.4** Test: `builder_mutref_self.fuse`
- [x] **W0.4.5** Test: `builder_chain_three.fuse`
- [x] **W0.4.6** Full test suite green

---

### Phase W0.5 — `Type.staticMethod()` Call Syntax [DONE]

- [x] **W0.5.1** Add `static_functions` map to `ModuleInfo` in checker
- [x] **W0.5.2** Route static methods (no `self` param) to `static_functions`
- [x] **W0.5.3** Add `resolve_static_function()` lookup
- [x] **W0.5.4** Update `check_call` for `Expr::Member` on type names
- [x] **W0.5.5** Add `statics` map to `LoadedModule` in codegen
- [x] **W0.5.6** Populate `statics` during module loading
- [x] **W0.5.7** Declare static function symbols in `declare_user_surface`
- [x] **W0.5.8** Compile static calls without receiver argument
- [x] **W0.5.9** Test: `static_method_basic.fuse`
- [x] **W0.5.10** Test: `static_method_constructor.fuse`
- [x] **W0.5.11** Full test suite green

---

### Phase W0.6 — Async/Await/Suspend Removal [DONE]

> Removed `async`, `await`, and `suspend` keywords from the language.
> Keep `spawn` as the sole concurrency primitive. Fuse does not have
> async/await. Concurrency is expressed through `spawn` blocks and
> channels.

**Step 1 — AST:**
- [x] **W0.6.1** Remove `is_async` from `FunctionDecl`
- [x] **W0.6.2** Remove `is_suspend` from `FunctionDecl`
- [x] **W0.6.3** Remove `is_async` from `SpawnStmt`
- [x] **W0.6.4** Remove `Await(AwaitExpr)` from `Expr` enum
- [x] **W0.6.5** Remove `Await` arm from `Expr::span()`
- [x] **W0.6.6** Remove `AwaitExpr` struct

**Step 2 — Parser:**
- [x] **W0.6.7** Remove `Async | Suspend` from declaration dispatch
- [x] **W0.6.8** Remove async/suspend flag parsing in `parse_function()`
- [x] **W0.6.9** Remove fields from `FunctionDecl` construction
- [x] **W0.6.10** Remove `is_async` from `parse_spawn()`
- [x] **W0.6.11** Remove `Await` arm from `parse_unary()`

**Step 3 — Checker:**
- [x] **W0.6.12** Remove synthetic `is_async`/`is_suspend` fields
- [x] **W0.6.13** Remove `Await` arm from `check_spawn_expr`
- [x] **W0.6.14** Remove write-guard-across-await warning
- [x] **W0.6.15** Remove `Await` arm from `infer_expr_type`

**Step 4 — Codegen:**
- [x] **W0.6.16** Remove `Await` passthrough from `compile_expr`
- [x] **W0.6.17** Remove fields from lambda `FunctionDecl`
- [x] **W0.6.18** Remove `Await` arm from `infer_expr_type`
- [x] **W0.6.19** Remove `Await` arm from `collect_expr_names`

**Step 5 — Evaluator:**
- [x] **W0.6.20** Remove synthetic fields (two locations)
- [x] **W0.6.21** Remove `Await` passthrough from `eval_expr`
- [x] **W0.6.22** Remove `Await` arm from `collect_expr_names`

**Step 6 — AST Dump:**
- [x] **W0.6.23** Remove `Await` printer arm from `main.rs`

**Step 7 — Lexer:**
- [x] **W0.6.24** Remove `Async`, `Await`, `Suspend` from `TokenKind`
- [x] **W0.6.25** Remove from `keyword_kind()`
- [x] **W0.6.26** `cargo build` — clean compilation

**Step 8 — Tests:**
- [x] **W0.6.27** Delete `tests/fuse/full/async/` (6 files)
- [x] **W0.6.28** `cargo test` — all remaining tests pass

**Step 9 — Stage 0:**
- [x] **W0.6.29** Remove keywords from `fuse_token.py`

**Step 10 — Language Spec:**
- [x] **W0.6.30** Remove section 1.18 (Async) from language guide
- [x] **W0.6.31** Remove from Fuse Full feature listing
- [x] **W0.6.32** Update `spawn async` examples to `spawn`
- [x] **W0.6.33** Remove `async_rt.rs` and `async_lint` references

---

### Phase W0.7 — Concurrency Guide Patterns [DONE]

> Add concurrency patterns from the concurrency model design to the
> language guide §1.17, completing the concurrency documentation.

- [x] **W0.7.1** Add "How I/O Works Without async/await" subsection to §1.17:
      sequential call vs `spawn` + channel pattern (two code examples).
- [x] **W0.7.2** Add "Patterns" subsection to §1.17 with: timeout via
      `recv_timeout`, worker pool (bounded channel + N spawned workers),
      and pipeline (spawn produces to output channel).
- [x] **W0.7.3** Fix spawn capture syntax in §1.17: remove `||` (not Fuse
      syntax). Use `spawn move { }` and `spawn ref { }`.
- [x] **W0.7.4** Run Stage 0 test suite to verify no test header references
      changed syntax.

---

## Wave 1 — Language Feature Completion

> **MANDATORY:** Before starting this wave, re-read the **Language
> Philosophy** and **Mandatory Rules** sections at the top of this
> document. Then read:
>
> - `docs/fuse-language-guide-2.md` — sections on structs, data classes,
>   generics, and module system
> - `stage1/fusec/src/ast/nodes.rs` — `DataClassDecl`, `FunctionDecl`,
>   `StructDecl`, `Declaration` enum
> - `stage1/fusec/src/parser/parser.rs` — `parse_data_class`,
>   `parse_function`, `parse_struct`
> - `stage1/fusec/src/hir/nodes.rs` — HIR equivalents
> - `stage1/fusec/src/codegen/object_backend.rs` — `declare_user_surface`,
>   `emit_object`, data class compilation loops
>
> **Verify all W0 tests are green before starting this wave.**

---

### Phase W1.1 — Struct Compilation [DONE]

**Root cause:** `declare_user_surface()` and `emit_object()` iterate
over functions, data class methods, and extern functions — but have NO
iteration over `loaded.structs`. Structs are parsed, lowered to HIR,
and registered in the checker, but completely skipped during codegen.

**Fix approach:** Compile struct methods the same way data class methods
are compiled. Generate constructor and destructor. Do NOT generate
public field accessors (structs are opaque).

- [x] **W1.1.1** Add struct iteration in `declare_user_surface` — declare
      struct method symbols (parallel to data class methods).
- [x] **W1.1.2** Add struct constructor (`__init__`) declaration.
- [x] **W1.1.3** Add struct destructor (`__del__`) declaration.
- [x] **W1.1.4** Add struct method compilation in `emit_object`.
- [x] **W1.1.5** Compile struct constructor body (allocate, set fields).
- [x] **W1.1.6** Compile struct destructor body (release fields).
- [x] **W1.1.7** Verify struct fields NOT accessible outside methods.
- [x] **W1.1.8** Test: `struct_compiled.fuse` — construct and call methods.
- [x] **W1.1.9** Test: `struct_destructor.fuse` — ASAP destruction fires.
- [x] **W1.1.10** Verify existing `struct_basic.fuse` passes.
- [x] **W1.1.11** Full test suite green.

---

### Phase W1.2 — Generic Data Class Syntax [DONE]

**Root cause:** `parse_data_class` expects `(` immediately after name —
no `<...>` type parameter parsing. `DataClassDecl` has no `type_params`.

**Fix approach:** Add `type_params: Vec<String>`. Parse `<T, U>` after
name. Pass through HIR. Codegen needs no change (type erasure).

- [x] **W1.2.1** Add `type_params: Vec<String>` to `DataClassDecl`.
- [x] **W1.2.2** Parse `<...>` type params after name in `parse_data_class`.
- [x] **W1.2.3** Update HIR `DataClassDecl` to carry `type_params`.
- [x] **W1.2.4** Update HIR lowering to propagate `type_params`.
- [x] **W1.2.5** Update checker: register type params in data class scope.
- [x] **W1.2.6** Verify codegen works without changes (type erasure).
- [x] **W1.2.7** Test: `generic_data_class.fuse` — `data class Pair<A, B>`.
- [x] **W1.2.8** Test: `generic_data_class_methods.fuse`.
- [x] **W1.2.9** Full test suite green.

---

### Phase W1.3 — Generic Free Functions [DONE]

**Root cause:** `FunctionDecl` has no `type_params`. `parse_function`
does not check for `<` after function name.

**Fix approach:** Add `type_params: Vec<String>`. Parse after name.
Pass through HIR. Codegen needs no change (type erasure).

- [x] **W1.3.1** Add `type_params: Vec<String>` to `FunctionDecl`.
- [x] **W1.3.2** Parse `<...>` type params after name in `parse_function`.
- [x] **W1.3.3** Update HIR `FunctionDecl` and lowering.
- [x] **W1.3.4** Update checker: register type params in function scope.
- [x] **W1.3.5** Verify codegen works without changes (type erasure).
- [x] **W1.3.6** Test: `generic_function.fuse` — `fn identity<T>(x: T) -> T`.
- [x] **W1.3.7** Test: `generic_function_multiple.fuse` — `fn pair<A, B>`.
- [x] **W1.3.8** Full test suite green.

---

### Phase W1.4 — Module-Level Constants [DONE]

**Root cause:** Module-level `val` declarations are not accessible from
importing modules. `path.SEPARATOR` cannot be read after `import path`.

- [x] **W1.4.1** Audit how module-level `val` are stored in evaluator
      module environment.
- [x] **W1.4.2** Add `module.CONSTANT` access in evaluator.
- [x] **W1.4.3** In codegen: compile module-level `val` as global symbols.
- [x] **W1.4.4** Test: `module_constant_access.fuse`.
- [x] **W1.4.5** Test: `module_constant_string.fuse`.
- [x] **W1.4.6** Full test suite green.

---

## Wave 2 — Numeric Type System

> **MANDATORY:** Before starting this wave, re-read the **Language
> Philosophy** and **Mandatory Rules** sections. Then read:
>
> - `docs/fuse-language-guide-2.md` — sections on numeric types
> - `stage1/fuse-runtime/src/value.rs` — `FuseValue` variants
> - `stage1/fusec/src/codegen/object_backend.rs` — `compile_literal`,
>   `compile_binary`, type inference functions
> - `stage1/fusec/src/checker/types.rs` — type checking rules
> - `stdlib/core/float.fuse` — existing Float (f64) module
>
> **Types added:** `Float32`, `Int8`, `UInt8`, `Int32`, `UInt32`, `UInt64`.
> No implicit coercion — all conversions are explicit.

---

### Phase W2.1 — Float32 Runtime Value [DONE]

- [x] **W2.1.1** Add `Float32(f32)` variant to `FuseValue` enum.
- [x] **W2.1.2** Add `fuse_rt_f32_new(val: f64) -> *mut FuseValue`.
- [x] **W2.1.3** Add `fuse_rt_f32_value(handle) -> f64`.
- [x] **W2.1.4** Add arithmetic: `fuse_rt_f32_add/sub/mul/div`.
- [x] **W2.1.5** Add comparison: `fuse_rt_f32_eq/lt/gt/le/ge`.
- [x] **W2.1.6** Add `fuse_rt_f32_to_string`.

---

### Phase W2.2 — Float32 Lexer & Parser [DONE]

- [x] **W2.2.1** Verify `Float32` accepted as type name (plain identifier).
- [x] **W2.2.2** Verify in parameter types, return types, annotations, generics.
- [x] **W2.2.3** Float32 via explicit conversion only (`Float32.from(3.14)`).

---

### Phase W2.3 — Float32 Checker [DONE]

- [x] **W2.3.1** Register `Float32` in checker's built-in type set.
- [x] **W2.3.2** `Float32` distinct from `Float`. No implicit coercion.
- [x] **W2.3.3** Add `Float32` to valid arithmetic/comparison types.
- [x] **W2.3.4** Verify `SIMD<Float32, N>` still works.
- [x] **W2.3.5** Test: type error assigning `Float` to `Float32`.

---

### Phase W2.4 — Float32 Codegen

- [ ] **W2.4.1** Register `fuse_rt_f32_*` in codegen runtime table.
- [ ] **W2.4.2** Handle `Float32` in `compile_binary`.
- [ ] **W2.4.3** Handle `Float32` in type inference.
- [ ] **W2.4.4** Test: `float32_basic.fuse`.
- [ ] **W2.4.5** Test: `float32_comparison.fuse`.
- [ ] **W2.4.6** Full test suite green.

---

### Phase W2.5 — Float32 Stdlib

- [ ] **W2.5.1** Create `stdlib/core/float32.fuse` with conversions.
- [ ] **W2.5.2** Add `Float32.toString`.
- [ ] **W2.5.3** Add `Float32.abs`, `Float32.sqrt`.
- [ ] **W2.5.4** Test: `float32_test.fuse`.
- [ ] **W2.5.5** Full test suite green.

---

### Phase W2.6 — Sized Integer Runtime Values

**Design:** All sized integers passed as `i64` across Cranelift ABI.
Runtime narrows/widens on store/load. Arithmetic wraps on overflow.

- [ ] **W2.6.1** Add variants: `Int8(i8)`, `UInt8(u8)`, `Int32(i32)`,
      `UInt32(u32)`, `UInt64(u64)`.
- [ ] **W2.6.2** Add allocation FFI for each type.
- [ ] **W2.6.3** Add extraction FFI for each type.
- [ ] **W2.6.4** Add arithmetic FFI (5 ops × 5 types = 25 functions).
- [ ] **W2.6.5** Add comparison FFI (5 ops × 5 types = 25 functions).
- [ ] **W2.6.6** Add `to_string` for each type.
- [ ] **W2.6.7** Document overflow behavior (wrapping semantics).

---

### Phase W2.7 — Sized Integer Checker

- [ ] **W2.7.1** Register all five types in checker's built-in type set.
- [ ] **W2.7.2** Each type distinct from `Int` and each other. No coercion.
- [ ] **W2.7.3** Add all five to valid arithmetic/comparison types.
- [ ] **W2.7.4** Verify `SIMD<Int32, N>` and `SIMD<Int64, N>` work.
- [ ] **W2.7.5** Test: type errors for cross-type assignment.

---

### Phase W2.8 — Sized Integer Codegen

- [ ] **W2.8.1** Register all runtime functions in codegen table.
- [ ] **W2.8.2** Handle sized types in `compile_binary`.
- [ ] **W2.8.3** Handle in type inference.
- [ ] **W2.8.4** Test: `int32_basic.fuse`.
- [ ] **W2.8.5** Test: `uint8_byte_ops.fuse`.
- [ ] **W2.8.6** Test: `uint64_large.fuse`.
- [ ] **W2.8.7** Test: `sized_int_no_coercion.fuse` (type error).
- [ ] **W2.8.8** Full test suite green.

---

### Phase W2.9 — Sized Integer Stdlib

- [ ] **W2.9.1** Create `stdlib/core/int8.fuse` (from, toInt, toString, MIN, MAX).
- [ ] **W2.9.2** Create `stdlib/core/uint8.fuse`.
- [ ] **W2.9.3** Create `stdlib/core/int32.fuse`.
- [ ] **W2.9.4** Create `stdlib/core/uint32.fuse`.
- [ ] **W2.9.5** Create `stdlib/core/uint64.fuse`.
- [ ] **W2.9.6** Add safe widening conversions between types.
- [ ] **W2.9.7** Test files for all five types.
- [ ] **W2.9.8** Full test suite green.

---

## Wave 3 — Stdlib Polish

> **MANDATORY:** Before starting this wave, read:
>
> - `docs/fuse-stdlib-spec.md`
> - Every stdlib module being modified
> - Corresponding test files in `tests/fuse/stdlib/`
>
> **Filter rule:** "Would a Fuse developer writing this today, with the
> improved compiler, write it differently?" If yes, update.
>
> **Verify all W0 and W1 tests are green before starting this wave.**

---

### Phase W3.1 — Static Method Promotion (*depends on W0.5*)

- [ ] **W3.1.1** `int.fuse`: `pub fn parse(s)` → `pub fn Int.parse(s)`.
- [ ] **W3.1.2** `float.fuse`: `pub fn parse(s)` → `pub fn Float.parse(s)`;
      add `Float.PI()`, `Float.E()`.
- [ ] **W3.1.3** `string.fuse`: `fromBytes` → `String.fromBytes`;
      `fromChar` → `String.fromChar`.
- [ ] **W3.1.4** `set.fuse`: `new()` → `Set.new()`; `of(...)` → `Set.of(...)`.
- [ ] **W3.1.5** Review `map.fuse` and `list.fuse` for candidates.
- [ ] **W3.1.6** Update test files for new signatures.
- [ ] **W3.1.7** Update internal stdlib imports.
- [ ] **W3.1.8** Full test suite green.

---

### Phase W3.2 — Data Class → Struct Restoration (*depends on W1.1*)

- [ ] **W3.2.1** `regex.fuse`: `data class Regex` → `struct Regex`.
- [ ] **W3.2.2** `log.fuse`: `data class Logger` → `struct Logger`.
- [ ] **W3.2.3** Review `io.fuse` — convert `File` if applicable.
- [ ] **W3.2.4** Review `json_schema.fuse` — convert `Schema` if applicable.
- [ ] **W3.2.5** Review `http_server.fuse` — convert `Router`/`Server`.
- [ ] **W3.2.6** Update test files.
- [ ] **W3.2.7** Full test suite green, behavior unchanged.

---

### Phase W3.3 — Builder Method Chaining (*depends on W0.4*)

- [ ] **W3.3.1** `log.fuse`: update builders to return `mutref Self`.
- [ ] **W3.3.2** `process.fuse`: update `Command` builders.
- [ ] **W3.3.3** Remove Int/Unit return workarounds.
- [ ] **W3.3.4** Test chained calls.
- [ ] **W3.3.5** Full test suite green.

---

## Wave 4 — Annotation System

> **MANDATORY:** Before starting this wave, read:
>
> - `stage1/fusec/src/parser/parser.rs` — decorator parsing
> - `stage1/fusec/src/ast/nodes.rs` — `decorators: Vec<String>` fields
> - `stage1/fusec/src/checker/mod.rs` — `@rank` validation
> - `stage1/fusec/src/codegen/object_backend.rs` — `@entrypoint` check
>
> **Current state:** Three decorators (`@entrypoint`, `@value`, `@rank(N)`)
> stored as raw `Vec<String>`. `@typo` is silently accepted.

---

### Phase W4.1 — Annotation AST Upgrade

- [ ] **W4.1.1** Define `Annotation { name, args: Vec<AnnotationArg> }`.
- [ ] **W4.1.2** Replace `decorators: Vec<String>` with
      `annotations: Vec<Annotation>` on declarations.
- [ ] **W4.1.3** Parse `@name` and `@name(args)` into `Annotation` structs.
- [ ] **W4.1.4** Unify `@rank(N)` through general annotation path.
- [ ] **W4.1.5** Update all code reading `.decorators` to `.annotations`.
- [ ] **W4.1.6** Full test suite green.

---

### Phase W4.2 — Checker Annotation Validation

| Annotation | Position | Arguments | Purpose |
|---|---|---|---|
| `@entrypoint` | Function | none | Program entry point |
| `@value` | Data class / struct | none | Auto-generate lifecycle |
| `@rank(N)` | Variable declaration | Int | Lock ordering |
| `@test` | Function | none | Test function marker |
| `@ignore(reason)` | Function | String | Skip test |
| `@deprecated(msg)` | Function, type | String | Warning on use |
| `@export(name)` | Function | String | Control C-ABI symbol |
| `@inline` | Function | none | Inlining hint |
| `@unsafe` | Function, block | none | Bypass ownership for FFI |

- [ ] **W4.2.1** Define annotation registry (name → valid positions + args).
- [ ] **W4.2.2** Reject unknown annotations: `unknown annotation '@typo'`.
- [ ] **W4.2.3** Validate argument count and types.
- [ ] **W4.2.4** Validate annotation position.
- [ ] **W4.2.5** `@deprecated`: emit warning on use.
- [ ] **W4.2.6** Test: `annotation_valid.fuse`.
- [ ] **W4.2.7** Test: `annotation_unknown_error.fuse`.
- [ ] **W4.2.8** Test: `annotation_wrong_position_error.fuse`.
- [ ] **W4.2.9** Test: `annotation_deprecated_warning.fuse`.
- [ ] **W4.2.10** Full test suite green.

---

### Phase W4.3 — Codegen Annotation Support

- [ ] **W4.3.1** `@export("name")`: use as linker symbol.
- [ ] **W4.3.2** `@inline`: set Cranelift inlining hint.
- [ ] **W4.3.3** `@test`: mark for test runner discovery.
- [ ] **W4.3.4** Verify `@entrypoint` and `@value` through new path.
- [ ] **W4.3.5** Test: `export_custom_name.fuse`.
- [ ] **W4.3.6** Full test suite green.

---

## Wave 5 — Interface System

> **MANDATORY:** Before starting this wave, re-read the **Language
> Philosophy** and **Mandatory Rules** sections. Then read:
>
> - `docs/fuse-language-guide-2.md` — §1.3 (Fuse Core vs Full),
>   §1.13 (Extension Functions), §1.14 (Generics)
> - `stage1/fusec/src/ast/nodes.rs` — `Declaration` enum, `DataClassDecl`,
>   `EnumDecl`
> - `stage1/fusec/src/parser/parser.rs` — `parse_top_level`,
>   `parse_data_class`, `parse_enum`
> - `stage1/fusec/src/checker/mod.rs` — `ModuleInfo`, extension/static
>   function maps
> - `stage1/fusec/src/codegen/object_backend.rs` — `declare_user_surface`,
>   `emit_object`, extension method compilation
>
> **Prerequisite:** Wave 1 complete (structs, generics).
>
> **Design:** Fuse interfaces define behavioral contracts. A type
> satisfies an interface by declaring `implements InterfaceName` and
> providing all required methods as extension methods. Conformance is
> always explicit. Dispatch is static (no vtables). Marker interfaces
> (no methods) are allowed. Both `data class` and `enum` types can
> implement interfaces.

---

### Phase W5.1 — Lexer & Parser: Interface Declaration

> **Before starting:** Read `lexer/token.rs`, `parser/parser.rs`,
> `ast/nodes.rs`.

- [ ] **W5.1.1** Add `Interface` and `Implements` variants to `TokenKind`.
- [ ] **W5.1.2** Add `"interface"` and `"implements"` to `keyword_kind()`.
- [ ] **W5.1.3** Add `InterfaceDecl` struct to `ast/nodes.rs`:
      ```rust
      pub struct InterfaceDecl {
          pub name: String,
          pub type_params: Vec<String>,
          pub parents: Vec<String>,
          pub methods: Vec<InterfaceMethod>,
          pub is_pub: bool,
          pub span: Span,
      }
      pub struct InterfaceMethod {
          pub name: String,
          pub params: Vec<Param>,
          pub return_type: Option<String>,
          pub span: Span,
      }
      ```
- [ ] **W5.1.4** Add `Interface(InterfaceDecl)` to `Declaration` enum.
- [ ] **W5.1.5** Add `implements: Vec<String>` to `DataClassDecl`.
- [ ] **W5.1.6** Add `implements: Vec<String>` to `EnumDecl`.
- [ ] **W5.1.7** Implement `parse_interface()`:
      consume `interface`, parse name, optional `<T, U>`,
      optional `: Parent1, Parent2`, `{` block of method signatures `}`.
- [ ] **W5.1.8** Add `TokenKind::Interface` dispatch in `parse_top_level`.
- [ ] **W5.1.9** Extend `parse_data_class()` to parse `implements X, Y`
      after field list and before body.
- [ ] **W5.1.10** Extend `parse_enum()` to parse `implements X, Y` after
      name and before `{`.
- [ ] **W5.1.11** Test: `interface_parse_basic.fuse` — simple interface.
- [ ] **W5.1.12** Test: `interface_parse_parents.fuse` — `: Parent`.
- [ ] **W5.1.13** Test: `interface_parse_generic.fuse` — `Convertible<T>`.
- [ ] **W5.1.14** Test: `interface_parse_marker.fuse` — empty interface.

---

### Phase W5.2 — HIR Lowering: Interface Nodes

> **Before starting:** Read `hir/nodes.rs`, `hir/lower.rs`.

- [ ] **W5.2.1** Add `InterfaceDecl` to HIR nodes (mirror AST or re-export).
- [ ] **W5.2.2** Add `Interface(InterfaceDecl)` to HIR `Declaration` enum.
- [ ] **W5.2.3** Add `implements: Vec<String>` to HIR `DataClassDecl`.
- [ ] **W5.2.4** Add `implements: Vec<String>` to HIR `EnumDecl`.
- [ ] **W5.2.5** Lower `Declaration::Interface` in `lower.rs`.
- [ ] **W5.2.6** Propagate `implements` for data classes and enums.
- [ ] **W5.2.7** `cargo build` succeeds. Existing tests green.

---

### Phase W5.3 — Checker: Registration & Conformance

> **Before starting:** Read `checker/mod.rs`, `checker/types.rs`,
> `checker/ownership.rs`, and the interface design decisions above.

- [ ] **W5.3.1** Add `InterfaceInfo` struct to checker:
      ```rust
      pub struct InterfaceInfo {
          pub name: String,
          pub type_params: Vec<String>,
          pub parents: Vec<String>,
          pub methods: Vec<InterfaceMethod>,
          pub default_methods: Vec<hir::FunctionDecl>,
          pub span: Span,
      }
      ```
- [ ] **W5.3.2** Add `interfaces: HashMap<String, InterfaceInfo>` to
      `ModuleInfo`.
- [ ] **W5.3.3** Add `implements: HashMap<String, Vec<String>>` to
      `ModuleInfo` (type name → interface names).
- [ ] **W5.3.4** Register interfaces during declaration pass.
- [ ] **W5.3.5** Register conformance for data classes and enums.
- [ ] **W5.3.6** Add `resolve_interface(name)` lookup.
- [ ] **W5.3.7** Implement parent resolution: verify parents exist,
      collect inherited methods transitively.
- [ ] **W5.3.8** Implement conformance checking: for each type with
      `implements`, verify all required methods have matching extension
      methods. Match by name, param count, types, return type, ownership.
- [ ] **W5.3.9** Error for missing methods: `"Type 'X' declares
      'implements Y' but does not implement method 'Z'"`.
- [ ] **W5.3.10** Implement ownership convention checking:
      `ref self` in interface → `ref self` in impl (not wider).
- [ ] **W5.3.11** Error for ownership mismatch.
- [ ] **W5.3.12** Implement generic bound checking: `<T: Interface>` at
      call site verifies concrete type has `implements`.
- [ ] **W5.3.13** Allow marker interfaces (empty method list = always satisfied).
- [ ] **W5.3.14** Test: `interface_missing_method.fuse` — EXPECTED ERROR.
- [ ] **W5.3.15** Test: `interface_wrong_ownership.fuse` — EXPECTED ERROR.
- [ ] **W5.3.16** Test: `interface_bound_satisfied.fuse` — EXPECTED OUTPUT.
- [ ] **W5.3.17** Test: `interface_bound_violated.fuse` — EXPECTED ERROR.
- [ ] **W5.3.18** Test: `interface_composition.fuse` — inherited methods.
- [ ] **W5.3.19** Test: `interface_marker.fuse` — empty interface accepted.
- [ ] **W5.3.20** Test: `enum_implements.fuse` — enum with interface.

---

### Phase W5.4 — Checker: Default Methods

> **Before starting:** Read Phase W5.3 implementation.

- [ ] **W5.4.1** Detect default methods: extension method on interface name
      (e.g., `fn Printable.debugPrint(ref self)`) → register as default.
- [ ] **W5.4.2** Store in `InterfaceInfo.default_methods`.
- [ ] **W5.4.3** During conformance checking: if method missing but default
      exists, mark as satisfied.
- [ ] **W5.4.4** Override resolution: type's own extension method takes
      priority over default. No duplicate error.
- [ ] **W5.4.5** Test: `interface_default_method.fuse` — use default.
- [ ] **W5.4.6** Test: `interface_default_override.fuse` — override default.

---

### Phase W5.5 — Codegen: Interface Method Dispatch

> **Before starting:** Read `codegen/object_backend.rs`, Phase W5.3-W5.4.

- [ ] **W5.5.1** Interface declarations produce no machine code. Verify
      skipped in `declare_user_surface` and `emit_object` without error.
- [ ] **W5.5.2** Default method compilation: for types using a default
      (not overridden), generate a forwarding symbol
      `fuse_ext_{module}_{Type}__{method}` calling the default body.
- [ ] **W5.5.3** Verify extension dispatch: `x.toString()` resolves to
      `fuse_ext_{module}_{T}__toString`. Should work via existing mechanism.
- [ ] **W5.5.4** Verify generic bound dispatch: calls inside `<T: Printable>`
      resolve to concrete type's extension method (monomorphization).
- [ ] **W5.5.5** Test: `interface_codegen_basic.fuse` — end-to-end.
- [ ] **W5.5.6** Test: `interface_codegen_default.fuse` — default method.
- [ ] **W5.5.7** Test: `interface_codegen_generic.fuse` — generic bound.

---

### Phase W5.6 — Language Guide Update

> **Before starting:** All W5.1-W5.5 must be complete.

- [ ] **W5.6.1** Add section to language guide: "Interfaces" — declaration,
      conformance, extension method implementation, default methods.
- [ ] **W5.6.2** Add section: "Generic Bounds" — `T: Interface`, `T: A + B`.
- [ ] **W5.6.3** Add section: "Interface Composition" — `: Parent`, diamond.
- [ ] **W5.6.4** Update keyword table to include `interface`, `implements`.
- [ ] **W5.6.5** Update Fuse Full listing to include interfaces.
- [ ] **W5.6.6** Review all guide sections for consistency.

---

## Wave 6 — Evaluator Robustness

> **MANDATORY:** Before starting this wave, read:
>
> - `stage1/fusec/src/evaluator.rs` — `call_user_function`, FFI block
> - `stage1/fusec/src/main.rs` — 8 MB stack workaround

---

### Phase W6.1 — Extract FFI Dispatch (Bug #11 Proper Fix)

**Root cause:** `call_user_function` is ~400 lines with a giant `match`
block. Stack frame is several KB. Overflows after ~5 cross-module calls.

**Current workaround:** 8 MB stack size.

**Fix:** Extract FFI match into `#[inline(never)]` `dispatch_ffi()`.

- [ ] **W6.1.1** Extract FFI match block into `dispatch_ffi()`.
- [ ] **W6.1.2** Update `call_user_function` to call `dispatch_ffi`.
- [ ] **W6.1.3** Test cross-module nested calls at depth 20+.
- [ ] **W6.1.4** Remove 8 MB stack size workaround.
- [ ] **W6.1.5** Test: `deep_call_chain.fuse` — 500+ depth.
- [ ] **W6.1.6** Full test suite green.

---

### Phase W6.2 — Recursion Depth Limit

- [ ] **W6.2.1** Add `recursion_depth: usize` to evaluator state.
- [ ] **W6.2.2** Increment on entry, decrement on exit.
- [ ] **W6.2.3** Error at depth 1000: `"stack overflow: recursion depth exceeded 1000"`.
- [ ] **W6.2.4** Full test suite green.

---

## Wave 7 — LSP Foundation

> **MANDATORY:** Before starting this wave, read:
>
> - LSP specification
> - `stage1/fusec/src/lib.rs` — public API
> - `stage1/fusec/src/error.rs` — diagnostic types
>
> **Architecture:** New Rust binary crate `stage1/fuse-lsp/` reusing
> `fusec` as a library. Stdio JSON-RPC. Implemented in Rust (not Fuse)
> because Fuse cannot yet compile itself.

---

### Phase W7.1 — LSP Crate Setup & Initialize

- [ ] **W7.1.1** Refactor `fusec/src/lib.rs`: expose `lex`, `parse`, `check`.
- [ ] **W7.1.2** Create `stage1/fuse-lsp/Cargo.toml`.
- [ ] **W7.1.3** Create `fuse-lsp/src/main.rs` with JSON-RPC loop.
- [ ] **W7.1.4** Implement `initialize`/`initialized` handshake.
- [ ] **W7.1.5** Verify: starts, responds, shuts down cleanly.

---

### Phase W7.2 — Diagnostics

- [ ] **W7.2.1** Implement `didOpen`/`didChange`/`didClose`.
- [ ] **W7.2.2** On change: run lexer → parser → checker.
- [ ] **W7.2.3** Convert to `publishDiagnostics` notifications.
- [ ] **W7.2.4** Map `Span` to LSP `Range`.
- [ ] **W7.2.5** Map severity.
- [ ] **W7.2.6** Test: syntax error shows red squiggles.

---

### Phase W7.3 — Go to Definition & Hover

- [ ] **W7.3.1** Implement `textDocument/definition`.
- [ ] **W7.3.2** Implement `textDocument/hover`.
- [ ] **W7.3.3** Handle: locals, params, imports, extensions, fields.
- [ ] **W7.3.4** Test: hover shows type; click navigates.

---

### Phase W7.4 — Completion

- [ ] **W7.4.1** Implement `textDocument/completion`.
- [ ] **W7.4.2** Include: locals, functions, imports, keywords, stdlib.
- [ ] **W7.4.3** After `.`: suggest extension methods for inferred type.
- [ ] **W7.4.4** Test: `list.` shows extension methods.

---

## Wave 8 — WASM Target

> **MANDATORY:** Before starting this wave, read:
>
> - Cranelift wasm32 backend documentation
> - WASI specification
> - `stage1/fuse-runtime/` — all source files
> - `stage1/fusec/src/codegen/object_backend.rs` — ISA configuration
>
> **Usage:**
> ```
> fusec app.fuse -o app          --target native    # default
> fusec app.fuse -o app.wasm     --target wasi      # WASI runtime
> ```

---

### Phase W8.1 — Cranelift WASM32 Backend

- [ ] **W8.1.1** Add `--target` flag (values: `native`, `wasi`).
- [ ] **W8.1.2** Configure Cranelift ISA as `wasm32` for `wasi`.
- [ ] **W8.1.3** Adjust pointer type: `i64` → `i32` for wasm32.
- [ ] **W8.1.4** Generate `.wasm` output.
- [ ] **W8.1.5** Test: produces valid `.wasm` file.

---

### Phase W8.2 — Runtime Compilation to WASI

- [ ] **W8.2.1** Add `wasm32-wasi` target to `fuse-runtime`.
- [ ] **W8.2.2** Audit for OS-specific code, add `#[cfg]` guards.
- [ ] **W8.2.3** Produce `fuse_runtime.wasm`.
- [ ] **W8.2.4** Test: compiles cleanly to `wasm32-wasi`.

---

### Phase W8.3 — Module Availability Validation

- [ ] **W8.3.1** Define per-target module availability.
- [ ] **W8.3.2** Validate imports against target in checker.
- [ ] **W8.3.3** Error: `module 'process' not available on target 'wasi'`.
- [ ] **W8.3.4** Test: import `process` with `--target wasi` → error.

---

### Phase W8.4 — WASI Integration Test

- [ ] **W8.4.1** Install `wasmtime` as test runner.
- [ ] **W8.4.2** Compile and run `four_functions.fuse` via WASI.
- [ ] **W8.4.3** Compile and run a stdlib test via WASI.
- [ ] **W8.4.4** Add CI step for WASM validation.

---

## Verification Matrix

| Checkpoint | Command | Expectation |
|---|---|---|
| After each phase | `cargo test` in `stage1/` | All tests green |
| After W0 complete | Full `tests/fuse/` fixture run | All tests green |
| After W1 complete | Compile each stdlib module | No regressions |
| After W2 complete | Float32 + sized int tests | Correct arithmetic |
| After W3 complete | Every `tests/fuse/stdlib/` test | Pass, behavior unchanged |
| After W4 complete | `@typo` on any declaration | Compile error |
| After W5 complete | Interface conformance test | Missing method = error |
| After W6 complete | Recursive function depth 1000 | Clean error, not crash |
| After W7 complete | Open `.fuse` in VS Code | Diagnostics + completion |
| After W8 complete | `wasmtime run hello.wasm` | Correct output |

---

## Evaluator Architectural Note

> The tree-walking evaluator (`--run` mode) uses value semantics (clone
> on pass). This prevents in-place mutation of List/Map/Set values
> through FFI calls. The compiled (Cranelift) path uses handle/pointer
> semantics and works correctly.
>
> This is an architectural limitation of the evaluator, not a compiler
> bug. The compiled binary is always the source of truth.
>
> **When to address:** If the evaluator is still needed after Stage 2
> (unlikely — the self-hosting compiler will replace it), consider
> rewriting it then.

---

## Appendix A — Concurrency Model Reference

### Why No async/await

| Problem in Rust/C# | How Fuse avoids it |
|---|---|
| **Function coloring** — async and sync are different worlds | No coloring. Every function is the same. Concurrency is a call-site decision. |
| **Lifetime infection** — futures borrow arguments | No lifetimes. ASAP destruction + ownership conventions handle it. |
| **No built-in runtime** — Rust requires Tokio/async-std | Fuse ships one lightweight runtime. No ecosystem split. |
| **suspend/resume complexity** — stackless coroutines | No coroutines. Spawn creates a real task. No hidden state machines. |

### Spawn Rules

| Syntax | Meaning |
|---|---|
| `spawn { ... }` | Create a new concurrent task |
| `spawn move { ... }` | Task takes ownership of captured values |
| `spawn ref { ... }` | Task gets read-only access to captured values |
| `spawn { mutref x }` | **COMPILE ERROR** — no mutable borrows across spawn |

### Runtime Model

- **Stage 1:** OS threads. `spawn` creates an OS thread.
- **Post-Stage 2:** Green threads (optimization, transparent API swap).
- **Pool limit:** Unbounded. Developer's responsibility.

## Appendix B — Interface System Reference

### Language Comparison

| Feature | Fuse | Rust | Go | Kotlin | Swift |
|---|---|---|---|---|---|
| Declaration | `interface` | `trait` | `interface` | `interface` | `protocol` |
| Conformance | explicit `implements` | explicit `impl T for S` | implicit | explicit `: Interface` | explicit `: Protocol` |
| Implementation | extension methods | `impl` block | methods on struct | class body | extension / class |
| Default methods | extension on interface | default in trait | ✗ | default in interface | extension on protocol |
| Generic bounds | `T: Interface` | `T: Trait` | `[T Interface]` | `T : Interface` | `T: Protocol` |
| Dispatch | static (initially) | static + `dyn` | dynamic (vtable) | dynamic (vtable) | static + existential |

### Example

```fuse
interface Printable {
    fn toString(ref self) -> String
}

fn Printable.debugPrint(ref self) {  // default method
    println(f"[DEBUG] {self.toString()}")
}

data class Point(val x: Float, val y: Float) implements Printable

fn Point.toString(ref self) -> String {
    f"({self.x}, {self.y})"
}

fn printAll<T: Printable>(items: List<T>) {
    for item in items {
        println(item.toString())
    }
}
```

---

*Document created: 2026-04-06.*
*Companion document: `docs/fuse-post-stage2.md`.*
*Authoritative spec: `docs/fuse-language-guide-2.md`.*
