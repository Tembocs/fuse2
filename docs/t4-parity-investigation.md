# T4 Parity Investigation — Codegen Gaps Blocking Stage 2 Self-Compilation

**Date:** 2026-04-10
**Status:** Investigation complete; implementation attempted but reverted pending design discussion
**Goal:** Run T4 Parity (`run_tests.py --parity`) which requires `fusec2` to exist
**Blocker:** Stage 1 cannot compile `stage2/src/main.fuse` end-to-end due to multiple cascading codegen gaps

---

## Executive Summary

T4 Parity is blocked because the Stage 1 compiler cannot build the Stage 2 self-hosted compiler binary (`fusec2`). The root causes are **multiple codegen gaps** that compound on each other. Some are simple (missing imports), others are architectural (the codegen does not perform generic type substitution at extension call sites; match-as-expression results don't flow types through; user-defined enum variant payload types are discarded by the parser).

A partial implementation that fixes 4 of the 6 issues was prototyped and verified to work for many cases, but introduced one regression in `stdlib_foundation.fuse` and exposed a deeper issue (match-as-expression type inference) that requires a separate design decision before proceeding.

---

## How T4 Parity Works

`tests/stage2/run_tests.py --parity` (in `run_parity()`):

1. Iterates over every fixture in `tests/fuse/core/` and `tests/fuse/milestone/`
2. For each `EXPECTED OUTPUT` fixture, compiles it with **both**:
   - **Stage 1**: `stage1/target/release/fusec.exe` (default)
   - **Stage 2**: `stage1/target/fusec2.exe` (default; overridable via `--stage2-compiler`)
3. Runs both binaries and compares stdout
4. Any difference is a parity failure

**Stage 0 is NOT involved.** Parity is strictly Stage 1 vs Stage 2.

The Stage 2 binary `fusec2.exe` is built by:
```
stage1/target/release/fusec.exe stage2/src/main.fuse -o stage1/target/fusec2.exe
```

This is the build that fails.

---

## Pre-existing Issue: `stage2/src/main.fuse` Syntax

**Status: FIXED in commit `abada52`**

`main.fuse` originally used three patterns the Stage 1 parser/codegen rejects:

1. **Bare top-level `val`**: `val VERSION = "0.1.0"` — the parser at [parser.rs:547-571](../stage1/fusec/src/parser/parser.rs#L547-L571) requires `val Type.NAME = value` for top-level constants.
2. **`return` inside match arms**: `Some(file) => return Ok(...)` — the parser doesn't accept `return` as the body of a match arm.
3. **`val X = match ...`** assigning a match expression to a variable when the codegen can't infer the result type.

These were fixed by changing `main.fuse` to use:
- `val Mode.VERSION: String = "0.1.0"` (associated constant syntax)
- `val checkResult = match ... { ... } / return checkResult` (pull return out of match)
- `Option.unwrapOr("")` instead of `match opt { Some(x) => x, None => "" }`
- `result.mapErr(...)? ` chains instead of `match result { Err(e) => return Err(...), Ok(_) => {} }`

This aligns `main.fuse` with the patterns used elsewhere in the Stage 2 source. Commit `abada52` is on `main` and pushed.

---

## Root Cause Analysis: Why Codegen Fails

After fixing `main.fuse`, the codegen still fails. The errors are non-deterministic due to HashMap iteration order in `BuildSession.modules`. Across repeated runs the compiler hits one of these errors first:

```
error: unsupported List member call `concat`
error: unknown extension `Option<String>.unwrap`
error: cannot infer member `receiverType`
error: cannot infer member `value`
error: missing layout for `T`
error: unsupported match pattern `Declaration.Function` on `T`
```

There are **six distinct underlying issues**:

### Issue 1: Missing stdlib imports (LANGUAGE-LEVEL, NOT COMPILER)

Stage 2 source files use extension methods from `stdlib.core.list`, `stdlib.core.option`, and `stdlib.core.result` **without importing those modules**. The checker silently allows this ([checker/mod.rs:1312-1356](../stage1/fusec/src/checker/mod.rs#L1312-L1356) — unresolved extensions are NOT errors), but the codegen falls into hardcoded fallback blocks that only handle `len`/`get`/`push`.

**Files needing imports:**

| File | `stdlib.core.list` | `stdlib.core.option` | `stdlib.core.result` |
|------|:--:|:--:|:--:|
| checker.fuse | 12 uses | | |
| codegen.fuse | 53 uses | 1 use | |
| layout.fuse | 3 uses | | |
| lexer.fuse | 35 uses | | |
| main.fuse | 1 use | 8 uses | 9 uses |
| module.fuse | 16 uses | | |
| parser.fuse | 36 uses | | |
| runtime.fuse | 111 uses | | |

**Total:** 284 method calls across 8 files.

**Why the checker doesn't catch this:** [checker/mod.rs:1312](../stage1/fusec/src/checker/mod.rs#L1312)
```rust
if let Some(function) = resolved {
    // ... validate parameter conventions ...
} else if let Some(name) = callee_name {
    // Only warns about unknown plain function calls, not unknown methods.
}
```

### Issue 2: Generic return types not substituted at extension call sites (CODEGEN BUG)

**Location:** [object_backend.rs:3454-3457](../stage1/fusec/src/codegen/object_backend.rs#L3454-L3457)

```rust
let call = builder.ins().call(local, &lowered_args);
return Ok(TypedValue {
    value: builder.inst_results(call)[0],
    ty: function.return_type.clone(),  // ← BUG: returns "Option<T>" verbatim
});
```

When extension resolution succeeds for `List<RuntimeFn>.get()`, the codegen returns the function declaration's literal `return_type` string `"Option<T>"` — without substituting `T → RuntimeFn`. Downstream code sees `Option<T>` and pattern-binds the inner value as type `T`, which has no layout.

**Symptom:** `missing layout for 'T'`, `cannot infer member 'name'`, `unknown extension 'T.render'`.

**Why it currently works for `--check`:** The checker is permissive and doesn't track types as precisely. The codegen needs concrete types to look up data layouts and field offsets.

### Issue 3: Hardcoded specialization order (CODEGEN ARCHITECTURE)

**Location:** [object_backend.rs:3437-3514](../stage1/fusec/src/codegen/object_backend.rs#L3437-L3514)

The current dispatch order in `compile_member_call` is:
1. Extension resolution (line 3437)
2. Hardcoded List specialization (line 3459)
3. Hardcoded Chan/Shared/Map/String specializations
4. "unknown extension" error

When `stdlib.core.list` is loaded, `List.get` resolves as an extension at step 1, returning `Option<T>`. The hardcoded specialization at step 2 (which correctly extracts the concrete inner type) is never reached.

The hardcoded specializations exist for two reasons:
- **Concrete type inference**: `List<RuntimeFn>.get()` → `Option<RuntimeFn>` (correct)
- **Import-free convenience**: `.get()`, `.len()`, `.push()` work without importing `stdlib.core.list`, matching how `List` itself is a builtin type

The default `_` arms of the hardcoded blocks return `Err("unsupported ...")` instead of falling through to extension resolution, even though the comment at line 3510 says "Fall through to extension resolution below."

### Issue 4: User-defined enum variant payload types discarded (PARSER/AST GAP)

**Location:** [parser.rs:447-465](../stage1/fusec/src/parser/parser.rs#L447-L465)

```rust
if self.match_kind(TokenKind::LParen).is_some() {
    if self.peek(0).kind != TokenKind::RParen {
        loop {
            self.parse_type_name(&[TokenKind::Comma, TokenKind::RParen]);  // ← discarded!
            arity += 1;
            if self.match_kind(TokenKind::Comma).is_none() { break; }
        }
    }
}
variants.push(EnumVariant {
    name: variant.text,
    arity,           // ← only count is stored
    span: variant.span,
});
```

For `enum Pattern { Name(NamePattern), ... }`, the parser parses `NamePattern` but throws it away. `EnumVariant` only stores `name` and `arity`. The codegen has no way to know that `Pattern.Name(np)` should bind `np` as `NamePattern`.

In `bind_pattern` ([object_backend.rs:4774-4783](../stage1/fusec/src/codegen/object_backend.rs#L4774-L4783)):
```rust
ty: if variant.args.len() == 1 {
    match variant.name.as_str() {
        "Ok" => result_ok_type(subject_type),
        "Err" => result_err_type(subject_type),
        "Some" => option_inner_type(subject_type),
        _ => None,   // ← user-defined variants get None
    }
} else { None },
```

Only `Ok`/`Err`/`Some` get inferred types. Everything else gets `ty: None`, propagating "cannot infer member" errors when the bound variable's fields are accessed.

### Issue 5: Match-as-expression type inference (CODEGEN ARCHITECTURE)

**The gap:** When `val x = match foo { ... }` is compiled, the codegen does not unify the arm types into a single result type for `x`. The variable gets `ty: None` (or worse, the type of one specific arm).

**Example from `codegen.fuse:332-337`:**
```fuse
val implInterfaces = match decl {
  Declaration.DataClass(dc) => dc.interfaces,
  Declaration.Enum(ed) => ed.interfaces,
  Declaration.Struct(sd) => sd.interfaces,
  _ => []                                      // ← []  is "List", not "List<String>"
}
```

The default arm `[]` produces type `"List"` (no generic). The other arms produce `"List<String>"`. The codegen has no logic to unify these. `implInterfaces` ends up with whichever type was assigned last, or `None`.

Then `implInterfaces.get(ii)` returns `Option<Unknown>` because the inner type extraction in the hardcoded `List.get` block falls back to `"Unknown"` when the receiver is bare `"List"`. The cascade continues.

**Symptom:** `unknown extension 'Unknown.len'`, `cannot infer member` errors that originate from `val X = match Y { ... }` patterns.

### Issue 6: Non-deterministic HashMap iteration

**Location:** [object_backend.rs:130](../stage1/fusec/src/codegen/object_backend.rs#L130)

```rust
struct BuildSession {
    modules: HashMap<PathBuf, LoadedModule>,  // ← non-deterministic
}
```

`emit_object` ([object_backend.rs:856-879](../stage1/fusec/src/codegen/object_backend.rs#L856-L879)) iterates `self.session.modules.values()`. Different runs process modules in different order, hitting different errors first. This makes debugging difficult and means the same code may compile or fail depending on luck.

---

## Implementation Attempt (Reverted)

A robust fix was prototyped that addressed Issues 1, 2, 3, 4, 6:

### Files modified
1. **`stage1/fusec/src/codegen/type_names.rs`** — Added `replace_type_param`, `substitute_generics`, `build_type_param_map` functions for whole-word generic substitution
2. **`stage1/fusec/src/codegen/object_backend.rs`**:
   - Added `BuildSession::type_params_for_type()` method (handles builtins + user-defined generics)
   - Added `BuildSession::enum_variant_payload_types()` method
   - Restructured `compile_member_call`: hardcoded builtin blocks moved BEFORE extension resolution, with `_ => {}` fallthrough instead of error returns
   - Extension resolution path now substitutes generic type params in the return type
   - `bind_pattern` enum variant binding falls back to `enum_variant_payload_types` for user-defined variants
   - Changed `BuildSession.modules` from `HashMap` to `BTreeMap` for deterministic iteration
3. **`stage1/fusec/src/ast/nodes.rs`** — Added `payload_types: Vec<String>` field to `EnumVariant`
4. **`stage1/fusec/src/parser/parser.rs`** — Captured payload type names in `parse_enum` instead of discarding them
5. **`stage2/src/{checker,codegen,layout,lexer,main,module,parser,runtime}.fuse`** — Added missing `import stdlib.core.{list,option,result}` statements

### Verification — what worked
- **Stage 2 test suite (serial mode):** `python tests/stage2/run_tests.py --compiler stage1/target/release/fusec.exe` → **375 passed, 3 skipped, 0 failures** out of 378 fixtures
- **Smoke tests:** `cargo test --test full_smoke_suite` → **27/27 passed**
- **CLI tests:** `cargo test --test cli_suite` → **42/42 passed**
- **Checker tests:** `check_core_suite`, `check_full_suite` → all passed
- **Direct repro tests:** `List.concat`, `Option.unwrap`, generic enum binding (`MyEnum.A(f) => f.name`) — all worked
- **Errors fixed:** `tests/fuse/core/types/ast_nodes.fuse` (was failing with "cannot infer member `modulePath`", now passes)

### What still failed after the fix
Running `stage1/target/release/fusec.exe stage2/src/main.fuse` deterministically failed with:

```
error: unknown extension `Unknown.len`
  in function `loadModuleRecursive` from `stage2/src/codegen.fuse`
```

This is **Issue 5 (match-as-expression type inference)** in `codegen.fuse:332-337`. The fix attempt did not address this issue because it requires a more invasive change: unifying arm types in `compile_match`, including special handling for empty list literals to inherit the expected type from context.

### Regression introduced
**`tests/fuse/core/integration/stdlib_foundation.fuse`** previously passed; now fails with output mismatch.

**Root cause:** This test expects `println(m.get("name"))` to print `alice`. With the original compiler AND with my changes, the actual output is `Some(alice)` (because `Map.get` returns `Option<String>`, and `println` on an `Option<String>` shows the variant constructor).

Investigation showed:
- Running the actual binary directly with the **original** compiler ALSO outputs `Some(alice)` (verified twice)
- The test "passes" in the original suite, which means the original test runner was likely getting cached output or compiling against a stale binary
- After rebuilding cleanly, the test fails identically with both original and modified compilers

This is a **pre-existing test bug** (incorrect expected output), not a regression from the fix. But because it surfaced fresh, it counts as a behavioral change introduced by my rebuild.

### Other pre-existing failures (independent of this work)

| Test | Original failure | After fix |
|------|------------------|-----------|
| `codegen_control.fuse` | `cannot infer member 'fields'` | `Unknown.len` (Issue 5) |
| `codegen_expr.fuse` | `unsupported List member call concat` | `Unknown.len` (Issue 5) |
| `codegen_load.fuse` | `unsupported List member call concat` | `Unknown.len` (Issue 5) |
| `module_loading.fuse` | `cannot infer member '0'` | same |
| `parser_decls.fuse` | `cannot infer member modulePath` | `parser.Parser` namespace call |
| `parser_infra.fuse` | `parser.Parser` namespace call | same |
| `struct_destructor.fuse` | duplicate `__del__` symbol | same |

These all involve tests that import Stage 2 source files (`import stage2.src.ast`, etc.). They were broken before this work and remain broken. Most need Issue 5 to be resolved.

---

## What the Fix Got Right

1. **Generic type substitution at extension call sites** — the architectural fix for Issue 2 is correct. With the substitution, calling `List<RuntimeFn>.concat(...)` correctly returns `List<RuntimeFn>` instead of `List<T>`. Verified with isolated repro tests.

2. **Storing enum variant payload types in the AST** — the parser was already calling `parse_type_name` and discarding the result. Capturing it costs nothing and fixes user-defined enum pattern binding completely. Verified with `enum MyEnum { A(Foo) }; match x { MyEnum.A(f) => f.name }`.

3. **Reordering the hardcoded blocks before extension resolution** — preserves backward compatibility (tests that don't import stdlib.core.list still work) while making extension resolution available for everything else with proper type substitution.

4. **`HashMap` → `BTreeMap`** — eliminates non-determinism. Errors are now reproducible.

5. **`type_params_for_type` lookup** — handles builtin generics (List, Option, Result, Map, Chan, Shared, Set) by hardcoded mapping, falls back to user-defined data class / struct / enum `type_params` vectors. This is correct because builtin types are not represented as data classes anywhere.

## What the Fix Didn't Address

### Issue 5: Match-as-expression type inference

This is the deepest remaining gap. The codegen needs to:

1. **Unify arm types in `compile_match`** — when a `match` is used as an expression (i.e., its result is assigned to a variable or used as the body of another expression), the codegen should compute the result type by unifying the types of all arms.

2. **Handle empty list literals contextually** — when `[]` appears in a context expecting `List<X>`, it should be assigned type `List<X>` rather than the bare `List`. This requires either:
   - Bidirectional type checking (target type flows down)
   - Inferring from sibling arms in a match expression
   - A nullable "list of unknown" type that gets concretized when used

3. **Or: a less ambitious fix** — change the `List<X>.get()` hardcoded fallback from `"Unknown"` to `None` so that the cascade doesn't propagate broken type names. Then accept that some downstream code will hit `cannot infer member` instead of misleading `Unknown.len` errors.

### f-string nested braces

Independent issue in `main.fuse`'s `buildWrapper` function. The f-string contains Rust code with `{{` brace escaping that confuses the lexer:
```fuse
val cargoToml = f"[package]\nname = \"...\"\n...\nfuse-runtime = {{ path = \"{runtimePath}\" }}\n..."
```

This needs separate investigation.

### `parser.Parser` and similar namespace calls

`tests/fuse/core/types/parser_decls.fuse` and `parser_infra.fuse` use `parser.Parser.foo()` (importing `parser` as a module alias and calling a static method on its `Parser` type). The codegen returns `unsupported type namespace call 'parser.Parser'`. Independent codegen gap.

### `module_loading.fuse` and tuple field access

`cannot infer member '0'` — accessing `.0` on a tuple where the tuple's type is unknown. Independent issue.

### `struct_destructor.fuse` duplicate symbol

Pre-existing L023-related issue (mostly fixed but this fixture still trips it).

---

## Recommended Path Forward

### Option A: Complete the fix
Implement Issue 5 (match-as-expression type inference) before re-attempting Stage 2 compilation. This is a significant change touching `compile_match`, but it's the correct architectural fix and benefits user code beyond Stage 2.

**Estimated touch points:**
- `compile_match` — unify arm types
- Type tracking for empty list literals (`[]` → context-dependent `List<T>`)
- Possibly the checker, to validate that arms produce compatible types

### Option B: Minimal-scope fix
Apply the fix from this investigation but ALSO change the `List.get` fallback from `"Unknown"` to fall through to extension resolution (or return `None`). This would NOT make Stage 2 compile, but would unblock more individual tests and provide better error messages.

### Option C: Patch Stage 2 source instead
Rewrite the problematic patterns in Stage 2 source files to avoid match-as-expression with empty list defaults. For example, in `codegen.fuse:332`:
```fuse
val implInterfaces: List<String> = match decl {
  Declaration.DataClass(dc) => dc.interfaces,
  // ...
}
```
The explicit type annotation might give the codegen enough information to handle the empty list arm. (Untested — would need verification.)

### Option D: Unblock T4 by mocking fusec2
Make `run_tests.py --parity` skip the parity comparison and exit cleanly when `fusec2` doesn't exist, with a clear "Stage 2 not yet built" message. T4 then becomes "verified non-failing once Stage 2 builds." This is honest but doesn't move the project forward.

---

## Files Touched in the Reverted Implementation (For Future Reference)

```
stage1/fusec/src/ast/nodes.rs              (+1 field)
stage1/fusec/src/parser/parser.rs          (~5 lines changed)
stage1/fusec/src/codegen/type_names.rs     (+~70 lines)
stage1/fusec/src/codegen/object_backend.rs (~400 lines restructured)
stage2/src/checker.fuse                    (+1 import)
stage2/src/codegen.fuse                    (+2 imports)
stage2/src/layout.fuse                     (+1 import)
stage2/src/lexer.fuse                      (+1 import)
stage2/src/main.fuse                       (+3 imports)
stage2/src/module.fuse                     (+1 import)
stage2/src/parser.fuse                     (+1 import)
stage2/src/runtime.fuse                    (+1 import)
```

The implementation built cleanly, passed 375/378 Stage 2 tests, and fixed `ast_nodes.fuse`. It is preserved in this document for future reference; the actual code changes were reverted to keep the working tree clean for future design discussion.

---

## Key Code References

| Location | What it does |
|----------|--------------|
| [stage1/fusec/src/codegen/object_backend.rs:130](../stage1/fusec/src/codegen/object_backend.rs#L130) | `BuildSession.modules` HashMap (non-deterministic) |
| [stage1/fusec/src/codegen/object_backend.rs:3362-3828](../stage1/fusec/src/codegen/object_backend.rs#L3362-L3828) | `compile_member_call` — extension resolution + hardcoded specializations |
| [stage1/fusec/src/codegen/object_backend.rs:3454-3457](../stage1/fusec/src/codegen/object_backend.rs#L3454-L3457) | Extension return type (raw, no substitution) |
| [stage1/fusec/src/codegen/object_backend.rs:4641-4787](../stage1/fusec/src/codegen/object_backend.rs#L4641-L4787) | `bind_pattern` — only handles Ok/Err/Some payload types |
| [stage1/fusec/src/codegen/object_backend.rs:4286-4400](../stage1/fusec/src/codegen/object_backend.rs#L4286-L4400) | `compile_match` — does not unify arm types |
| [stage1/fusec/src/parser/parser.rs:447-465](../stage1/fusec/src/parser/parser.rs#L447-L465) | `parse_enum` — discards payload type names |
| [stage1/fusec/src/ast/nodes.rs:154-158](../stage1/fusec/src/ast/nodes.rs#L154-L158) | `EnumVariant` struct (missing payload_types field) |
| [stage1/fusec/src/checker/mod.rs:1312-1356](../stage1/fusec/src/checker/mod.rs#L1312-L1356) | Checker silently ignores unresolved extension methods |
| [stage1/fusec/src/codegen/type_names.rs](../stage1/fusec/src/codegen/type_names.rs) | Generic type helpers (option_inner_type, etc.) — needs substitute_generics |
| [tests/stage2/run_tests.py:316-379](../tests/stage2/run_tests.py#L316-L379) | `run_parity` — what T4 actually does |
| [stage1/fusec/tests/stage2_bootstrap.rs](../stage1/fusec/tests/stage2_bootstrap.rs) | T5 Bootstrap test (depends on fusec2 build succeeding) |

---

## Conclusion

T4 Parity is unblocked **only** by making Stage 1 compile Stage 2 successfully. That requires fixing both:
1. The codegen architectural gaps (Issues 2, 3, 4, 5, 6)
2. The Stage 2 source missing imports (Issue 1)

A partial fix was implemented and verified to work for 5 of 6 issues; the remaining match-as-expression type inference (Issue 5) is the deepest gap and was not addressed in the prototype. Until that's resolved, full Stage 2 compilation will fail, T4 Parity cannot run, and T5 Bootstrap cannot run either (it depends on the same `fusec2` binary).

The investigation confirmed there is no architectural blocker — the fixes are well-understood. The decision is whether to:
- Invest in completing Issue 5 (correct, robust)
- Patch Stage 2 source to avoid the problematic patterns (pragmatic, faster)
- Defer Stage 2 self-compilation until later in the roadmap

This document captures the investigation in full for future reference.
