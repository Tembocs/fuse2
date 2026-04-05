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
