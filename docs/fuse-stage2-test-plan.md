# Fuse Stage 2 — Comprehensive Test Plan

> Test what ships. Ship what's tested.

This document specifies every test the Stage 2 compiler must pass before and
after each release. The plan is exhaustive by design — every language feature
is tested from multiple angles. Memory safety tests are treated as first-class
citizens with dedicated tiers and categories.

---

## Table of Contents

1. [Philosophy](#philosophy)
2. [Test Runner](#test-runner)
3. [Fixture Format](#fixture-format)
4. [Directory Layout](#directory-layout)
5. [Tier Definitions](#tier-definitions)
6. [T0 — Smoke](#t0--smoke)
7. [T1 — Feature Isolation](#t1--feature-isolation)
8. [T2 — Composition](#t2--composition)
9. [T3 — Error Diagnostics](#t3--error-diagnostics)
10. [M — Memory Safety](#m--memory-safety)
11. [T4 — Parity](#t4--parity)
12. [T5 — Bootstrap](#t5--bootstrap)
13. [LSP Testing](#lsp-testing)
14. [Adding a New Test](#adding-a-new-test)
15. [Run Commands](#run-commands)

---

## Philosophy

1. **Every feature has a test.** If it's in the language guide, it has at
   least one fixture. If it has edge cases, each edge case has a fixture.
2. **Memory safety is the headline promise.** The M-tier tests are the most
   important tests in the suite. They prove Fuse delivers what Rust delivers
   (no use-after-free, no double-free, no data races, no null derefs, no
   resource leaks, no deadlocks) — without lifetime annotations.
3. **Isolation first, composition second.** T1 tests one feature per file.
   T2 tests deliberate combinations. When a T2 test fails, a T1 test should
   already have pinpointed which feature broke.
4. **Error messages are features.** T3 tests verify the compiler produces
   the *right* diagnostic with the *right* message, span, and hint — not
   just that it rejects bad code.
5. **Python runner for ease of change.** The test runner is a Python script.
   Fixtures are `.fuse` files with `EXPECTED OUTPUT` / `EXPECTED ERROR` /
   `EXPECTED WARNING` headers. Adding a test means adding one `.fuse` file.

---

## Test Runner

**File:** `tests/stage2/run_tests.py`

The runner:
1. Discovers all `.fuse` files under `tests/stage2/`.
2. Reads the `EXPECTED` header to determine mode (output, error, warning).
3. For **output** fixtures: compiles with `fusec2`, runs the binary, compares
   stdout line-by-line with the expected block.
4. For **error** fixtures: compiles with `fusec2`, expects compilation to fail,
   compares stderr with expected error messages (substring match per line).
5. For **warning** fixtures: compiles with `fusec2 --check`, expects success,
   compares stderr warnings with expected messages.
6. Reports pass/fail/skip counts, prints diffs for failures.
7. Supports `--filter <pattern>` to run a subset of tests.
8. Supports `--compiler <path>` to point at a specific binary (default:
   `stage1/target/fusec2`).
9. Supports `--parallel <n>` for parallel execution.
10. Exit code 0 if all tests pass, 1 otherwise.

---

## Fixture Format

Every `.fuse` test file starts with an expected block:

```fuse
// EXPECTED OUTPUT
// Hello, world!
// 42

@entrypoint
fn main() {
    println("Hello, world!")
    println(42)
}
```

Three modes:
- `// EXPECTED OUTPUT` — compile, run binary, match stdout exactly.
- `// EXPECTED ERROR` — compile only, match compiler error messages
  (each `//` line is a substring that must appear in stderr).
- `// EXPECTED WARNING` — compile with `--check`, match warning messages.

---

## Directory Layout

```
tests/stage2/
  t0_smoke/                    # 5-10 tests: bare minimum sanity
  t1_features/                 # ~150 tests: one feature, many angles
    types/
    variables/
    functions/
    control_flow/
    data_structures/
    collections/
    pattern_matching/
    generics/
    interfaces/
    modules/
    strings/
    operators/
    error_handling/
    annotations/
    concurrency/
  t2_composition/              # ~60 tests: deliberate feature combos
    ownership_with_control_flow/
    generics_with_interfaces/
    collections_with_closures/
    concurrency_patterns/
    real_world_patterns/
  t3_errors/                   # ~80 tests: diagnostic quality
    parser_errors/
    checker_errors/
    ownership_errors/
    type_errors/
    import_errors/
    exhaustiveness_errors/
    concurrency_errors/
  m_memory/                    # ~100 tests: memory safety exhaustive
    asap_destruction/
    ownership_ref/
    ownership_mutref/
    ownership_owned/
    ownership_move/
    use_after_move/
    defer_cleanup/
    no_null/
    no_data_races/
    ranked_locking/
    resource_lifecycle/
  t4_parity/                   # automated: Stage 1 vs Stage 2
  t5_bootstrap/                # automated: three-generation self-compile
  lsp/                         # LSP-specific tests
    diagnostics/
    completions/
    hover/
    go_to_definition/
  run_tests.py                 # Python test runner
```

---

## Tier Definitions

| Tier | Code | Tests | Purpose |
|------|------|-------|---------|
| Smoke | T0 | ~8 | Does the compiler start? Can it compile hello world? |
| Feature | T1 | ~150 | Each language feature in isolation, multiple angles |
| Composition | T2 | ~60 | Deliberate multi-feature combinations |
| Errors | T3 | ~80 | Diagnostic messages, spans, and hints |
| Memory | M | ~100 | Memory safety guarantees (the headline promise) |
| Parity | T4 | All shared | Stage 1 output == Stage 2 output |
| Bootstrap | T5 | 1 | Three-generation self-compilation |
| LSP | LSP | ~40 | Language server protocol correctness |

**Total: ~440+ test fixtures**

---

## T0 — Smoke

Bare minimum. If these fail, nothing else matters.

| ID | File | Description | Expected |
|----|------|-------------|----------|
| T0.01 | `t0_hello.fuse` | `println("Hello, world!")` | OUTPUT: `Hello, world!` |
| T0.02 | `t0_int_add.fuse` | `println(2 + 2)` | OUTPUT: `4` |
| T0.03 | `t0_float.fuse` | `println(3.14)` | OUTPUT: `3.14` |
| T0.04 | `t0_bool.fuse` | `println(true)` | OUTPUT: `true` |
| T0.05 | `t0_string_concat.fuse` | `println("a" + "b")` | OUTPUT: `ab` |
| T0.06 | `t0_function_call.fuse` | Call a free function | OUTPUT: function result |
| T0.07 | `t0_if_else.fuse` | Simple conditional | OUTPUT: correct branch |
| T0.08 | `t0_empty_main.fuse` | Empty `@entrypoint fn main() {}` | OUTPUT: (empty) |

---

## T1 — Feature Isolation

One feature per file. Multiple angles per feature.

### T1.1 — Types

| ID | File | What It Tests |
|----|------|---------------|
| T1.1.01 | `types/int_literal.fuse` | Integer literals: `0`, `1`, `-1`, `999999` |
| T1.1.02 | `types/int_arithmetic.fuse` | `+`, `-`, `*`, `/`, `%` on Int |
| T1.1.03 | `types/int_division_truncation.fuse` | `7 / 2 == 3` (truncating) |
| T1.1.04 | `types/int_modulo.fuse` | Modulo with positive and negative |
| T1.1.05 | `types/int_comparison.fuse` | `<`, `>`, `<=`, `>=`, `==`, `!=` |
| T1.1.06 | `types/int_overflow_large.fuse` | Large Int values near 64-bit limits |
| T1.1.07 | `types/float_literal.fuse` | `0.0`, `1.5`, `-3.14`, `0.001` |
| T1.1.08 | `types/float_arithmetic.fuse` | `+`, `-`, `*`, `/` on Float |
| T1.1.09 | `types/float_division.fuse` | `1.0 / 2.0 == 0.5` (true division) |
| T1.1.10 | `types/float_comparison.fuse` | Float comparisons |
| T1.1.11 | `types/bool_true_false.fuse` | `println(true)`, `println(false)` |
| T1.1.12 | `types/bool_logical_and.fuse` | `true and false`, `true and true` |
| T1.1.13 | `types/bool_logical_or.fuse` | `false or true`, `false or false` |
| T1.1.14 | `types/bool_not.fuse` | `not true`, `not false` |
| T1.1.15 | `types/bool_short_circuit.fuse` | `false and side_effect()` skips right |
| T1.1.16 | `types/string_empty.fuse` | `""` is valid, `len()` is 0 |
| T1.1.17 | `types/string_escape_sequences.fuse` | `\n`, `\t`, `\\`, `\"` |
| T1.1.18 | `types/string_len.fuse` | byte length `len()` |
| T1.1.19 | `types/string_charcount.fuse` | character count vs byte length |
| T1.1.20 | `types/unit_return.fuse` | Function returning nothing returns Unit |

### T1.2 — Variables

| ID | File | What It Tests |
|----|------|---------------|
| T1.2.01 | `variables/val_binding.fuse` | `val x = 42; println(x)` |
| T1.2.02 | `variables/var_binding.fuse` | `var x = 1; x = 2; println(x)` |
| T1.2.03 | `variables/val_immutable_error.fuse` | EXPECTED ERROR: assign to `val` |
| T1.2.04 | `variables/type_inference.fuse` | Infer type from initializer |
| T1.2.05 | `variables/explicit_type_annotation.fuse` | `val x: Int = 42` |
| T1.2.06 | `variables/var_reassign_multiple.fuse` | Reassign `var` many times |
| T1.2.07 | `variables/shadowing_same_scope.fuse` | `val x = 1; val x = 2` behavior |
| T1.2.08 | `variables/shadowing_nested_scope.fuse` | Inner scope shadows outer |

### T1.3 — Functions

| ID | File | What It Tests |
|----|------|---------------|
| T1.3.01 | `functions/free_function.fuse` | Define and call a free function |
| T1.3.02 | `functions/return_value.fuse` | Function returns Int |
| T1.3.03 | `functions/multiple_params.fuse` | Function with 2+ params |
| T1.3.04 | `functions/expression_body.fuse` | `fn add(a: Int, b: Int) -> Int => a + b` |
| T1.3.05 | `functions/block_body.fuse` | Multi-statement body, last expr returned |
| T1.3.06 | `functions/early_return.fuse` | `return` exits early |
| T1.3.07 | `functions/return_unit.fuse` | `return` with no value |
| T1.3.08 | `functions/recursive.fuse` | Function calls itself |
| T1.3.09 | `functions/mutual_recursion.fuse` | Two functions calling each other |
| T1.3.10 | `functions/extension_method.fuse` | `fn Int.double(ref self) -> Int` |
| T1.3.11 | `functions/static_method.fuse` | `fn Type.create() -> Type` (no self) |
| T1.3.12 | `functions/entrypoint.fuse` | `@entrypoint` compiles and runs |
| T1.3.13 | `functions/generic_identity.fuse` | `fn id<T>(x: T) -> T => x` |
| T1.3.14 | `functions/generic_two_params.fuse` | `fn pair<A, B>(a: A, b: B)` |
| T1.3.15 | `functions/nested_calls.fuse` | `f(g(h(x)))` |
| T1.3.16 | `functions/extension_chaining.fuse` | `x.double().double()` |

### T1.4 — Control Flow

| ID | File | What It Tests |
|----|------|---------------|
| T1.4.01 | `control_flow/if_true.fuse` | `if true { ... }` |
| T1.4.02 | `control_flow/if_false.fuse` | `if false { ... }` skips body |
| T1.4.03 | `control_flow/if_else.fuse` | Both branches |
| T1.4.04 | `control_flow/if_else_if.fuse` | Chained else-if |
| T1.4.05 | `control_flow/if_as_expression.fuse` | `val x = if cond { 1 } else { 2 }` |
| T1.4.06 | `control_flow/nested_if.fuse` | If inside if |
| T1.4.07 | `control_flow/while_basic.fuse` | Count from 0 to 4 |
| T1.4.08 | `control_flow/while_never_enters.fuse` | `while false { ... }` |
| T1.4.09 | `control_flow/while_break.fuse` | `break` exits while |
| T1.4.10 | `control_flow/while_continue.fuse` | `continue` skips iteration |
| T1.4.11 | `control_flow/for_list.fuse` | `for item in [1,2,3]` |
| T1.4.12 | `control_flow/for_empty_list.fuse` | `for item in []` — zero iterations |
| T1.4.13 | `control_flow/for_break.fuse` | `break` in for loop |
| T1.4.14 | `control_flow/for_continue.fuse` | `continue` in for loop |
| T1.4.15 | `control_flow/loop_break.fuse` | `loop { break }` |
| T1.4.16 | `control_flow/loop_return.fuse` | `loop { return }` |
| T1.4.17 | `control_flow/nested_loops.fuse` | Break/continue in nested loops |
| T1.4.18 | `control_flow/while_with_var.fuse` | Mutable counter in while |

### T1.5 — Data Structures

| ID | File | What It Tests |
|----|------|---------------|
| T1.5.01 | `data_structures/data_class_basic.fuse` | `data class Point(val x: Int, val y: Int)` |
| T1.5.02 | `data_structures/data_class_field_access.fuse` | `p.x`, `p.y` |
| T1.5.03 | `data_structures/data_class_equality.fuse` | `Point(1,2) == Point(1,2)` |
| T1.5.04 | `data_structures/data_class_tostring.fuse` | Auto-generated toString |
| T1.5.05 | `data_structures/data_class_method.fuse` | Method defined on data class |
| T1.5.06 | `data_structures/enum_unit_variants.fuse` | `enum Color { Red, Green, Blue }` |
| T1.5.07 | `data_structures/enum_data_variants.fuse` | `enum Shape { Circle(Float), Rect(Float, Float) }` |
| T1.5.08 | `data_structures/enum_match.fuse` | Match on enum variants |
| T1.5.09 | `data_structures/struct_basic.fuse` | `struct Counter { var count: Int }` |
| T1.5.10 | `data_structures/struct_methods.fuse` | Extension methods on struct |
| T1.5.11 | `data_structures/struct_val_field.fuse` | Immutable field on struct |
| T1.5.12 | `data_structures/struct_var_field.fuse` | Mutable field on struct |
| T1.5.13 | `data_structures/result_ok.fuse` | `Result.Ok(42)` |
| T1.5.14 | `data_structures/result_err.fuse` | `Result.Err("fail")` |
| T1.5.15 | `data_structures/option_some.fuse` | `Option.Some(42)` |
| T1.5.16 | `data_structures/option_none.fuse` | `Option.None` |

### T1.6 — Collections

| ID | File | What It Tests |
|----|------|---------------|
| T1.6.01 | `collections/list_literal.fuse` | `[1, 2, 3]` |
| T1.6.02 | `collections/list_empty.fuse` | `[]` |
| T1.6.03 | `collections/list_push.fuse` | `.push()` appends element |
| T1.6.04 | `collections/list_get.fuse` | `.get(i)` retrieves element |
| T1.6.05 | `collections/list_len.fuse` | `.len()` returns count |
| T1.6.06 | `collections/list_is_empty.fuse` | `.isEmpty()` on empty and non-empty |
| T1.6.07 | `collections/list_iteration.fuse` | `for item in list` |
| T1.6.08 | `collections/list_map.fuse` | `.map(fn(x) => x * 2)` |
| T1.6.09 | `collections/list_filter.fuse` | `.filter(fn(x) => x > 0)` |
| T1.6.10 | `collections/list_sorted.fuse` | `.sorted()` |
| T1.6.11 | `collections/list_first_last.fuse` | `.first()`, `.last()` |
| T1.6.12 | `collections/list_nested.fuse` | `List<List<Int>>` |
| T1.6.13 | `collections/map_new.fuse` | `Map::<String, Int>.new()` |
| T1.6.14 | `collections/map_set_get.fuse` | `.set()` then `.get()` |
| T1.6.15 | `collections/map_contains.fuse` | `.contains()` |
| T1.6.16 | `collections/map_remove.fuse` | `.remove()` |
| T1.6.17 | `collections/map_keys_values.fuse` | `.keys()`, `.values()` |
| T1.6.18 | `collections/map_len_empty.fuse` | `.len()`, `.isEmpty()` |
| T1.6.19 | `collections/map_overwrite.fuse` | `.set()` same key twice |
| T1.6.20 | `collections/map_get_missing.fuse` | `.get()` missing key returns None |

### T1.7 — Pattern Matching

| ID | File | What It Tests |
|----|------|---------------|
| T1.7.01 | `pattern_matching/match_int.fuse` | Match on integer value |
| T1.7.02 | `pattern_matching/match_string.fuse` | Match on string value |
| T1.7.03 | `pattern_matching/match_bool.fuse` | Match on true/false |
| T1.7.04 | `pattern_matching/match_enum.fuse` | Match on custom enum |
| T1.7.05 | `pattern_matching/match_option_some.fuse` | `Some(x) =>` with binding |
| T1.7.06 | `pattern_matching/match_option_none.fuse` | `None =>` arm |
| T1.7.07 | `pattern_matching/match_result_ok.fuse` | `Ok(x) =>` with binding |
| T1.7.08 | `pattern_matching/match_result_err.fuse` | `Err(e) =>` with binding |
| T1.7.09 | `pattern_matching/match_wildcard.fuse` | `_ =>` catch-all |
| T1.7.10 | `pattern_matching/match_nested.fuse` | `Some(Ok(x)) =>` |
| T1.7.11 | `pattern_matching/match_tuple.fuse` | `(a, b) =>` |
| T1.7.12 | `pattern_matching/match_as_expression.fuse` | `val x = match y { ... }` |
| T1.7.13 | `pattern_matching/match_multiple_arms.fuse` | 5+ arms |
| T1.7.14 | `pattern_matching/match_qualified_variant.fuse` | `Status.Ok =>` |
| T1.7.15 | `pattern_matching/when_basic.fuse` | `when { cond => ... }` |
| T1.7.16 | `pattern_matching/when_else.fuse` | `when { ... else => ... }` |
| T1.7.17 | `pattern_matching/when_multiple.fuse` | Multiple conditions |
| T1.7.18 | `pattern_matching/when_as_expression.fuse` | `val x = when { ... }` |

### T1.8 — Generics

| ID | File | What It Tests |
|----|------|---------------|
| T1.8.01 | `generics/identity.fuse` | `fn id<T>(x: T) -> T` |
| T1.8.02 | `generics/two_type_params.fuse` | `fn pair<A, B>(a: A, b: B)` |
| T1.8.03 | `generics/generic_struct.fuse` | `struct Box<T> { val item: T }` |
| T1.8.04 | `generics/generic_data_class.fuse` | `data class Pair<A,B>(val a: A, val b: B)` |
| T1.8.05 | `generics/generic_enum.fuse` | Custom generic enum |
| T1.8.06 | `generics/type_inference.fuse` | Inferred type args at call site |
| T1.8.07 | `generics/explicit_type_args.fuse` | `func::<Int>(...)` |
| T1.8.08 | `generics/bounded_single.fuse` | `<T: Printable>` |
| T1.8.09 | `generics/bounded_multiple.fuse` | `<T: Hashable, V: Comparable>` |
| T1.8.10 | `generics/generic_list_ops.fuse` | Generic function on List<T> |
| T1.8.11 | `generics/nested_generics.fuse` | `List<Option<Int>>` |
| T1.8.12 | `generics/generic_return.fuse` | Return generic type |

### T1.9 — Interfaces

| ID | File | What It Tests |
|----|------|---------------|
| T1.9.01 | `interfaces/basic_interface.fuse` | Define interface, implement it |
| T1.9.02 | `interfaces/multiple_methods.fuse` | Interface with 2+ methods |
| T1.9.03 | `interfaces/implements_keyword.fuse` | `data class X implements Y` |
| T1.9.04 | `interfaces/default_method.fuse` | Default method on interface |
| T1.9.05 | `interfaces/override_default.fuse` | Type overrides default method |
| T1.9.06 | `interfaces/interface_composition.fuse` | `Debuggable : Printable` |
| T1.9.07 | `interfaces/marker_interface.fuse` | Interface with no methods |
| T1.9.08 | `interfaces/equatable.fuse` | Built-in Equatable |
| T1.9.09 | `interfaces/comparable.fuse` | Built-in Comparable |
| T1.9.10 | `interfaces/hashable.fuse` | Built-in Hashable |
| T1.9.11 | `interfaces/multiple_implements.fuse` | Type implements 2+ interfaces |
| T1.9.12 | `interfaces/generic_interface.fuse` | `interface Convertible<T>` |

### T1.10 — Modules & Imports

| ID | File | What It Tests |
|----|------|---------------|
| T1.10.01 | `modules/basic_import.fuse` | Import and use a module |
| T1.10.02 | `modules/import_destructured.fuse` | `import a.b.{X, Y}` |
| T1.10.03 | `modules/pub_visibility.fuse` | `pub fn` visible, non-pub hidden |
| T1.10.04 | `modules/private_error.fuse` | EXPECTED ERROR: import private item |
| T1.10.05 | `modules/stdlib_import.fuse` | `import core.string` |
| T1.10.06 | `modules/multiple_imports.fuse` | Several imports in one file |
| T1.10.07 | `modules/qualified_access.fuse` | `module.function()` syntax |

### T1.11 — Strings

| ID | File | What It Tests |
|----|------|---------------|
| T1.11.01 | `strings/fstring_basic.fuse` | `f"hello {name}"` |
| T1.11.02 | `strings/fstring_expression.fuse` | `f"sum = {a + b}"` |
| T1.11.03 | `strings/fstring_nested.fuse` | f-string inside f-string |
| T1.11.04 | `strings/fstring_method_call.fuse` | `f"{list.len()}"` |
| T1.11.05 | `strings/concat.fuse` | `"a" + "b"` |
| T1.11.06 | `strings/contains.fuse` | `.contains()` |
| T1.11.07 | `strings/starts_ends_with.fuse` | `.startsWith()`, `.endsWith()` |
| T1.11.08 | `strings/split.fuse` | `.split(",")` |
| T1.11.09 | `strings/trim.fuse` | `.trim()` |
| T1.11.10 | `strings/replace.fuse` | `.replace("old", "new")` |
| T1.11.11 | `strings/upper_lower.fuse` | `.toUpper()`, `.toLower()` |
| T1.11.12 | `strings/substring.fuse` | `.substring(start, end)` |
| T1.11.13 | `strings/charat.fuse` | `.charAt(i)` |
| T1.11.14 | `strings/byteat.fuse` | `.byteAt(i)` |
| T1.11.15 | `strings/parse_int.fuse` | `parseInt("42")` |
| T1.11.16 | `strings/parse_float.fuse` | `parseFloat("3.14")` |
| T1.11.17 | `strings/equality.fuse` | `"abc" == "abc"` |
| T1.11.18 | `strings/comparison.fuse` | `"a" < "b"` |

### T1.12 — Operators

| ID | File | What It Tests |
|----|------|---------------|
| T1.12.01 | `operators/precedence_mul_add.fuse` | `2 + 3 * 4 == 14` |
| T1.12.02 | `operators/precedence_parens.fuse` | `(2 + 3) * 4 == 20` |
| T1.12.03 | `operators/unary_minus.fuse` | `-x` |
| T1.12.04 | `operators/elvis.fuse` | `opt ?: default` |
| T1.12.05 | `operators/optional_chain.fuse` | `x?.field` |
| T1.12.06 | `operators/question_propagate.fuse` | `val x = fallible()?` |
| T1.12.07 | `operators/mixed_int_float.fuse` | `1 + 2.0` type coercion |
| T1.12.08 | `operators/equality_types.fuse` | `==` on Int, String, Bool |
| T1.12.09 | `operators/chained_comparison.fuse` | `a < b and b < c` |

### T1.13 — Error Handling

| ID | File | What It Tests |
|----|------|---------------|
| T1.13.01 | `error_handling/result_ok_unwrap.fuse` | Create Ok, match Ok |
| T1.13.02 | `error_handling/result_err_unwrap.fuse` | Create Err, match Err |
| T1.13.03 | `error_handling/question_on_result.fuse` | `?` unwraps Ok, returns Err |
| T1.13.04 | `error_handling/question_on_option.fuse` | `?` unwraps Some, returns None |
| T1.13.05 | `error_handling/question_chain.fuse` | Multiple `?` in one function |
| T1.13.06 | `error_handling/optional_chain.fuse` | `a?.b?.c` |
| T1.13.07 | `error_handling/elvis_chain.fuse` | `a ?: b ?: c` |
| T1.13.08 | `error_handling/result_match_exhaustive.fuse` | Must cover Ok and Err |
| T1.13.09 | `error_handling/option_match_exhaustive.fuse` | Must cover Some and None |
| T1.13.10 | `error_handling/nested_result.fuse` | `Result<Option<Int>, String>` |

### T1.14 — Annotations

| ID | File | What It Tests |
|----|------|---------------|
| T1.14.01 | `annotations/entrypoint.fuse` | `@entrypoint` on main |
| T1.14.02 | `annotations/value_on_struct.fuse` | `@value` auto-generates lifecycle |
| T1.14.03 | `annotations/inline_hint.fuse` | `@inline` doesn't break compilation |

### T1.15 — Concurrency

| ID | File | What It Tests |
|----|------|---------------|
| T1.15.01 | `concurrency/spawn_basic.fuse` | `spawn { println("hello") }` |
| T1.15.02 | `concurrency/channel_bounded.fuse` | Create bounded channel, send/recv |
| T1.15.03 | `concurrency/channel_unbounded.fuse` | Create unbounded channel |
| T1.15.04 | `concurrency/shared_read.fuse` | `Shared::new(v)`, `.read()` |
| T1.15.05 | `concurrency/shared_write.fuse` | `.write()` modifies value |
| T1.15.06 | `concurrency/rank_basic.fuse` | `@rank(1)` annotation |
| T1.15.07 | `concurrency/spawn_move.fuse` | `spawn move { ... }` |
| T1.15.08 | `concurrency/channel_close.fuse` | `tx.close()` |

---

## T2 — Composition

Deliberate combinations of features. Each test exercises 2-4 features
together to catch interaction bugs.

### T2.1 — Ownership × Control Flow

| ID | File | What It Tests |
|----|------|---------------|
| T2.1.01 | `ownership_with_control_flow/ref_in_if.fuse` | `ref` param used in both if/else branches |
| T2.1.02 | `ownership_with_control_flow/mutref_in_while.fuse` | Modify `mutref` inside while loop |
| T2.1.03 | `ownership_with_control_flow/move_in_one_branch.fuse` | Move in if, use in else — must be error |
| T2.1.04 | `ownership_with_control_flow/ref_in_for.fuse` | `ref` param used during for loop |
| T2.1.05 | `ownership_with_control_flow/mutref_early_return.fuse` | `mutref` writeback on early return |
| T2.1.06 | `ownership_with_control_flow/owned_in_loop.fuse` | Owned value consumed in loop iteration |
| T2.1.07 | `ownership_with_control_flow/move_before_loop.fuse` | Move then loop — error |
| T2.1.08 | `ownership_with_control_flow/ref_in_match.fuse` | `ref` value used in match arms |

### T2.2 — Generics × Interfaces

| ID | File | What It Tests |
|----|------|---------------|
| T2.2.01 | `generics_with_interfaces/bounded_generic_call.fuse` | Call interface method on `<T: Printable>` |
| T2.2.02 | `generics_with_interfaces/generic_collection_bounded.fuse` | `List<T: Comparable>` sorted |
| T2.2.03 | `generics_with_interfaces/interface_chain.fuse` | Debuggable extends Printable, use both |
| T2.2.04 | `generics_with_interfaces/generic_struct_implements.fuse` | Generic struct implementing interface |

### T2.3 — Collections × Control Flow

| ID | File | What It Tests |
|----|------|---------------|
| T2.3.01 | `collections_with_closures/list_map_with_closure.fuse` | `.map(fn(x) => x * 2)` |
| T2.3.02 | `collections_with_closures/list_filter_with_condition.fuse` | `.filter(fn(x) => x > 0)` |
| T2.3.03 | `collections_with_closures/map_iterate_entries.fuse` | For-loop over map entries |
| T2.3.04 | `collections_with_closures/nested_list_ops.fuse` | map then filter then sorted |
| T2.3.05 | `collections_with_closures/list_in_match_arm.fuse` | Build list inside match arm |
| T2.3.06 | `collections_with_closures/map_in_loop.fuse` | Build map with loop |

### T2.4 — Concurrency Patterns

| ID | File | What It Tests |
|----|------|---------------|
| T2.4.01 | `concurrency_patterns/producer_consumer.fuse` | Spawn producer, main consumes via channel |
| T2.4.02 | `concurrency_patterns/shared_counter.fuse` | Multiple spawns incrementing Shared counter |
| T2.4.03 | `concurrency_patterns/fan_out.fuse` | One sender, multiple receivers |
| T2.4.04 | `concurrency_patterns/ranked_multiple_locks.fuse` | Acquire rank 1 then rank 2 correctly |
| T2.4.05 | `concurrency_patterns/spawn_with_result.fuse` | Send Result through channel |

### T2.5 — Real-World Patterns

| ID | File | What It Tests |
|----|------|---------------|
| T2.5.01 | `real_world_patterns/mini_calculator.fuse` | Parse string, compute, print result |
| T2.5.02 | `real_world_patterns/word_count.fuse` | Split string, use Map to count |
| T2.5.03 | `real_world_patterns/linked_pipeline.fuse` | Function A → B → C → D, chained calls |
| T2.5.04 | `real_world_patterns/state_machine.fuse` | Enum-based state machine in loop |
| T2.5.05 | `real_world_patterns/builder_pattern.fuse` | Data class with builder methods |
| T2.5.06 | `real_world_patterns/error_recovery.fuse` | Chain of `?` with fallback at top |
| T2.5.07 | `real_world_patterns/recursive_tree.fuse` | Recursive data + recursive function |
| T2.5.08 | `real_world_patterns/option_pipeline.fuse` | `?.` chain 4 deep with `?:` fallback |
| T2.5.09 | `real_world_patterns/multi_module_program.fuse` | Import 3+ modules, use all together |
| T2.5.10 | `real_world_patterns/generic_cache.fuse` | `Map<String, T>` with generic get/set |

---

## T3 — Error Diagnostics

Every test is `EXPECTED ERROR` or `EXPECTED WARNING`. The purpose is to verify
the *quality* of diagnostics — message text, location, and hint.

### T3.1 — Parser Errors

| ID | File | Expected Error Substring |
|----|------|--------------------------|
| T3.1.01 | `parser_errors/missing_brace.fuse` | `expected '}'` |
| T3.1.02 | `parser_errors/missing_paren.fuse` | `expected ')'` |
| T3.1.03 | `parser_errors/missing_arrow.fuse` | `expected '->'` |
| T3.1.04 | `parser_errors/unexpected_token.fuse` | `unexpected token` |
| T3.1.05 | `parser_errors/missing_param_type.fuse` | `expected type` |
| T3.1.06 | `parser_errors/empty_file.fuse` | (varies) |
| T3.1.07 | `parser_errors/double_comma.fuse` | `unexpected ','` |
| T3.1.08 | `parser_errors/unterminated_string.fuse` | `unterminated string` |
| T3.1.09 | `parser_errors/invalid_escape.fuse` | `unknown escape` or similar |
| T3.1.10 | `parser_errors/missing_entrypoint.fuse` | `no @entrypoint` |

### T3.2 — Type Errors

| ID | File | Expected Error Substring |
|----|------|--------------------------|
| T3.2.01 | `type_errors/type_mismatch_assign.fuse` | `type mismatch` |
| T3.2.02 | `type_errors/wrong_arg_count.fuse` | `expected N arguments` |
| T3.2.03 | `type_errors/wrong_arg_type.fuse` | `expected .* got` |
| T3.2.04 | `type_errors/unknown_type.fuse` | `unknown type` |
| T3.2.05 | `type_errors/return_type_mismatch.fuse` | `return type` |
| T3.2.06 | `type_errors/if_branch_mismatch.fuse` | `branches .* different types` |
| T3.2.07 | `type_errors/unknown_function.fuse` | `undefined` or `not found` |
| T3.2.08 | `type_errors/unknown_method.fuse` | `no method` |
| T3.2.09 | `type_errors/interface_not_satisfied.fuse` | `does not implement` |
| T3.2.10 | `type_errors/duplicate_function.fuse` | `already defined` |

### T3.3 — Ownership Errors

| ID | File | Expected Error Substring |
|----|------|--------------------------|
| T3.3.01 | `ownership_errors/assign_to_val.fuse` | `cannot assign to immutable` |
| T3.3.02 | `ownership_errors/use_after_move.fuse` | `use after move` |
| T3.3.03 | `ownership_errors/double_move.fuse` | `already moved` |
| T3.3.04 | `ownership_errors/mutref_without_callsite.fuse` | `expected .* mutref` |
| T3.3.05 | `ownership_errors/mutref_on_val.fuse` | `cannot pass .* as mutref` |
| T3.3.06 | `ownership_errors/move_in_loop.fuse` | `move .* in loop` |
| T3.3.07 | `ownership_errors/ref_modification.fuse` | `cannot modify .* ref` |
| T3.3.08 | `ownership_errors/spawn_mutref_capture.fuse` | `cannot capture .* mutref .* spawn` |
| T3.3.09 | `ownership_errors/move_then_access.fuse` | `use after move` |
| T3.3.10 | `ownership_errors/mutref_conflict.fuse` | `exclusive` or `already borrowed` |

### T3.4 — Exhaustiveness Errors

| ID | File | Expected Error Substring |
|----|------|--------------------------|
| T3.4.01 | `exhaustiveness_errors/match_missing_none.fuse` | `non-exhaustive .* None` |
| T3.4.02 | `exhaustiveness_errors/match_missing_err.fuse` | `non-exhaustive .* Err` |
| T3.4.03 | `exhaustiveness_errors/match_missing_variant.fuse` | `non-exhaustive` |
| T3.4.04 | `exhaustiveness_errors/match_bool_one_arm.fuse` | `non-exhaustive .* Bool` |
| T3.4.05 | `exhaustiveness_errors/when_missing_else.fuse` | `else` or `non-exhaustive` |

### T3.5 — Import Errors

| ID | File | Expected Error Substring |
|----|------|--------------------------|
| T3.5.01 | `import_errors/unknown_module.fuse` | `module not found` |
| T3.5.02 | `import_errors/private_import.fuse` | `not public` or `private` |
| T3.5.03 | `import_errors/circular_import.fuse` | `circular` |

### T3.6 — Concurrency Errors

| ID | File | Expected Error Substring |
|----|------|--------------------------|
| T3.6.01 | `concurrency_errors/rank_violation.fuse` | `rank` or `lock order` |
| T3.6.02 | `concurrency_errors/missing_rank.fuse` | `@rank required` |
| T3.6.03 | `concurrency_errors/spawn_mutref.fuse` | `cannot capture .* mutref` |

---

## M — Memory Safety

> This is the most important section of the test plan.
>
> Fuse promises: "Everything Rust guarantees, without the borrow checker pain."
> Every guarantee must have tests that prove it.

### M.1 — ASAP Destruction

Tests that verify values are destroyed at their last use point, not at scope
exit. Observable via custom `__del__` that calls `println`.

| ID | File | What It Tests | Expected |
|----|------|---------------|----------|
| M.1.01 | `asap_destruction/destroy_at_last_use.fuse` | Value destroyed immediately after last use | OUTPUT: use, then del, then next statement |
| M.1.02 | `asap_destruction/destroy_order_sequential.fuse` | Two values, second used last — first destroyed first | OUTPUT: correct destruction order |
| M.1.03 | `asap_destruction/not_destroyed_if_still_used.fuse` | Value used later stays alive | OUTPUT: no early del |
| M.1.04 | `asap_destruction/destroy_in_if_branch.fuse` | Value used in one branch destroyed in that branch | OUTPUT: del in correct branch |
| M.1.05 | `asap_destruction/destroy_after_both_branches.fuse` | Value used in both branches destroyed after if/else | OUTPUT: del after both branches |
| M.1.06 | `asap_destruction/destroy_in_loop_body.fuse` | Temporary created in loop body destroyed each iteration | OUTPUT: del per iteration |
| M.1.07 | `asap_destruction/destroy_after_loop.fuse` | Value used in loop destroyed after loop ends | OUTPUT: loop, then del |
| M.1.08 | `asap_destruction/destroy_param_not_used.fuse` | Owned param not used — destroyed immediately | OUTPUT: immediate del |
| M.1.09 | `asap_destruction/destroy_return_value_unused.fuse` | Unused return value destroyed at call site | OUTPUT: del after call |
| M.1.10 | `asap_destruction/destroy_nested_function.fuse` | Value passed to function — destroyed inside callee | OUTPUT: del in callee |
| M.1.11 | `asap_destruction/destroy_multiple_paths.fuse` | If/else with return in one branch — del timing correct | OUTPUT: correct per path |
| M.1.12 | `asap_destruction/destroy_reassigned_var.fuse` | Old value destroyed when var reassigned | OUTPUT: del of old value |

### M.2 — Ownership: ref

| ID | File | What It Tests | Expected |
|----|------|---------------|----------|
| M.2.01 | `ownership_ref/ref_read_only.fuse` | `ref` param can be read | OUTPUT: printed value |
| M.2.02 | `ownership_ref/ref_caller_keeps.fuse` | Caller uses value after `ref` call | OUTPUT: value still accessible |
| M.2.03 | `ownership_ref/ref_multiple_concurrent.fuse` | Same value passed as `ref` to two functions | OUTPUT: both succeed |
| M.2.04 | `ownership_ref/ref_modify_error.fuse` | Attempt to modify `ref` param | ERROR: cannot modify |
| M.2.05 | `ownership_ref/ref_move_error.fuse` | Attempt to move `ref` param | ERROR: cannot move |
| M.2.06 | `ownership_ref/ref_in_loop.fuse` | Pass `ref` in every iteration | OUTPUT: works each time |
| M.2.07 | `ownership_ref/ref_to_list_element.fuse` | Read list element via ref | OUTPUT: element value |
| M.2.08 | `ownership_ref/ref_to_struct_field.fuse` | Read struct field via ref | OUTPUT: field value |

### M.3 — Ownership: mutref

| ID | File | What It Tests | Expected |
|----|------|---------------|----------|
| M.3.01 | `ownership_mutref/mutref_modify.fuse` | Modify value via `mutref`, caller sees change | OUTPUT: modified value |
| M.3.02 | `ownership_mutref/mutref_writeback.fuse` | Writeback after function returns | OUTPUT: caller has new value |
| M.3.03 | `ownership_mutref/mutref_exclusive.fuse` | No second borrow while mutref active | ERROR: exclusive |
| M.3.04 | `ownership_mutref/mutref_callsite_required.fuse` | Must write `mutref` at call site | ERROR: expected mutref |
| M.3.05 | `ownership_mutref/mutref_early_return_writeback.fuse` | Writeback happens on early return | OUTPUT: written back |
| M.3.06 | `ownership_mutref/mutref_in_while.fuse` | Mutref in loop body, writeback each iteration | OUTPUT: accumulated changes |
| M.3.07 | `ownership_mutref/mutref_on_val_error.fuse` | Cannot pass `val` as mutref | ERROR: cannot pass immutable |
| M.3.08 | `ownership_mutref/mutref_nested_call.fuse` | Mutref passed to inner function, writeback propagates | OUTPUT: change visible at top |
| M.3.09 | `ownership_mutref/mutref_list_modify.fuse` | Mutref List, push inside function | OUTPUT: list modified |
| M.3.10 | `ownership_mutref/mutref_struct_field.fuse` | Modify struct field via mutref | OUTPUT: field changed |

### M.4 — Ownership: owned

| ID | File | What It Tests | Expected |
|----|------|---------------|----------|
| M.4.01 | `ownership_owned/owned_consume.fuse` | Owned param consumed by callee | OUTPUT: consumed value |
| M.4.02 | `ownership_owned/owned_destroy_in_callee.fuse` | Owned value destroyed inside callee (ASAP) | OUTPUT: del in callee |
| M.4.03 | `ownership_owned/owned_store.fuse` | Owned value stored in data structure | OUTPUT: stored successfully |
| M.4.04 | `ownership_owned/owned_transfer_chain.fuse` | Owned: A → B → C (chain of transfers) | OUTPUT: final consumer |

### M.5 — Ownership: move

| ID | File | What It Tests | Expected |
|----|------|---------------|----------|
| M.5.01 | `ownership_move/move_basic.fuse` | `move x` transfers ownership | OUTPUT: value in callee |
| M.5.02 | `ownership_move/move_prevents_reuse.fuse` | Use after `move x` is compile error | ERROR: use after move |
| M.5.03 | `ownership_move/move_in_if.fuse` | Move in one branch, reuse in other — error | ERROR: may have been moved |
| M.5.04 | `ownership_move/move_in_both_branches.fuse` | Move in both if/else — subsequent use is error | ERROR: use after move |
| M.5.05 | `ownership_move/move_in_loop_error.fuse` | Move inside loop body — error on second iteration | ERROR: move in loop |
| M.5.06 | `ownership_move/move_then_rebind.fuse` | Move then `val y = new_value` — ok | OUTPUT: new binding works |
| M.5.07 | `ownership_move/double_move_error.fuse` | Moving same value twice — error | ERROR: already moved |
| M.5.08 | `ownership_move/move_into_spawn.fuse` | `spawn move { use val }` — ok | OUTPUT: value in spawned task |
| M.5.09 | `ownership_move/move_no_copy.fuse` | Moved value not secretly copied | OUTPUT: only one del |

### M.6 — Use-After-Move (Exhaustive)

Every scenario where use-after-move could slip through.

| ID | File | What It Tests | Expected |
|----|------|---------------|----------|
| M.6.01 | `use_after_move/linear_move_then_print.fuse` | Move then println | ERROR: use after move |
| M.6.02 | `use_after_move/move_then_pass_to_fn.fuse` | Move then pass to another function | ERROR: use after move |
| M.6.03 | `use_after_move/move_then_field_access.fuse` | Move struct then access field | ERROR: use after move |
| M.6.04 | `use_after_move/move_then_method_call.fuse` | Move then call method | ERROR: use after move |
| M.6.05 | `use_after_move/move_in_nested_scope.fuse` | Move in inner block, use in outer | ERROR: use after move |
| M.6.06 | `use_after_move/move_conditional_always.fuse` | Move in both if/else, use after | ERROR: use after move |
| M.6.07 | `use_after_move/move_conditional_maybe.fuse` | Move in one branch only, use after | ERROR: may have been moved |
| M.6.08 | `use_after_move/move_in_match_arm.fuse` | Move in one match arm, use after match | ERROR: may have been moved |
| M.6.09 | `use_after_move/move_twice.fuse` | Move same value in two statements | ERROR: already moved |
| M.6.10 | `use_after_move/move_in_while.fuse` | Move in while body, second iteration | ERROR: move in loop |

### M.7 — Defer & Cleanup

| ID | File | What It Tests | Expected |
|----|------|---------------|----------|
| M.7.01 | `defer_cleanup/defer_runs_at_exit.fuse` | Defer fires at function exit | OUTPUT: body then defer |
| M.7.02 | `defer_cleanup/defer_lifo_order.fuse` | Multiple defers run in LIFO order | OUTPUT: third, second, first |
| M.7.03 | `defer_cleanup/defer_on_early_return.fuse` | Defer fires even on early return | OUTPUT: defer runs |
| M.7.04 | `defer_cleanup/defer_captures_value.fuse` | Deferred stmt references outer value | OUTPUT: correct value printed |
| M.7.05 | `defer_cleanup/defer_after_asap.fuse` | ASAP destroys first, defer runs last | OUTPUT: asap del, then defer |
| M.7.06 | `defer_cleanup/defer_in_loop.fuse` | Defer inside loop fires each iteration | OUTPUT: defer per iteration |
| M.7.07 | `defer_cleanup/defer_keeps_alive.fuse` | Value in defer body not ASAP'd early | OUTPUT: defer uses value |
| M.7.08 | `defer_cleanup/defer_resource_close.fuse` | Open file / acquire resource, defer close | OUTPUT: resource closed |
| M.7.09 | `defer_cleanup/multiple_defer_with_asap.fuse` | Mix of ASAP destruction and defers | OUTPUT: correct interleaving |
| M.7.10 | `defer_cleanup/defer_nested_scopes.fuse` | Defer in nested function scope | OUTPUT: both defers fire |

### M.8 — No Null (Option<T> Enforcement)

| ID | File | What It Tests | Expected |
|----|------|---------------|----------|
| M.8.01 | `no_null/option_some_match.fuse` | Match Some, extract value | OUTPUT: value |
| M.8.02 | `no_null/option_none_match.fuse` | Match None, take fallback | OUTPUT: fallback |
| M.8.03 | `no_null/option_safe_chain.fuse` | `x?.y?.z` returns None safely | OUTPUT: None |
| M.8.04 | `no_null/option_elvis_default.fuse` | `x ?: 0` provides default | OUTPUT: default |
| M.8.05 | `no_null/map_get_returns_option.fuse` | `map.get(key)` returns Option, not nullable | OUTPUT: Some or None |
| M.8.06 | `no_null/list_first_returns_option.fuse` | `list.first()` returns Option | OUTPUT: Some or None |
| M.8.07 | `no_null/no_null_assignment.fuse` | Cannot assign null/nil to anything | ERROR: syntax/type error |
| M.8.08 | `no_null/option_exhaustive_match.fuse` | Match on Option must cover both | ERROR if missing None |
| M.8.09 | `no_null/question_propagate_none.fuse` | `?` on None returns None from function | OUTPUT: None |
| M.8.10 | `no_null/nested_option.fuse` | `Option<Option<Int>>` handled correctly | OUTPUT: nested unwrap |

### M.9 — No Data Races

| ID | File | What It Tests | Expected |
|----|------|---------------|----------|
| M.9.01 | `no_data_races/spawn_ref_only.fuse` | Spawn with ref captures — safe | OUTPUT: reads succeed |
| M.9.02 | `no_data_races/spawn_mutref_error.fuse` | Spawn capturing mutref — compile error | ERROR: cannot capture mutref |
| M.9.03 | `no_data_races/spawn_move_safe.fuse` | Spawn move — value owned by task, safe | OUTPUT: task owns value |
| M.9.04 | `no_data_races/shared_read_concurrent.fuse` | Multiple tasks .read() simultaneously | OUTPUT: all succeed |
| M.9.05 | `no_data_races/shared_write_exclusive.fuse` | Only one .write() at a time | OUTPUT: sequential writes |
| M.9.06 | `no_data_races/no_raw_mutable_sharing.fuse` | Cannot share var across spawn without Shared | ERROR: cannot capture mutable |
| M.9.07 | `no_data_races/channel_ownership_transfer.fuse` | `tx.send(move v)` — no concurrent access | OUTPUT: received in other task |
| M.9.08 | `no_data_races/shared_guard_asap.fuse` | Lock guard destroyed via ASAP (no held-too-long) | OUTPUT: guard released promptly |

### M.10 — Ranked Locking (No Deadlocks)

| ID | File | What It Tests | Expected |
|----|------|---------------|----------|
| M.10.01 | `ranked_locking/ascending_order.fuse` | Acquire rank 1, then rank 2 — ok | OUTPUT: both acquired |
| M.10.02 | `ranked_locking/descending_order_error.fuse` | Acquire rank 2, then rank 1 — error | ERROR: rank violation |
| M.10.03 | `ranked_locking/same_rank_ok.fuse` | Two @rank(1) acquired — ok (independent) | OUTPUT: both acquired |
| M.10.04 | `ranked_locking/rank_zero_first.fuse` | @rank(0) must be first lock | ERROR if not first |
| M.10.05 | `ranked_locking/three_ranks.fuse` | Acquire 1 → 2 → 3 — ok | OUTPUT: all three |
| M.10.06 | `ranked_locking/missing_rank_error.fuse` | Shared without @rank — error | ERROR: @rank required |
| M.10.07 | `ranked_locking/rank_in_function_call.fuse` | Rank checking through function boundaries | ERROR or OUTPUT depending on order |
| M.10.08 | `ranked_locking/try_write_timeout.fuse` | `try_write(Timeout.ms(50))` returns Err on timeout | OUTPUT: timeout handled |

### M.11 — Resource Lifecycle

End-to-end tests that verify the full lifecycle of resources (open, use, close)
without leaks.

| ID | File | What It Tests | Expected |
|----|------|---------------|----------|
| M.11.01 | `resource_lifecycle/struct_with_del.fuse` | Custom `__del__` fires at ASAP point | OUTPUT: constructor, use, del |
| M.11.02 | `resource_lifecycle/nested_struct_del.fuse` | Outer struct del fires, inner struct del fires | OUTPUT: both dels |
| M.11.03 | `resource_lifecycle/list_of_structs_del.fuse` | Each struct in list destroyed | OUTPUT: N dels for N items |
| M.11.04 | `resource_lifecycle/early_return_cleanup.fuse` | Early return destroys all local resources | OUTPUT: all dels fire |
| M.11.05 | `resource_lifecycle/exception_path_cleanup.fuse` | Error result path cleans up local resources | OUTPUT: cleanup on error |
| M.11.06 | `resource_lifecycle/scoped_resource.fuse` | Resource lives exactly as long as needed (ASAP) | OUTPUT: precise lifetime |
| M.11.07 | `resource_lifecycle/reassign_drops_old.fuse` | `var x = A; x = B` drops A then assigns B | OUTPUT: del A, then use B |
| M.11.08 | `resource_lifecycle/copy_independent.fuse` | Copy creates independent lifecycle (del for each) | OUTPUT: two independent dels |
| M.11.09 | `resource_lifecycle/move_no_double_del.fuse` | Move does not double-delete | OUTPUT: exactly one del |
| M.11.10 | `resource_lifecycle/defer_plus_asap.fuse` | Full lifecycle: create, ASAP some, defer others | OUTPUT: correct combined order |

---

## T4 — Parity

**Automated.** The Python runner compiles every `tests/fuse/core/` and
`tests/fuse/milestone/` fixture with *both* Stage 1 (`fusec`) and Stage 2
(`fusec2`), then compares stdout. Any difference is a failure.

This reuses the existing 145+ core fixtures. No new files needed.

Implementation: `run_tests.py --parity` mode.

---

## T5 — Bootstrap

**Automated.** Already implemented in `stage1/fusec/tests/stage2_bootstrap.rs`.
Also available via `run_tests.py --bootstrap`:

1. Stage 1 compiles Stage 2 → `fusec2-bootstrap`
2. `fusec2-bootstrap` compiles Stage 2 → `fusec2-stage2`
3. `fusec2-stage2` compiles Stage 2 → `fusec2-verified`
4. Semantic equivalence check (compile fixture, compare output)
5. Core test suite with `fusec2-verified`

---

## LSP Testing

The Fuse LSP server (`stage1/fuse-lsp/`) currently implements:
- **Diagnostics** (publish on didChange/didOpen)
- **Hover** (symbol type info and signatures)
- **Completions** (keywords, functions, variables, dot-completions)
- **Go-to-Definition** (jump to symbol definition)

### LSP Test Approach

LSP tests use a Python client that speaks JSON-RPC over stdin/stdout to the
LSP server binary. Each test:
1. Sends `initialize` and `initialized`.
2. Opens a document with `didOpen` (inline source text).
3. Sends a request (hover, completion, definition, or reads diagnostics).
4. Asserts on the response.
5. Sends `shutdown` and `exit`.

### LSP — Diagnostics

| ID | File | What It Tests |
|----|------|---------------|
| LSP.D.01 | `diagnostics/valid_program.py` | No diagnostics for valid source |
| LSP.D.02 | `diagnostics/syntax_error.py` | Diagnostic with correct line/column |
| LSP.D.03 | `diagnostics/type_error.py` | Type error reported with message |
| LSP.D.04 | `diagnostics/warning.py` | Warning-level diagnostic |
| LSP.D.05 | `diagnostics/multiple_errors.py` | 2+ diagnostics returned |
| LSP.D.06 | `diagnostics/edit_fixes_error.py` | Fix source → diagnostic cleared |
| LSP.D.07 | `diagnostics/ownership_error.py` | Use-after-move reported |
| LSP.D.08 | `diagnostics/exhaustiveness.py` | Non-exhaustive match reported |

### LSP — Hover

| ID | File | What It Tests |
|----|------|---------------|
| LSP.H.01 | `hover/function_signature.py` | Hover on function name → shows signature |
| LSP.H.02 | `hover/variable_type.py` | Hover on variable → shows inferred type |
| LSP.H.03 | `hover/parameter_type.py` | Hover on param → shows type + convention |
| LSP.H.04 | `hover/struct_field.py` | Hover on field → shows field type |
| LSP.H.05 | `hover/import_symbol.py` | Hover on imported item → shows source module |

### LSP — Completions

| ID | File | What It Tests |
|----|------|---------------|
| LSP.C.01 | `completions/keywords.py` | `fn`, `val`, `var`, `if`, etc. offered |
| LSP.C.02 | `completions/local_variables.py` | In-scope variables offered |
| LSP.C.03 | `completions/functions.py` | Defined functions offered |
| LSP.C.04 | `completions/dot_methods.py` | After `.`, extension methods offered |
| LSP.C.05 | `completions/dot_fields.py` | After `.` on struct, fields offered |
| LSP.C.06 | `completions/types.py` | Type names offered (Int, String, etc.) |
| LSP.C.07 | `completions/imports.py` | After `import`, module paths offered |
| LSP.C.08 | `completions/no_private.py` | Private items not offered from other modules |

### LSP — Go-to-Definition

| ID | File | What It Tests |
|----|------|---------------|
| LSP.G.01 | `go_to_definition/function.py` | Jump to function definition |
| LSP.G.02 | `go_to_definition/variable.py` | Jump to variable declaration |
| LSP.G.03 | `go_to_definition/struct.py` | Jump to struct definition |
| LSP.G.04 | `go_to_definition/enum.py` | Jump to enum definition |
| LSP.G.05 | `go_to_definition/import.py` | Jump to imported module |
| LSP.G.06 | `go_to_definition/parameter.py` | Jump to parameter in signature |
| LSP.G.07 | `go_to_definition/interface.py` | Jump to interface definition |

---

## Adding a New Test

1. **Choose the tier** (T0/T1/T2/T3/M).
2. **Create a `.fuse` file** in the appropriate subdirectory.
3. **Add the expected header**:
   - `// EXPECTED OUTPUT` + expected lines for runtime tests.
   - `// EXPECTED ERROR` + expected error substrings for diagnostic tests.
   - `// EXPECTED WARNING` + expected warning substrings for warning tests.
4. **Name the file** descriptively: `feature_angle.fuse` (e.g.,
   `mutref_early_return_writeback.fuse`).
5. **Run**: `python tests/stage2/run_tests.py --filter filename`.
6. **No registration needed.** The runner discovers all files automatically.

For LSP tests:
1. Create a `.py` file in the appropriate `lsp/` subdirectory.
2. Use the `lsp_client` helper to send JSON-RPC requests.
3. Assert on responses.
4. Run: `python tests/stage2/run_tests.py --lsp --filter filename`.

---

## Run Commands

```bash
# Run all Stage 2 tests
python tests/stage2/run_tests.py

# Run a specific tier
python tests/stage2/run_tests.py --filter t0_
python tests/stage2/run_tests.py --filter t1_
python tests/stage2/run_tests.py --filter m_memory

# Run parity (Stage 1 vs Stage 2)
python tests/stage2/run_tests.py --parity

# Run bootstrap
python tests/stage2/run_tests.py --bootstrap

# Run LSP tests
python tests/stage2/run_tests.py --lsp

# Use a specific compiler binary
python tests/stage2/run_tests.py --compiler ./fusec2-verified.exe

# Run in parallel
python tests/stage2/run_tests.py --parallel 8

# Filter by name
python tests/stage2/run_tests.py --filter mutref
```

---

*Document created: 2026-04-08.*
*Companion documents: `docs/fuse-stage2-plan.md`, `docs/fuse-language-guide-2.md`.*
