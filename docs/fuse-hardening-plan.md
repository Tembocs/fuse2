# Fuse Stage 1 — Compiler Hardening Plan

> **Status:** Not Started
> **Prerequisite:** Wave 6 stdlib (jsonrpc, uri, argparse) complete
> **Scope:** 8 waves, 30 phases, ~177 tasks
> **Gate:** This is the final pre-Stage-2 gate. Its purpose is to eliminate
> every known compiler limitation, close all workaround patterns in the
> stdlib, and build absolute trust in the Stage 1 compiler before
> self-hosting begins.
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
hardening pass must serve these three properties. If a change
undermines memory safety, concurrency safety, or developer experience,
it is wrong — regardless of how clever or expedient it may be.**

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

Every item in this plan is scoped to be completed within this
hardening pass. There are no items marked "TODO", "defer", "later",
or "workaround".

- If a task is in a phase, it must be completed in that phase.
- If a task cannot be completed due to a blocker, the blocker is
  fixed first — not deferred.
- Workarounds that were applied during stdlib implementation must be
  replaced with proper fixes. The stdlib must not retain workaround
  patterns after hardening.
- Work that is genuinely out of scope for this plan is listed in the
  **Not In Scope** section at the bottom of this document with an
  explicit schedule specifying **when** and **where** it will be
  implemented. "Not In Scope" does not mean "forgotten" — it means
  "scheduled elsewhere with a concrete timeline."

After completing each phase, scan all modified files for `TODO`,
`FIXME`, `HACK`, and `WORKAROUND`. If any are found, resolve them
before marking the phase done.

### Rule 3: Zero Regressions

Every fix must have at least one new regression test. Every existing
test must remain green after every phase. If a phase introduces a
test failure in any existing test, the phase is not complete.

- After each phase: `cargo test` in `stage1/` — all tests green.
- After each wave: full test suite run including `tests/fuse/` fixtures.
- After Wave H3 (stdlib polish): every `tests/fuse/stdlib/` test must
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

| Wave | Name | Phases | Tasks | Depends On |
|------|------|--------|-------|------------|
| H0 | Critical Bug Fixes | 5 | 40 | — |
| H1 | Language Feature Completion | 4 | 30 | H0 |
| H2 | Numeric Type System | 9 | 52 | H0.2, H0.5 |
| H3 | Stdlib Polish | 3 | 19 | H0 + H1 |
| H4 | Annotation System | 3 | 18 | H0 + H1 |
| H5 | Evaluator Robustness | 2 | 9 | — |
| H6 | LSP Foundation | 4 | 20 | H0 + H1 |
| H7 | WASM Target | 4 | 16 | H0 + H1 + H2 |
| **Total** | | **34** | **204** | |

---

## Wave H0 — Critical Bug Fixes (STRICT)

> **MANDATORY:** Before starting this wave, re-read the **Language
> Philosophy** and **Mandatory Rules** sections at the top of this
> document. Then read:
>
> - `docs/fuse-language-guide-2.md` — sections on expressions, if/else,
>   string interpolation, types, and function declarations
> - `docs/stdlib_implementation_learning.md` — complete catalog of bugs
>   found during stdlib implementation
> - `docs/fuse-stdlib-implementation-plan.md` — "Known Limitations"
>   section at the end of Wave 1
> - `stage1/fusec/src/codegen/object_backend.rs` — `compile_if`
>   (line ~3137), `compile_match` (line ~3189), `compile_literal`
>   (line ~1410), `emit_object` extension method loop (line ~444),
>   `compile_type_namespace_call` (line ~2193)
> - `stage1/fusec/src/lexer/lexer.rs` — `read_string` (line ~172)
> - `stage1/fusec/src/checker/mod.rs` — `extension_functions` map
>   (line ~93), `resolve_extension` (line ~934)
>
> **Do not present half-cooked work and give excuses. Every task must be
> complete, tested, and green before it is marked done. After this wave
> completes, stop and report before proceeding.**

---

### Phase H0.1 — if/else as Return Expression

**Root cause:** `compile_if` in `object_backend.rs` (line ~3137–3193)
always returns `TypedValue { ty: Some("Unit".to_string()) }` regardless
of branch content. Both then and else branches jump to the `done` block
with `runtime_nullary(unit)` instead of the actual last expression value.

**Reference:** `compile_match` (line ~3189) does this correctly — it
uses `self.infer_expr_type(expr)` on arm body expressions and jumps to
`done` with the actual arm value. The fix must follow the same pattern.

**How if/else works in the AST:** There is no `Statement::If` variant.
The parser produces `Statement::Expr(ExprStmt { expr: Expr::If(...) })`.
`Expr::If(IfExpr)` has `then_branch: Block`, `else_branch: Option<ElseBranch>`,
and `ElseBranch` is either `Block` or `IfExpr` (recursive for `else if`).

**Fix approach:**
In each branch, check if the last statement is `Statement::Expr(ExprStmt { expr, .. })`.
If so, compile the prefix statements, then compile the final expression separately
and jump to `done` with that expression's value. On the final `TypedValue` return,
use `self.infer_expr_type()` on the then-branch's last expression to determine
the type. This is identical to how `compile_match` handles `ArmBody::Expr`.

- [ ] **H0.1.1** Add helper function `split_block_final_expr(stmts) -> (prefix, Option<Expr>)`
      that extracts the last expression from a block's statements — same
      pattern as function body `final_expr` extraction (line ~647).
- [ ] **H0.1.2** Update then-branch in `compile_if`: if final expr exists,
      compile prefix statements, compile final expr, jump to `done` with
      the expr's value instead of `runtime_nullary(unit)`.
- [ ] **H0.1.3** Update `ElseBranch::Block` in `compile_if`: same treatment
      as then-branch — extract and compile final expr.
- [ ] **H0.1.4** Update `ElseBranch::IfExpr` recursive case: propagate
      the inner `compile_if` result value to `done` block (partially done,
      verify type propagation is correct).
- [ ] **H0.1.5** Update return `TypedValue` type: use `self.infer_expr_type()`
      on the then-branch's final expression. If no final expression or no
      else branch, type remains `"Unit"`.
- [ ] **H0.1.6** Preserve existing behavior when no else branch (type = Unit,
      value = unit). An if-without-else is a statement, not an expression.
- [ ] **H0.1.7** Add test: `if_else_return_value.fuse` —
      `val x = if true { 1 } else { 2 }; println(x)` → `1`
- [ ] **H0.1.8** Add test: `if_elif_else_value.fuse` — chained `else if`
      returning values: `val x = if false { 1 } else if true { 2 } else { 3 }; println(x)` → `2`
- [ ] **H0.1.9** Add test: `if_else_in_function.fuse` — function body
      ending with if/else as implicit return value.
- [ ] **H0.1.10** Run full test suite — all existing tests green. Verify
      stdlib tests that use `match` as if/else workaround still pass.

---

### Phase H0.2 — Float Literal Codegen

**Root cause:** `compile_literal` in `object_backend.rs` (line ~1410)
returns a hard error: `"float literals are not implemented in the real backend yet"`
for `LiteralValue::Float(_)`.

**Fix approach:**
Use `builder.ins().f64const(value)` to create a Cranelift f64 constant,
then box it via a runtime allocation function. If `fuse_rt_float_from_f64`
does not exist in `fuse-runtime/src/value.rs`, add it.

- [ ] **H0.2.1** Check `fuse-runtime/src/value.rs` for existing float
      allocation from raw f64. If not present, add
      `fuse_rt_float_from_f64(val: f64) -> *mut FuseValue` following the
      same pattern as `fuse_rt_int_new`.
- [ ] **H0.2.2** Register `fuse_rt_float_from_f64` in codegen's runtime
      function table (same as other runtime functions in the import block).
- [ ] **H0.2.3** Implement `LiteralValue::Float(f)` case in `compile_literal`:
      emit `f64const`, call `fuse_rt_float_from_f64`, return
      `TypedValue { value, ty: Some("Float".to_string()) }`.
- [ ] **H0.2.4** Add test: `float_literal_basic.fuse` —
      `val x = 3.14; println(x)` → `3.14`
- [ ] **H0.2.5** Add test: `float_literal_arithmetic.fuse` —
      `println(1.5 + 2.5)` → `4.0`
- [ ] **H0.2.6** Add test: `float_literal_negative.fuse` —
      `val x = -0.5; println(x)` → `-0.5`
- [ ] **H0.2.7** Verify existing `float_test.fuse` stdlib test still passes.
- [ ] **H0.2.8** Run full test suite — all tests green.

---

### Phase H0.3 — F-String Nested Quote Support

**Root cause:** `read_string` in `lexer.rs` (line ~172–206) treats any
`"` character as the string terminator, regardless of whether the scan
position is inside a `{...}` interpolation brace. There is no brace
depth counter.

**Fix approach:**
Add a `brace_depth: usize` counter. When `formatted` is true, increment
on `{`, decrement on `}`. Only treat `"` as a terminator when
`brace_depth == 0`.

- [ ] **H0.3.1** Add `brace_depth: usize = 0` variable before the scan
      loop in `read_string`.
- [ ] **H0.3.2** When `formatted == true` and char is `{`: increment
      `brace_depth`.
- [ ] **H0.3.3** When `formatted == true` and char is `}`: decrement
      `brace_depth` (guard against underflow).
- [ ] **H0.3.4** Only treat `"` as terminator when `brace_depth == 0`.
- [ ] **H0.3.5** Handle escaped braces `\{` and `\}` — these must not
      affect depth tracking.
- [ ] **H0.3.6** Add test: `fstring_nested_quotes.fuse` —
      `val s = f"{list.join(",")}"; println(s)` — quotes inside braces
      do not terminate the f-string.
- [ ] **H0.3.7** Add test: `fstring_method_in_braces.fuse` —
      `f"result: {map.get("key")}"` — method call with string arg inside braces.
- [ ] **H0.3.8** Verify all existing f-string tests still pass.
- [ ] **H0.3.9** Run full test suite.

---

### Phase H0.4 — Builder Method `mutref Self` Return Type

**Root cause:** Extension method compilation in `object_backend.rs`
(line ~444–452) resolves the `self` parameter type to the receiver type,
but never resolves the `return_type` field when it contains `"Self"`.
A function declared as `fn Type.method(mutref self) -> mutref Self`
compiles with return type literally `"mutref Self"` instead of
`"mutref Type"`.

**Fix approach:**
After the self-parameter resolution block, check if
`function.return_type` contains `"Self"`. If yes, replace `"Self"` with
the receiver type. Apply the same fix in both `emit_object()` (line ~444)
and `collect_ir_text()` (line ~483).

- [ ] **H0.4.1** In `emit_object()` extension function loop: after
      self-param resolution, add return type resolution — replace `"Self"`
      with `receiver_type` in `function.return_type`.
- [ ] **H0.4.2** Handle both `"Self"` and `"mutref Self"` patterns:
      `"Self"` → `"{receiver_type}"`,
      `"mutref Self"` → `"mutref {receiver_type}"`.
- [ ] **H0.4.3** Apply identical resolution in `collect_ir_text()`
      extension function loop.
- [ ] **H0.4.4** Add test: `builder_mutref_self.fuse` — data class with
      builder method returning `mutref Self`, verify method chaining:
      `builder.setA(1).setB(2)`.
- [ ] **H0.4.5** Add test: `builder_chain_three.fuse` — three chained
      builder calls returning `mutref Self`.
- [ ] **H0.4.6** Run full test suite.

---

### Phase H0.5 — `Type.staticMethod()` Call Syntax

**Root cause (checker):** `ModuleInfo` in `checker/mod.rs` (line ~38)
stores ALL functions with a `receiver_type` in the `extension_functions`
map (line ~93) — no distinction between instance methods (first param
is `self`) and static methods (no `self` param). There is no
`static_functions` map and no `resolve_static_function()` method.

**Root cause (codegen):** `compile_type_namespace_call` in
`object_backend.rs` (line ~2193) only handles hardcoded types (`Chan`,
`SIMD`, `Map`), then falls through to an error for any user-defined
`Type.method()` call.

**Fix approach:**
1. Add `static_functions: HashMap<(String, String), hir::FunctionDecl>`
   to `ModuleInfo`. During function registration, if a function has
   `receiver_type` AND its first parameter's name is NOT `"self"`, store
   in `static_functions` instead of `extension_functions`.
2. Add `resolve_static_function(type_name, method_name)` to the checker.
3. In codegen, add a `statics` field to `LoadedModule`. In
   `compile_type_namespace_call`, before the hardcoded fallback, check
   `loaded.statics` for matching functions. Compile the call WITHOUT
   prepending a receiver as the first argument (static methods have no
   implicit `self`).

- [ ] **H0.5.1** Add `static_functions: HashMap<(String, String), hir::FunctionDecl>`
      to `ModuleInfo` in checker.
- [ ] **H0.5.2** Update function registration in checker: if function has
      `receiver_type` and first param name is NOT `"self"`, store in
      `static_functions`. Otherwise store in `extension_functions`.
- [ ] **H0.5.3** Add `resolve_static_function(type_name: &str, method_name: &str)`
      lookup method to checker.
- [ ] **H0.5.4** Update `check_call` for `Expr::Member` to check
      `static_functions` when the member target is a type name (not a value).
- [ ] **H0.5.5** Add `statics: HashMap<(String, String), hir::FunctionDecl>`
      to `LoadedModule` in codegen.
- [ ] **H0.5.6** Populate `statics` during module loading in `emit_object`.
- [ ] **H0.5.7** Declare static function symbols in `declare_user_surface`.
- [ ] **H0.5.8** In `compile_type_namespace_call`: before the hardcoded
      fallback, look up `statics` and compile the call without a receiver
      argument.
- [ ] **H0.5.9** Add test: `static_method_basic.fuse` —
      `fn Int.parse(s: String) -> Int { ... }; val x = Int.parse("42")`
- [ ] **H0.5.10** Add test: `static_method_constructor.fuse` —
      `fn Set.new() -> Set<T> { ... }; val s = Set.new()`
- [ ] **H0.5.11** Run full test suite.

---

## Wave H1 — Language Feature Completion (STRICT)

> **MANDATORY:** Before starting this wave, re-read the **Language
> Philosophy** and **Mandatory Rules** sections at the top of this
> document. Then read:
>
> - `docs/fuse-language-guide-2.md` — sections on structs, data classes,
>   generics, and module system
> - `stage1/fusec/src/ast/nodes.rs` — `DataClassDecl` (line ~85),
>   `FunctionDecl` (line ~71), `StructDecl`, `Statement` enum (line ~120)
> - `stage1/fusec/src/parser/parser.rs` — `parse_data_class` (line ~276),
>   `parse_function` (line ~155), `parse_struct`
> - `stage1/fusec/src/hir/nodes.rs` — HIR equivalents
> - `stage1/fusec/src/codegen/object_backend.rs` — `declare_user_surface`
>   (line ~371), `emit_object` (line ~437), data class compilation loops
>
> **Verify all H0 tests are green before starting this wave.**

---

### Phase H1.1 — Struct Compilation

**Root cause:** `declare_user_surface()` in `object_backend.rs`
(line ~371–431) and `emit_object()` (line ~437–462) iterate over
functions, data class methods, and extern functions — but have NO
iteration over `loaded.structs`. Structs are parsed, lowered to HIR,
and registered in the checker, but completely skipped during code
generation.

**Fix approach:**
Structs are behaviorally identical to data classes but with private
fields by default. In codegen, compile struct methods the same way
data class methods are compiled. Generate `__init__` constructor and
`__del__` destructor. Do NOT generate public field accessors (structs
are opaque — that is their purpose).

- [ ] **H1.1.1** Add struct iteration in `declare_user_surface` — declare
      struct method symbols (parallel to data class methods, line ~385–411).
- [ ] **H1.1.2** Add struct constructor (`__init__`) declaration.
- [ ] **H1.1.3** Add struct destructor (`__del__`) declaration.
- [ ] **H1.1.4** Add struct method compilation in `emit_object` — compile
      methods (parallel to data class methods, line ~454–462).
- [ ] **H1.1.5** Compile struct constructor body (allocate, set fields).
- [ ] **H1.1.6** Compile struct destructor body (release fields).
- [ ] **H1.1.7** Verify struct fields are NOT accessible outside methods
      (no public field accessor generation).
- [ ] **H1.1.8** Add test: `struct_compiled.fuse` — struct with methods,
      construct and call methods, verify correct output.
- [ ] **H1.1.9** Add test: `struct_destructor.fuse` — verify ASAP
      destruction fires for struct instances.
- [ ] **H1.1.10** Verify existing `struct_basic.fuse` test still passes.
- [ ] **H1.1.11** Run full test suite.

---

### Phase H1.2 — Generic Data Class Syntax

**Root cause:** `parse_data_class` in `parser.rs` (line ~276–285)
expects `(` immediately after the data class name — no `<...>` type
parameter parsing exists. `DataClassDecl` in `ast/nodes.rs` (line ~85)
has no `type_params` field.

**Fix approach:**
Add `type_params: Vec<String>` to `DataClassDecl`. After parsing the
name, check for `TokenKind::Lt` and parse comma-separated type parameter
identifiers until `TokenKind::Gt`. Pass through HIR. The checker
registers type params as valid type names within the data class scope.
Codegen needs no change — generics are type-erased to FuseHandle at the
Cranelift level.

- [ ] **H1.2.1** Add `type_params: Vec<String>` to `DataClassDecl` in
      `ast/nodes.rs`.
- [ ] **H1.2.2** In `parse_data_class`, after parsing name: check for
      `TokenKind::Lt`, parse comma-separated identifiers until
      `TokenKind::Gt`, store as `type_params`.
- [ ] **H1.2.3** Update HIR `DataClassDecl` to carry `type_params`.
- [ ] **H1.2.4** Update HIR lowering to propagate `type_params`.
- [ ] **H1.2.5** Update checker: register type params as valid type names
      within the data class scope during type checking.
- [ ] **H1.2.6** Verify codegen works without changes (type erasure to
      handles means generic data classes compile identically to concrete ones).
- [ ] **H1.2.7** Add test: `generic_data_class.fuse` —
      `data class Pair<A, B>(val first: A, val second: B)` with construction
      and field access.
- [ ] **H1.2.8** Add test: `generic_data_class_methods.fuse` — generic
      data class with methods that use type parameters.
- [ ] **H1.2.9** Run full test suite.

---

### Phase H1.3 — Generic Free Functions

**Root cause:** `FunctionDecl` in `ast/nodes.rs` (line ~71) has no
`type_params` field. `parse_function` in `parser.rs` (line ~155) does
not check for `<` after the function name.

**Fix approach:**
Add `type_params: Vec<String>` to `FunctionDecl`. In `parse_function`,
after parsing the name, check for `TokenKind::Lt` and parse type params.
Pass through HIR. Checker registers type params in function scope.
Codegen needs no change (type erasure).

- [ ] **H1.3.1** Add `type_params: Vec<String>` to `FunctionDecl` in
      `ast/nodes.rs`.
- [ ] **H1.3.2** In `parse_function`, after parsing name: check for
      `TokenKind::Lt`, parse type params, store in declaration.
- [ ] **H1.3.3** Update HIR `FunctionDecl` and lowering.
- [ ] **H1.3.4** Update checker: register type params as valid type names
      within the function scope.
- [ ] **H1.3.5** Verify codegen works without changes (type erasure).
- [ ] **H1.3.6** Add test: `generic_function.fuse` —
      `fn identity<T>(x: T) -> T { x }; println(identity(42))`
- [ ] **H1.3.7** Add test: `generic_function_multiple.fuse` —
      `fn pair<A, B>(a: A, b: B) -> (A, B)`
- [ ] **H1.3.8** Run full test suite.

---

### Phase H1.4 — Module-Level Constants

**Root cause:** Module-level `val` declarations are not accessible
from importing modules. `path.SEPARATOR` and similar constants cannot
be read after `import path`.

**Fix approach:**
Identify how module-level `val` declarations are stored in the
evaluator's module environment and in the codegen's module loading.
Add support for `module.NAME` access that resolves to the constant's
value.

- [ ] **H1.4.1** Audit how module-level `val` declarations are stored
      in the evaluator's module environment after `load_module`.
- [ ] **H1.4.2** Add support for `module.CONSTANT` access pattern in
      evaluator — resolve as a field access on the module object.
- [ ] **H1.4.3** In codegen: compile module-level `val` declarations as
      global symbols that can be referenced from importing modules.
- [ ] **H1.4.4** Add test: `module_constant_access.fuse` — define a
      constant in module, import module, access `module.CONSTANT`.
- [ ] **H1.4.5** Add test: `module_constant_string.fuse` — string constant
      access (covers the `path.SEPARATOR` use case).
- [ ] **H1.4.6** Run full test suite.

---

## Wave H2 — Numeric Type System

> **MANDATORY:** Before starting this wave, re-read the **Language
> Philosophy** and **Mandatory Rules** sections at the top of this
> document. Then read:
>
> - `docs/fuse-language-guide-2.md` — sections on numeric types and
>   type conversions
> - `stage1/fuse-runtime/src/value.rs` — current `FuseValue` variants,
>   `fuse_rt_int_new`, `fuse_rt_float_*` functions
> - `stage1/fusec/src/codegen/object_backend.rs` — `compile_literal`,
>   `compile_binary`, type inference functions
> - `stage1/fusec/src/checker/types.rs` — type checking rules
> - `stdlib/core/float.fuse` — existing Float (f64) module for reference
> - `stage1/hardening.md` — SIMD phases H5.1–H5.3 where `Float32` is
>   already recognised as a valid SIMD type parameter
>
> **Fuse currently has only two numeric types: `Int` (i64) and `Float`
> (f64).** This is insufficient for a language with FFI as a core feature.
> Every C library uses `u8`, `i32`, `u32`. Binary protocols, byte buffers,
> WASM (i32 native), and SIMD all require exact-width types.
>
> This wave adds the minimum viable set of numeric types. Each type gets
> a runtime value variant, checker registration, codegen support, and a
> stdlib module. No implicit coercion between types — all conversions
> are explicit.
>
> **Types added in this wave:**
>
> | Type | Width | Why now |
> |---|---|---|
> | `Float32` | 32-bit float | C interop, SIMD backing, graphics |
> | `Int8` | 8-bit signed | Byte buffers, binary I/O |
> | `UInt8` | 8-bit unsigned | Byte buffers, strings, crypto |
> | `Int32` | 32-bit signed | C interop, WASM native, SIMD backing |
> | `UInt32` | 32-bit unsigned | C interop, network/file formats, hashes |
> | `UInt64` | 64-bit unsigned | Sizes, addresses, hashes |
>
> **Note:** `Float32`, `Int32`, and `Int64` are already used as SIMD
> type parameters in the checker (see hardening.md H5.1). This wave
> makes them first-class standalone types.

---

### Phase H2.1 — Float32 Runtime Value

**What:** Add `Float32` value representation to the Fuse runtime so
that 32-bit float values can be allocated, stored, and extracted.

- [ ] **H2.1.1** Add `Float32(f32)` variant to `FuseValue` enum in
      `fuse-runtime/src/value.rs` (alongside existing `Int(i64)` and
      `Float(f64)` variants).
- [ ] **H2.1.2** Add `fuse_rt_f32_new(val: f64) -> *mut FuseValue` — create
      a Float32 by narrowing from f64 (the Cranelift calling convention
      passes f64; the runtime narrows to f32).
- [ ] **H2.1.3** Add `fuse_rt_f32_value(handle: *mut FuseValue) -> f64` —
      extract Float32 as f64 for Cranelift operations.
- [ ] **H2.1.4** Add arithmetic FFI: `fuse_rt_f32_add`, `fuse_rt_f32_sub`,
      `fuse_rt_f32_mul`, `fuse_rt_f32_div` — all take two handles, return
      handle. Operations performed in f32 precision.
- [ ] **H2.1.5** Add comparison FFI: `fuse_rt_f32_eq`, `fuse_rt_f32_lt`,
      `fuse_rt_f32_gt`, `fuse_rt_f32_le`, `fuse_rt_f32_ge`.
- [ ] **H2.1.6** Add `fuse_rt_f32_to_string(handle) -> *mut FuseValue` —
      string representation of Float32 values.

---

### Phase H2.2 — Float32 Lexer & Parser

**What:** Ensure the lexer and parser handle `Float32` as a valid type
name and support conversion syntax.

- [ ] **H2.2.1** Verify `Float32` is already accepted as a type name by
      the parser (it should be — it is a plain identifier). If not, add
      it to the set of recognised type names.
- [ ] **H2.2.2** Verify `Float32` can appear in parameter types, return
      types, variable type annotations, and generic type arguments.
- [ ] **H2.2.3** Float32 literal syntax decision: a float literal produces
      `Float` (f64) by default. `Float32` values are created via explicit
      conversion: `Float32.from(3.14)` (requires H0.5 static methods) or
      `3.14.toFloat32()` (extension method). No new literal suffix needed.

---

### Phase H2.3 — Float32 Checker

**What:** Register `Float32` as a first-class type in the checker's
type system alongside `Int`, `Float`, `Bool`, `String`.

- [ ] **H2.3.1** Register `Float32` in the checker's built-in type set.
- [ ] **H2.3.2** Add type compatibility rules: `Float32` is distinct from
      `Float`. No implicit coercion. Explicit conversion required.
- [ ] **H2.3.3** Add `Float32` to the set of valid types for arithmetic
      and comparison operators.
- [ ] **H2.3.4** Verify SIMD`<Float32, N>` continues to work correctly
      with the new first-class Float32 type.
- [ ] **H2.3.5** Add type error test: assigning `Float` to `Float32`
      without conversion produces a type error.

---

### Phase H2.4 — Float32 Codegen

**What:** Teach the code generator to compile Float32 operations.

- [ ] **H2.4.1** Register Float32 runtime functions (`fuse_rt_f32_*`) in
      codegen's runtime function table.
- [ ] **H2.4.2** Handle `Float32` type in `compile_binary`: dispatch to
      Float32 arithmetic/comparison FFI functions.
- [ ] **H2.4.3** Handle `Float32` in type inference: expressions involving
      Float32 operands produce Float32 results.
- [ ] **H2.4.4** Add test: `float32_basic.fuse` — create Float32 values,
      perform arithmetic, print results.
- [ ] **H2.4.5** Add test: `float32_comparison.fuse` — compare Float32
      values with `<`, `>`, `==`, etc.
- [ ] **H2.4.6** Run full test suite.

---

### Phase H2.5 — Float32 Stdlib Integration

**What:** Create `stdlib/core/float32.fuse` with conversion methods and
standard operations.

- [ ] **H2.5.1** Create `stdlib/core/float32.fuse` with:
      `pub fn Float.toFloat32(ref self) -> Float32` — narrowing conversion,
      `pub fn Float32.toFloat(ref self) -> Float` — widening conversion,
      `pub fn Float32.from(val: Float) -> Float32` — static constructor.
- [ ] **H2.5.2** Add `pub fn Float32.toString(ref self) -> String`.
- [ ] **H2.5.3** Add `pub fn Float32.abs(ref self) -> Float32`,
      `pub fn Float32.sqrt(ref self) -> Float32` — basic math operations.
- [ ] **H2.5.4** Create `tests/fuse/stdlib/core/float32_test.fuse` with
      comprehensive tests.
- [ ] **H2.5.5** Run full test suite.

---

### Phase H2.6 — Sized Integer Runtime Values

**What:** Add `Int8`, `UInt8`, `Int32`, `UInt32`, `UInt64` value
representations to the Fuse runtime. All follow the same pattern as
existing `Int(i64)` and the new `Float32(f32)` — a `FuseValue` variant
plus FFI functions for allocation, extraction, arithmetic, comparison,
and string conversion.

**Design:** All sized integers are passed as `i64` across the Cranelift
Calling convention (same as `Int`). The runtime narrows/widens on
store/load. This keeps the codegen uniform — `FuseHandle` remains
pointer-sized everywhere.

- [ ] **H2.6.1** Add `FuseValue` variants: `Int8(i8)`, `UInt8(u8)`,
      `Int32(i32)`, `UInt32(u32)`, `UInt64(u64)`.
- [ ] **H2.6.2** Add allocation FFI for each type:
      `fuse_rt_i8_new(val: i64) -> FuseHandle`,
      `fuse_rt_u8_new(val: i64) -> FuseHandle`,
      `fuse_rt_i32_new(val: i64) -> FuseHandle`,
      `fuse_rt_u32_new(val: i64) -> FuseHandle`,
      `fuse_rt_u64_new(val: i64) -> FuseHandle`.
      Each narrows from i64 to the target width.
- [ ] **H2.6.3** Add extraction FFI for each type:
      `fuse_rt_i8_value`, `fuse_rt_u8_value`, `fuse_rt_i32_value`,
      `fuse_rt_u32_value`, `fuse_rt_u64_value` — all return `i64`.
- [ ] **H2.6.4** Add arithmetic FFI for each type (add, sub, mul, div,
      mod) — 5 operations × 5 types = 25 functions. Operations performed
      at the target width with wrapping semantics.
- [ ] **H2.6.5** Add comparison FFI for each type (eq, lt, gt, le, ge) —
      5 operations × 5 types = 25 functions.
- [ ] **H2.6.6** Add `fuse_rt_{type}_to_string` for each type.
- [ ] **H2.6.7** Add overflow behavior documentation: sized integer
      arithmetic wraps on overflow (consistent with Rust `wrapping_*`
      semantics). No silent truncation without explicit conversion.

---

### Phase H2.7 — Sized Integer Checker

**What:** Register all five sized integer types in the checker's type
system. Each is a distinct type with no implicit coercion.

- [ ] **H2.7.1** Register `Int8`, `UInt8`, `Int32`, `UInt32`, `UInt64`
      in the checker's built-in type set.
- [ ] **H2.7.2** Add type compatibility rules: each sized integer is
      distinct from `Int` and from each other. No implicit coercion.
      `val x: Int32 = 42` is a type error — must use `Int32.from(42)`.
- [ ] **H2.7.3** Add all five types to the set of valid types for
      arithmetic and comparison operators.
- [ ] **H2.7.4** Verify SIMD`<Int32, N>` and SIMD`<Int64, N>` continue
      to work correctly.
- [ ] **H2.7.5** Add type error tests: assigning `Int` to `Int32`, `UInt8`
      to `Int8`, `UInt32` to `Int32` — all produce type errors without
      explicit conversion.

---

### Phase H2.8 — Sized Integer Codegen

**What:** Teach the code generator to compile sized integer operations.
Same pattern as Float32 codegen (H2.4).

- [ ] **H2.8.1** Register all sized integer runtime functions in
      codegen's runtime function table.
- [ ] **H2.8.2** Handle sized integer types in `compile_binary`: dispatch
      to the correct type-specific arithmetic/comparison FFI functions.
- [ ] **H2.8.3** Handle sized integers in type inference: expressions
      involving `Int32` operands produce `Int32` results, etc.
- [ ] **H2.8.4** Add test: `int32_basic.fuse` — create Int32 values,
      arithmetic, print results.
- [ ] **H2.8.5** Add test: `uint8_byte_ops.fuse` — UInt8 arithmetic,
      boundary values (0, 255).
- [ ] **H2.8.6** Add test: `uint64_large.fuse` — UInt64 large value
      handling.
- [ ] **H2.8.7** Add test: `sized_int_no_coercion.fuse` — verify that
      mixing `Int` and `Int32` in an expression produces a type error.
- [ ] **H2.8.8** Run full test suite.

---

### Phase H2.9 — Sized Integer Stdlib

**What:** Create stdlib modules for sized integer types with conversion
methods and standard operations.

- [ ] **H2.9.1** Create `stdlib/core/int8.fuse` with:
      `pub fn Int8.from(val: Int) -> Int8`,
      `pub fn Int8.toInt(ref self) -> Int`,
      `pub fn Int8.toString(ref self) -> String`,
      `pub fn Int8.MIN() -> Int8`, `pub fn Int8.MAX() -> Int8`.
- [ ] **H2.9.2** Create `stdlib/core/uint8.fuse` with:
      `pub fn UInt8.from(val: Int) -> UInt8`,
      `pub fn UInt8.toInt(ref self) -> Int`,
      `pub fn UInt8.toString(ref self) -> String`,
      `pub fn UInt8.MIN() -> UInt8`, `pub fn UInt8.MAX() -> UInt8`.
- [ ] **H2.9.3** Create `stdlib/core/int32.fuse` with:
      `pub fn Int32.from(val: Int) -> Int32`,
      `pub fn Int32.toInt(ref self) -> Int`,
      `pub fn Int32.toString(ref self) -> String`,
      `pub fn Int32.MIN() -> Int32`, `pub fn Int32.MAX() -> Int32`.
- [ ] **H2.9.4** Create `stdlib/core/uint32.fuse` with:
      `pub fn UInt32.from(val: Int) -> UInt32`,
      `pub fn UInt32.toInt(ref self) -> Int`,
      `pub fn UInt32.toString(ref self) -> String`,
      `pub fn UInt32.MIN() -> UInt32`, `pub fn UInt32.MAX() -> UInt32`.
- [ ] **H2.9.5** Create `stdlib/core/uint64.fuse` with:
      `pub fn UInt64.from(val: Int) -> UInt64`,
      `pub fn UInt64.toInt(ref self) -> Int`,
      `pub fn UInt64.toString(ref self) -> String`,
      `pub fn UInt64.MIN() -> UInt64`, `pub fn UInt64.MAX() -> UInt64`.
- [ ] **H2.9.6** Add cross-type conversions:
      `Int8.toInt32()`, `UInt8.toInt32()`, `UInt8.toUInt32()`,
      `Int32.toUInt32()`, `UInt32.toUInt64()`, etc. — safe widenings
      only. Narrowing requires explicit `Type.from()` which may truncate.
- [ ] **H2.9.7** Create test files: `int8_test.fuse`, `uint8_test.fuse`,
      `int32_test.fuse`, `uint32_test.fuse`, `uint64_test.fuse` — each
      covering construction, arithmetic, comparisons, conversions, and
      boundary values.
- [ ] **H2.9.8** Run full test suite.

---

## Wave H3 — Stdlib Polish

> **MANDATORY:** Before starting this wave, re-read the **Language
> Philosophy** and **Mandatory Rules** sections at the top of this
> document. Then read:
>
> - `docs/fuse-stdlib-spec.md` — the specification for how each stdlib
>   module should look
> - Every stdlib module being modified in this wave
> - The corresponding test files in `tests/fuse/stdlib/`
> - `docs/fuse-stdlib-implementation-plan.md` — the "Known Limitations"
>   sections for context on why workarounds were applied
>
> **Purpose:** Now that the compiler supports static methods (H0.5),
> struct compilation (H1.1), and builder returns (H0.4), update stdlib
> modules that were written with workarounds to be idiomatic.
>
> **Filter rule:** "Would a Fuse developer writing this today, with the
> improved compiler, write it differently?" If yes, update. If the FFI
> boundary is the natural design regardless of compiler capabilities,
> leave it.
>
> **Verify all H0 and H1 tests are green before starting this wave.**

---

### Phase H3.1 — Static Method Promotion (*depends on H0.5*)

**What:** Convert module-level `pub fn` workarounds to proper
`Type.staticMethod()` syntax. These functions were written as free
functions because the checker previously rejected `Type.method()` calls
on user-defined static methods.

- [ ] **H3.1.1** `stdlib/core/int.fuse`: `pub fn parse(s)` →
      `pub fn Int.parse(s: String) -> Int`
- [ ] **H3.1.2** `stdlib/core/float.fuse`: `pub fn parse(s)` →
      `pub fn Float.parse(s: String) -> Float`; add `pub fn Float.PI() -> Float`,
      `pub fn Float.E() -> Float` as static accessors.
- [ ] **H3.1.3** `stdlib/core/string.fuse`: `pub fn fromBytes(b)` →
      `pub fn String.fromBytes(b: List<Int>) -> String`;
      `pub fn fromChar(c)` → `pub fn String.fromChar(c: Int) -> String`.
- [ ] **H3.1.4** `stdlib/core/set.fuse`: `pub fn new()` →
      `pub fn Set.new() -> Set<T>`; `pub fn of(...)` →
      `pub fn Set.of(...) -> Set<T>`.
- [ ] **H3.1.5** Review `stdlib/core/map.fuse` and `stdlib/core/list.fuse`
      for additional static method candidates. Promote any that apply.
- [ ] **H3.1.6** Update all `tests/fuse/stdlib/core/` test files that call
      the old free-function signatures.
- [ ] **H3.1.7** Update all stdlib modules that import these functions
      internally.
- [ ] **H3.1.8** Run full test suite — every stdlib test green.

---

### Phase H3.2 — Data Class → Struct Restoration (*depends on H1.1*)

**What:** Restore opaque struct types that were forced to data class
because struct compilation was not supported. These types expose their
internal fields (e.g., `Regex(val handle: Int)`) when they should be
opaque.

- [ ] **H3.2.1** `stdlib/ext/regex.fuse`:
      `data class Regex(val handle: Int)` → `struct Regex` with private
      `handle` field and the same public methods.
- [ ] **H3.2.2** `stdlib/ext/log.fuse`:
      `data class Logger(var level, var prefix, var filePath)` →
      `struct Logger` with private fields and the same public methods.
- [ ] **H3.2.3** Review `stdlib/full/io.fuse` — convert `File` type to
      struct if applicable.
- [ ] **H3.2.4** Review `stdlib/ext/json_schema.fuse` — convert `Schema`
      type to struct if applicable.
- [ ] **H3.2.5** Review `stdlib/ext/http_server.fuse` — convert `Router`
      and `Server` types to struct if applicable.
- [ ] **H3.2.6** Update corresponding test files.
- [ ] **H3.2.7** Run full test suite — every stdlib test green, behavior
      unchanged.

---

### Phase H3.3 — Builder Method Chaining (*depends on H0.4*)

**What:** Fix builder methods to return `mutref Self` for proper method
chaining. Currently these methods return `Int` or `Unit` as workarounds.

- [ ] **H3.3.1** `stdlib/ext/log.fuse`: update `Logger.withLevel()`,
      `.withPrefix()`, `.toFile()` to return `mutref Self`.
- [ ] **H3.3.2** `stdlib/full/process.fuse`: update `Command.arg()`,
      `.cwd()`, `.stdin()`, `.stdout()`, `.stderr()` to return `mutref Self`.
- [ ] **H3.3.3** Remove Int/Unit return workarounds.
- [ ] **H3.3.4** Update test files to exercise chained calls:
      `Logger.new().withLevel("debug").withPrefix("[app]")`.
- [ ] **H3.3.5** Run full test suite.

---

## Wave H4 — Annotation System

> **MANDATORY:** Before starting this wave, re-read the **Language
> Philosophy** and **Mandatory Rules** sections at the top of this
> document. Then read:
>
> - `docs/fuse-language-guide-2.md` — sections on annotations and
>   decorators (if present)
> - `stage1/fusec/src/parser/parser.rs` — current decorator parsing
>   (line ~71–89 for top-level, line ~512–519 for `@rank`)
> - `stage1/fusec/src/ast/nodes.rs` — `decorators: Vec<String>` fields
>   on `FunctionDecl` and `DataClassDecl`
> - `stage1/fusec/src/checker/mod.rs` — `@rank` validation (line ~314–326),
>   decorator checks
> - `stage1/fusec/src/codegen/object_backend.rs` — `@entrypoint` check
>   (line ~95–101)
>
> **Current state:** Three decorators exist (`@entrypoint`, `@value`,
> `@rank(N)`). They are stored as raw `Vec<String>` with no validation —
> `@typo` is silently accepted. `@rank` has hardcoded argument parsing.

---

### Phase H4.1 — Annotation AST Upgrade

**What:** Replace the raw `Vec<String>` decorator storage with a proper
`Annotation` type that supports names with arguments.

- [ ] **H4.1.1** Define `Annotation { name: String, args: Vec<AnnotationArg> }`
      where `AnnotationArg` is `Int(i64) | Float(f64) | Str(String) | Bool(bool)`
      in `ast/nodes.rs`.
- [ ] **H4.1.2** Replace `decorators: Vec<String>` with
      `annotations: Vec<Annotation>` on `FunctionDecl`, `DataClassDecl`,
      and `StructDecl`.
- [ ] **H4.1.3** Update parser `parse_top_level` to parse `@name` and
      `@name(arg1, arg2)` into `Annotation` structs. Arguments are
      literal values (int, float, string, bool).
- [ ] **H4.1.4** Unify `@rank(N)` parsing: remove the hardcoded
      `parse_rank_decorator` and handle `@rank` through the general
      annotation parsing path. Store rank value in `VarDecl.annotations`
      instead of a separate `rank` field (or keep the separate field for
      performance — decide during implementation).
- [ ] **H4.1.5** Update all code that reads `.decorators` to use
      `.annotations`.
- [ ] **H4.1.6** Run full test suite — no regressions.

---

### Phase H4.2 — Checker Annotation Validation

**What:** Reject unknown annotations at compile time. Each annotation
has a defined set of valid positions and argument types.

Compiler-defined annotations (complete set for this hardening pass):

| Annotation | Position | Arguments | Purpose |
|---|---|---|---|
| `@entrypoint` | Function | none | Program entry point |
| `@value` | Data class / struct | none | Auto-generate lifecycle methods |
| `@rank(N)` | Variable declaration | Int | Lock ordering for `Shared<T>` |
| `@test` | Function | none | Mark as test function |
| `@ignore(reason)` | Function | String | Skip test with reason |
| `@deprecated(msg)` | Function, type, field | String | Emit warning on use |
| `@export(name)` | Function | String | Control C-ABI symbol name |
| `@inline` | Function | none | Hint to inline at call sites |
| `@unsafe` | Function, block | none | Bypass ownership checks for raw FFI |

- [ ] **H4.2.1** Define the annotation registry: a table mapping annotation
      name → valid positions + expected argument types.
- [ ] **H4.2.2** In checker, after parsing: validate every annotation's
      name against the registry. Unknown annotations produce a compile
      error: `unknown annotation '@typo'`.
- [ ] **H4.2.3** Validate annotation arguments: `@rank` requires one Int,
      `@deprecated` requires one String, `@export` requires one String, etc.
      Wrong argument count or type produces error.
- [ ] **H4.2.4** Validate annotation position: `@rank` on a function
      produces error; `@entrypoint` on a variable produces error; etc.
- [ ] **H4.2.5** `@deprecated`: when a deprecated function/type is used,
      emit a compiler warning (not error) with the deprecation message.
- [ ] **H4.2.6** Add test: `annotation_valid.fuse` — all valid annotations
      in their correct positions are accepted.
- [ ] **H4.2.7** Add test: `annotation_unknown_error.fuse` — `@typo`
      produces compile error.
- [ ] **H4.2.8** Add test: `annotation_wrong_position_error.fuse` —
      `@rank(1)` on a function produces error.
- [ ] **H4.2.9** Add test: `annotation_deprecated_warning.fuse` —
      using a `@deprecated("use X")` function produces warning.
- [ ] **H4.2.10** Run full test suite.

---

### Phase H4.3 — Codegen Annotation Support

**What:** Wire annotations that affect code generation.

- [ ] **H4.3.1** `@export("name")`: when present, use the provided name
      as the linker symbol instead of the mangled name.
- [ ] **H4.3.2** `@inline`: set Cranelift function attribute for inlining
      hint on the generated function.
- [ ] **H4.3.3** `@test`: mark function for test runner discovery — emit
      a symbol or metadata that the test harness can enumerate.
- [ ] **H4.3.4** Verify `@entrypoint` and `@value` continue to work
      correctly through the new annotation path.
- [ ] **H4.3.5** Add test: `export_custom_name.fuse` — `@export("my_fn")`
      produces symbol named `my_fn` in the object file.
- [ ] **H4.3.6** Run full test suite.

---

## Wave H5 — Evaluator Robustness

> **MANDATORY:** Before starting this wave, re-read the **Language
> Philosophy** and **Mandatory Rules** sections at the top of this
> document. Then read:
>
> - `stage1/fusec/src/evaluator.rs` — `call_user_function` (line ~720),
>   the FFI match block spanning lines ~711–1104
> - `docs/stdlib_implementation_learning.md` — Bug #11 detailed analysis
> - `stage1/fusec/src/main.rs` — 8 MB stack workaround
>
> **Context:** The evaluator is the `--run` mode (tree-walking
> interpreter). It is not the production compilation path, but it is
> the primary development tool for rapid testing. A stack overflow crash
> during development is unacceptable.

---

### Phase H5.1 — Bug #11 Proper Fix: Extract FFI Dispatch

**Root cause:** `call_user_function` in `evaluator.rs` (line ~720) is
a ~400-line function containing a giant `match` block with every FFI
handler. Rust allocates stack space for the entire function's locals on
entry — estimated several KB per frame. Cross-module nested calls
overflow the stack after only ~5 levels.

**Current workaround:** Main thread stack size increased to 8 MB via
`std::thread::Builder::new().stack_size(8 * 1024 * 1024)`. Module
environment caching added to reduce redundant reconstruction.

**Proper fix:** Extract the FFI `match` block into a separate
`#[inline(never)]` function. This reduces `call_user_function`'s stack
frame to only the user-function path. The FFI dispatch function
allocates its own independent frame only when (and if) it is called.

- [ ] **H5.1.1** Extract the FFI match block (lines ~711–1104) into a
      separate function: `fn dispatch_ffi(&mut self, name: &str, args: Vec<Value>) -> Result<Value, ...>`.
      Mark it `#[inline(never)]`.
- [ ] **H5.1.2** Update `call_user_function` to call `dispatch_ffi` for
      FFI-routed calls.
- [ ] **H5.1.3** Verify stack frame reduction: test with cross-module
      nested calls at depth 20+.
- [ ] **H5.1.4** Remove the 8 MB stack size workaround from `main.rs`.
- [ ] **H5.1.5** Add test: `deep_call_chain.fuse` — function chain at 500+
      call depth completes without stack overflow.
- [ ] **H5.1.6** Run full test suite.

---

### Phase H5.2 — Evaluator Recursion Depth Limit

**What:** Add a configurable recursion depth limit to the evaluator.
Currently, unbounded recursion crashes the process with a stack overflow.
Replace the crash with a clear runtime error.

- [ ] **H5.2.1** Add `recursion_depth: usize` counter to evaluator state.
- [ ] **H5.2.2** Increment on function entry, decrement on function exit
      (including early returns and error paths).
- [ ] **H5.2.3** If depth exceeds 1000, produce a clear runtime error:
      `"stack overflow: recursion depth exceeded 1000"`.
- [ ] **H5.2.4** Run full test suite.

---

## Wave H6 — LSP Foundation

> **MANDATORY:** Before starting this wave, re-read the **Language
> Philosophy** and **Mandatory Rules** sections at the top of this
> document. Then read:
>
> - LSP specification: https://microsoft.github.io/language-server-protocol/
> - `stage1/fusec/src/lib.rs` — current public API surface of the
>   compiler library
> - `stage1/fusec/src/lexer/lexer.rs` — lexer entry points
> - `stage1/fusec/src/parser/parser.rs` — parser entry points
> - `stage1/fusec/src/checker/mod.rs` — checker entry points
> - `stage1/fusec/src/error.rs` — diagnostic types
> - `stdlib/ext/jsonrpc.fuse` — Wave 6 JSON-RPC module (for reference)
>
> **Architecture:** A new Rust binary crate `stage1/fuse-lsp/` that
> reuses `fusec` as a library. Communicates via stdio JSON-RPC.
> The LSP is implemented in Rust (not Fuse) because Fuse cannot yet
> compile itself. After Stage 2, the LSP can be rewritten in Fuse.

---

### Phase H6.1 — LSP Crate Setup & Initialize

**What:** Create the LSP binary crate, refactor `fusec` to expose its
APIs as a library, and implement the LSP initialization handshake.

- [ ] **H6.1.1** Refactor `fusec/src/lib.rs`: expose `lex(source) -> Vec<Token>`,
      `parse(tokens) -> Result<Module, Vec<Diagnostic>>`,
      `check(module) -> Vec<Diagnostic>` as public API functions.
- [ ] **H6.1.2** Create `stage1/fuse-lsp/Cargo.toml` with dependency on
      `fusec` (as library).
- [ ] **H6.1.3** Create `fuse-lsp/src/main.rs` with stdin/stdout JSON-RPC
      read/write loop (Content-Length framing).
- [ ] **H6.1.4** Implement `initialize` / `initialized` LSP handshake.
      Declare capabilities: `textDocumentSync`, `diagnosticProvider`.
- [ ] **H6.1.5** Build and verify: LSP binary starts, responds to
      `initialize`, and shuts down cleanly on `shutdown`/`exit`.

---

### Phase H6.2 — Diagnostics

**What:** On every document change, run the compiler pipeline and
publish diagnostics (errors, warnings) to the editor.

- [ ] **H6.2.1** Implement `textDocument/didOpen`, `textDocument/didChange`,
      `textDocument/didClose` document synchronization.
- [ ] **H6.2.2** On each document change: run lexer → parser → checker
      on the document content.
- [ ] **H6.2.3** Convert `Diagnostic` errors to LSP
      `textDocument/publishDiagnostics` notifications.
- [ ] **H6.2.4** Map `Span` to LSP `Range` (line/character positions).
- [ ] **H6.2.5** Map severity: compiler error → LSP Error, compiler
      warning → LSP Warning, compiler info → LSP Information.
- [ ] **H6.2.6** Test: open a `.fuse` file with a syntax error in an editor,
      verify red squiggles appear at the correct location.

---

### Phase H6.3 — Go to Definition & Hover

**What:** Navigate to where a symbol is defined. Show type information
on hover.

- [ ] **H6.3.1** Implement `textDocument/definition` — resolve identifier
      at cursor position to its declaration location (file + line).
- [ ] **H6.3.2** Implement `textDocument/hover` — show type and signature
      information for the identifier at cursor.
- [ ] **H6.3.3** Handle: local variables, function parameters, imported
      names, extension methods, data class fields.
- [ ] **H6.3.4** Test: hover over a variable shows its type and declaration;
      Ctrl+click on a function name navigates to its definition.

---

### Phase H6.4 — Completion

**What:** Suggest identifiers in scope as the developer types.

- [ ] **H6.4.1** Implement `textDocument/completion` — return identifiers
      visible at the cursor position.
- [ ] **H6.4.2** Include in completion results: local variables, function
      names, imported names, keywords, stdlib module names.
- [ ] **H6.4.3** After `.` token: suggest extension methods for the
      expression's inferred type.
- [ ] **H6.4.4** Test: type `list.` and verify extension method suggestions
      appear in the completion list.

---

## Wave H7 — WASM Target

> **MANDATORY:** Before starting this wave, re-read the **Language
> Philosophy** and **Mandatory Rules** sections at the top of this
> document. Then read:
>
> - Cranelift wasm32 backend documentation
> - WASI specification: https://wasi.dev
> - `stage1/fuse-runtime/` — all runtime source files
> - `stage1/fusec/src/main.rs` — CLI argument parsing
> - `stage1/fusec/src/codegen/object_backend.rs` — ISA configuration,
>   pointer type setup
> - `stage1/cranelift-ffi/` — Cranelift integration layer
>
> **Architecture:** The compiler handles target-specific runtime linking
> via `--target`. Fuse source code is identical across targets. The
> compiler packs the appropriate runtime backend into the output binary.
>
> **Usage:**
> ```
> fusec app.fuse -o app          --target native    # default
> fusec app.fuse -o app.wasm     --target wasi      # WASI runtime
> ```

---

### Phase H7.1 — Cranelift WASM32 Backend

**What:** Add `--target wasi` flag and configure Cranelift to emit
wasm32 code.

- [ ] **H7.1.1** Add `--target` flag to `fusec` CLI argument parsing.
      Valid values: `native` (default), `wasi`.
- [ ] **H7.1.2** When target is `wasi`: configure Cranelift ISA as
      `wasm32`.
- [ ] **H7.1.3** Adjust pointer type from `i64` (native) to `i32`
      (wasm32) when targeting WASI.
- [ ] **H7.1.4** Generate `.wasm` output file instead of native executable.
- [ ] **H7.1.5** Test: `fusec hello.fuse -o hello.wasm --target wasi`
      produces a valid `.wasm` file.

---

### Phase H7.2 — Runtime Compilation to WASI

**What:** Compile the `fuse-runtime` crate to `wasm32-wasi` so the
runtime functions are available in the WASM binary.

- [ ] **H7.2.1** Add `wasm32-wasi` target support to `fuse-runtime`
      crate's build configuration.
- [ ] **H7.2.2** Audit all runtime functions for OS-specific code. Add
      `#[cfg(target_arch = "wasm32")]` conditional compilation for
      platform-specific paths (threads, file system, networking).
- [ ] **H7.2.3** Produce `fuse_runtime.wasm` that exports all FFI
      function symbols.
- [ ] **H7.2.4** Test: `fuse-runtime` compiles cleanly to `wasm32-wasi`
      with no errors.

---

### Phase H7.3 — Module Availability Validation

**What:** Validate at compile time that imported stdlib modules are
available on the selected target. Produce clear errors for unavailable
modules.

- [ ] **H7.3.1** Define module availability per target:
      - `native`: all modules available
      - `wasi`: all except `process` (limited), `net` (depends on
        wasi-sockets maturity)
- [ ] **H7.3.2** In checker: when target is not `native`, validate each
      import against the target's available module set.
- [ ] **H7.3.3** Produce clear error message:
      `error: module 'process' is not available on target 'wasi'`
- [ ] **H7.3.4** Test: compile a file that imports `process` with
      `--target wasi` → clear error at the import statement.

---

### Phase H7.4 — WASI Integration Test

**What:** End-to-end validation: compile Fuse source to `.wasm`, run
with a WASI runtime, verify output.

- [ ] **H7.4.1** Install `wasmtime` as the WASI test runner.
- [ ] **H7.4.2** Compile `tests/fuse/milestone/four_functions.fuse` to
      `.wasm`, run with `wasmtime`, verify output matches expected.
- [ ] **H7.4.3** Compile and run a stdlib test (e.g.,
      `tests/fuse/stdlib/core/string_test.fuse`) via WASI and verify output.
- [ ] **H7.4.4** Add CI step for WASM validation.

---

## Verification Matrix

| Checkpoint | Command | Expectation |
|---|---|---|
| After each phase | `cargo test` in `stage1/` | All tests green |
| After H0 complete | Full `tests/fuse/` fixture run | All 89+ tests green |
| After H0 | Manual review | All stdlib workaround comments addressable |
| After H1 complete | Compile each stdlib module with `fusec` | No regressions |
| After H2 complete | Float32 tests pass | f32 arithmetic correct |
| After H3 complete | Every `tests/fuse/stdlib/` test | All pass, behavior unchanged |
| After H4 complete | `@typo` on any declaration | Compile error produced |
| After H5 complete | Recursive function at depth 1000 | Clean error, not crash |
| After H6 complete | Open `.fuse` file in VS Code with LSP | Diagnostics + completion work |
| After H7 complete | `wasmtime run hello.wasm` | Correct output |

---

## Not In Scope — Scheduled Elsewhere

> Items listed here are **not forgotten**. Each has an explicit schedule
> for when and where it will be implemented. This section exists to
> prevent scope creep in the hardening plan while ensuring nothing falls
> through the cracks.

### Post-Hardening, Pre-Stage 2

> **When:** After this hardening plan completes, before Stage 2
> self-hosting begins. These are general-purpose language features that
> benefit Stage 2 but are not critical path for compiler correctness.

| Item | Description | Why not in hardening |
|------|-------------|---------------------|
| **Operator overloading** | `vec1 + vec2`, `matrix * vector`, custom `+`/`-`/`*` dispatch on user types | Significant parser/checker/codegen work. Stage 2 can be written without it. Implement after hardening validates the compiler is solid. |
| **Fixed-size arrays `[T; N]`** | Contiguous stack-allocated arrays with compile-time size | FFI struct interop and performance. Not needed for Stage 2 bootstrap. |
| **Int16 / UInt16** | 16-bit signed/unsigned integers | Uncommon outside audio and legacy formats. Add when first FFI use case demands it. |

### During Stage 2

> **When:** During Stage 2 implementation phase. These are features
> that exist as applications of the Fuse language — not compiler fixes.

| Item | Description | Why during Stage 2 |
|------|-------------|-------------------|
| **MCP server** | Model Context Protocol server using JSON-RPC over stdio | Built as a Fuse application using the Stage 2 compiler. Validates the language for real-world protocol implementation. |

### Post-Stage 2

> **When:** After Stage 2 is working and the language is validated
> through self-hosting. These are domain-specific features that serve
> specialized use cases.

| Item | Description |
|------|-------------|
| **Browser WASM target** | Separate `fuse-runtime-browser` crate with JS stubs for `fetch`, `performance.now`, `crypto.getRandomValues` |
| **Float16 / BFloat16** | Half-precision floats for AI inference. Requires Float32 first (done in H2), then extend. |
| **Arena allocation** | Batch allocation/deallocation for GB-scale models. Language-level allocator API. |
| **User-defined annotations** | Derive macros, custom annotations, annotation composition. Requires a macro system. |
| **Runtime reflection** | Querying annotations at runtime. Requires metadata embedding in binaries. |
| **Linear algebra stdlib** | `Vec2`, `Vec3`, `Vec4`, `Mat4`, `Quaternion` in `stdlib/ext/linalg.fuse`. Requires operator overloading + f32. |
| **GPU access** | FFI to wgpu for Vulkan/Metal/DX12/WebGPU draw calls and compute shaders. |
| **Tensor type** | N-dimensional arrays with shape, strides. Requires f32 + operator overloading. |
| **Automatic differentiation** | Computational graph recording + backward pass for training. Requires tensor type. |
| **Shader interop** | Compile Fuse subset to SPIR-V or pass WGSL/GLSL strings. Requires GPU access. |
| **CUDA/ROCm native** | Direct GPU compute access beyond wgpu. |

### Stdlib Backlog

> **When:** As Stage 2 matures, driven by demand. These are stdlib
> features that were identified during stdlib implementation but
> deferred because they require compiler features or design decisions
> beyond what was available at the time.

| Item | Module | Why deferred | Unblocked by |
|------|--------|-------------|-------------|
| **File buffered methods** | `io.fuse` | `readLine`, `readChunk`, `readAll`, `write`, `writeBytes`, `flush`, `seek`, `pos`, `size` — requires struct method dispatch refinement | H1.1 (structs) |
| **HttpClient builder** | `http.fuse` | Convenience builder needs opaque mutable state | H0.4 (mutref Self) + H1.1 (structs) |
| **Response.json()** | `http.fuse` | Needs cross-module import of `json.fuse` | Module system maturity |
| **Router middleware** | `http_server.fuse` | Requires higher-order function composition with current closure ABI | Closure improvements |
| **Map.get / Map.getPath** | `json.fuse` | Need map access by key | Map runtime support |
| **JSON parseFile** | `json.fuse` | Trivial `io.readFile` + `parse` — low priority | Nothing (can be added anytime) |
| **List shuffle / sample** | `random.fuse` | Require list index mutation | List mutation codegen |

### Evaluator Architectural Note

> **Evaluator value semantics** — The tree-walking evaluator (`--run`
> mode) uses value semantics (clone on pass). This prevents in-place
> mutation of List/Map/Set values through FFI calls. The compiled
> (Cranelift) path uses handle/pointer semantics and works correctly.
>
> This is an architectural limitation of the evaluator, not a compiler
> bug. Fixing it would require rewriting the evaluator to use reference
> semantics — which is not justified given that the evaluator is a
> development convenience tool, not the production path. The compiled
> binary is always the source of truth.
>
> **When to address:** If the evaluator is still needed after Stage 2
> (unlikely — the self-hosting compiler will replace it), consider
> rewriting it then.

---

*Document created: 2025. Last updated: pending implementation start.*
*Companion documents: `docs/fuse-stdlib-implementation-plan.md`,
`docs/fuse-language-guide-2.md`, `stage1/hardening.md` (Shared/Async/SIMD
hardening — complete).*
