# Fuse Standard Library — Implementation Plan

> **Status:** In Progress
> **Spec:** `docs/fuse-stdlib-spec.md`
> **Purpose:** Implement the standard library in Fuse, stress-testing the
> Stage 1 compiler and building the foundation for Phase 9 self-hosting.
>
> **Bug Policy:** Any compiler bug found during stdlib work is fixed in the
> compiler immediately. No workarounds. See spec preamble for details.
>
> **No-Workaround Policy:** Every compiler feature required by the stdlib
> spec is implemented before the stdlib code that needs it. No stubs, no
> deferred features, no shortcuts. A workaround today becomes a landmine
> during self-hosting.

---

## Status Key

- `[ ]` — not started
- `[~]` — in progress
- `[x]` — done
- `[!]` — blocked (reason noted)

**Rule:** After completion of each phase, every task in that phase must
be marked `[x]` before moving to the next phase. This document is the
single source of truth for progress — if it is not marked done here,
it is not done.

---

## Wave 0 — Compiler Foundation

**Goal:** Add every compiler feature the stdlib spec requires, so that
Waves 1-5 can be implemented naturally with no workarounds. Each feature
includes tests that prove it works in isolation before stdlib code
relies on it.

**Principle:** The standard library must be written the way any Fuse
developer would naturally write it. If the compiler cannot compile
natural code, the compiler is incomplete. Fix it first.

---

### Phase 0.1 — First-Class Functions

**What:** Support `fn(T) -> U` as a type that can be passed as a
parameter, stored in a variable, and called. This is the single most
impactful feature — it unblocks `map`, `filter`, `reduce`, `forEach`,
`sortedBy`, and every callback-based API across all 5 waves.

- [x] **0.1.1** Lexer/Parser: parse `fn(T) -> U` and `fn(T, U) -> V` as
      type annotations in parameter positions, variable declarations,
      and return types.
- [x] **0.1.2** Lexer/Parser: parse lambda/closure syntax. Choose one of:
      `fn(x) => expr`, `fn(x, y) => expr`, or `fn(x) { body }`.
      Multi-statement bodies must be supported.
- [x] **0.1.3** AST: add `Lambda` expression node with params, optional
      return type, and body. Add `FnType` to type representation.
- [x] **0.1.4** Checker: type-check function parameters with `fn` types.
      Validate argument count and (where annotated) argument types at
      call sites that pass lambdas.
- [x] **0.1.5** Checker: validate that a value of type `fn(T) -> U` can
      be called with `value(arg)` syntax, producing type `U`.
- [x] **0.1.6** Codegen: emit Cranelift IR for lambda expressions.
      Strategy: lift each lambda to a top-level function, pass as
      function pointer. Closures over local variables are not required
      for Wave 1 but document the limitation.
- [x] **0.1.7** Codegen: emit indirect call instructions for calling
      values of `fn` type (Cranelift `call_indirect`).
- [x] **0.1.8** Test: `tests/fuse/core/types/first_class_fn.fuse` —
      pass a named function as argument, call it, verify output.
- [x] **0.1.9** Test: `tests/fuse/core/types/lambda_basic.fuse` —
      inline lambda expression passed to a function, verify output.
- [x] **0.1.10** Test: `tests/fuse/core/types/lambda_multiline.fuse` —
      multi-statement lambda body.
- [x] **0.1.11** Test: `tests/fuse/core/types/fn_type_mismatch.fuse` —
      wrong argument type to fn parameter produces compile error.

---

### Phase 0.2 — User-Defined Enums in Codegen

**What:** The parser already handles `enum` syntax. Extend the checker
and codegen so user-defined enums can be constructed, matched, and
passed around — not just the built-in `Result`/`Option`.

- [x] **0.2.1** Checker: register user-defined enum types. Track variant
      names, arities, and payload types per enum.
- [x] **0.2.2** Checker: validate `match` exhaustiveness against user
      enum variants (extend existing exhaustiveness checker).
- [x] **0.2.3** Checker: validate enum variant construction expressions
      (e.g., `JsonValue.Str("hello")`) — correct variant name, correct
      arity, correct payload types.
- [x] **0.2.4** Codegen: decide runtime representation for user enums.
      Recommended: tag integer + optional payload handle(s). Reuse
      `ValueKind` infrastructure or add a new `Enum` variant.
- [x] **0.2.5** Codegen: emit construction code for enum variants.
- [x] **0.2.6** Codegen: emit match dispatch on enum tag, binding
      payload values to pattern variables.
- [x] **0.2.7** Runtime: add `fuse_enum_new(tag, payloads...)` and
      `fuse_enum_tag(handle)` and `fuse_enum_payload(handle, index)`
      to `fuse-runtime` if needed.
- [x] **0.2.8** Test: `tests/fuse/core/types/user_enum_basic.fuse` —
      define enum, construct variants, match on them.
- [x] **0.2.9** Test: `tests/fuse/core/types/user_enum_payload.fuse` —
      variants with payloads, extract values via match.
- [x] **0.2.10** Test: `tests/fuse/core/types/user_enum_exhaustive.fuse`
      — missing arm produces compile error.
- [x] **0.2.11** Test: `tests/fuse/core/types/user_enum_methods.fuse` —
      extension functions on user-defined enums.

---

### Phase 0.3 — User FFI / Extern Declarations

**What:** Allow Fuse code to declare and call external C-ABI functions
from the runtime. This is the bridge between Fuse stdlib modules and
Rust-backed operations (file I/O, math, networking, etc.).

- [x] **0.3.1** Parser: parse `extern fn name(params) -> RetType` as a
      top-level declaration. No body — just signature.
- [x] **0.3.2** AST/HIR: add `ExternFn` declaration node.
- [x] **0.3.3** Checker: register extern functions in scope. Validate
      calls to extern functions like normal calls (arity, types).
- [x] **0.3.4** Codegen: declare extern functions as imported symbols in
      the Cranelift module. Map Fuse types to ABI types (Int → i64,
      Float → f64, String → FuseHandle, Bool → i64, etc.).
- [x] **0.3.5** Codegen: emit call instructions to extern function
      symbols at call sites.
- [x] **0.3.6** Runtime: create `fuse-runtime/src/ffi.rs` as the
      canonical home for all `#[unsafe(no_mangle)] pub unsafe extern "C"`
      functions that Fuse code can call.
- [x] **0.3.7** Test: add a simple FFI function to the runtime (e.g.,
      `fuse_rt_test_add_ints(a: i64, b: i64) -> i64`). Write
      `tests/fuse/core/types/extern_fn_basic.fuse` that calls it.
- [x] **0.3.8** Test: FFI function that takes and returns a FuseHandle
      (String or List). Verify no memory corruption.
- [x] **0.3.9** Test: calling extern fn with wrong arity → compile error.

---

### Phase 0.4 — Tuple Types

**What:** Support `(T, U)` as a type, `(expr1, expr2)` as construction,
and destructuring in `val` bindings and `match` patterns.

- [x] **0.4.1** Lexer/Parser: parse tuple type syntax `(T, U)` and
      `(T, U, V)` in type annotations. Distinguish from parenthesised
      expressions by context.
- [x] **0.4.2** Lexer/Parser: parse tuple construction `(expr1, expr2)`.
- [x] **0.4.3** Lexer/Parser: parse tuple destructuring in `val`:
      `val (a, b) = someTuple`.
- [x] **0.4.4** AST: add `Tuple` expression node and `TupleType`.
- [x] **0.4.5** Checker: infer and check tuple types. Validate element
      access via `.0`, `.1`, etc.
- [x] **0.4.6** Codegen: represent tuples at runtime. Options: reuse
      data class machinery (anonymous 2/3-field struct) or a dedicated
      `ValueKind::Tuple`.
- [x] **0.4.7** Codegen: emit tuple construction, element access, and
      destructuring.
- [x] **0.4.8** Test: `tests/fuse/core/types/tuple_basic.fuse` — create
      tuple, access `.0` and `.1`, print values.
- [x] **0.4.9** Test: `tests/fuse/core/types/tuple_destructure.fuse` —
      `val (a, b) = ...` binding.
- [x] **0.4.10** Test: `tests/fuse/core/types/tuple_in_list.fuse` —
      `List<(Int, String)>`, iterate and access elements.

---

### Phase 0.5 — Variadic Parameters

**What:** Support `T...` in function parameter lists, allowing callers
to pass zero or more arguments that are collected into a `List<T>`.

- [x] **0.5.1** Lexer/Parser: parse `name: T...` as a variadic parameter.
      Must be the last parameter.
- [x] **0.5.2** AST: mark parameter as variadic in `Param` node.
- [x] **0.5.3** Checker: validate that variadic is the last param. At
      call sites, collect extra arguments into a list.
- [x] **0.5.4** Codegen: at the call site, pack variadic arguments into
      a `List<T>`. Inside the function, the parameter is a normal
      `List<T>`.
- [x] **0.5.5** Test: `tests/fuse/core/types/variadic_basic.fuse` —
      function taking `items: Int...`, call with 0, 1, and 3 args.
- [x] **0.5.6** Test: `tests/fuse/core/types/variadic_strings.fuse` —
      variadic `String...` parameter.
- [x] **0.5.7** Test: variadic not last param → compile error.

---

### Phase 0.6 — Struct (Opaque Types)

**What:** Support `struct Name { }` as a type with opaque internal state,
distinct from `data class` (which exposes fields). Methods are defined
via extension functions. Construction is via explicit static methods.

- [x] **0.6.1** Parser: parse `struct Name { }` and
      `pub struct Name { }` declarations.
- [x] **0.6.2** AST/HIR: add `StructDecl` node. Fields are private and
      unnamed from Fuse's perspective — the struct is opaque.
- [x] **0.6.3** Checker: register struct types. Prevent direct field
      access from outside. Allow extension functions on struct types.
- [x] **0.6.4** Codegen: represent struct as a runtime value. Simplest
      approach: wrap a `FuseHandle` (pointer to heap-allocated Rust
      object). The Rust side manages internal state.
- [x] **0.6.5** Test: `tests/fuse/core/types/struct_basic.fuse` — define
      struct, create via static method, call methods.
- [x] **0.6.6** Test: direct field access on struct → compile error.

---

### Phase 0.7 — Never Type

**What:** Support `!` as a return type meaning the function never
returns (e.g., `sys.exit`, `panic`, `test.fail`).

- [x] **0.7.1** Lexer/Parser: parse `!` as a valid return type annotation.
- [x] **0.7.2** Checker: treat `!` as a bottom type that is compatible
      with any expected type (coerces to anything in type unification).
      Functions returning `!` need not have a `return` statement.
- [x] **0.7.3** Codegen: functions returning `!` emit an `unreachable`
      trap after their body (as a safety net).
- [x] **0.7.4** Test: `tests/fuse/core/types/never_type.fuse` — function
      returning `!` that calls a diverging operation. Verify code after
      call is not reached.
- [x] **0.7.5** Test: function declared `-> !` that actually returns →
      compile error.

---

### Phase 0.8 — Type-Level Constants

**What:** Support `val Type.CONSTANT: T = expr` as a module-level
constant associated with a type. Read via `Type.CONSTANT`.

- [x] **0.8.1** Parser: parse `val Type.NAME: T = expr` at top level.
      Also parse `val module.NAME: T = expr` for module-level constants
      like `math.PI`.
- [x] **0.8.2** AST/HIR: add `TypeConstant` declaration node.
- [x] **0.8.3** Checker: register type constants in a lookup table.
      Validate that the initializer expression type matches the declared
      type. Constants must be immutable.
- [x] **0.8.4** Codegen: emit constant initialization. Options: global
      variable initialized once at module load, or inline the value at
      every use site (for primitives).
- [x] **0.8.5** Test: `tests/fuse/core/types/type_constant.fuse` —
      define `val Int.MAX: Int = 9223372036854775807`, read it, print it.
- [x] **0.8.6** Test: `val math.PI: Float = 3.141592653589793`, use in
      expression.
- [x] **0.8.7** Test: attempt to assign to a type constant → compile
      error.

---

### Phase 0.9 — Pub Visibility Enforcement

**What:** Enforce `pub` annotations in codegen and the checker. Non-pub
declarations in an imported module must not be accessible from outside.

- [x] **0.9.1** Checker: when resolving a name from an imported module,
      verify the declaration is marked `pub`. If not, emit a diagnostic:
      `"error: 'name' is not public in module 'mod'"`.
- [x] **0.9.2** Checker: handle selective imports `import mod.{A, B}` —
      verify each imported name is `pub`.
- [x] **0.9.3** Codegen: no changes needed if checker enforces correctly.
- [x] **0.9.4** Test: `tests/fuse/core/modules/pub_enforcement.fuse` —
      import a module, try to access a non-pub function → compile error.
- [x] **0.9.5** Test: `tests/fuse/core/modules/pub_allowed.fuse` —
      access a `pub` function from imported module → works.

---

### Phase 0.10 — Generic Extension Functions

**What:** Allow extension functions to introduce new type parameters
beyond those of the receiver type. E.g.,
`fn Result<T,E>.map(owned self, f: fn(T) -> U) -> Result<U, E>`
introduces `U` which is not part of `Result<T, E>`.

- [x] **0.10.1** Checker: when type-checking an extension function call,
      infer new type parameters from the argument types. If `f` has type
      `fn(Int) -> String`, then `U = String`.
- [x] **0.10.2** Checker: propagate inferred type parameters to the
      return type. `Result<U, E>` becomes `Result<String, E>`.
- [x] **0.10.3** Codegen: no changes needed if generics remain erased
      (untyped handles at runtime). The type parameters only affect
      compile-time checking.
- [x] **0.10.4** Test: extension function on `Option<T>` that maps to
      `Option<U>` via a lambda. Verify the output type is correct.
- [x] **0.10.5** Test: chained generic extension calls.

---

### Phase 0.11 — Map Built-In Type in Codegen

**What:** Ensure `Map<K, V>` is fully operational in codegen — not just
parsed but actually constructible, readable, writable, and iterable
from compiled code.

- [ ] **0.11.1** Runtime: verify or add `fuse_map_new()`,
      `fuse_map_set(map, key, value)`, `fuse_map_get(map, key)`,
      `fuse_map_remove(map, key)`, `fuse_map_len(map)`,
      `fuse_map_contains(map, key)`, `fuse_map_keys(map)`,
      `fuse_map_values(map)`, `fuse_map_entries(map)`.
- [ ] **0.11.2** Codegen: handle `Map::<K,V>.new()` construction.
- [ ] **0.11.3** Codegen: handle `map.set(key, val)`, `map.get(key)`,
      `map.remove(key)`, `map.len()`, `map.isEmpty()`, `map.contains(key)`
      method calls — dispatch to runtime functions.
- [ ] **0.11.4** Codegen: handle `map.keys()`, `map.values()`,
      `map.entries()` — return `List` values.
- [ ] **0.11.5** Codegen: handle `for entry in map.entries() { ... }` —
      iteration over map entries.
- [ ] **0.11.6** Test: `tests/fuse/core/types/map_basic.fuse` — create
      map, set/get/remove, print len.
- [ ] **0.11.7** Test: `tests/fuse/core/types/map_iteration.fuse` —
      iterate keys, values, entries.
- [ ] **0.11.8** Test: `tests/fuse/core/types/map_contains.fuse` —
      contains check on present and absent keys.

---

### Phase 0.12 — Wave 0 Verification

**What:** End-to-end validation that all compiler features work together.

- [ ] **0.12.1** Write `tests/fuse/core/integration/stdlib_foundation.fuse`
      — a single program that uses first-class functions, user enums,
      extern FFI, tuples, variadics, structs, never type, type constants,
      pub enforcement, generic extensions, and Map. This is the "all
      features" smoke test.
- [ ] **0.12.2** Run full existing test suite — no regressions.
- [ ] **0.12.3** Run `cargo test` on all Stage 1 crates — no regressions.
- [ ] **0.12.4** Document any known limitations discovered during Wave 0
      in this plan (not as blockers — as accepted boundaries).

---

## Wave 1 — Core (`stdlib/core/`)

**Goal:** Implement the 11 core modules. Pure computation, no OS
interaction. All compiler features from Wave 0 are available.

**Dependency:** Modules are ordered so each can import the previous.

---

### Phase 1.1 — `result.fuse`

Extension methods on the built-in `Result<T, E>` type.

- [ ] **1.1.1** Create `stdlib/core/result.fuse` with module header.
- [ ] **1.1.2** Implement `Result.unwrap(owned self) -> T` — match on
      Ok/Err, panic on Err with error message.
- [ ] **1.1.3** Implement `Result.unwrapOr(owned self, default: T) -> T`.
- [ ] **1.1.4** Implement `Result.unwrapOrElse(owned self, f: fn(E) -> T) -> T`.
- [ ] **1.1.5** Implement `Result.isOk(ref self) -> Bool`.
- [ ] **1.1.6** Implement `Result.isErr(ref self) -> Bool`.
- [ ] **1.1.7** Implement `Result.map(owned self, f: fn(T) -> U) -> Result<U, E>`.
- [ ] **1.1.8** Implement `Result.mapErr(owned self, f: fn(E) -> F) -> Result<T, F>`.
- [ ] **1.1.9** Implement `Result.flatten(owned self) -> Result<T, E>`.
- [ ] **1.1.10** Implement `Result.ok(owned self) -> Option<T>`.
- [ ] **1.1.11** Implement `Result.err(owned self) -> Option<E>`.
- [ ] **1.1.12** Create `tests/fuse/stdlib/core/result_test.fuse` — test
      every method with happy path and edge cases.
- [ ] **1.1.13** Run tests. Fix any compiler bugs found.

---

### Phase 1.2 — `option.fuse`

Extension methods on the built-in `Option<T>` type.

- [ ] **1.2.1** Create `stdlib/core/option.fuse` with module header.
- [ ] **1.2.2** Implement `Option.unwrap(owned self) -> T`.
- [ ] **1.2.3** Implement `Option.unwrapOr(owned self, default: T) -> T`.
- [ ] **1.2.4** Implement `Option.unwrapOrElse(owned self, f: fn() -> T) -> T`.
- [ ] **1.2.5** Implement `Option.isSome(ref self) -> Bool`.
- [ ] **1.2.6** Implement `Option.isNone(ref self) -> Bool`.
- [ ] **1.2.7** Implement `Option.map(owned self, f: fn(T) -> U) -> Option<U>`.
- [ ] **1.2.8** Implement `Option.filter(owned self, f: fn(ref T) -> Bool) -> Option<T>`.
- [ ] **1.2.9** Implement `Option.orElse(owned self, f: fn() -> Option<T>) -> Option<T>`.
- [ ] **1.2.10** Implement `Option.flatten(owned self) -> Option<T>`.
- [ ] **1.2.11** Implement `Option.okOr(owned self, err: E) -> Result<T, E>`.
- [ ] **1.2.12** Create `tests/fuse/stdlib/core/option_test.fuse`.
- [ ] **1.2.13** Run tests. Fix any compiler bugs found.

---

### Phase 1.3 — `bool.fuse`

Extension methods on `Bool`. Pure Fuse.

- [ ] **1.3.1** Create `stdlib/core/bool.fuse`.
- [ ] **1.3.2** Implement `Bool.not(ref self) -> Bool`.
- [ ] **1.3.3** Implement `Bool.toString(ref self) -> String`.
- [ ] **1.3.4** Implement `Bool.toInt(ref self) -> Int`.
- [ ] **1.3.5** Create `tests/fuse/stdlib/core/bool_test.fuse`.
- [ ] **1.3.6** Run tests. Fix any compiler bugs found.

---

### Phase 1.4 — `int.fuse`

Extension methods on `Int`.

- [ ] **1.4.1** Create `stdlib/core/int.fuse`.
- [ ] **1.4.2** Implement `Int.abs`, `Int.min`, `Int.max`, `Int.clamp`.
- [ ] **1.4.3** Implement `Int.pow(ref self, exp: Int) -> Int`.
- [ ] **1.4.4** Implement `Int.gcd` (Euclid's algorithm) and `Int.lcm`.
- [ ] **1.4.5** Implement predicates: `isEven`, `isOdd`, `isPositive`,
      `isNegative`, `isZero`.
- [ ] **1.4.6** Implement `Int.toFloat(ref self) -> Float` — via FFI
      `fuse_rt_int_to_float`.
- [ ] **1.4.7** Implement `Int.toString(ref self) -> String` — via f-string.
- [ ] **1.4.8** Implement `Int.toHex`, `Int.toBits`, `Int.toOctal` — pure
      Fuse string-building with `%` and `/` loops.
- [ ] **1.4.9** Implement `Int.parse(s: String) -> Result<Int, String>` —
      wraps built-in `parseInt`.
- [ ] **1.4.10** Implement `Int.parseHex`, `Int.parseBinary` — pure Fuse
      char-by-char parsing.
- [ ] **1.4.11** Define `val Int.MIN` and `val Int.MAX` type constants.
- [ ] **1.4.12** Create `tests/fuse/stdlib/core/int_test.fuse`.
- [ ] **1.4.13** Run tests. Fix any compiler bugs found.

---

### Phase 1.5 — `float.fuse`

Extension methods on `Float`. FFI-backed math operations.

- [ ] **1.5.1** Create `stdlib/core/float.fuse`.
- [ ] **1.5.2** Add FFI functions to runtime: `fuse_rt_float_abs`,
      `fuse_rt_float_floor`, `fuse_rt_float_ceil`, `fuse_rt_float_round`,
      `fuse_rt_float_trunc`, `fuse_rt_float_fract`, `fuse_rt_float_sqrt`,
      `fuse_rt_float_pow`, `fuse_rt_float_is_nan`,
      `fuse_rt_float_is_infinite`, `fuse_rt_float_is_finite`,
      `fuse_rt_float_to_int`, `fuse_rt_float_parse`.
- [ ] **1.5.3** Implement all math methods: `abs`, `floor`, `ceil`,
      `round`, `trunc`, `fract`, `sqrt`, `pow`, `min`, `max`, `clamp`.
- [ ] **1.5.4** Implement predicates: `isNaN`, `isInfinite`, `isFinite`,
      `isPositive`, `isNegative`.
- [ ] **1.5.5** Implement `approxEq(ref self, other: Float, epsilon: Float)`.
- [ ] **1.5.6** Implement `toInt`, `toString`, `toStringFixed`.
- [ ] **1.5.7** Implement `Float.parse(s: String) -> Result<Float, String>`.
- [ ] **1.5.8** Define type constants: `Float.PI`, `Float.E`, `Float.NAN`,
      `Float.INFINITY`, `Float.NEG_INFINITY`, `Float.EPSILON`.
- [ ] **1.5.9** Create `tests/fuse/stdlib/core/float_test.fuse`.
- [ ] **1.5.10** Run tests. Fix any compiler bugs found.

---

### Phase 1.6 — `math.fuse`

Free mathematical functions.

- [ ] **1.6.1** Create `stdlib/core/math.fuse`.
- [ ] **1.6.2** Add FFI functions to runtime: `fuse_rt_math_sin`,
      `fuse_rt_math_cos`, `fuse_rt_math_tan`, `fuse_rt_math_asin`,
      `fuse_rt_math_acos`, `fuse_rt_math_atan`, `fuse_rt_math_atan2`,
      `fuse_rt_math_exp`, `fuse_rt_math_exp2`, `fuse_rt_math_ln`,
      `fuse_rt_math_log2`, `fuse_rt_math_log10`, `fuse_rt_math_cbrt`,
      `fuse_rt_math_hypot`.
- [ ] **1.6.3** Implement trig functions: `sin`, `cos`, `tan`, `asin`,
      `acos`, `atan`, `atan2`.
- [ ] **1.6.4** Implement exp/log: `exp`, `exp2`, `ln`, `log2`, `log10`,
      `log`.
- [ ] **1.6.5** Implement float math: `sqrt`, `cbrt`, `hypot`, `floor`,
      `ceil`, `round`, `trunc`, `abs`, `minFloat`, `maxFloat`,
      `clampFloat`.
- [ ] **1.6.6** Implement pure-Fuse integer math: `absInt`, `minInt`,
      `maxInt`, `clampInt`, `gcd`, `lcm`, `isPrime`, `factorial`.
- [ ] **1.6.7** Implement `degreesToRadians`, `radiansToDegrees`.
- [ ] **1.6.8** Define constants: `PI`, `E`, `TAU`, `SQRT2`, `LN2`, `LN10`.
- [ ] **1.6.9** Create `tests/fuse/stdlib/core/math_test.fuse`.
- [ ] **1.6.10** Run tests. Fix any compiler bugs found.

---

### Phase 1.7 — `fmt.fuse`

String formatting utilities. Pure Fuse.

- [ ] **1.7.1** Create `stdlib/core/fmt.fuse`.
- [ ] **1.7.2** Implement number formatting: `fmt.hex`, `fmt.hexUpper`,
      `fmt.binary`, `fmt.octal`, `fmt.thousands`.
- [ ] **1.7.3** Implement `fmt.decimal`, `fmt.percent`,
      `fmt.thousandsFloat`, `fmt.scientific`.
- [ ] **1.7.4** Implement string alignment: `fmt.padLeft`, `fmt.padRight`,
      `fmt.padCenter`, `fmt.padLeftWith`, `fmt.padRightWith`.
- [ ] **1.7.5** Implement `fmt.truncate`, `fmt.truncateEllipsis`.
- [ ] **1.7.6** Implement `fmt.repeatChar`, `fmt.ruler`.
- [ ] **1.7.7** Implement `fmt.columns`.
- [ ] **1.7.8** Create `tests/fuse/stdlib/core/fmt_test.fuse`.
- [ ] **1.7.9** Run tests. Fix any compiler bugs found.

---

### Phase 1.8 — `string.fuse`

Extension methods on `String`.

- [ ] **1.8.1** Create `stdlib/core/string.fuse`.
- [ ] **1.8.2** Add FFI functions to runtime: `fuse_rt_string_to_lower`,
      `fuse_rt_string_chars`, `fuse_rt_string_char_count`,
      `fuse_rt_string_to_bytes`, `fuse_rt_string_from_bytes`,
      `fuse_rt_string_from_char`.
- [ ] **1.8.3** Implement search methods: `contains`, `startsWith`,
      `endsWith`, `indexOf`, `lastIndexOf`.
- [ ] **1.8.4** Implement transform methods: `trim`, `trimStart`,
      `trimEnd`, `replace`, `replaceFirst`, `split`, `splitLines`,
      `repeat`, `reverse`.
- [ ] **1.8.5** Implement `toLower`, `capitalize`, `padStart`, `padEnd`.
- [ ] **1.8.6** Implement conversion: `toInt`, `toFloat`, `toBool`,
      `toBytes`, `chars`, `charCount`.
- [ ] **1.8.7** Implement `String.fromBytes`, `String.fromChar`.
- [ ] **1.8.8** Implement `compareTo`.
- [ ] **1.8.9** Create `tests/fuse/stdlib/core/string_test.fuse`.
- [ ] **1.8.10** Run tests. Fix any compiler bugs found.

---

### Phase 1.9 — `list.fuse`

Extension methods on `List<T>`.

- [ ] **1.9.1** Create `stdlib/core/list.fuse`.
- [ ] **1.9.2** Implement query methods: `len`, `isEmpty`, `get`, `first`,
      `last`, `contains`, `indexOf`.
- [ ] **1.9.3** Implement HOF query methods: `count`, `any`, `all`.
- [ ] **1.9.4** Implement mutation methods: `push`, `pop`, `insert`,
      `removeAt`, `removeWhere`, `clear`, `sortInPlace`, `reverseInPlace`.
- [ ] **1.9.5** Implement non-HOF transformations: `reversed`, `slice`,
      `take`, `drop`, `concat`, `join`.
- [ ] **1.9.6** Implement HOF transformations: `map`, `filter`, `flatMap`,
      `reduce`, `sorted`, `sortedBy`, `unique`.
- [ ] **1.9.7** Implement `zip` — returns `List<(T, U)>` (uses tuples).
- [ ] **1.9.8** Implement `flatten`.
- [ ] **1.9.9** Implement constructors: `List.new()`, `List.of(items: T...)`,
      `List.repeat(item, n)`, `List.range(start, end)`,
      `List.rangeClosed(start, end)`.
- [ ] **1.9.10** Create `tests/fuse/stdlib/core/list_test.fuse`.
- [ ] **1.9.11** Run tests. Fix any compiler bugs found.

---

### Phase 1.10 — `map.fuse`

Extension methods on `Map<K, V>`.

- [ ] **1.10.1** Create `stdlib/core/map.fuse`.
- [ ] **1.10.2** Implement `getOrDefault`, `getOrInsert`.
- [ ] **1.10.3** Implement `mapValues`, `filter`, `forEach`.
- [ ] **1.10.4** Implement `merge`.
- [ ] **1.10.5** Implement `toList` — returns `List<(K, V)>` (uses tuples).
- [ ] **1.10.6** Implement `invert`.
- [ ] **1.10.7** Create `tests/fuse/stdlib/core/map_test.fuse`.
- [ ] **1.10.8** Run tests. Fix any compiler bugs found.

---

### Phase 1.11 — `set.fuse`

`Set<T>` built on `Map<T, Bool>`.

- [ ] **1.11.1** Create `stdlib/core/set.fuse` — define
      `data class Set<T>(val inner: Map<T, Bool>)`.
- [ ] **1.11.2** Implement constructors: `Set.new()`, `Set.of(items: T...)`,
      `Set.fromList(items: List<T>)`.
- [ ] **1.11.3** Implement query: `contains`, `len`, `isEmpty`, `toList`.
- [ ] **1.11.4** Implement mutation: `add`, `remove`, `clear`.
- [ ] **1.11.5** Implement set operations: `union`, `intersect`,
      `difference`, `symmetricDiff`, `isSubsetOf`, `isSupersetOf`,
      `isDisjoint`.
- [ ] **1.11.6** Implement `forEach`, `filter`, `map`.
- [ ] **1.11.7** Create `tests/fuse/stdlib/core/set_test.fuse`.
- [ ] **1.11.8** Run tests. Fix any compiler bugs found.

---

## Wave 2 — Full I/O and System (`stdlib/full/`)

**Goal:** File I/O, paths, OS operations, environment, system info, time,
random numbers, and process spawning.

---

### Phase 2.1 — `io.fuse`

File I/O and stdin/stdout access.

- [ ] **2.1.1** Add FFI functions to `fuse-runtime/src/ffi.rs`:
      `fuse_rt_io_read_file`, `fuse_rt_io_read_file_bytes`,
      `fuse_rt_io_write_file`, `fuse_rt_io_write_file_bytes`,
      `fuse_rt_io_append_file`, `fuse_rt_io_read_line`,
      `fuse_rt_io_read_all`.
- [ ] **2.1.2** Create `stdlib/full/io.fuse`.
- [ ] **2.1.3** Define `IOError` data class and `OpenMode` enum.
- [ ] **2.1.4** Implement free functions: `readFile`, `readFileBytes`,
      `writeFile`, `writeFileBytes`, `appendFile`, `readLine`, `readAll`.
- [ ] **2.1.5** Add FFI for buffered File: `fuse_rt_file_open`,
      `fuse_rt_file_create`, `fuse_rt_file_read_line`,
      `fuse_rt_file_read_chunk`, `fuse_rt_file_read_all`,
      `fuse_rt_file_write`, `fuse_rt_file_write_bytes`,
      `fuse_rt_file_flush`, `fuse_rt_file_seek`, `fuse_rt_file_pos`,
      `fuse_rt_file_size`, `fuse_rt_file_close`.
- [ ] **2.1.6** Define `File` struct with `__del__` destructor.
- [ ] **2.1.7** Implement `File.open`, `File.create`, `File.readLine`,
      `File.readChunk`, `File.readAll`, `File.write`, `File.writeBytes`,
      `File.flush`, `File.seek`, `File.pos`, `File.size`, `File.close`.
- [ ] **2.1.8** Create `tests/fuse/stdlib/full/io_test.fuse`.
- [ ] **2.1.9** Run tests. Fix any compiler bugs found.

---

### Phase 2.2 — `path.fuse`

Path manipulation. Mostly pure Fuse string ops.

- [ ] **2.2.1** Create `stdlib/full/path.fuse`.
- [ ] **2.2.2** Add FFI: `fuse_rt_path_separator` (returns platform sep).
- [ ] **2.2.3** Define `val path.SEPARATOR`.
- [ ] **2.2.4** Implement pure-Fuse functions: `basename`, `stem`,
      `extension`, `parent`, `components`, `isAbsolute`, `isRelative`,
      `normalize`, `withExtension`, `withBasename`, `fromParts`, `join`.
- [ ] **2.2.5** Implement `toAbsolute` — uses `sys.cwd()`.
- [ ] **2.2.6** Create `tests/fuse/stdlib/full/path_test.fuse`.
- [ ] **2.2.7** Run tests. Fix any compiler bugs found.

---

### Phase 2.3 — `os.fuse`

Filesystem operations.

- [ ] **2.3.1** Add FFI functions to runtime: `fuse_rt_os_exists`,
      `fuse_rt_os_is_file`, `fuse_rt_os_is_dir`, `fuse_rt_os_stat`,
      `fuse_rt_os_read_dir`, `fuse_rt_os_mkdir`, `fuse_rt_os_mkdir_all`,
      `fuse_rt_os_create_file`, `fuse_rt_os_copy_file`,
      `fuse_rt_os_copy_dir`, `fuse_rt_os_rename`, `fuse_rt_os_remove_file`,
      `fuse_rt_os_remove_dir`, `fuse_rt_os_remove_dir_all`,
      `fuse_rt_os_create_symlink`, `fuse_rt_os_read_symlink`,
      `fuse_rt_os_set_read_only`, `fuse_rt_os_temp_dir`,
      `fuse_rt_os_temp_file`, `fuse_rt_os_temp_dir_create`.
- [ ] **2.3.2** Create `stdlib/full/os.fuse`.
- [ ] **2.3.3** Define `EntryKind` enum, `DirEntry` and `FileInfo` data
      classes.
- [ ] **2.3.4** Implement all querying functions: `exists`, `isFile`,
      `isDir`, `stat`, `readDir`.
- [ ] **2.3.5** Implement creating functions: `mkdir`, `mkdirAll`,
      `createFile`.
- [ ] **2.3.6** Implement copy/move: `copyFile`, `copyDir`, `rename`,
      `move`.
- [ ] **2.3.7** Implement delete: `removeFile`, `removeDir`, `removeDirAll`.
- [ ] **2.3.8** Implement symlinks: `createSymlink`, `readSymlink`.
- [ ] **2.3.9** Implement `setReadOnly`, `tempDir`, `tempFile`,
      `tempDirCreate`.
- [ ] **2.3.10** Implement `readDirRecursive` in Fuse over `readDir`.
- [ ] **2.3.11** Create `tests/fuse/stdlib/full/os_test.fuse`.
- [ ] **2.3.12** Run tests. Fix any compiler bugs found.

---

### Phase 2.4 — `env.fuse`

Environment variable access.

- [ ] **2.4.1** Add FFI: `fuse_rt_env_get`, `fuse_rt_env_set`,
      `fuse_rt_env_remove`, `fuse_rt_env_all`, `fuse_rt_env_has`.
- [ ] **2.4.2** Create `stdlib/full/env.fuse`.
- [ ] **2.4.3** Implement all functions: `get`, `getOrDefault`, `set`,
      `remove`, `all`, `has`.
- [ ] **2.4.4** Create `tests/fuse/stdlib/full/env_test.fuse`.
- [ ] **2.4.5** Run tests. Fix any compiler bugs found.

---

### Phase 2.5 — `sys.fuse`

Process-level information.

- [ ] **2.5.1** Add FFI: `fuse_rt_sys_args`, `fuse_rt_sys_exit`,
      `fuse_rt_sys_cwd`, `fuse_rt_sys_set_cwd`, `fuse_rt_sys_pid`,
      `fuse_rt_sys_platform`, `fuse_rt_sys_arch`, `fuse_rt_sys_num_cpus`,
      `fuse_rt_sys_mem_total`.
- [ ] **2.5.2** Create `stdlib/full/sys.fuse`.
- [ ] **2.5.3** Implement all functions. `sys.exit` returns `!`.
- [ ] **2.5.4** Create `tests/fuse/stdlib/full/sys_test.fuse`.
- [ ] **2.5.5** Run tests. Fix any compiler bugs found.

---

### Phase 2.6 — `time.fuse`

Timestamps, durations, and calendar dates.

- [ ] **2.6.1** Add FFI: `fuse_rt_time_instant_now`,
      `fuse_rt_time_system_now`, `fuse_rt_time_elapsed_nanos`.
- [ ] **2.6.2** Create `stdlib/full/time.fuse`.
- [ ] **2.6.3** Define `Instant`, `Duration`, `DateTime` data classes.
- [ ] **2.6.4** Implement `Duration` methods — pure Fuse arithmetic on
      nanos field.
- [ ] **2.6.5** Implement `Instant.now()`, `Instant.elapsed()`,
      `Instant.durationSince()`.
- [ ] **2.6.6** Implement `DateTime` methods: `now()`, `fromUnix()`,
      `toString()`, `toDate()`, `toTime()`, `add()`, `sub()`, `diff()`,
      `dayOfWeek()`, `isLeapYear()`.
- [ ] **2.6.7** Implement `DateTime.parse()` — ISO 8601, pure Fuse.
- [ ] **2.6.8** Create `tests/fuse/stdlib/full/time_test.fuse`.
- [ ] **2.6.9** Run tests. Fix any compiler bugs found.

---

### Phase 2.7 — `random.fuse`

Pseudo-random number generation.

- [ ] **2.7.1** Add FFI: `fuse_rt_random_new`, `fuse_rt_random_seeded`,
      `fuse_rt_random_next_int`, `fuse_rt_random_next_float`,
      `fuse_rt_random_next_int_range`, `fuse_rt_random_next_float_range`.
- [ ] **2.7.2** Create `stdlib/full/random.fuse`.
- [ ] **2.7.3** Define `Rng` struct.
- [ ] **2.7.4** Implement `Rng.new()`, `Rng.seeded()`, `Rng.nextInt()`,
      `Rng.nextFloat()`, `Rng.nextIntRange()`, `Rng.nextFloatRange()`,
      `Rng.nextBool()`.
- [ ] **2.7.5** Implement `Rng.nextGaussian()` — Box-Muller in Fuse.
- [ ] **2.7.6** Implement `Rng.shuffle`, `Rng.choose`, `Rng.sample`.
- [ ] **2.7.7** Implement convenience functions: `random.int()`,
      `random.intRange()`, `random.float()`, `random.bool()`.
- [ ] **2.7.8** Create `tests/fuse/stdlib/full/random_test.fuse`.
- [ ] **2.7.9** Run tests. Fix any compiler bugs found.

---

### Phase 2.8 — `process.fuse`

Child process spawning.

- [ ] **2.8.1** Add FFI: `fuse_rt_process_run`, `fuse_rt_process_shell`,
      `fuse_rt_process_command_new`, `fuse_rt_process_command_arg`,
      `fuse_rt_process_command_env`, `fuse_rt_process_command_cwd`,
      `fuse_rt_process_command_stdin`, `fuse_rt_process_command_run`.
- [ ] **2.8.2** Create `stdlib/full/process.fuse`.
- [ ] **2.8.3** Define `ProcessError`, `Output` data classes.
- [ ] **2.8.4** Define `Command` struct with builder methods.
- [ ] **2.8.5** Implement `Command.new()`, `.arg()`, `.args()`, `.env()`,
      `.envClear()`, `.cwd()`, `.stdin()`, `.run()`, `.status()`,
      `.output()`.
- [ ] **2.8.6** Implement `process.run()`, `process.shell()`.
- [ ] **2.8.7** Create `tests/fuse/stdlib/full/process_test.fuse`.
- [ ] **2.8.8** Run tests. Fix any compiler bugs found.

---

## Wave 3 — Full Networking and Data (`stdlib/full/`)

**Goal:** TCP/UDP networking, JSON parsing, and HTTP client.

---

### Phase 3.1 — `net.fuse`

TCP and UDP networking.

- [ ] **3.1.1** Add FFI: `fuse_rt_net_tcp_connect`,
      `fuse_rt_net_tcp_connect_timeout`, `fuse_rt_net_tcp_read`,
      `fuse_rt_net_tcp_read_exact`, `fuse_rt_net_tcp_read_line`,
      `fuse_rt_net_tcp_read_all`, `fuse_rt_net_tcp_write`,
      `fuse_rt_net_tcp_write_bytes`, `fuse_rt_net_tcp_flush`,
      `fuse_rt_net_tcp_set_read_timeout`,
      `fuse_rt_net_tcp_set_write_timeout`,
      `fuse_rt_net_tcp_local_addr`, `fuse_rt_net_tcp_peer_addr`,
      `fuse_rt_net_tcp_close`,
      `fuse_rt_net_tcp_bind`, `fuse_rt_net_tcp_accept`,
      `fuse_rt_net_tcp_listener_local_addr`,
      `fuse_rt_net_tcp_listener_close`,
      `fuse_rt_net_udp_bind`, `fuse_rt_net_udp_send_to`,
      `fuse_rt_net_udp_recv_from`, `fuse_rt_net_udp_set_broadcast`,
      `fuse_rt_net_udp_close`.
- [ ] **3.1.2** Create `stdlib/full/net.fuse`.
- [ ] **3.1.3** Define `NetError` data class.
- [ ] **3.1.4** Implement `TcpStream`: all methods including `__del__`.
- [ ] **3.1.5** Implement `TcpListener`: all methods including `__del__`.
- [ ] **3.1.6** Implement `UdpSocket`: all methods including `__del__`.
- [ ] **3.1.7** Create `tests/fuse/stdlib/full/net_test.fuse`.
- [ ] **3.1.8** Run tests. Fix any compiler bugs found.

---

### Phase 3.2 — `json.fuse`

JSON parsing and serialisation. Parser is pure Fuse.

- [ ] **3.2.1** Create `stdlib/full/json.fuse`.
- [ ] **3.2.2** Define `JsonError` data class and `JsonValue` enum
      (`Null`, `Bool(Bool)`, `Number(Float)`, `Str(String)`,
      `Array(List<JsonValue>)`, `Object(Map<String, JsonValue>)`).
- [ ] **3.2.3** Implement `json.parse(s: String)` — hand-written
      recursive descent parser in pure Fuse.
- [ ] **3.2.4** Implement `json.stringify(value)` and
      `json.stringifyPretty(value, indent)`.
- [ ] **3.2.5** Implement `JsonValue` type-check helpers: `isNull`,
      `isBool`, `isNumber`, `isString`, `isArray`, `isObject`.
- [ ] **3.2.6** Implement `JsonValue` extraction helpers: `asBool`,
      `asNumber`, `asInt`, `asString`, `asArray`, `asObject`.
- [ ] **3.2.7** Implement `JsonValue.get(key)` and
      `JsonValue.getPath(path)`.
- [ ] **3.2.8** Implement `JsonValue.object(entries)` and
      `JsonValue.array(items)` construction helpers.
- [ ] **3.2.9** Implement `json.parseFile` — uses `io.readFile`.
- [ ] **3.2.10** Create `tests/fuse/stdlib/full/json_test.fuse`.
- [ ] **3.2.11** Run tests. Fix any compiler bugs found.

---

### Phase 3.3 — `http.fuse`

HTTP client. Replaces existing stub.

- [ ] **3.3.1** Add `reqwest` (blocking) dependency to
      `fuse-runtime/Cargo.toml`.
- [ ] **3.3.2** Add FFI: `fuse_rt_http_get`, `fuse_rt_http_post`,
      `fuse_rt_http_put`, `fuse_rt_http_delete`,
      `fuse_rt_http_request`.
- [ ] **3.3.3** Replace `stdlib/full/http.fuse` with full implementation.
- [ ] **3.3.4** Define `HttpError`, `Response` data classes.
- [ ] **3.3.5** Implement convenience functions: `http.get`, `http.post`,
      `http.postJson`, `http.put`, `http.delete`.
- [ ] **3.3.6** Define `HttpClient` struct with builder: `new()`,
      `withTimeout()`, `withHeader()`, `withBasicAuth()`,
      `withBearerToken()`.
- [ ] **3.3.7** Implement `HttpClient.get`, `.post`, `.postJson`, `.put`,
      `.delete`.
- [ ] **3.3.8** Implement `Response.ok()`, `Response.json()`.
- [ ] **3.3.9** Create `tests/fuse/stdlib/full/http_test.fuse`.
- [ ] **3.3.10** Run tests. Fix any compiler bugs found.

---

## Wave 4 — Full Concurrency (`stdlib/full/`)

**Goal:** Upgrade existing stubs for channels, shared state, timers,
and SIMD to match the spec.

---

### Phase 4.1 — `chan.fuse`

Upgrade channel API to match spec.

- [ ] **4.1.1** Review existing `stdlib/full/chan.fuse` and runtime.
- [ ] **4.1.2** Replace with spec-compliant version.
- [ ] **4.1.3** Implement `Chan.bounded() -> (Chan<T>, Chan<T>)` and
      `Chan.unbounded() -> (Chan<T>, Chan<T>)` (tuple returns).
- [ ] **4.1.4** Implement `send`, `recv` with `Result` return types.
- [ ] **4.1.5** Implement `tryRecv`, `close`, `isClosed`, `len`, `cap`.
- [ ] **4.1.6** Add any missing FFI to runtime.
- [ ] **4.1.7** Update `tests/fuse/full/concurrency/chan_*.fuse` tests.
- [ ] **4.1.8** Create `tests/fuse/stdlib/full/chan_test.fuse`.
- [ ] **4.1.9** Run tests. Fix any compiler bugs found.

---

### Phase 4.2 — `shared.fuse`

Upgrade shared state API.

- [ ] **4.2.1** Review existing `stdlib/full/shared.fuse` and runtime.
- [ ] **4.2.2** Replace with spec-compliant version.
- [ ] **4.2.3** Verify `read`, `write`, `tryWrite` match spec signatures.
- [ ] **4.2.4** Add `tryRead`.
- [ ] **4.2.5** Update existing tests.
- [ ] **4.2.6** Create `tests/fuse/stdlib/full/shared_test.fuse`.
- [ ] **4.2.7** Run tests. Fix any compiler bugs found.

---

### Phase 4.3 — `timer.fuse`

Upgrade timer from stub to working implementation.

- [ ] **4.3.1** Add FFI: `fuse_rt_timer_sleep_ms`.
- [ ] **4.3.2** Replace `stdlib/full/timer.fuse` with spec-compliant
      version.
- [ ] **4.3.3** Implement `Timer.sleep()`, `Timer.sleepSecs()`.
- [ ] **4.3.4** Implement `Timeout.ms()`, `Timeout.secs()`,
      `Timeout.never()`.
- [ ] **4.3.5** Create `tests/fuse/stdlib/full/timer_test.fuse`.
- [ ] **4.3.6** Run tests. Fix any compiler bugs found.

---

### Phase 4.4 — `simd.fuse`

Upgrade SIMD API to match spec.

- [ ] **4.4.1** Review existing `stdlib/full/simd.fuse` and runtime.
- [ ] **4.4.2** Replace with spec-compliant version.
- [ ] **4.4.3** Implement `broadcast`, `fromList`, `toList`.
- [ ] **4.4.4** Implement `add`, `sub`, `mul`, `div`, `sum`, `dot`.
- [ ] **4.4.5** Implement `min`, `max`, `abs`, `sqrt`, `get`, `len`.
- [ ] **4.4.6** Add any missing FFI to runtime.
- [ ] **4.4.7** Update existing SIMD tests.
- [ ] **4.4.8** Create `tests/fuse/stdlib/full/simd_test.fuse`.
- [ ] **4.4.9** Run tests. Fix any compiler bugs found.

---

## Wave 5 — Ext (`stdlib/ext/`)

**Goal:** Optional, heavyweight modules. Each is independent.

---

### Phase 5.1 — `test.fuse`

Test assertion utilities.

- [ ] **5.1.1** Create `stdlib/ext/test.fuse`.
- [ ] **5.1.2** Implement `assert(cond, message)`,
      `assertEq(a, b, message)`, `assertNe(a, b, message)`.
- [ ] **5.1.3** Implement `assertOk`, `assertErr`, `assertSome`,
      `assertNone`.
- [ ] **5.1.4** Implement `assertPanics` — requires panic-catching
      mechanism in runtime.
- [ ] **5.1.5** Implement `assertApprox(a, b, epsilon)`.
- [ ] **5.1.6** Implement `fail(message) -> !`, `skip(message) -> !`.
- [ ] **5.1.7** Implement `describe(name, f: fn())`.
- [ ] **5.1.8** Create `tests/fuse/stdlib/ext/test_test.fuse`.
- [ ] **5.1.9** Run tests. Fix any compiler bugs found.

---

### Phase 5.2 — `log.fuse`

Structured logging.

- [ ] **5.2.1** Create `stdlib/ext/log.fuse`.
- [ ] **5.2.2** Define `Level` enum and `Logger` data class.
- [ ] **5.2.3** Implement `Logger` builder and log methods.
- [ ] **5.2.4** Implement global `log.*` convenience functions.
- [ ] **5.2.5** Create `tests/fuse/stdlib/ext/log_test.fuse`.
- [ ] **5.2.6** Run tests. Fix any compiler bugs found.

---

### Phase 5.3 — `regex.fuse`

Regular expressions backed by Rust's `regex` crate.

- [ ] **5.3.1** Add `regex` dependency to `fuse-runtime/Cargo.toml`.
- [ ] **5.3.2** Add FFI: `fuse_rt_regex_compile`, `fuse_rt_regex_is_match`,
      `fuse_rt_regex_find`, `fuse_rt_regex_find_all`,
      `fuse_rt_regex_replace`, `fuse_rt_regex_replace_all`,
      `fuse_rt_regex_split`, `fuse_rt_regex_captures`.
- [ ] **5.3.3** Create `stdlib/ext/regex.fuse`.
- [ ] **5.3.4** Define `RegexError`, `Match` data classes and `Regex`
      struct.
- [ ] **5.3.5** Implement all `Regex` methods.
- [ ] **5.3.6** Create `tests/fuse/stdlib/ext/regex_test.fuse`.
- [ ] **5.3.7** Run tests. Fix any compiler bugs found.

---

### Phase 5.4 — `toml.fuse`

TOML parsing backed by Rust's `toml` crate.

- [ ] **5.4.1** Add `toml` dependency to `fuse-runtime/Cargo.toml`.
- [ ] **5.4.2** Add FFI: `fuse_rt_toml_parse`, `fuse_rt_toml_stringify`.
- [ ] **5.4.3** Create `stdlib/ext/toml.fuse`.
- [ ] **5.4.4** Define `TomlError` data class and `TomlValue` enum.
- [ ] **5.4.5** Implement `toml.parse`, `toml.parseFile`,
      `toml.stringify`.
- [ ] **5.4.6** Create `tests/fuse/stdlib/ext/toml_test.fuse`.
- [ ] **5.4.7** Run tests. Fix any compiler bugs found.

---

### Phase 5.5 — `yaml.fuse`

YAML parsing backed by Rust's `serde_yaml` crate.

- [ ] **5.5.1** Add `serde_yaml` dependency to `fuse-runtime/Cargo.toml`.
- [ ] **5.5.2** Add FFI: `fuse_rt_yaml_parse`, `fuse_rt_yaml_stringify`.
- [ ] **5.5.3** Create `stdlib/ext/yaml.fuse`.
- [ ] **5.5.4** Define `YamlError` data class and `YamlValue` enum.
- [ ] **5.5.5** Implement `yaml.parse`, `yaml.parseFile`,
      `yaml.stringify`, `yaml.stringifyPretty`.
- [ ] **5.5.6** Create `tests/fuse/stdlib/ext/yaml_test.fuse`.
- [ ] **5.5.7** Run tests. Fix any compiler bugs found.

---

### Phase 5.6 — `json_schema.fuse`

JSON Schema validation. Pure Fuse over `JsonValue`.

- [ ] **5.6.1** Create `stdlib/ext/json_schema.fuse`.
- [ ] **5.6.2** Define `ValidationError` data class and `Schema` struct.
- [ ] **5.6.3** Implement `Schema.compile`, `Schema.validate`,
      `Schema.isValid`.
- [ ] **5.6.4** Create `tests/fuse/stdlib/ext/json_schema_test.fuse`.
- [ ] **5.6.5** Run tests. Fix any compiler bugs found.

---

### Phase 5.7 — `crypto.fuse`

Cryptographic primitives backed by Rust crates.

- [ ] **5.7.1** Add dependencies: `sha2`, `hmac`, `md5`, `blake3`,
      `getrandom` to `fuse-runtime/Cargo.toml`.
- [ ] **5.7.2** Add FFI: `fuse_rt_crypto_sha256`, `fuse_rt_crypto_sha512`,
      `fuse_rt_crypto_md5`, `fuse_rt_crypto_blake3`,
      `fuse_rt_crypto_hmac_sha256`, `fuse_rt_crypto_random_bytes`.
- [ ] **5.7.3** Create `stdlib/ext/crypto.fuse`.
- [ ] **5.7.4** Implement `hash.*`, `hmac.*`, `rand.*`,
      `crypto.constantTimeEq`.
- [ ] **5.7.5** Implement `rand.uuid4()` — pure Fuse formatting.
- [ ] **5.7.6** Create `tests/fuse/stdlib/ext/crypto_test.fuse`.
- [ ] **5.7.7** Run tests. Fix any compiler bugs found.

---

### Phase 5.8 — `http_server.fuse`

HTTP server backed by Rust crate.

- [ ] **5.8.1** Add server dependency (e.g., `tiny_http`) to
      `fuse-runtime/Cargo.toml`.
- [ ] **5.8.2** Add FFI: `fuse_rt_http_server_new`,
      `fuse_rt_http_server_listen`, `fuse_rt_http_server_route`.
- [ ] **5.8.3** Create `stdlib/ext/http_server.fuse`.
- [ ] **5.8.4** Define `Request`, `Response` data classes, `Router` and
      `Server` structs.
- [ ] **5.8.5** Implement `Response.ok`, `.json`, `.status`, `.redirect`.
- [ ] **5.8.6** Implement `Router` builder: `.get()`, `.post()`, `.put()`,
      `.delete()`, `.use()`.
- [ ] **5.8.7** Implement `Server.new()`, `.withPort()`, `.withHost()`,
      `.withThreads()`, `.listen()`.
- [ ] **5.8.8** Create `tests/fuse/stdlib/ext/http_server_test.fuse`.
- [ ] **5.8.9** Run tests. Fix any compiler bugs found.

---

## Task Summary

| Wave | Phases | Modules | Tasks |
|------|--------|---------|-------|
| **0 — Compiler Foundation** | 0.1–0.12 | — | 84 |
| **1 — Core** | 1.1–1.11 | result, option, bool, int, float, math, fmt, string, list, map, set | 107 |
| **2 — Full I/O & System** | 2.1–2.8 | io, path, os, env, sys, time, random, process | 69 |
| **3 — Full Networking** | 3.1–3.3 | net, json, http | 29 |
| **4 — Full Concurrency** | 4.1–4.4 | chan, shared, timer, simd | 31 |
| **5 — Ext** | 5.1–5.8 | test, log, regex, toml, yaml, json_schema, crypto, http_server | 62 |
| **Total** | **38 phases** | **34 modules** | **382 tasks** |

---

## Compiler Bug Log

Bugs discovered during implementation are logged here with links to
fix commits.

| # | Wave | Description | Minimal Repro | Fix Commit |
|---|------|-------------|---------------|------------|
| | | | | |

---

*End of Fuse Standard Library Implementation Plan*
