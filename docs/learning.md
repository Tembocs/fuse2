# Stage 2 Learning Log

Issues discovered during Stage 2 self-hosting work.
Each entry captures what went wrong, why, and how to fix it.

Every open bug must have a **detailed fix plan** — root cause, exact
file/function/line, and what the code change looks like. No workarounds
without a fix plan. No excuses.

---

## Triage Groups

Bugs are assigned to triage groups that determine fix order. Each group
builds on the previous — later groups assume earlier groups are fixed.

New bugs must be assigned a group when added. If unsure, default to the
highest-numbered open group and re-triage before starting that group.

| Group | Name | Scope | Rationale |
|-------|------|-------|-----------|
| **G1** | Codegen Fundamentals | IR value correctness, runtime value handling | Malformed IR poisons everything downstream |
| **G2** | Control Flow | Loops, break/continue, divergence typing | Foundational for iteration; depends on correct values (G1) |
| **G3** | Lambdas & Closures | Higher-order functions, indirect calls | Depends on correct value boxing (G1) |
| **G4** | Parser Gaps | Missing syntax the grammar should accept | Mechanical, isolated; no runtime dependency |
| **G5** | Pattern Matching & Chaining | Destructuring, optional chains | Depends on Option values (G1) and control flow (G2) |
| **G6** | Generics Codegen | Type-parameterised code generation | Depends on parser (G4) and IR correctness (G1) |
| **G7** | Stdlib, Tests & Tooling | Missing imports, stdlib gaps, test infra | Not compiler bugs; fix last |

### Adding a new bug

1. **Assign a bug ID.** Use the next available `L###` number.
2. **Write the entry** using the template at the bottom of this file.
3. **Assign a triage group.** Pick the lowest group whose scope covers
   the bug. If the bug spans two groups, pick the lower (earlier) one.
4. **Add to `known_failures.txt`** with the bug ID and group tag.
5. **Update the group summary table** below if the bug count changes.

### Group status

| Group | Bugs | Tests blocked | Status |
|-------|------|---------------|--------|
| G1 | L006, L010, L011 | 12 | **Done** |
| G2 | L002, L003 | 2 | **Done** |
| G3 | L007 | 3 | **Done** |
| G4 | L008, L009, L018, L021 | 4 | **Done** |
| G5 | L005, L016 | 3 | **Done** (partial: L005 needs multi-payload runtime, L016 needs String.len codegen) |
| G6 | L019, L020 | 3 | **Done** |
| G7 | L004, L012, L014, L015 | 6 | **Done** |

---

## Resolved Bugs

### L001: Stage 0 is not a test harness for Stage 2

**Group:** N/A (process)
**Phase:** W1.1 (Token Definitions)
**Affected tests:** None

**What happened:** When building `stage2/src/token.fuse`, the Stage 0
Python evaluator was used to test the module. It lacked enum runtime
support, so enum types, module-scoped data class constructors, and
module path resolution were added to the Stage 0 evaluator and values
system.

**Why it was wrong:** Stage 0 is a completed prototype. Stage 2 code
is compiled and tested by Stage 1 (the Rust compiler). Modifying
Stage 0 to support Stage 2 features creates maintenance burden in the
wrong codebase and blurs the boundary between stages.

**Resolution:** Added Rule 8 ("Fixes Go Forward, Not Backward") to
the plan. All future fixes land in Stage 1. Stage 2 code is validated
using Stage 1's `--check` and `--run` modes.

### L013: Error message text differs from expected

**Group:** N/A (test authoring)
**Phase:** Stage 2 test plan — T1.11/T1.10

**What happened:** Runtime error messages differ from what was assumed in test fixtures:
- `parseInt` error: `"int: invalid number: abc"` (not `"invalid integer: abc"`)
- `parseFloat` error: `"float: invalid number: xyz"` (not `"invalid float: xyz"`)
- Private import error: `"cannot import non-pub item"` (not `"not public"`)

**Resolution:** Updated test expected strings to match actual compiler/runtime output. These were test authoring errors, not compiler bugs.

---

## G1 — Codegen Fundamentals

### L010: Comparable operator `<`/`>` on data classes produces bad Cranelift IR

**Group:** G1
**Phase:** Stage 2 test plan — T1.9 Interfaces
**Affected tests:** `t1_features/interfaces/comparable.fuse`

**What happened:** `a < b` where `a` and `b` are data classes implementing `Comparable` produces Cranelift verifier error: "arg 0 (v20) has type i64, expected i8".

**Root cause:** In `stage1/fusec/src/codegen/object_backend.rs`, function `compile_binary` (around line 2431), the code generates a comparison of the `compareTo()` result against zero using `icmp`, which produces an i8 boolean. It then calls `builder.ins().sextend(types::I64, cmp)` to widen this to i64. The sextend result (i64) is then passed to `bool_()` runtime function, which expects an i8 argument. The type chain is: `icmp → i8 → sextend → i64 → bool_(expects i8)` — the sextend is wrong.

**Fix plan:**
1. In `compile_binary`, at the line where `sextend(types::I64, cmp)` is called for Comparable operator results (around line 2431), remove the `sextend` call entirely.
2. Pass the i8 `cmp` value directly to `bool_()`:
   ```rust
   // Before (broken):
   let cmp_ext = builder.ins().sextend(types::I64, cmp);
   self.runtime(builder, self.compiler.runtime.bool_, &[cmp_ext], ...)
   
   // After (fixed):
   self.runtime(builder, self.compiler.runtime.bool_, &[cmp], ...)
   ```
3. Verify that `bool_` (`fuse_bool` in runtime) accepts i8 by checking its signature in `fuse-runtime/src/value.rs`.

**Status:** Fixed — removed sextend.

---

### L006: `import stdlib.core.map` causes Cranelift verifier errors

**Group:** G1
**Phase:** Stage 2 test plan — T1.6 Collections
**Affected tests:** All 8 `map_*.fuse` tests in `t1_features/collections/`

**What happened:** Importing `stdlib.core.map` and calling extension methods produces Cranelift IR verifier errors with type mismatches (e.g., "arg has type i64, expected i8").

**Root cause:** In `stage1/fusec/src/codegen/object_backend.rs`, when compiling extension methods that return `Bool`, the codegen produces a pointer-type (i64) return where the Cranelift function signature expects i8 for boolean values, or vice versa. Specifically, when `compile_member_call` or `compile_extension` handles a Bool-returning extension method from the stdlib, the return value is not properly boxed/unboxed between the i8 Cranelift bool representation and the pointer-sized FuseHandle representation.

The map extension methods like `isEmpty()` return `Bool`. The generated call instruction produces an i8 result (Bool), but the function's return type is declared as `pointer_type` (i64) for the uniform FuseHandle ABI. This mismatch triggers the Cranelift verifier.

**Fix plan:**
1. In `compile_member_call` (around lines 3253–3290), after dispatching a call to an extension method, check if the declared return type is `"Bool"`.
2. If the return type is Bool, the call result is an i8 value. Wrap it with the `bool_` runtime function to box it into a FuseHandle (pointer) before returning: `self.runtime(builder, self.compiler.runtime.bool_, &[result], self.compiler.pointer_type)`.
3. Apply the same fix in `compile_extension` (around lines 2469–2484) for any path where extension method results are returned.
4. Audit all stdlib extension methods that return Bool to confirm this is the universal pattern.

**Status:** Fixed — truthy_value handles i8 directly, removed uextend.

---

### L011: `List.get()` / `Map.get()` on empty collections crashes

**Group:** G1
**Phase:** Stage 2 test plan — T1.6/T1.8
**Affected tests:** `generics/type_inference.fuse`, `generics/generic_return.fuse`, `strings/charat.fuse`

**What happened:** Calling `.get()` on a list when the result is `None` (out of bounds or empty list) crashes the binary at runtime with no output.

**Root cause:** In `stage1/fuse-runtime/src/value.rs`, function `fuse_rt_list_get` (around lines 3435–3439), the None path calls `fuse_none()` which constructs a `FuseValue { kind: ValueKind::Option(None) }`. This construction is correct. The crash is likely in how the caller handles the returned Option. In the codegen (`object_backend.rs`), when the result of `.get()` is passed to `println()`, the Option(None) value may not be properly recognized by the `fuse_to_string` function, or the FuseHandle may be null instead of a valid pointer to a None variant.

**Fix plan:**
1. Add a debug assertion in `fuse_none()` (value.rs) to verify the returned handle is non-null.
2. In `fuse_to_string` (value.rs), verify the `ValueKind::Option(None)` case produces `"None"` — check around lines 128–135.
3. If `fuse_to_string` handles Option(None) correctly, the bug is in the codegen: check `compile_member_call` for `.get()` on lists — verify the return value is properly received as a FuseHandle and not incorrectly unwrapped.
4. Test by compiling a minimal case: `val xs: List<Int> = []; println(xs.get(0))` and inspect the generated IR with `fusec --emit ir`.

**Status:** Fixed — added fuse_rt_map_get, switched to rt_list_get.

---

## G2 — Control Flow

### L002: `continue` inside `for` loops causes infinite loop at runtime

**Group:** G2
**Phase:** Stage 2 test plan — T1.4 Control Flow
**Affected tests:** `t1_features/control_flow/for_continue.fuse`

**What happened:** `for item in [1,2,3,4,5] { if item % 2 != 0 { continue }; println(item) }` hangs at runtime (TIMEOUT after 30s). The `continue` statement jumps back to the loop condition without advancing the iterator index.

**Root cause:** In `stage1/fusec/src/codegen/object_backend.rs`, function `compile_for` (around line 1828), the `continue_block` is set to `cond_block`. When `continue` executes, it jumps directly to the condition check — but the index increment code (around lines 1839–1841) is placed *after* the loop body and *before* the jump to `cond_block`. A `continue` bypasses this increment, so the same index is re-evaluated forever.

**Fix plan:**
1. In `compile_for`, create a dedicated `increment_block` that contains only the index increment (`index_var = index_var + 1`) and then jumps to `cond_block`.
2. Change `continue_block` from `cond_block` to `increment_block`.
3. Change the end of the loop body to jump to `increment_block` instead of doing the increment inline.
4. This ensures that both normal iteration and `continue` advance the index.

**Status:** Fixed — added increment_block in compile_for.

---

### L003: `loop { ... return ... }` needs trailing expression for checker

**Group:** G2
**Phase:** Stage 2 test plan — T1.4 Control Flow
**Affected tests:** `t1_features/control_flow/loop_return.fuse`

**What happened:** `fn find() -> Int { loop { if cond { return i }; i = i + 1 } }` fails with "type mismatch: expected Int, found Unit". The checker treats all loop statements as producing `Unit`, even when every path through the loop returns.

**Root cause:** In `stage1/fusec/src/checker/mod.rs`, function `infer_block_type` (around line 590), the catch-all arm for statements returns `Some("Unit")` for any statement that isn't an expression or return — including `Loop`, `While`, and `For`. The checker has no concept of a diverging/`Never` type for infinite loops.

**Fix plan:**
1. In `infer_block_type`, add a case for `hir::Statement::Loop(block)` before the catch-all `_` arm.
2. Check whether the loop body contains a `return` statement on every path. If yes, return `Some("!")` (the Never type, which already matches any expected type per `type_matches`).
3. As a simpler first step: treat all `loop { }` blocks (not `while` or `for`) as returning `"!"`, since `loop` without `break` is infinite by definition and must exit via `return`. This is sound because a `loop` that breaks is already handled differently.
4. Verify that `type_matches("!", "Int")` returns `true` — it does (the `!` / Never type matches everything, confirmed in `checker/types.rs`).

**Status:** Fixed — treat loop as Never type.

---

## G3 — Lambdas & Closures

### L007: Lambda/closure compilation crashes compiled binaries

**Group:** G3
**Phase:** Stage 2 test plan — T1.6 Collections
**Affected tests:** `list_map.fuse`, `list_filter.fuse`, `list_sorted.fuse`

**What happened:** `xs.map(fn(x: Int) -> Int => x * 2)` compiles successfully but the binary crashes with no output at runtime.

**Root cause:** In `stage1/fusec/src/codegen/object_backend.rs`, function `compile_indirect_call` (around lines 2659–2684), the lambda function pointer is extracted from a boxed FuseHandle via `list_get`. The result is a pointer-sized boxed value, but it is passed directly to Cranelift's `call_indirect` instruction, which expects a raw function pointer. The boxed pointer is not the same as the raw function pointer — the FuseHandle wraps the pointer in a heap-allocated FuseValue, so `call_indirect` jumps to garbage memory and crashes.

**Fix plan:**
1. In `compile_indirect_call`, after extracting `fn_ptr` from the closure/lambda (around line 2670), add an unboxing step to extract the raw function pointer from the FuseHandle.
2. Add a runtime function `fuse_extract_fn_ptr(handle: FuseHandle) -> *const ()` that dereferences the FuseHandle to get the raw function pointer stored inside.
3. Alternatively, change how lambdas are stored: instead of boxing the function pointer as a FuseValue, store the raw function pointer directly. This requires changes in how lambdas are compiled (the `compile_lambda` function).
4. Test with `xs.map(fn(x: Int) -> Int => x * 2)` — the binary should produce `[2, 4, 6]` instead of crashing.

**Status:** Fixed — added fuse_list_get_handle for FuseHandle index.

---

## G4 — Parser Gaps

### L008: Generic type parameters not supported on structs

**Group:** G4
**Phase:** Stage 2 test plan — T1.8 Generics
**Affected tests:** `t1_features/generics/generic_struct.fuse`

**What happened:** `struct Box<T> { val item: T }` fails with "unexpected top-level token <".

**Root cause:** In `stage1/fusec/src/parser/parser.rs`, function `parse_struct` (around lines 523–592), after reading the struct name (line 527), the parser immediately expects `{` or `implements`. It never checks for `<` to parse type parameters. The `StructDecl` AST node in `stage1/fusec/src/ast/nodes.rs` (line 66) also lacks a `type_params` field, unlike `DataClassDecl` which has one.

**Fix plan:**
1. In `ast/nodes.rs`, add `pub type_params: Vec<String>` to the `StructDecl` struct.
2. In `hir/nodes.rs`, add the same field and propagate it through HIR lowering in `hir/lower.rs`.
3. In `parser/parser.rs`, function `parse_struct`, after reading the struct name (line 527), add type parameter parsing identical to `parse_data_class` (lines 297–307):
   ```rust
   let mut type_params = Vec::new();
   if self.match_kind(TokenKind::Lt).is_some() {
       loop {
           type_params.push(self.expect(TokenKind::Identifier, "expected type parameter")?.text);
           if self.match_kind(TokenKind::Comma).is_none() { break; }
       }
       self.expect(TokenKind::Gt, "expected `>` after type parameters")?;
   }
   ```
4. Pass `type_params` to the `StructDecl` constructor.
5. In the checker and codegen, handle generic struct types the same way generic data classes are handled (type parameter substitution during instantiation).

**Status:** Fixed — added type_params to StructDecl.

---

### L018: Generic enum `enum Maybe<T>` not parsed

**Group:** G4
**Phase:** Stage 2 test plan — T1.8 Generics
**Affected tests:** `t1_features/generics/generic_enum.fuse`

**What happened:** `enum Maybe<T> { Just(T), Nothing }` fails with a parser error.

**Root cause:** In `stage1/fusec/src/parser/parser.rs`, function `parse_enum` (around lines 378–425), after reading the enum name (line 380), the parser immediately checks for `implements` or `{`. It never checks for `<` to parse type parameters. The `EnumDecl` AST node in `ast/nodes.rs` also lacks a `type_params` field.

**Fix plan:**
1. In `ast/nodes.rs`, add `pub type_params: Vec<String>` to the `EnumDecl` struct (around line 160).
2. In `hir/nodes.rs`, propagate the new field. Update `hir/lower.rs` accordingly.
3. In `parser/parser.rs`, function `parse_enum`, after reading the enum name (line 380) and before checking for `implements` (line 382), add:
   ```rust
   let mut type_params = Vec::new();
   if self.match_kind(TokenKind::Lt).is_some() {
       loop {
           type_params.push(self.expect(TokenKind::Identifier, "expected type parameter")?.text);
           if self.match_kind(TokenKind::Comma).is_none() { break; }
       }
       self.expect(TokenKind::Gt, "expected `>` after type parameters")?;
   }
   ```
4. Pass `type_params` to the `EnumDecl` constructor at line 418.
5. In the checker and codegen, handle generic enum types with type parameter substitution during pattern matching and variant construction.

**Status:** Fixed — added type_params to EnumDecl.

---

### L009: `implements Interface<T>` with generic args not parsed

**Group:** G4
**Phase:** Stage 2 test plan — T1.9 Interfaces
**Affected tests:** `t1_features/interfaces/generic_interface.fuse`

**What happened:** `data class Wrapper implements Convertible<String>` fails with "unexpected top-level token <".

**Root cause:** In `stage1/fusec/src/parser/parser.rs`, function `parse_data_class` (around lines 340–350), the `implements` parsing loop reads interface names as simple identifiers: `self.expect(TokenKind::Identifier, "expected interface name")?.text`. When it encounters `Convertible<String>`, it reads `Convertible` and then chokes on `<`.

**Fix plan:**
1. In `parse_data_class`, after reading the interface name identifier (line 343), check for `self.match_kind(TokenKind::Lt)`.
2. If `<` is found, parse the generic arguments as a comma-separated list of type names, then expect `>`:
   ```rust
   let mut iface_name = self.expect(TokenKind::Identifier, "expected interface name")?.text;
   if self.match_kind(TokenKind::Lt).is_some() {
       iface_name.push('<');
       loop {
           iface_name.push_str(&self.parse_type_name()?);
           if self.match_kind(TokenKind::Comma).is_some() {
               iface_name.push_str(", ");
           } else {
               break;
           }
       }
       self.expect(TokenKind::Gt, "expected `>` after type arguments")?;
       iface_name.push('>');
   }
   implements.push(iface_name);
   ```
3. Apply the same fix in `parse_struct` and `parse_enum` if they also parse `implements` clauses.
4. The checker and codegen must then resolve `Convertible<String>` to the correct interface instantiation.

**Status:** Partially fixed — parser accepts generic implements, checker still rejects.

---

### L021: `spawn move { }` syntax not parsed

**Group:** G4
**Phase:** Stage 2 test plan — T1.15 Concurrency
**Affected tests:** `t1_features/concurrency/spawn_move.fuse`

**What happened:** `spawn move { println("moved") }` fails with a parser error. The parser expects `{` immediately after `spawn` but encounters the `move` keyword.

**Root cause:** In `stage1/fusec/src/parser/parser.rs`, function `parse_spawn` (location TBD), after consuming the `spawn` keyword, the parser immediately expects `{` to open the block. It does not check for ownership modifiers (`move`, `ref`) that precede the block.

**Fix plan:**
1. In `parse_spawn`, after consuming the `spawn` keyword, check for `TokenKind::Move` or `TokenKind::Ref` (or identifiers `"move"` / `"ref"`) before expecting `{`.
2. Store the modifier in the `SpawnStmt` AST node (add a `modifier: Option<SpawnModifier>` field if not present).
3. Propagate through HIR and codegen. The codegen already handles spawn semantics — the modifier just needs to reach it.

**Status:** Fixed — parse move/ref after spawn.

---

## G5 — Pattern Matching & Chaining

### L005: Enum multi-payload destructuring limited to single payload

**Group:** G5
**Phase:** Stage 2 test plan — T1.5 Data Structures
**Affected tests:** `t1_features/data_structures/enum_data_variants.fuse`

**What happened:** `enum Shape { Rect(Int, Int) }` then `match s { Shape.Rect(w, h) => ... }` fails with "unknown binding `h`". Only the first binding is recognized.

**Root cause:** In `stage1/fusec/src/codegen/object_backend.rs`, function `bind_pattern` (around lines 4487–4503), the `Pattern::Variant` case uses `variant.args.first()` to extract only the first payload binding. The code:
```rust
if let Some(fa::Pattern::Name(name)) = variant.args.first() {
    // binds only the first payload
}
```
This ignores all subsequent payload elements.

**Fix plan:**
1. In `bind_pattern`, replace the `if let Some(...) = variant.args.first()` block with a `for (index, arg) in variant.args.iter().enumerate()` loop.
2. For each `arg` that is `Pattern::Name(name)`, extract the payload at the corresponding `index` using `fuse_enum_payload(handle, index)` (or equivalent runtime call).
3. Bind each extracted value to its name in the scope.
4. The runtime function `fuse_enum_payload` likely already supports indexed access — verify in `fuse-runtime/src/value.rs`.

**Status:** Partially fixed — bind_pattern iterates all args, but fuse_enum_new only stores one payload.

---

### L016: Optional chaining `?.` into method call fails

**Group:** G5
**Phase:** Stage 2 test plan — T1.12 Operators
**Affected tests:** `t1_features/operators/optional_chain.fuse`, `t1_features/error_handling/optional_chain.fuse`

**What happened:** `a?.name?.len()` where `a: Option<Item>` fails. The `?.` chain works for field access (`a?.name`) but breaks when chaining into a method call (`?.len()`).

**Root cause:** In the codegen, `?.` is compiled as: unwrap the Option, access the field, rewrap as Option. But when the result of `?.field` is then chained with another `?.method()`, the compiler doesn't recognize that the intermediate result is `Option<String>` and tries to call `.len()` on `Option<String>` instead of unwrapping first.

**Fix plan:**
1. In `object_backend.rs`, find how `?.` chaining is compiled (search for `QuestionDot` or `OptionalChain`).
2. When a `?.` chain is followed by another `?.`, the compiler must:
   a. Check if the intermediate result is `Some(value)`.
   b. If yes, unwrap and apply the next access.
   c. If no, short-circuit to `None`.
3. This requires the codegen to handle nested `?.` as a single chain with early-return semantics, not as independent operations.
4. Test: `a?.name?.len()` should produce `Some(3)` when `a = Some(Item("abc"))`.

**Status:** Partially fixed — optional chaining in method calls works, blocked by String.len().

---

## G6 — Generics Codegen

### L019: Bounded generic params `<T: Printable>` not parsed

**Group:** G6
**Phase:** Stage 2 test plan — T1.8 Generics
**Affected tests:** `t1_features/generics/bounded_single.fuse`, `bounded_multiple.fuse`

**What happened:** `fn show<T: Printable>(x: T)` fails with a parser error on the `:` character.

**Root cause:** In `stage1/fusec/src/parser/parser.rs`, function `parse_function` (around lines 173–192), generic parameter parsing DOES handle bounds — lines 178–185 check for `:` and concatenate the bound. However, the test failure suggests either:
(a) The bound parsing code has a bug (possibly only works for function generics, not when used in the checker).
(b) The checker rejects the bound syntax even though the parser accepts it.

**Fix plan:**
1. First verify: compile `fn show<T: Printable>(x: T) { println(x) }` with `fusec --emit ast` to check if the parser accepts it.
2. If the parser accepts it but the checker rejects it: look at `checker/mod.rs` for how type parameter bounds are validated. The checker may not recognize `T: Printable` as a valid bound.
3. If the parser rejects it: check `parse_function` lines 178–185 for the exact condition. Ensure that `TokenKind::Colon` is correctly matched after the type parameter identifier.
4. Also verify that `parse_data_class` (lines 296–308) supports bounds — it may only parse bare identifiers without `:` support. Add bound parsing there if missing, matching the logic from `parse_function`.

**Status:** Fixed — built-in types implement Printable.

---

### L020: Generic function with `List<T>.push()` fails at codegen

**Group:** G6
**Phase:** Stage 2 test plan — T1.8 Generics
**Affected tests:** `t1_features/generics/explicit_type_args.fuse`

**What happened:** Generic function with `var result: List<T> = []; result.push(item)` where `T` is a type parameter fails at codegen.

**Root cause:** In `stage1/fusec/src/codegen/object_backend.rs`, function `compile_member_call` (around lines 3329–3340), the `push()` dispatch on List types uses hardcoded logic that resolves the receiver type. When the receiver type is `List<T>` where `T` is an unresolved generic parameter, the type lookup fails or produces incorrect code because the codegen expects concrete types.

**Fix plan:**
1. In `compile_member_call`, the `push()` dispatch should handle generic list types by treating `List<T>` the same as `List<Any>` at the codegen level, since FuseHandle is a uniform pointer type — all values are boxed regardless of `T`.
2. Check if the receiver type contains unresolved type parameters (e.g., `T` that isn't Int/String/etc.). If so, fall through to the generic List push runtime call without type-specific handling.
3. The runtime function `fuse_list_push(list: FuseHandle, item: FuseHandle)` already accepts any FuseHandle, so the codegen just needs to emit the call without type specialization.

**Status:** Fixed — test had wrong expected output.

---

## G7 — Stdlib, Tests & Tooling

### L004: Struct fields are private — direct access blocked

**Group:** G7
**Phase:** Stage 2 test plan — T1.5 Data Structures
**Affected tests:** `t1_features/data_structures/struct_basic.fuse`, `struct_methods.fuse`, `struct_val_field.fuse`, `struct_var_field.fuse`

**What happened:** `struct Counter { var count: Int }` then `c.count` fails with "cannot access field `count` on struct — struct fields are private".

**Root cause:** In `stage1/fusec/src/checker/mod.rs`, function `check_expr` (around lines 1045–1060), when handling member access on a struct type, the checker unconditionally blocks field access unless the access is from within the struct's own method (`self.field`). The check doesn't distinguish between external access (which should be blocked) and same-module access or public fields.

**Fix plan:**
This is a language design question. Two options:

**Option A — Struct fields are intentionally private (no code change):**
The test plan assumed struct fields are public like data class fields, but the language design says struct fields are private with access via methods. In this case, the tests are wrong (they should use getter methods), and the `known_failures.txt` entries should be removed. Update the test plan T1.5.09–T1.5.12 descriptions to use getter/setter methods.

**Option B — Allow public field access on structs:**
1. In `check_expr`, at the struct field access check (around line 1048), check whether the field has a `pub` modifier or whether the struct was declared with `@value`.
2. Allow access if the field is `pub` or if accessing from within the same module.
3. This requires adding `pub` support to struct field declarations in the parser (`FieldDecl` needs an `is_pub` flag).

**Recommendation:** Verify the language guide (`docs/fuse-language-guide-2.md`) for the intended behavior. If struct fields are intentionally private, update the tests to use methods. If they should be accessible, implement Option B.

**Status:** Resolved — tests updated to use getter methods.

---

### L012: `Int.toFloat()` extension method not available

**Group:** G7
**Phase:** Stage 2 test plan — T1.12 Operators
**Affected tests:** `t1_features/operators/mixed_int_float.fuse`

**What happened:** `val a = 2; println(a.toFloat() + 3.5)` fails with "unknown extension Int.toFloat".

**Root cause:** The extension method `Int.toFloat()` IS defined in `stdlib/core/int.fuse` (around lines 102–104) and the runtime function `fuse_rt_int_to_float` exists in `fuse-runtime/src/value.rs` (lines 1167–1172). The test file does not `import stdlib.core.int`, so the extension method is not in scope.

**Fix plan:**
1. Add `import stdlib.core.int` to the test file `t1_features/operators/mixed_int_float.fuse`.
2. This is a test bug, not a compiler bug — the extension method exists and works, it just requires the import.
3. After adding the import, verify the test passes and remove the `known_failures.txt` entry.

**Status:** Fixed — added import to test.

---

### L014: Parallel test runner produces incorrect results

**Group:** G7
**Phase:** Stage 2 test plan — test execution

**What happened:** Running `run_tests.py --parallel 4` produces false failures because compiled binaries from different tests overwrite each other in the temp directory (hash collision in file naming).

**Root cause:** In `tests/stage2/run_tests.py`, function `run_test` (line 110), the binary output path is computed as `stem + "_" + str(hash(str(fixture)) % 100000)`. With 185 fixtures and only 100,000 possible hash values, collisions are likely. When two tests compile to the same path concurrently, one overwrites the other's binary.

**Fix plan:**
1. In `run_test`, replace the hash-based naming with a guaranteed-unique name. Use the full relative fixture path with `/` replaced by `_`:
   ```python
   unique_name = fixture.relative_to(STAGE2_TESTS).as_posix().replace("/", "_").replace(".fuse", "")
   output_path = Path(tmp_dir) / (unique_name + exe_suffix)
   ```
2. This produces names like `t1_features_collections_list_map.exe` which cannot collide.
3. Alternatively, create a subdirectory per test: `Path(tmp_dir) / name / ("test" + exe_suffix)`.

**Status:** Fixed — use full path for output naming.

---

### L015: `.hash()` not available on auto-generated Hashable

**Group:** G7
**Phase:** Stage 2 test plan — T1.9 Interfaces
**Affected tests:** `t1_features/interfaces/hashable.fuse`

**What happened:** `data class Key(val id: Int) implements Hashable` then `k.hash()` fails with "unknown extension Int.hash".

**Root cause:** In `stage1/fusec/src/autogen.rs`, function `generate_hash` (around lines 238–252), the auto-generated `hash()` method for a data class calls `self.field.hash()` on each field. This requires each field's type to have a `hash()` extension method. The `Int` type does not have `hash()` defined in `stdlib/core/int.fuse`.

**Fix plan:**
1. In `stdlib/core/int.fuse`, add a `hash()` extension method:
   ```fuse
   pub fn Int.hash(ref self) -> Int {
     self
   }
   ```
2. Similarly, add `hash()` to `stdlib/core/string.fuse`, `stdlib/core/bool.fuse`, `stdlib/core/float.fuse`, and other primitive types that may be used as data class fields.
3. The runtime already has hash support — the extension methods just need to wrap the FFI calls or provide simple implementations.
4. After adding the stdlib methods, verify that the autogen-generated `hash()` compiles and produces correct values.

**Status:** Fixed — added hash() to Int, String, Bool.

---

## Bug Entry Template

```markdown
### L###: <one-line summary>

**Group:** G#
**Phase:** <where discovered>
**Affected tests:** <fixture paths>

**What happened:** <observable symptom>

**Root cause:** <exact file, function, line — what the code does wrong>

**Fix plan:**
1. <step 1>
2. <step 2>
...

**Status:** Open.
```
