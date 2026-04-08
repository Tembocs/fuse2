# Stage 2 Learning Log

Issues discovered and resolved during Stage 2 self-hosting work.
Each entry captures what went wrong, why, and how it was fixed.

---

## L001: Stage 0 is not a test harness for Stage 2

**Phase:** W1.1 (Token Definitions)
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

---

## L002: `continue` inside `for` loops causes infinite loop at runtime

**Phase:** Stage 2 test plan — T1.4 Control Flow
**What happened:** `for item in [1,2,3,4,5] { if item % 2 != 0 { continue }; println(item) }` hangs at runtime (TIMEOUT after 30s). The `continue` statement inside a `for` loop does not properly advance the loop iterator.

**Workaround:** Use `while` loops with manual index management when `continue` is needed. The `for` loop desugaring appears to re-evaluate the current element on `continue` rather than advancing to the next.

**Status:** Open — compiler bug in for-loop continue handling.

---

## L003: `loop { ... return ... }` needs trailing expression for checker

**Phase:** Stage 2 test plan — T1.4 Control Flow
**What happened:** `fn find() -> Int { loop { if cond { return i }; i = i + 1 } }` fails with "type mismatch: expected Int, found Unit". The checker doesn't recognize that a `loop` with guaranteed `return` inside is exhaustive.

**Workaround:** Add an unreachable trailing expression after the loop: `loop { ... } 0`.

**Status:** Open — checker doesn't model exhaustive return analysis through loops.

---

## L004: Struct fields are private — must use getter/setter methods

**Phase:** Stage 2 test plan — T1.5 Data Structures
**What happened:** `struct Counter { var count: Int }` then `c.count` fails with "cannot access field `count` on struct — struct fields are private". Unlike `data class` where fields are accessible, `struct` fields are always private.

**Resolution:** Use extension methods (getter/setter) to access struct fields: `fn Counter.getCount(ref self) -> Int => self.count`.

---

## L005: Enum variant destructuring limited to single payload

**Phase:** Stage 2 test plan — T1.5 Data Structures
**What happened:** `enum Shape { Rect(Int, Int) }` then `match s { Shape.Rect(w, h) => ... }` fails with "unknown binding `h`". Multi-payload destructuring only binds the first variable.

**Workaround:** Use single-payload enum variants, or use `_` for unused bindings.

**Status:** Open — compiler limitation in multi-payload enum pattern matching.

---

## L006: `import stdlib.core.map` causes Cranelift verifier errors

**Phase:** Stage 2 test plan — T1.6 Collections
**What happened:** Importing `stdlib.core.map` and using extension methods like `isEmpty()` produces Cranelift IR verifier errors (type mismatches in generated code). The built-in `Map` methods (`set`, `get`, `len`, `contains`, `remove`) work without importing the stdlib module.

**Workaround:** Use built-in Map methods directly without `import stdlib.core.map`. The Map type and its basic methods are available without import.

**Status:** Open — codegen bug when compiling stdlib map extension methods.

---

## L007: Lambda/closure syntax crashes compiled binaries

**Phase:** Stage 2 test plan — T1.6 Collections
**What happened:** `xs.map(fn(x: Int) -> Int => x * 2)` compiles successfully but the resulting binary crashes (empty output, no error). Higher-order functions with lambda arguments don't work in compiled mode.

**Workaround:** Use explicit loops instead of `.map()`, `.filter()`, `.sorted()` with lambdas.

**Status:** Open — codegen bug with lambda/closure compilation.

---

## L008: Generic type parameters not supported on structs

**Phase:** Stage 2 test plan — T1.8 Generics
**What happened:** `struct Box<T> { val item: T }` fails with "unexpected top-level token". Generic type parameters work on `data class` and `fn` but not on `struct`.

**Workaround:** Use `data class Box<T>(val item: T)` instead.

**Status:** Open — parser/checker limitation.

---

## L009: `implements Interface<T>` with generic args not supported

**Phase:** Stage 2 test plan — T1.9 Interfaces
**What happened:** `data class Wrapper implements Convertible<String>` fails with "unexpected top-level token <". Generic type arguments in the `implements` clause are not parsed.

**Workaround:** Define the interface without generic parameters.

**Status:** Open — parser limitation.

---

## L010: Comparison operators on data classes produce codegen errors

**Phase:** Stage 2 test plan — T1.9 Interfaces
**What happened:** `data class Score implements Comparable` with `a < b` produces Cranelift verifier errors: "arg has type i64, expected i8". The `<`/`>` operators on user-defined Comparable types generate incorrect IR for the boolean result.

**Workaround:** Call `.compareTo()` directly and compare the Int result.

**Status:** Open — codegen bug in operator dispatch for Comparable.

---

## L011: `List.get()` / `Map.get()` on empty collections crashes

**Phase:** Stage 2 test plan — T1.6/T1.8
**What happened:** Calling `.get()` on an empty list or map produces a binary that crashes (empty output) rather than returning `None`.

**Workaround:** Check `.len()` before calling `.get()`, or avoid `.get()` on empty collections.

**Status:** Open — runtime bug in Option return from get on empty collections.

---

## L012: `Int.toFloat()` extension method not available

**Phase:** Stage 2 test plan — T1.12 Operators
**What happened:** `val a = 2; println(a.toFloat() + 3.5)` fails with "unknown extension Int.toFloat".

**Workaround:** Use float literals directly (`2.0` instead of `2.toFloat()`).

**Status:** Open — missing stdlib extension method.

---

## L013: Error message text differs from expected

**Phase:** Stage 2 test plan — T1.11/T1.10
**What happened:** Runtime error messages differ from what was assumed in test fixtures:
- `parseInt` error: `"int: invalid number: abc"` (not `"invalid integer: abc"`)
- `parseFloat` error: `"float: invalid number: xyz"` (not `"invalid float: xyz"`)
- Private import error: `"cannot import non-pub item"` (not `"not public"`)

**Resolution:** Updated test expected strings to match actual compiler/runtime output.

---

## L014: Parallel test runner produces incorrect results

**Phase:** Stage 2 test plan — test execution
**What happened:** Running `run_tests.py --parallel 4` produces many false failures because compiled binaries from different tests overwrite each other in the temp directory (hash collision in file naming). Test outputs get mixed up.

**Workaround:** Run tests sequentially (no `--parallel` flag) for reliable results.

**Status:** Open — test runner needs unique temp directories per test.
