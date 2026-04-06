# Fuse — Copilot Instructions

## Language Philosophy

Fuse is a statically-typed, compiled language with three non-negotiable properties:

1. **Memory safety without garbage collection.** ASAP (As Soon As Possible) deterministic destruction — values are destroyed at their last use point. No GC pauses. No manual free. No dangling pointers.
2. **Concurrency safety without a borrow checker.** Ownership conventions (`ref`, `mutref`, `owned`, `move`) declared at function signatures. `Shared<T>` with ranked locking prevents data races and deadlocks at compile time. No lifetime annotations.
3. **Developer experience as a first-class concern.** Clean syntax. Helpful error messages. Fast compilation.

Fuse does not experiment — it integrates. Every feature has been proven in production at scale elsewhere. The authoritative spec is `docs/fuse-language-guide-2.md`. If the guide says it, implement it. If the guide does not say it, do not invent it.

## Ownership Model

| Convention | Meaning | Use When |
|---|---|---|
| `ref` | Shared immutable borrow (default) | Reading without modification |
| `mutref` | Exclusive mutable borrow | Modifying the caller's value |
| `owned` | Caller gives up ownership | Consuming or storing the value |
| `move` | Transfer at call site (explicit) | `move x` to transfer ownership into a function |

Functions declare conventions on parameters: `fn process(ref data: List<Int>)`. The compiler enforces that `mutref` is exclusive, `ref` is shared, and moved values are not reused.

## Project Structure

```
stage0/   — Reference interpreter (Python). Fully functional. All core tests pass.
stage1/   — Production compiler (Rust + Cranelift). Current focus.
  fusec/     — Compiler: lexer, parser, AST, HIR, checker, codegen
  fuse-runtime/ — Runtime library: FFI functions called by compiled binaries
  cranelift-ffi/ — Cranelift integration layer
stdlib/   — Standard library (.fuse files, 34 modules across core/ext/full)
tests/    — Shared test suite (.fuse fixtures used by both stages)
docs/     — Specs, plans, ADRs
```

**Stage 1 is the active compiler.** Stage 0 is the reference. Stage 2 (self-hosting in Fuse) comes after hardening.

## Compiler Pipeline

```
Source (.fuse)
  → Lexer (lexer/lexer.rs)      → Vec<Token>
  → Parser (parser/parser.rs)   → AST (ast/nodes.rs)
  → HIR Lowering (hir/lower.rs) → HIR (hir/nodes.rs)
  → Checker (checker/mod.rs)    → Type checking + ownership validation
  → Codegen (codegen/object_backend.rs) → Cranelift IR → Native binary
```

All values are represented as opaque `FuseHandle` (pointer-sized) at the Cranelift level. Generics are type-erased — `List<Int>` and `List<String>` produce identical machine code.

## Symbol Naming Conventions

```
Free functions:      fuse_fn_{module_path}_{name}
Extension methods:   fuse_ext_{module_path}_{Type}__{method}
Runtime functions:   fuse_{operation}  (e.g., fuse_add, fuse_println)
Entry point:         fuse_user_entry  (function marked @entrypoint)
```

Runtime FFI functions in `fuse-runtime/` use `#[unsafe(no_mangle)]` and C ABI. All take/return `FuseHandle` (pointer-sized integer). Examples: `fuse_int`, `fuse_float`, `fuse_add`, `fuse_eq`, `fuse_println`, `fuse_some`, `fuse_none`.

## Test Fixture Format

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
- `// EXPECTED OUTPUT` — compile, run binary, match stdout
- `// EXPECTED ERROR` — compile only, match compiler error messages
- `// EXPECTED WARNING` — compile only, match compiler warnings

Test harness: `stage1/fusec/tests/harness.rs`. Tests live in `tests/fuse/` organized by `core/`, `full/`, `stdlib/`, `milestone/`.

## Key Codegen Patterns

**Checking block termination:** Before emitting any instruction after a branch/return, always call `self.current_block_is_terminated(builder)`. A terminated block cannot accept more instructions — Cranelift panics with `"block already filled"`.

```rust
if !self.current_block_is_terminated(builder) {
    self.jump_value(builder, done, value);
}
```

**ASAP release:** Locals track `destroy: bool`. When compiling inner scopes (match arms, loop bodies), protect outer-scope locals by setting `binding.destroy = false` before entering the scope. Otherwise ASAP release may destroy outer variables inside the inner scope.

**Extension method compilation:** The extension loop in `emit_object` resolves `self` parameter type from `receiver_type`. If the first param is named `"self"` and has no explicit type, its type is set to the receiver type.

**Final expression extraction:** Function bodies use `split_last()` on statements to find the final `Statement::Expr(ExprStmt { expr, .. })`. This is the implicit return value. `compile_match` infers the return type via `self.infer_expr_type()` on arm expressions.

## Stdlib Conventions

- Extension methods use `fn Type.method(ref self, ...)` syntax
- Static methods use `fn Type.method(...)` (no `self` parameter)
- FFI backing functions are declared with `extern fn name(...)` in `.fuse` files
- Collection HOFs (`map`, `filter`, `sorted`, etc.) take `fn(T) -> U` parameters
- The `data` keyword is reserved — never use it as a parameter name (use `input`)
- Modules: `core/` (always available), `ext/` (optional, no OS deps), `full/` (OS features)

## Current Limitations (Pre-Hardening)

These are known. See `docs/fuse-hardening-plan.md` for the fix plan:

- `if/else` blocks always return `Unit` in codegen — use `match` as workaround for conditional return values
- Float literals produce a hard error in codegen — use FFI functions instead
- F-string nested quotes `f"{s.join(",")}"` terminate early — assign to variable first
- `Type.staticMethod()` rejected by checker — use module-level `pub fn` instead
- Structs not compiled in codegen — use `data class` instead
- `data class Name<T>(...)` generic syntax not parsed — type params in field annotations only
- Builder methods cannot return `mutref Self` — return `Int` as workaround

## Mandatory References

Before modifying any compiler code, read:
- `docs/fuse-language-guide-2.md` — the authoritative language specification
- `docs/fuse-hardening-plan.md` — current fix plan with root causes and solutions
- `docs/stdlib_implementation_learning.md` — catalog of 15 bugs found and lessons learned
- `docs/fuse-stdlib-implementation-plan.md` — complete stdlib implementation history
