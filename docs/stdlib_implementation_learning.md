# Fuse Standard Library — Compiler Bug & Learning Reference

> **Purpose:** This document records every compiler issue discovered and
> fixed during stdlib implementation. It exists so that future work
> (self-hosting, new language features, optimisation passes) can reference
> concrete cases where the compiler was wrong, understand the root cause,
> and avoid reintroducing the same class of bug.
>
> **Policy:** When a compiler bug is found during stdlib work, it is fixed
> immediately (per the Bug Policy in the implementation plan), and then
> documented here. Each entry includes: the symptom, the minimal
> reproduction, the root cause, and the fix. Entries are ordered by
> discovery time.

---

## Status Key

- Entries are numbered sequentially across all waves.
- Each entry links to the wave/phase where it was discovered.
- The "Category" column groups bugs for pattern analysis.

---

## Bug Reference

### Bug #1 — Zero-arg extension function dispatch (Wave 1, Phase 1.1)

**Symptom:** Calling a zero-argument extension function on a user-defined
type (e.g., `result.isOk()`) failed at compile time with
`"unsupported zero-arg member call"`.

**Minimal reproduction:**
```fuse
val r: Result<Int, String> = Ok(1)
println(r.isOk())
```

**Root cause:** `call_zero_arg_member` in the codegen only resolved
hardcoded built-in types (Chan, Map, String). User-registered extension
functions were not consulted for zero-argument calls.

**Category:** Extension function resolution

**Fix:** Extended `call_zero_arg_member` to look up user extension
functions in the same way multi-arg calls do — by checking the extension
function registry keyed on `(receiver_base_type, method_name)`.

---

### Bug #2 — Cranelift "block already filled" after return in match arm (Wave 1, Phase 1.1)

**Symptom:** A `match` arm containing a block body with an explicit
`return` statement caused a Cranelift panic:
`"block already filled"`.

**Minimal reproduction:**
```fuse
fn tryUnwrap(r: Result<Int, String>) -> Int {
  match r {
    Ok(v) => v
    Err(e) => { return 0 }
  }
}
```

**Root cause:** `compile_two_arm_match` and `compile_match` unconditionally
emitted a `runtime_nullary` (Unit value) after the match arm body, even
when the body already terminated with a `return` instruction. Cranelift
correctly rejected the unreachable instruction because the block's
terminator was already set.

**Category:** Codegen — unreachable code after early return

**Fix:** Added a check after compiling each match arm body: if the
current block is already sealed (has a terminator), skip emitting the
post-body unit value. This mirrors how `if/else` codegen already handles
early returns.

---

### Bug #3 — result.fuse shipped with concrete types instead of generic signatures (Wave 1, Phase 1.1)

**Symptom:** The initial `stdlib/core/result.fuse` implementation used
`Int` and `String` instead of generic type variables (`T`, `E`, `U`, `F`).
This meant the library only worked for `Result<Int, String>` — any other
instantiation would fail or silently produce wrong types.

**Root cause:** Not a compiler bug per se. The implementation was written
conservatively using concrete types, deviating from the spec signatures.
The compiler's generic extension function support (Phase 0.10) was
designed to handle type variable substitution in return types and callback
inference, but this had not been stress-tested with fully generic
parameter signatures.

**Category:** Spec conformance / generic type variables

**Fix:** Rewrote `result.fuse` to use `T`, `E`, `U`, `F` type variables
matching the spec signatures exactly. If the compiler's type variable
substitution does not handle parameter positions (not just return types),
that is a compiler bug to be filed and fixed under the Bug Policy.

---

### Bug #4 — unwrap returned 0 instead of panicking (Wave 1, Phase 1.1)

**Symptom:** `Result.unwrap()` on an `Err` value printed a message but
returned `0` instead of aborting. This violated the spec contract that
`unwrap` panics on `Err`.

**Root cause:** No panic mechanism existed in the stdlib. The never type
(`!`) and its trap instruction were implemented in Phase 0.7 but had not
been used to build a user-facing panic function.

**Category:** Missing language primitive

**Fix:** Added a `resultPanic(msg: String) -> !` helper function. This
prints the error message via `println`, then the never-type codegen emits
a Cranelift trap instruction that aborts the process. The `Err` arm of
`unwrap` calls `resultPanic`, whose return type `!` coerces to `T` via
the bottom-type rule (Phase 0.7.2).

**Learning:** The never type + trap instruction is the correct mechanism
for panic in Stage 1. Future stdlib modules that need panic (e.g.,
`Option.unwrap`, `List.get` out-of-bounds) should use the same pattern.
A shared `panic` function should be extracted to a common location once
multiple modules need it.

---

### Bug #5 — F-string interpolation silently drops method calls (Wave 1, Phase 1.2)

**Symptom:** Method calls inside f-string interpolation braces returned
the receiver value instead of the call result. For example,
`f"{s.isSome()}"` where `s = Some(42)` produced `"42"` (the inner value
of the receiver) instead of `"true"` (the method return value).

**Minimal reproduction:**
```fuse
import stdlib.core.option

@entrypoint
fn main() {
  val s = Some(42)
  println(f"isSome: {s.isSome()}")
  // Expected: isSome: true
  // Actual:   isSome: 42
}
```

**Root cause:** The evaluator's `interpolate` function used hand-rolled
string splitting (`expr.split('.')`) that only supported simple
`name.field` access chains. It could not handle method calls (the `()`
suffix), operators, or any other non-trivial expression. When it
encountered `s.isSome()`, it split on `.` to get `["s", "isSome()"]`,
looked up `s`, then tried to resolve `"isSome()"` as a field name on an
Option value. The field lookup fell through to a catch-all that returned
the current value (`42`, the inner value of `Some(42)`), silently
discarding the method call entirely.

**Category:** Evaluator — f-string expression evaluation

**Fix:** Replaced the hand-rolled `interpolate` function with one that
parses the interpolated expression as real Fuse code via `parse_source`
(wrapping it in `fn __fstr__() => EXPR`), then evaluates the parsed AST
through the normal `eval_expr` path. This ensures that method calls,
operators, nested calls, and all other expression forms work correctly
inside f-string braces.

**Learning:** Any time the evaluator needs to evaluate a sub-expression
from a string (f-strings, REPL input, etc.), it should go through the
real parser + evaluator pipeline rather than implementing a mini
expression language. The parser already handles all Fuse syntax
correctly — duplicating that logic in string-based form is both fragile
and incomplete.

---

### Bug #6 — Parser rejects keywords as member/method names (Wave 1, Phase 1.3)

**Symptom:** Calling `t.not()` on a Bool value produced a parse error:
`"expected member name after '.'"`. The method `not` could not be
defined or called as an extension function.

**Minimal reproduction:**
```fuse
pub fn Bool.not(ref self) -> Bool {
  if self { false } else { true }
}

@entrypoint
fn main() {
  val t = true
  println(t.not())
}
```

**Root cause:** The parser used `expect(TokenKind::Identifier)` in two
places: member access parsing (line 712) and extension function name
parsing (line 173). Since `not` is a keyword (`TokenKind::Not`), it was
rejected by `expect(Identifier)`. Any keyword used as a method name
would hit the same issue (`match`, `return`, `if`, etc.).

**Category:** Parser — keyword/identifier ambiguity

**Fix:** Changed both member access and extension function name parsing
to accept any token with non-empty text (using `self.take()` with an
EOF/empty check) instead of strictly requiring an Identifier token.
This allows keywords to be used as method names in member position,
which is standard in most languages (e.g., Kotlin's `.not()`,
Rust's `.match` on iterators).

**Learning:** Member/method names occupy a different syntactic position
than keywords in statement/expression position. The parser should
allow any word-like token as a member name, not just identifiers. This
is the same principle that allows field names to shadow keywords in
most languages.

---

### Bug #7 — Evaluator displays whole-number floats without `.0` (Wave 1, Phase 1.4)

**Symptom:** `42.toFloat()` printed `42` instead of `42.0`. Float values
that happen to be whole numbers were indistinguishable from integers.

**Minimal reproduction:**
```fuse
@entrypoint
fn main() {
  println(42.toFloat())
  // Expected: 42.0
  // Actual:   42
}
```

**Root cause:** The evaluator's `stringify` function used Rust's
`f64::to_string()` which produces `"42"` for `42.0_f64` (no decimal
point for whole numbers). This is standard Rust Display behavior but
violates Fuse's expectation that floats always show a decimal point.

**Category:** Evaluator — float display

**Fix:** Added a post-check in `stringify` for Float values: if the
string doesn't contain `.`, `NaN`, or `inf`, append `.0`.

---

### Bug #11 — Evaluator stack overflow on cross-module nested calls (Wave 2, Phase 2.2)

**Status: WORKAROUND — not fixed. If this recurs in Phase 2.3+, extract
the FFI dispatch into a separate function as the proper fix.**

**Symptom:** Calling a function in an imported module that internally
calls another function in the same module causes a stack overflow after
only 5 levels of nesting (e.g., `path.join` → `joinTwo` → `isAbsolute`
→ `strLen` → `fuse_rt_string_len`).

**Minimal reproduction:**
```fuse
// mymod.fuse
extern fn fuse_rt_string_len(s: String) -> Int
extern fn fuse_rt_string_char_at(s: String, index: Int) -> String
// ... (5+ extern declarations total)

fn strLen(s: String) -> Int { fuse_rt_string_len(s) }
fn charAt(s: String, i: Int) -> String { fuse_rt_string_char_at(s, i) }

pub fn isAbsolute(p: String) -> Bool {
  val len = strLen(p)
  if len == 0 { return false }
  charAt(p, 0) == "/"
}

fn joinTwo(base: String, part: String) -> String {
  if isAbsolute(part) { return part }
  f"{base}/{part}"
}

pub fn join(base: String, parts: String...) -> String {
  var result = base
  for part in parts { result = joinTwo(result, part) }
  result
}
```
```fuse
// test.fuse — overflows at runtime
import mymod
@entrypoint
fn main() { println(mymod.join("foo", "bar")) }
```

**Root cause:** `call_user_function` is a ~400-line function containing a
giant `match` block with every FFI handler. The Rust compiler allocates
stack space for the entire function's locals on entry — estimated several
KB per frame. With only 5 nested cross-module calls the default 1 MB
Windows stack is exhausted before any FFI arm is even reached.

**Proper fix (not yet applied):** Extract the FFI `match` block
(lines ~711–1104 of `evaluator.rs`) into a separate `#[inline(never)]`
function `dispatch_ffi(&str, &[Value]) -> Option<Result<Value, RuntimeError>>`.
This isolates the large stack frame so it is only allocated when a
function with an empty body (i.e., an FFI stub) is called, not on every
user function call.

**Workaround applied:** Increased main thread stack size to 8 MB via
`std::thread::Builder::new().stack_size(8 * 1024 * 1024)` in `main.rs`.
Added module environment caching to avoid redundant `module_env`
reconstruction. Both are band-aids: the stack increase masks the problem,
and the cache is a performance optimization unrelated to the root cause.

**Category:** Evaluator — stack frame size

**Decision:** Documented rather than fixed because:
1. The fix (function extraction) is mechanical but touches 400 lines of
   match arms that all need `return Ok(...)` → `return Some(Ok(...))`
   conversion — high churn, easy to introduce typos.
2. The workaround is sufficient for Phase 2.2.
3. If the bug recurs in Phase 2.3+ (likely, as modules grow), the proper
   fix must be applied immediately — no further stack size increases.

---

## Pattern Analysis

| Category | Count | Notes |
|---|---|---|
| Extension function resolution | 1 | Zero-arg calls took a different path than multi-arg calls |
| Codegen — unreachable code | 1 | Match arms with early return need terminator checks |
| Spec conformance | 1 | Always write generic signatures from the start |
| Missing language primitive | 1 | Never type is the panic building block |
| Evaluator — f-string evaluation | 1 | Hand-rolled expression parsers silently drop unsupported syntax |
| Parser — keyword ambiguity | 1 | Keywords must be allowed as member/method names after `.` |
| Evaluator — float display | 1 | Rust's f64 Display drops `.0` for whole numbers |
| Evaluator — float arithmetic | 1 | Float+Float fell through to string concatenation |
| Evaluator — float comparison | 1 | compare_binary only handled Int, not Float |
| Evaluator — ASAP name extraction | 1 | F-string `collect_expr_names` missed variables inside call args |
| Evaluator — stack frame size | 1 | Giant FFI match block inflates every call frame (**workaround only**) |

---

## Wave 1 Summary

Wave 1 (stdlib/core/) implemented 11 modules across 12 phases:
result, option, bool, int, float, math, fmt, string, list, map, set.

**10 compiler/evaluator bugs** found and fixed during implementation:
- 2 codegen bugs (#1 extension resolution, #2 match arm termination)
- 2 spec conformance issues (#3 concrete types, #4 missing panic)
- 6 evaluator bugs (#5 f-string eval, #6 keywords as members,
  #7 float display, #8 float arithmetic, #9 float comparison,
  #10 f-string ASAP names)

**Key evaluator limitation discovered:** The tree-walking evaluator uses
value semantics (clone on pass). This prevents in-place mutation of
List and Map values through FFI calls. Workaround: implement
collection-building HOF methods (map, filter, sorted, etc.) natively
in the evaluator's ListMethod/MapMethod handlers. This limitation does
NOT affect the Cranelift compilation path (handle/pointer semantics).

**Key lexer limitation discovered:** Nested double quotes inside f-string
interpolation braces are not supported (`f"{s.join(",")}"` fails).
Workaround: assign to a local variable first. This is a lexer limitation
that should be addressed in a future phase.

---

## How to Add New Entries

When you fix a compiler bug during stdlib implementation:

1. Add a new `### Bug #N` section following the template above.
2. Include: **Symptom**, **Minimal reproduction** (runnable `.fuse` code),
   **Root cause**, **Category**, and **Fix**.
3. Update the **Pattern Analysis** table.
4. Add a row to the Compiler Bug Log in
   `docs/fuse-stdlib-implementation-plan.md`.
5. Commit the learning doc update alongside the compiler fix.

---

*End of Fuse Standard Library — Compiler Bug & Learning Reference*
