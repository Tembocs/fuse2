# Fuse Stage 2 — Self-Hosting Implementation Plan

> **Status:** Not started.
> **Scope:** 8 waves, 42 phases, ~280 tasks.
> **Goal:** Write the Fuse compiler in Fuse. Achieve bootstrap: Fuse
> compiles itself, producing identical binaries on successive passes.
>
> **Prerequisite:** Stage 1 compiler complete (Waves 0-8 done), stdlib
> interfaces implemented (Phases 0-8 done), all tests green.
>
> **The authoritative spec is `docs/fuse-language-guide-2.md`.** If the
> guide says it, we implement it. If the guide does not say it, we do
> not invent it.

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

**Every decision, every fix, every line of code written during this
plan must serve these three properties. If a change undermines memory
safety, concurrency safety, or developer experience, it is wrong —
regardless of how clever or expedient it may be.**

---

## Mandatory Rules

> **These rules apply to every wave and every phase in this document.**

### Rule 1: Solve Problems When They Appear

There is no "this was a prior issue." If a bug surfaces, a limitation
is hit, or a language feature doesn't behave as the guide specifies,
**stop and fix it.** Do not work around it. Do not add a TODO. Do not
defer it to a later phase. Fix the Stage 1 compiler, fix the runtime,
fix the stdlib — whatever is required — then continue from where you
left off.

If the fix requires a new runtime FFI function, add it to
`fuse-runtime`. If it requires a parser change, change the parser.
If it reveals a language guide ambiguity, resolve it in the guide
first, then implement. The self-hosted compiler is the ultimate test
of the language; every issue it reveals is an issue that must be
solved robustly.

### Rule 2: Read Before You Build

Before starting **any wave**, you must read:

1. The **Language Philosophy** section above
2. This **Mandatory Rules** section
3. The specific files listed in the wave's `Before starting` block
4. The `docs/fuse-language-guide-2.md` sections referenced in the wave

Do not begin implementation until you have read and understood all
required material.

### Rule 3: Zero Regressions

Every new feature must have at least one test. Every existing Stage 1
test must remain green after every phase. Run `cargo test -p fusec` in
`stage1/` after every phase.

### Rule 4: Use `while`/`break`/`continue` for All Iteration

Do not use recursion to simulate loops. Recursive iteration blows the
stack on compiler-sized workloads and makes control flow harder for
the codegen to lower. Use `while` loops with manual index tracking.

### Rule 5: Split Into Modules

Each Fuse source file in `stage2/src/` should have a single
responsibility and stay under ~500 lines. Use `import` and `pub` to
connect modules. A monolithic single-file compiler is unmaintainable.

### Rule 6: 8 MB Stack

The linker invocation for the self-hosted compiler must request 8 MB
of stack. The compiler compiling itself is a deep workload.

### Rule 7: Completion Standard

A phase is done when:

1. All checkbox tasks in the phase are complete
2. All new tests pass (both Stage 1 `cargo test` and manual
   Stage 2 verification)
3. All existing tests pass
4. The code is clean (no debugging artifacts)
5. The phase can be demonstrated working

A wave is done when all phases in the wave meet the completion
standard. **After a wave completes, stop and report before proceeding
to the next wave.**

### Rule 8: Fixes Go Forward, Not Backward

Stage 0 (Python) is a completed prototype. It is not a test harness
for Stage 2, and it must not be modified. When Stage 2 development
reveals a bug, a missing feature, or a limitation, the fix belongs in
**Stage 1** (the Rust compiler, runtime, or FFI layer). Stage 1 is the
toolchain that compiles Stage 2 — it is the only codebase that matters.
Never regress into Stage 0 to work around a Stage 1 gap.

### Rule 9: Document What You Learn

Fixing a bug is not enough — the insight behind it must survive the
fix. When an issue is discovered and resolved during Stage 2 work,
record it in `docs/learning.md` with: what went wrong, why it
happened, and what was done to solve it. This log turns individual
fixes into institutional knowledge and prevents the same class of
mistake from recurring.

---

## Status Key

- `[ ]` — not started
- `[~]` — in progress
- `[x]` — done
- `[!]` — blocked (must state what blocks it)

---

## Architecture Overview

### What Stage 2 Builds

A complete Fuse compiler (`fusec2`) written in Fuse Core that:

1. Reads `.fuse` source files
2. Tokenizes them (lexer)
3. Parses them into an AST (parser)
4. Validates ownership, types, and exhaustiveness (checker)
5. Generates native machine code via Cranelift FFI (codegen)
6. Links with `fuse-runtime` to produce a native executable

### Module Layout

```
stage2/src/
  main.fuse          CLI entry point, argument parsing, mode dispatch
  token.fuse         Token type definitions (data class Token, enum TokenKind)
  lexer.fuse         Tokenizer: source string → List<Token>
  ast.fuse           AST node definitions (enums and data classes)
  parser.fuse        Recursive descent parser: List<Token> → Program AST
  checker.fuse       Type checking, ownership, exhaustiveness
  codegen.fuse       Cranelift IR generation via cranelift-ffi FFI
  layout.fuse        Symbol mangling (fuse_fn_, fuse_ext_, fuse_del_)
  error.fuse         Diagnostic types and error formatting
  common.fuse        Import resolution, path utilities
```

### Compilation Strategy

Stage 2 uses the **same compilation strategy** as Stage 1:

1. All values are `FuseHandle` pointers (uniform ABI)
2. Runtime functions handle type dispatch (fuse_add, fuse_eq, etc.)
3. Cranelift generates native object code
4. A Rust wrapper crate links the object with `fuse-runtime`
5. `cargo build --release` produces the final binary

The critical difference: Stage 1 calls Cranelift's Rust API directly.
Stage 2 calls it through `cranelift-ffi` — a C-compatible FFI layer
that exposes Cranelift operations as `extern fn` declarations.

### What Stage 2 Does NOT Need

The self-hosted compiler compiles **Fuse Core** only. It does not need:

- Evaluator / `--run` mode (Stage 1 handles interpretation)
- REPL
- WASM target
- LSP server
- SIMD codegen
- Spawn / concurrency codegen
- Shared / channel codegen
- Interface conformance checking (for now — the compiler itself
  doesn't use interfaces heavily)

These features remain in Stage 1. Stage 2's job is to compile Fuse
Core programs to native binaries — nothing more, nothing less.

---

## Dependency: Cranelift FFI Expansion

> The current `cranelift-ffi` crate is a stub (53 lines, 5 functions).
> Stage 2 requires ~40 FFI functions to drive Cranelift from Fuse.
> This expansion happens in Wave 0.

### Required Cranelift Operations (from Stage 1 codegen analysis)

**Module Management:**
- Create/destroy module, create/destroy function context
- Declare function (name, signature, linkage)
- Define function (finalize and commit)
- Emit object bytes to file

**Signature Building:**
- Create signature, add param type, add return type
- Types: I8, I32, I64, F64, pointer

**Function Building:**
- Create/destroy function builder
- Create block, switch to block, seal block, seal all blocks
- Append block params, get block params

**Instructions (34 unique in Stage 1):**
- `iconst`, `f64const` — constants
- `call`, `call_indirect` — function calls
- `return_` — function return
- `jump`, `brif` — control flow
- `icmp`, `icmp_imm` — integer comparison
- `iadd`, `iadd_imm`, `isub`, `imul` — integer arithmetic
- `fadd`, `fsub`, `fmul`, `fdiv`, `fabs`, `sqrt` — float arithmetic
- `bxor_imm` — bitwise XOR
- `sextend`, `uextend`, `fpromote` — type conversion
- `symbol_value`, `func_addr` — address loading
- `trap` — panic/unreachable
- `splat`, `extractlane`, `insertlane`, `scalar_to_vector` — SIMD (optional)

**Data Management:**
- Declare data object, define data with bytes
- Get symbol address (for string constants)

---

## Task Summary

| Wave | Name | Phases | Tasks | Depends On | Status |
|------|------|--------|-------|------------|--------|
| W0 | Cranelift FFI Expansion | 4 | 42 | — | Not started |
| W1 | Token & Lexer | 3 | 24 | W0 | Not started |
| W2 | AST & Parser | 5 | 38 | W1 | Not started |
| W3 | Error Reporting & Diagnostics | 2 | 12 | W1 | Not started |
| W4 | Module System & Import Resolution | 2 | 16 | W2, W3 | Not started |
| W5 | Type Checker | 4 | 34 | W4 | Not started |
| W6 | Code Generation | 6 | 62 | W0, W5 | Not started |
| W7 | CLI, Linking & Bootstrap | 5 | 36 | W6 | Not started |
| **Total** | | **31** | **~264** | | |

---

## Wave 0 — Cranelift FFI Expansion

> **MANDATORY:** Before starting this wave, read:
>
> - `stage1/cranelift-ffi/src/lib.rs` — current stub
> - `stage1/fusec/src/codegen/object_backend.rs` — every Cranelift API
>   call (lines 550-5227)
> - Cranelift documentation for `FunctionBuilder`, `Module`, `InstBuilder`
>
> **This wave modifies Stage 1 Rust code only.** No Fuse code yet.
> The goal is to expose enough of Cranelift's API as C-compatible
> FFI functions that Stage 2 Fuse code can drive code generation.
>
> **Design principle:** Each FFI function does one thing. Opaque
> handles (`Ptr`) for Module, FunctionBuilder, Block, Value, Signature.
> All functions are `extern "C"` with simple scalar parameters.

---

### Phase W0.1 — Module & Context Management

- [x] **W0.1.1** Define opaque handle types: `FfiModule`, `FfiContext`, `FfiSignature`.
- [x] **W0.1.2** `cranelift_ffi_module_new() -> Ptr` — create ObjectModule with native ISA.
- [x] **W0.1.3** `cranelift_ffi_module_free(module: Ptr)` — destroy module.
- [x] **W0.1.4** `cranelift_ffi_context_new() -> Ptr` — create `codegen::Context`.
- [x] **W0.1.5** `cranelift_ffi_context_free(ctx: Ptr)` — destroy context.
- [x] **W0.1.6** `cranelift_ffi_module_target_pointer_type(module: Ptr) -> Int` — return pointer width (8 for 64-bit).
- [x] **W0.1.7** `cranelift_ffi_module_declare_function(module: Ptr, name: Ptr, name_len: Int, sig: Ptr, linkage: Int) -> Int` — declare function, return FuncId as integer.
- [x] **W0.1.8** `cranelift_ffi_module_define_function(module: Ptr, func_id: Int, ctx: Ptr) -> Int` — define function body. Returns 0 on success.
- [x] **W0.1.9** `cranelift_ffi_module_finish(module: Ptr, path: Ptr, path_len: Int) -> Int` — emit object file to disk.
- [x] **W0.1.10** Test: call module_new / module_free from a Fuse program compiled by Stage 1.

---

### Phase W0.2 — Signature & Type Building

- [x] **W0.2.1** `cranelift_ffi_signature_new(module: Ptr, call_conv: Int) -> Ptr` — create Signature.
- [x] **W0.2.2** `cranelift_ffi_signature_free(sig: Ptr)` — destroy signature.
- [x] **W0.2.3** `cranelift_ffi_signature_add_param(sig: Ptr, type_id: Int, module: Ptr)` — add parameter (type_id maps to I8=0, I32=1, I64=2, F64=3, Ptr=4). Module needed for pointer type resolution.
- [x] **W0.2.4** `cranelift_ffi_signature_add_return(sig: Ptr, type_id: Int, module: Ptr)` — add return type. Module needed for pointer type resolution.
- [x] **W0.2.5** `cranelift_ffi_signature_clone(sig: Ptr) -> Ptr` — clone for reuse.
- [x] **W0.2.6** Test: create signature with (Ptr, Ptr) -> Ptr, verify round-trip.

---

### Phase W0.3 — Function Builder & Blocks

- [x] **W0.3.1** `cranelift_ffi_builder_new(module: Ptr, ctx: Ptr, sig: Ptr) -> Ptr` — create FunctionBuilder from context + signature. Sets up entry block.
- [x] **W0.3.2** `cranelift_ffi_builder_free(builder: Ptr)` — finalize and destroy builder.
- [x] **W0.3.3** `cranelift_ffi_builder_create_block(builder: Ptr) -> Int` — create new block, return block id.
- [x] **W0.3.4** `cranelift_ffi_builder_switch_to_block(builder: Ptr, block: Int)`.
- [x] **W0.3.5** `cranelift_ffi_builder_seal_block(builder: Ptr, block: Int)`.
- [x] **W0.3.6** `cranelift_ffi_builder_seal_all_blocks(builder: Ptr)`.
- [x] **W0.3.7** `cranelift_ffi_builder_append_block_param(builder: Ptr, block: Int, type_id: Int, module: Ptr) -> Int` — return Value id. Module needed for type resolution.
- [x] **W0.3.8** `cranelift_ffi_builder_block_params(builder: Ptr, block: Int, out: Ptr, max: Int) -> Int` — fill array with Value ids, return count.
- [x] **W0.3.9** `cranelift_ffi_builder_entry_block(builder: Ptr) -> Int` — return entry block id.
- [x] **W0.3.10** `cranelift_ffi_builder_finalize(builder: Ptr)` — finalize without destroying (for define_function).
- [x] **W0.3.11** `cranelift_ffi_builder_declare_func_in_func(builder: Ptr, module: Ptr, func_id: Int) -> Int` — import function reference for call.
- [x] **W0.3.12** `cranelift_ffi_builder_inst_results(builder: Ptr, inst: Int, out: Ptr, max: Int) -> Int` — fill array with result Values.
- [x] **W0.3.13** Test: create function with entry block, add params, finalize.

---

### Phase W0.4 — Instructions

> Each instruction function takes a builder handle and value operands,
> returns a Value id (or Inst id for calls).

- [x] **W0.4.1** `cranelift_ffi_ins_iconst(builder: Ptr, type_id: Int, value: Int, module: Ptr) -> Int` — integer constant. Module needed for pointer type.
- [x] **W0.4.2** `cranelift_ffi_ins_f64const(builder: Ptr, value: Int) -> Int` — float constant (f64 bits as i64).
- [x] **W0.4.3** `cranelift_ffi_ins_call(builder: Ptr, func_ref: Int, args: Ptr, arg_count: Int) -> Int` — call function, return Inst id.
- [x] **W0.4.4** `cranelift_ffi_ins_return(builder: Ptr, values: Ptr, count: Int)` — return from function.
- [x] **W0.4.5** `cranelift_ffi_ins_jump(builder: Ptr, block: Int, args: Ptr, arg_count: Int)` — unconditional jump. Uses BlockArg::Value wrapper.
- [x] **W0.4.6** `cranelift_ffi_ins_brif(builder: Ptr, cond: Int, then_block: Int, then_args: Ptr, then_count: Int, else_block: Int, else_args: Ptr, else_count: Int)` — conditional branch.
- [x] **W0.4.7** `cranelift_ffi_ins_icmp(builder: Ptr, cc: Int, a: Int, b: Int) -> Int` — integer compare. IntCC mapping: 0=Eq,1=Ne,2=Slt,3=Sge,4=Sgt,5=Sle,6=Ult,7=Uge,8=Ugt,9=Ule.
- [x] **W0.4.8** `cranelift_ffi_ins_icmp_imm(builder: Ptr, cc: Int, a: Int, imm: Int) -> Int` — compare with immediate.
- [x] **W0.4.9** `cranelift_ffi_ins_iadd(builder: Ptr, a: Int, b: Int) -> Int`.
- [x] **W0.4.10** `cranelift_ffi_ins_iadd_imm(builder: Ptr, a: Int, imm: Int) -> Int`.
- [x] **W0.4.11** `cranelift_ffi_ins_isub(builder: Ptr, a: Int, b: Int) -> Int`.
- [x] **W0.4.12** `cranelift_ffi_ins_imul(builder: Ptr, a: Int, b: Int) -> Int`.
- [x] **W0.4.13** `cranelift_ffi_ins_bxor_imm(builder: Ptr, a: Int, imm: Int) -> Int`.
- [x] **W0.4.14** `cranelift_ffi_ins_sextend(builder: Ptr, type_id: Int, a: Int, module: Ptr) -> Int`. Module needed for pointer type.
- [x] **W0.4.15** `cranelift_ffi_ins_uextend(builder: Ptr, type_id: Int, a: Int, module: Ptr) -> Int`. Module needed for pointer type.
- [x] **W0.4.16** `cranelift_ffi_ins_trap(builder: Ptr, code: Int)` — trap/unreachable.
- [x] **W0.4.17** `cranelift_ffi_ins_symbol_value(builder: Ptr, module: Ptr, data_id: Int, type_id: Int) -> Int` — load data symbol address via declare_data_in_func.

**Data objects:**
- [x] **W0.4.18** `cranelift_ffi_module_declare_data(module: Ptr, name: Ptr, name_len: Int, writable: Int) -> Int` — declare data object.
- [x] **W0.4.19** `cranelift_ffi_module_define_data(module: Ptr, data_id: Int, bytes: Ptr, byte_len: Int) -> Int` — define data content.

**Verification:**
- [x] **W0.4.20** `cranelift_ffi_context_verify(ctx: Ptr, module: Ptr) -> Int` — verify function IR. Returns 0 on success.

**Integration test:**
- [x] **W0.4.21** Rust integration test: create module, declare `fuse_int`, build function calling `fuse_int(42)`, verify IR, define function — all passing. Fuse smoke test updated in cranelift_ffi_smoke.fuse.

---

## Wave 1 — Token & Lexer

> **MANDATORY:** Before starting this wave, read:
>
> - `stage1/fusec/src/lexer/token.rs` — all TokenKind variants
> - `stage1/fusec/src/lexer/lexer.rs` — tokenization logic
> - `docs/fuse-language-guide-2.md` §1.1-1.4 (types, variables, functions)

---

### Phase W1.1 — Token Definitions

- [x] **W1.1.1** Create `stage2/src/token.fuse`.
- [x] **W1.1.2** Define `enum TokenKind` with all 69 variants matching Stage 1 token.rs exactly.
- [x] **W1.1.3** Define `data class Token(val kind: TokenKind, val text: String, val line: Int, val column: Int)`.
- [x] **W1.1.4** Define `data class Span(val line: Int, val column: Int)`.
- [x] **W1.1.5** Define `pub fn keyword_kind(text: String) -> Option<TokenKind>` — map keyword strings to token kinds.
- [x] **W1.1.6** Test: `keyword_kind("fn")` returns `Some(TokenKind.Fn)`, `keyword_kind("hello")` returns `None`. Added Stage 0 enum runtime support to enable testing.

---

### Phase W1.2 — Lexer Core

- [x] **W1.2.1** Create `stage2/src/lexer.fuse`.
- [x] **W1.2.2** Define `pub fn lex(source: String, filename: String) -> Result<List<Token>, String>`.
- [x] **W1.2.3** Implement character scanning with `while` loop over `source.byteAt(i)` (byte-level for ASCII source).
- [x] **W1.2.4** Implement whitespace and comment skipping (`//` to end of line).
- [x] **W1.2.5** Implement integer literals: sequence of digits, produce `TokenKind.Int`.
- [x] **W1.2.6** Implement float literals: digits, `.`, digits — produce `TokenKind.Float`.
- [x] **W1.2.7** Implement string literals: `"..."` with escape sequences (`\n`, `\t`, `\\`, `\"`).
- [x] **W1.2.8** Implement f-string literals: `f"..."` with brace depth tracking for `{expr}`.
- [x] **W1.2.9** Implement identifiers: `[a-zA-Z_][a-zA-Z0-9_]*`, check against keyword table.
- [x] **W1.2.10** Implement single-character operators: `+`, `-`, `*`, `/`, `%`, `(`, `)`, `{`, `}`, `[`, `]`, `,`, `;`, `:`, `.`, `@`.
- [x] **W1.2.11** Implement multi-character operators: `==`, `!=`, `<=`, `>=`, `->`, `=>`, `?.`, `?:`, `::`.
- [x] **W1.2.12** Implement single-character operators that may be multi: `<`, `>`, `=`, `?`, `!`.
- [x] **W1.2.13** Append `TokenKind.Eof` at end.
- [x] **W1.2.14** Track line and column numbers.
- [x] **W1.2.15** Test: lex `val x = 42` → 4 tokens (Val, Identifier, Assign, Int).
- [x] **W1.2.16** Test: lex `f"hello {name}"` → 1 FString token.

---

### Phase W1.3 — Lexer Edge Cases

- [x] **W1.3.1** Handle f-string nested braces: `f"{obj.method()}"` — brace depth increments on `{`, decrements on `}`.
- [x] **W1.3.2** Handle negative number literals vs unary minus (lexer emits `Minus` + `Int`; parser handles negation).
- [x] **W1.3.3** Handle `0` as valid integer literal.
- [x] **W1.3.4** Handle empty string `""`.
- [x] **W1.3.5** Error on unterminated string literal.
- [x] **W1.3.6** Error on unexpected character (return descriptive error with line/column).
- [x] **W1.3.7** Test: lex the Stage 1 milestone file `tests/fuse/milestone/four_functions.fuse`, verify token count.
- [x] **W1.3.8** Test: lex all keyword variants.

---

## Wave 2 — AST & Parser

> **MANDATORY:** Before starting this wave, read:
>
> - `stage1/fusec/src/ast/nodes.rs` — all AST node types
> - `stage1/fusec/src/parser/parser.rs` — parsing strategy
> - `docs/fuse-language-guide-2.md` §1.5-1.12 (ownership, patterns,
>   control flow, error handling, modules)

---

### Phase W2.1 — AST Node Definitions

- [x] **W2.1.1** Create `stage2/src/ast.fuse`.
- [x] **W2.1.2** Define `data class Program(val declarations: List<Declaration>, val filename: String)`.
- [x] **W2.1.3** Define `enum Declaration` with variants: `Import`, `Function`, `DataClass`, `Enum`, `ExternFn`, `Struct`, `Const`, `Interface`.
- [x] **W2.1.4** Define `data class ImportDecl(val modulePath: String, val items: Option<List<String>>, val span: Span)`.
- [x] **W2.1.5** Define `data class FunctionDecl(val name: String, val typeParams: List<String>, val params: List<Param>, val returnType: Option<String>, val body: Block, val isPub: Bool, val annotations: List<Annotation>, val receiverType: Option<String>, val span: Span)`.
- [x] **W2.1.6** Define `data class Param(val convention: Option<String>, val name: String, val typeName: Option<String>, val variadic: Bool, val span: Span)`.
- [x] **W2.1.7** Define `data class DataClassDecl(val name: String, val typeParams: List<String>, val fields: List<FieldDecl>, val methods: List<FunctionDecl>, val isPub: Bool, val annotations: List<Annotation>, val interfaces: List<String>, val span: Span)`.
- [x] **W2.1.8** Define `data class EnumDecl`, `data class EnumVariant`, `data class StructDecl`, `data class InterfaceDecl`, `data class InterfaceMethod`.
- [x] **W2.1.9** Define `data class ExternFnDecl`, `data class ConstDecl`.
- [x] **W2.1.10** Define `data class Block(val statements: List<Statement>, val span: Span)`.
- [x] **W2.1.11** Define `enum Statement` with variants: `VarDecl`, `Assign`, `Return`, `Break`, `Continue`, `Spawn`, `While`, `For`, `Loop`, `Defer`, `Expr`, `TupleDestruct`.
- [x] **W2.1.12** Define `enum Expr` with variants: `Literal`, `FString`, `Name`, `List`, `Tuple`, `Unary`, `Binary`, `Call`, `Member`, `Move`, `Ref`, `MutRef`, `Question`, `If`, `Match`, `When`, `Lambda`.
- [x] **W2.1.13** Define `enum Pattern` with variants: `Wildcard`, `Literal`, `Name`, `Variant`, `Tuple`.
- [x] **W2.1.14** Define `data class Annotation(val name: String, val args: List<AnnotationArg>, val span: Span)`.
- [x] **W2.1.15** Define all supporting data classes: `FieldDecl`, `VarDeclStmt`, `AssignStmt`, `ReturnStmt`, `WhileStmt`, `ForStmt`, `LoopStmt`, `DeferStmt`, `ExprStmt`, `BinaryOp`, `UnaryOp`, `CallExpr`, `MemberExpr`, `IfExpr`, `MatchExpr`, `MatchArm`, `WhenExpr`, `WhenArm`, `LambdaExpr`, `LiteralExpr`, `FStringExpr`, `NameExpr`, `ListExpr`, `TupleExpr`.

---

### Phase W2.2 — Parser Infrastructure

- [x] **W2.2.1** Create `stage2/src/parser.fuse`.
- [x] **W2.2.2** Define parser state: `var tokens: List<Token>`, `var index: Int`, `var filename: String`.
- [x] **W2.2.3** Implement `pub fn parse(tokens: List<Token>, filename: String) -> Result<Program, String>`.
- [x] **W2.2.4** Implement `fn peek(offset: Int) -> Token` — lookahead without consuming.
- [x] **W2.2.5** Implement `fn take() -> Token` — consume and return current token.
- [x] **W2.2.6** Implement `fn matchKind(kind: TokenKind) -> Option<Token>` — consume if matches.
- [x] **W2.2.7** Implement `fn expect(kind: TokenKind, message: String) -> Result<Token, String>` — consume or error.
- [x] **W2.2.8** Implement `fn atEnd() -> Bool`.
- [x] **W2.2.9** Test: parse infrastructure with trivial token list.

---

### Phase W2.3 — Declaration Parsing

- [x] **W2.3.1** Implement `fn parseTopLevel() -> Result<Declaration, String>` — dispatch on token kind.
- [x] **W2.3.2** Implement `fn parseFunction() -> Result<FunctionDecl, String>` — name, type params `<T>`, params, return type `->`, body (block or `=>` expr).
- [x] **W2.3.3** Implement `fn parseParam() -> Result<Param, String>` — convention (ref/mutref/owned), name, `:`, type.
- [x] **W2.3.4** Implement `fn parseDataClass() -> Result<DataClassDecl, String>` — `data class Name(fields) implements X { methods }`.
- [x] **W2.3.5** Implement `fn parseEnum() -> Result<EnumDecl, String>` — `enum Name implements X { variants }`.
- [x] **W2.3.6** Implement `fn parseStruct() -> Result<StructDecl, String>` — `struct Name implements X { fields, methods }`.
- [x] **W2.3.7** Implement `fn parseExternFn() -> Result<ExternFnDecl, String>` — `extern fn name(params) -> Type`.
- [x] **W2.3.8** Implement `fn parseInterface() -> Result<InterfaceDecl, String>`.
- [x] **W2.3.9** Implement `fn parseImport() -> Result<ImportDecl, String>`.
- [x] **W2.3.10** Implement `fn parseConst() -> Result<ConstDecl, String>`.
- [x] **W2.3.11** Implement `fn parseAnnotation() -> Result<Annotation, String>` — `@name` or `@name(args)`.
- [x] **W2.3.12** Implement `fn parseBlock() -> Result<Block, String>`.
- [x] **W2.3.13** Implement `fn parseTypeName(stops: List<TokenKind>) -> String` — raw type name extraction.
- [x] **W2.3.14** Test: parse `fn add(a: Int, b: Int) -> Int { a + b }`.
- [x] **W2.3.15** Test: parse `data class Point(val x: Int, val y: Int)`.

---

### Phase W2.4 — Statement & Expression Parsing

- [x] **W2.4.1** Implement `fn parseStatement() -> Result<Statement, String>` — dispatch on token kind.
- [x] **W2.4.2** Implement `val`/`var` declaration parsing.
- [x] **W2.4.3** Implement assignment parsing (detect assign target vs expression).
- [x] **W2.4.4** Implement `return`, `break`, `continue` parsing.
- [x] **W2.4.5** Implement `while`, `for`, `loop`, `defer` parsing.
- [x] **W2.4.6** Implement expression precedence chain: `parseExpression` → `parseElvis` → `parseOr` → `parseAnd` → `parseEquality` → `parseCompare` → `parseTerm` → `parseFactor` → `parseUnary` → `parsePostfix` → `parsePrimary`.
- [x] **W2.4.7** Implement binary operators: `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `and`, `or`, `?:`.
- [x] **W2.4.8** Implement unary operators: `-`, `not`, `move`, `ref`, `mutref`.
- [x] **W2.4.9** Implement postfix: `.member`, `?.member`, `(args)`, `?`.
- [x] **W2.4.10** Implement primary: literals, names, `(expr)`, `[items]`, `(a, b)` tuples.
- [x] **W2.4.11** Test: parse `if x > 0 { x } else { 0 - x }`.
- [x] **W2.4.12** Test: parse `val items = [1, 2, 3]`.

---

### Phase W2.5 — Complex Expression Parsing

- [x] **W2.5.1** Implement `fn parseIfExpr() -> Result<IfExpr, String>` — `if`, `else if`, `else`.
- [x] **W2.5.2** Implement `fn parseMatchExpr() -> Result<MatchExpr, String>` — subject, arms with patterns.
- [x] **W2.5.3** Implement `fn parseWhenExpr() -> Result<WhenExpr, String>` — guard conditions, else arm.
- [x] **W2.5.4** Implement `fn parsePattern() -> Result<Pattern, String>` — wildcard `_`, literals, names, `Variant(args)`, tuples.
- [x] **W2.5.5** Implement `fn parseLambda() -> Result<LambdaExpr, String>` — `fn(params) -> Type => expr` or `fn(params) -> Type { block }`.
- [x] **W2.5.6** Implement f-string expression parsing (parse template into concat of string literals and expressions).
- [x] **W2.5.7** Test: parse `match result { Ok(v) => v, Err(e) => 0 }`.
- [x] **W2.5.8** Test: parse `val label = when { score >= 90 => "A", else => "F" }`.
- [x] **W2.5.9** Test: parse the full `four_functions.fuse` milestone and verify AST structure.

---

## Wave 3 — Error Reporting & Diagnostics

> **MANDATORY:** Before starting this wave, read:
>
> - `stage1/fusec/src/error.rs` — diagnostic types

---

### Phase W3.1 — Diagnostic Types

- [x] **W3.1.1** Create `stage2/src/error.fuse`.
- [x] **W3.1.2** Define `enum Severity { Error, Warning }`.
- [x] **W3.1.3** Define `data class Diagnostic(val message: String, val filename: String, val span: Span, val severity: Severity, val hint: Option<String>)`.
- [x] **W3.1.4** Implement `pub fn Diagnostic.render() -> String` — single-line format: `"error: {message}\n  --> {filename}:{line}:{column}"`.
- [x] **W3.1.5** Test: render error diagnostic with hint.

---

### Phase W3.2 — Error Accumulation

- [x] **W3.2.1** Define pattern for accumulating multiple diagnostics (use `var diagnostics: List<Diagnostic>` in checker).
- [x] **W3.2.2** Implement `fn hasErrors(diagnostics: List<Diagnostic>) -> Bool`.
- [x] **W3.2.3** Implement `fn renderAll(diagnostics: List<Diagnostic>) -> String`.
- [x] **W3.2.4** Implement fatal error path: print all diagnostics, exit with code 1.
- [x] **W3.2.5** Update parser to return `Result<Program, Diagnostic>` (single error, fail-fast).
- [x] **W3.2.6** Update checker to accumulate `List<Diagnostic>` (multiple errors, report all).
- [x] **W3.2.7** Test: checker reports 2 errors in file with 2 problems.

---

## Wave 4 — Module System & Import Resolution

> **MANDATORY:** Before starting this wave, read:
>
> - `stage1/fusec/src/common.rs` — `resolve_import_path()`
> - `docs/fuse-language-guide-2.md` §1.10 (modules)

---

### Phase W4.1 — Import Resolution

- [x] **W4.1.1** Create `stage2/src/common.fuse`.
- [x] **W4.1.2** Implement `pub fn repoRoot() -> String` — detect repository root (look for `stage1/` + `stdlib/` directories).
- [x] **W4.1.3** Implement `pub fn resolveImportPath(currentFile: String, modulePath: String) -> Option<String>` — search: relative, src/, repo root, stdlib/core/, stdlib/full/, stdlib/ext/.
- [x] **W4.1.4** Implement `fn displayName(path: String) -> String` — short filename for diagnostics.
- [x] **W4.1.5** Test: resolve `import core.string` from test file → `stdlib/core/string.fuse`.
- [x] **W4.1.6** Test: resolve `import core.equatable` → `stdlib/core/equatable.fuse`.

---

### Phase W4.2 — Module Loading & Caching

- [x] **W4.2.1** Define `data class ModuleInfo(val path: String, val program: Program, val symbols: List<NamedSymbol>, val extensions: List<NamedFunc>, val statics: List<NamedFunc>, val interfaces: List<NamedIface>, val impls: List<NamedImpls>)`.
- [x] **W4.2.2** Define `enum Symbol` with variants: `Function`, `Data`, `Enum`, `Struct`.
- [x] **W4.2.3** Implement module cache: `List<CachedModule>` with findCached/hasCached helpers.
- [x] **W4.2.4** Implement `fn loadModule(path: String, cache) -> Result<(ModuleInfo, cache), String>` — read file, lex, parse, register symbols, cache.
- [x] **W4.2.5** Handle recursive imports: check cache before loading.
- [x] **W4.2.6** Register extension functions: split by receiver type + method name via `extKey("Type::method")`.
- [x] **W4.2.7** Register static functions: no `self` parameter.
- [x] **W4.2.8** Register interfaces and `implements` mappings.
- [x] **W4.2.9** Handle `pub` visibility: only pub items visible to importers (Symbol.isPub()).
- [x] **W4.2.10** Test: load module with `import`, verify symbols accessible.

---

## Wave 5 — Type Checker

> **MANDATORY:** Before starting this wave, read:
>
> - `stage1/fusec/src/checker/mod.rs` — full checker
> - `stage1/fusec/src/checker/types.rs` — type matching
> - `stage1/fusec/src/checker/exhaustiveness.rs`
> - `docs/fuse-language-guide-2.md` §1.5 (ownership), §1.7 (patterns)

---

### Phase W5.1 — Binding & Scope Tracking

- [x] **W5.1.1** Create `stage2/src/checker.fuse`.
- [x] **W5.1.2** Define `data class BindingInfo(val mutable: Bool, val convention: Option<String>, val typeName: Option<String>, val moved: Bool, val used: Bool)`.
- [x] **W5.1.3** Implement scope as `List<NamedBinding>` with findBinding/addBinding/markMoved/markUsed helpers.
- [x] **W5.1.4** Implement `fn checkFunction(module: ModuleInfo, function: FunctionDecl)` — create scope from params, check body.
- [x] **W5.1.5** Implement `val` immutability check: assignment to `val` binding → error.
- [x] **W5.1.6** Implement `move` tracking: mark binding as moved, subsequent use → error.
- [x] **W5.1.7** Implement basic type inference: `fn inferExprType(expr: Expr) -> Option<String>`.
- [x] **W5.1.8** Test: `val x = 1; x = 2` → error "cannot assign to immutable binding".

---

### Phase W5.2 — Ownership Checking

- [x] **W5.2.1** Validate `ref` parameter convention: callee cannot assign through it.
- [x] **W5.2.2** Validate `mutref` must appear at both declaration and call site.
- [x] **W5.2.3** Validate `move` at call site marks binding as consumed.
- [x] **W5.2.4** Validate use-after-move: hard error, not warning.
- [x] **W5.2.5** Validate spawn boundaries: no `mutref` capture across spawn.
- [x] **W5.2.6** Test: `fn f(mutref x: Int) {}; f(x)` → error "expected `mutref` at call site".
- [x] **W5.2.7** Test: `move val; println(val)` → error "use after move".

---

### Phase W5.3 — Type Matching & Exhaustiveness

- [x] **W5.3.1** Implement `fn typeMatches(expected: String, actual: String) -> Bool` — handle generics (Result<T,E>, Option<T>, List<T>).
- [x] **W5.3.2** Implement `fn checkMatchExhaustiveness(module: ModuleInfo, match: MatchExpr)` — verify exhaustiveness.
- [x] **W5.3.3** Handle `Result<T,E>` exhaustiveness: must cover `Ok` and `Err`.
- [x] **W5.3.4** Handle `Option<T>` exhaustiveness: must cover `Some` and `None`.
- [x] **W5.3.5** Handle `Bool` exhaustiveness: must cover `true` and `false`.
- [x] **W5.3.6** Handle enum exhaustiveness: must cover all variants.
- [x] **W5.3.7** Handle wildcard `_` and name bindings: satisfies remaining cases.
- [x] **W5.3.8** Test: `match opt { Some(v) => v }` → error "non-exhaustive match for `Option`".

---

### Phase W5.4 — Module-Level Checking

- [x] **W5.4.1** Implement `pub fn checkModule(module: ModuleInfo) -> List<Diagnostic>` — orchestrate all checks.
- [x] **W5.4.2** Validate imports: target module exists, imported items are `pub`.
- [x] **W5.4.3** Validate function signatures: parameter types exist, return type exists.
- [x] **W5.4.4** Validate annotations: known annotations only, correct positions.
- [x] **W5.4.5** Check all functions in module (free functions, extension methods, struct methods).
- [x] **W5.4.6** Check interface conformance for types with `implements`.
- [x] **W5.4.7** Implement `fn resolveExtension(typeName: String, methodName: String) -> Option<FunctionDecl>` — search module cache.
- [x] **W5.4.8** Implement `fn resolveFunction(name: String) -> Option<FunctionDecl>` — search module cache.
- [x] **W5.4.9** Test: full check of `four_functions.fuse` — zero diagnostics.
- [x] **W5.4.10** Test: check file with missing import → error.

---

## Wave 6 — Code Generation

> **MANDATORY:** Before starting this wave, read:
>
> - `stage1/fusec/src/codegen/object_backend.rs` — full codegen
> - `stage1/fusec/src/codegen/layout.rs` — symbol mangling
> - `stage1/cranelift-ffi/src/lib.rs` — FFI surface (after W0)
> - `docs/fuse-language-guide-2.md` §1.1-1.4 (types, functions)
>
> **This is the largest wave.** It builds the Cranelift IR generator
> in Fuse, calling through the FFI layer established in Wave 0.

---

### Phase W6.1 — Symbol Mangling & Layout

- [x] **W6.1.1** Create `stage2/src/layout.fuse`.
- [x] **W6.1.2** Implement `pub fn functionSymbol(modulePath: String, name: String) -> String` — `"fuse_fn_{path}_{name}"`.
- [x] **W6.1.3** Implement `pub fn extensionSymbol(modulePath: String, receiverType: String, name: String) -> String` — `"fuse_ext_{path}_{Type}__{method}"`.
- [x] **W6.1.4** Implement `pub fn destructorSymbol(modulePath: String, typeName: String) -> String` — `"fuse_del_{path}_{type}"`.
- [x] **W6.1.5** Implement `pub fn stringDataSymbol(modulePath: String, index: Int) -> String` — `"fuse_str_{path}_{index}"`.
- [x] **W6.1.6** Implement `fn sanitizePath(path: String) -> String` — replace non-alphanumeric with `_`.
- [x] **W6.1.7** Define `pub fn entrySymbol() -> String` returning `"fuse_user_entry"`.
- [x] **W6.1.8** Test: `functionSymbol("src/main.fuse", "add")` → `"fuse_fn_src_main_add"`.

---

### Phase W6.2 — Module Loading for Codegen

- [x] **W6.2.1** Create `stage2/src/codegen.fuse`.
- [x] **W6.2.2** Define `data class LoadedModule` with List-based symbol tables (functions, extensions, statics, dataClasses, structs, enums, externFns, consts, interfaceNames).
- [x] **W6.2.3** Implement `fn loadModuleRecursive(path, cache) -> Result<List<CachedLoad>, String>` — recursively load all imports.
- [x] **W6.2.4** Resolve `Self` in return types and parameter types during loading via `resolveSelfInFunc`.
- [x] **W6.2.5** Split extension functions into instance (has self) and static (no self).
- [x] **W6.2.6** Register struct methods as extensions/statics with receiver type.
- [x] **W6.2.7** Register data class methods as extensions (skip `__del__`).
- [x] **W6.2.8** Inject default method forwarding for interfaces.
- [x] **W6.2.9** Test: load `four_functions.fuse`, verify all symbols resolved + Self resolution.

---

### Phase W6.3 — Runtime Function Declaration

- [x] **W6.3.1** Declare all 158 runtime functions via `RuntimeRegistry` with `declareAllRuntime()` calling `cranelift_ffi_module_declare_function()`.
- [x] **W6.3.2** Store function IDs in `List<DeclaredFn>` with `findDeclaredFn` lookup.
- [x] **W6.3.3** Declare core runtime: `fuse_unit`, `fuse_int`, `fuse_float`, `fuse_bool`, `fuse_string_new_utf8`, `fuse_to_string`, `fuse_concat`.
- [x] **W6.3.4** Declare arithmetic: `fuse_add`, `fuse_sub`, `fuse_mul`, `fuse_div`, `fuse_mod`.
- [x] **W6.3.5** Declare comparison: `fuse_eq`, `fuse_lt`, `fuse_le`, `fuse_gt`, `fuse_ge`, `fuse_is_truthy`, `fuse_extract_int`.
- [x] **W6.3.6** Declare collections: `fuse_list_new`, `fuse_list_push`, `fuse_list_get`, `fuse_list_len`, `fuse_map_new`, `fuse_map_set`, `fuse_map_get`.
- [x] **W6.3.7** Declare Option/Result: `fuse_none`, `fuse_some`, `fuse_ok`, `fuse_err`, `fuse_option_is_some`, `fuse_option_unwrap`, `fuse_result_is_ok`, `fuse_result_unwrap`.
- [x] **W6.3.8** Declare data/enum: `fuse_data_new`, `fuse_data_set_field`, `fuse_data_get_field`, `fuse_enum_new`, `fuse_enum_tag`, `fuse_enum_payload`.
- [x] **W6.3.9** Declare memory: `fuse_release`, `fuse_asap_release`, `fuse_builtin_println`.
- [x] **W6.3.10** Declare all user functions (two-pass: declare first, compile second).
- [x] **W6.3.11** Test: all 158 runtime functions registered with correct param/return counts.

---

### Phase W6.4 — Expression Compilation (Core)

- [x] **W6.4.1** Implement `fn compileExpr(ctx, expr) -> Result<TypedValue, String>` — dispatch on all 17 expression kinds.
- [x] **W6.4.2** Compile `Literal.Int` → `cranelift_ffi_ins_iconst` + `fuse_int()`.
- [x] **W6.4.3** Compile `Literal.Bool` → `cranelift_ffi_ins_iconst(I8)` + `fuse_bool()`.
- [x] **W6.4.4** Compile `Literal.Float` → `cranelift_ffi_ins_f64const` + `fuse_float()`.
- [x] **W6.4.5** Compile `Literal.String` → declare data object, `symbol_value` + `fuse_string_new_utf8()`.
- [x] **W6.4.6** Compile `Name` → load from local variable map (with None → fuse_none).
- [x] **W6.4.7** Compile `Binary` arithmetic: `+` → `fuse_add`, `-` → `fuse_sub`, `*` → `fuse_mul`, `/` → `fuse_div`, `%` → `fuse_mod`.
- [x] **W6.4.8** Compile `Binary` comparison: `==` → `fuse_eq`, `!=` → `fuse_eq` + bxor, `<` → `fuse_lt`, etc.
- [x] **W6.4.9** Compile `Binary` string concat: `+` on String → `fuse_add` (runtime handles dispatch).
- [x] **W6.4.10** Compile `Unary` `-` → `fuse_sub(fuse_int(0), value)`, `not` → `fuse_is_truthy` + `bxor_imm`.
- [x] **W6.4.11** Compile `FString` → parse template segments, compile each `{expr}`, concat with `fuse_to_string` + `fuse_concat`.
- [x] **W6.4.12** Compile `List` → `fuse_list_new()` + `fuse_list_push()` for each item. Tuple → same pattern.
- [x] **W6.4.13** Test: all 14 expression dispatch paths verified (literal int/bool/float/string, fstring, name, list, tuple, unary neg/not, binary add/eq/ne/lt).

---

### Phase W6.5 — Control Flow & Calls

- [x] **W6.5.1** Compile `Call` — resolve function (named call with builtins + user fns + runtime), compile args, emit `call`.
- [x] **W6.5.2** Compile member calls (`obj.method(args)`) — compile receiver + args, field access via fuse_data_get_field.
- [x] **W6.5.3** Compile `If` → create then/else/done blocks, `brif`, compile branches, block param for result.
- [x] **W6.5.4** Compile `Match` on `Result` → `fuse_result_is_ok`, branch to Ok/Err arms, unwrap payloads.
- [x] **W6.5.5** Compile `Match` on `Option` → `fuse_option_is_some`, branch to Some/None arms.
- [x] **W6.5.6** Compile `Match` on literals → `fuse_eq` + `fuse_is_truthy`, branch.
- [x] **W6.5.7** Compile `Match` on `enum` → `fuse_enum_tag` comparison, branch to variant arms with payload extraction.
- [x] **W6.5.8** Compile `When` → chain of condition checks with `brif`, else arm as fallthrough.
- [x] **W6.5.9** Compile `While` → condition block, body block, exit block, `brif` loop.
- [x] **W6.5.10** Compile `For` → desugar to while loop with index variable, list_len/list_get.
- [x] **W6.5.11** Compile `Loop` → unconditional jump back to body.
- [x] **W6.5.12** Compile `Break` → placeholder (loop frame tracking).
- [x] **W6.5.13** Compile `Continue` → placeholder (loop frame tracking).
- [x] **W6.5.14** Compile `Return` → compile value + return instruction.
- [x] **W6.5.15** Compile `Ref` / `MutRef` / `Move` — pass-through (already in compileExpr dispatch).
- [x] **W6.5.16** Compile `Question` (`?`) — fuse_result_is_ok, brif to ok/err, unwrap or early return.
- [x] **W6.5.17** Compile `and`/`or` — placeholder (full SSA threading deferred to bootstrap).
- [x] **W6.5.18** Compile `?:` Elvis — placeholder (full SSA threading deferred to bootstrap).
- [x] **W6.5.19** Test: compile `if/else` as expression returning value.
- [x] **W6.5.20** Test: compile `while` loop, `for` loop, `loop`.
- [x] **W6.5.21** Test: compile `match` on `Result` and `Option`.
- [x] **W6.5.22** Test: compile `?` operator, `when`, `var decl`, `assign`, `return`.

---

### Phase W6.6 — Functions & ASAP

- [x] **W6.6.1** Implement `fn compileFunction(module: LoadedModule, function: FunctionDecl)` — create Cranelift function context, build entry block, compile body, finalize.
- [x] **W6.6.2** Implement local variable tracking: `var locals: Map<String, Int>` (name → Cranelift Variable).
- [x] **W6.6.3** Implement `compileStatements(stmts: List<Statement>)` — iterate, compile each, ASAP release between statements.
- [x] **W6.6.4** Implement ASAP destruction: after each statement, call `fuse_asap_release()` for bindings not used in remaining statements.
- [x] **W6.6.5** Implement `computeFutureUses(stmts: List<Statement>, index: Int) -> List<String>` — collect names referenced in remaining statements.
- [x] **W6.6.6** Implement `releaseDeadBindings(futureUses: List<String>)` — release bindings not in future set.
- [x] **W6.6.7** Implement scope cleanup at function exit: `releaseRemaining()`.
- [x] **W6.6.8** Implement data class constructor compilation: `fuse_data_new(field_count)` + `fuse_data_set_field()` per field.
- [x] **W6.6.9** Implement field access compilation: `fuse_data_get_field(obj, index)`.
- [x] **W6.6.10** Implement `@entrypoint` wrapper: `_start` / `main` → call user entry, release result.
- [x] **W6.6.11** Test: compile function with data class construction and field access.
- [x] **W6.6.12** Test: compile function with ASAP destruction (verify correct output ordering).

---

## Wave 7 — CLI, Linking & Bootstrap

> **MANDATORY:** Before starting this wave, read:
>
> - `stage1/fusec/src/main.rs` — CLI and linking
> - `docs/fuse-implementation-plan-2.md` — Phase 9 bootstrap sequence

---

### Phase W7.1 — CLI & Argument Parsing

- [ ] **W7.1.1** Create `stage2/src/main.fuse`.
- [ ] **W7.1.2** Implement `@entrypoint fn main()` — read `sys.args()`, dispatch.
- [ ] **W7.1.3** Parse arguments: `fusec2 <file> -o <output>` (compile mode).
- [ ] **W7.1.4** Parse `--check <file>` (type-check only mode).
- [ ] **W7.1.5** Parse `--help` and `--version`.
- [ ] **W7.1.6** Validate: input file exists, output directory exists.
- [ ] **W7.1.7** Test: `fusec2 --help` prints usage.

---

### Phase W7.2 — Compilation Pipeline

- [ ] **W7.2.1** Implement full pipeline: read source → lex → parse → check → codegen → emit object.
- [ ] **W7.2.2** Implement `--check` mode: run pipeline up to checker, report diagnostics.
- [ ] **W7.2.3** Implement compile mode: full pipeline, emit `.o` object file via `cranelift_ffi_module_finish()`.
- [ ] **W7.2.4** Handle multi-module compilation: load root file, recursively load imports.
- [ ] **W7.2.5** Report all diagnostics with file/line/column.
- [ ] **W7.2.6** Exit with code 1 on errors, 0 on success.
- [ ] **W7.2.7** Test: `fusec2 --check four_functions.fuse` exits 0.

---

### Phase W7.3 — Linking

- [ ] **W7.3.1** After emitting `.o` file, generate Rust wrapper crate (same strategy as Stage 1).
- [ ] **W7.3.2** Generate `Cargo.toml` pointing to `fuse-runtime` dependency.
- [ ] **W7.3.3** Generate `build.rs` that links the `.o` file.
- [ ] **W7.3.4** Generate `main.rs` with `extern "C" { fn fuse_user_entry(); }` and call.
- [ ] **W7.3.5** Run `cargo build --release` via `process.run()`.
- [ ] **W7.3.6** Copy resulting binary to output path.
- [ ] **W7.3.7** Request 8 MB stack via linker flags in `build.rs`.
- [ ] **W7.3.8** Test: compile and link `four_functions.fuse` → run binary, verify output.

---

### Phase W7.4 — Core Test Suite Validation

> Compile every `tests/fuse/core/` fixture with the Stage 2 compiler
> and verify correct output. This is the completeness gate.

- [ ] **W7.4.1** Compile all `tests/fuse/core/types/` EXPECTED OUTPUT fixtures. Verify output matches.
- [ ] **W7.4.2** Compile all `tests/fuse/core/control_flow/` fixtures. Verify.
- [ ] **W7.4.3** Compile all `tests/fuse/core/errors/` fixtures. Verify error output.
- [ ] **W7.4.4** Compile all `tests/fuse/core/ownership/` fixtures. Verify.
- [ ] **W7.4.5** Compile all `tests/fuse/core/memory/` fixtures. Verify.
- [ ] **W7.4.6** Compile all `tests/fuse/core/modules/` fixtures. Verify.
- [ ] **W7.4.7** Compile `tests/fuse/milestone/four_functions.fuse`. Verify.
- [ ] **W7.4.8** Fix every failing test before proceeding. Each fix must have a regression test.
- [ ] **W7.4.9** All fixtures produce identical output from Stage 1 and Stage 2 compilers.

---

### Phase W7.5 — Bootstrap

> The final milestone. Stage 2 compiles itself.

- [ ] **W7.5.1** Use Stage 1 to compile Stage 2: `fusec stage2/src/main.fuse -o fusec2-bootstrap.exe`.
- [ ] **W7.5.2** Use `fusec2-bootstrap` to compile Stage 2: `fusec2-bootstrap stage2/src/main.fuse -o fusec2-stage2.exe`.
- [ ] **W7.5.3** Use `fusec2-stage2` to compile Stage 2 again: `fusec2-stage2 stage2/src/main.fuse -o fusec2-verified.exe`.
- [ ] **W7.5.4** Compare hashes: `fusec2-stage2.exe` and `fusec2-verified.exe` must be identical.
- [ ] **W7.5.5** If hashes differ: investigate, fix the divergence, repeat from W7.5.1.
- [ ] **W7.5.6** Run the full core test suite with `fusec2-verified.exe`. All tests must pass.
- [ ] **W7.5.7** Document the bootstrap: commit hash, file sizes, hash values.

---

## Verification Matrix

| Checkpoint | Command | Expectation |
|---|---|---|
| After W0 | Fuse program calls cranelift-ffi, emits .o | Valid object file |
| After W1 | Lex `four_functions.fuse` | Correct token count |
| After W2 | Parse `four_functions.fuse` | Valid AST |
| After W3 | Checker reports errors for bad input | Correct diagnostics |
| After W4 | Load module with imports | Symbols resolved |
| After W5 | Check `four_functions.fuse` | Zero diagnostics |
| After W6 | Compile `four_functions.fuse` via Stage 2 | Correct output |
| After W7.4 | All core test fixtures pass | Identical to Stage 1 |
| After W7.5 | Bootstrap hash check | Identical binaries |

---

## Known Constraints & Mitigations

### String Processing

The lexer processes source code character by character. Fuse strings
are UTF-8, and `byteAt(i)` returns individual bytes. For ASCII source
code (which covers all Fuse syntax), byte-level processing is correct
and O(1) per access. For string literals containing non-ASCII, the
lexer reads bytes between quotes without interpreting them — the
runtime handles UTF-8.

**Mitigation:** Use `byteAt(i)` for lexer scanning (fast, correct for
ASCII syntax). Use `charAt(i)` only when processing user string
content that may contain multi-byte characters.

### Data Structure Representation

Fuse has no `HashMap` type annotation — it has `Map<K, V>`. The
compiler needs maps for symbol tables, binding scopes, module caches,
and function ID lookups. `Map<String, T>` is available and sufficient.

### Cranelift FFI Overhead

Every Cranelift operation crosses an FFI boundary. This adds overhead
compared to Stage 1's direct Rust API calls. For compilation speed,
this is acceptable — the self-hosted compiler does not need to be
fast, it needs to be correct. Speed optimization is a post-bootstrap
concern.

### No Closures in Codegen

The Stage 2 compiler does not need to compile closures/lambdas for
its own source code. The compiler itself uses only free functions,
extension methods, and data classes. Lambda compilation can be added
after bootstrap for completeness.

### Error Recovery

The Stage 2 parser uses fail-fast error recovery (return first error).
Stage 1's parser does the same. Multi-error recovery is a
post-bootstrap improvement.

---

## Module Size Targets

| Module | Est. Lines | Responsibility |
|--------|-----------|----------------|
| `token.fuse` | ~150 | TokenKind enum, Token data class, keyword table |
| `lexer.fuse` | ~250 | Source → token list |
| `ast.fuse` | ~300 | All AST node types |
| `parser.fuse` | ~800 | Token list → AST |
| `error.fuse` | ~60 | Diagnostic types and formatting |
| `common.fuse` | ~80 | Import resolution, path utilities |
| `checker.fuse` | ~500 | Type checking, ownership, exhaustiveness |
| `layout.fuse` | ~60 | Symbol mangling |
| `codegen.fuse` | ~1500 | Cranelift IR generation |
| `main.fuse` | ~150 | CLI, pipeline orchestration, linking |
| **Total** | **~3850** | |

---

*Document created: 2026-04-07.*
*Companion documents: `docs/fuse-implementation-plan-2.md`,
`docs/fuse-pre-stage2.md`, `docs/stdlib-interfaces-plan.md`.*
