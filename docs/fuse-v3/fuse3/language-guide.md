# Fuse Language Guide

> **Status:** normative. This document is the specification of the Fuse programming language. If the compiler disagrees with this document, the compiler is wrong. If this document is silent, the feature does not exist.
>
> **Companion documents** (same directory):
> - `rules.md` — discipline rules for contributors and AI agents. Read this on every session.
> - `repository-layout.md` — physical layout of the Fuse repository.
> - `implementation-plan.md` — wave-by-wave plan for building the compiler.

---

## Table of contents

0. [Preamble](#0-preamble)
1. [Philosophy and pillars](#1-philosophy-and-pillars)
2. [Source files and modules](#2-source-files-and-modules)
3. [Lexical structure](#3-lexical-structure)
4. [Primitive types](#4-primitive-types)
5. [Compound types](#5-compound-types)
6. [Ownership and memory model](#6-ownership-and-memory-model)
7. [Expressions](#7-expressions)
8. [Statements and control flow](#8-statements-and-control-flow)
9. [Pattern matching](#9-pattern-matching)
10. [Functions and closures](#10-functions-and-closures)
11. [Traits](#11-traits)
12. [Error handling](#12-error-handling)
13. [Concurrency](#13-concurrency)
14. [Foreign function interface](#14-foreign-function-interface)
15. [Runtime surface contract](#15-runtime-surface-contract)
16. [Standard library structure](#16-standard-library-structure)
17. [Compilation, targets, and the CLI](#17-compilation-targets-and-the-cli)
18. [Reserved but not shipped](#18-reserved-but-not-shipped)
19. [Appendix A — Keyword list](#19-appendix-a--keyword-list)
20. [Appendix B — Operator precedence](#20-appendix-b--operator-precedence)
21. [Appendix C — Grammar summary](#21-appendix-c--grammar-summary)

---

## 0. Preamble

### 0.1 What this document is

A normative description of the Fuse language, sufficient for a compiler author to build a conforming implementation and for a library author to write portable code against it. Every feature described here is either shipped on day one or explicitly marked as reserved.

### 0.2 What this document is not

Not a tutorial. Not a history. Not a collection of examples for their own sake. Examples appear only where they clarify semantics that prose alone makes ambiguous.

### 0.3 Guide precedes implementation

No feature may appear in the compiler without first appearing in this document. If a contributor finds that the compiler needs a feature not described here, the contributor MUST update this document first, get the change reviewed, and only then implement it. This rule is not negotiable; it is the single discipline that prevents a language from becoming whatever the last commit happened to do.

### 0.4 Conforming implementations

A conforming implementation:

1. Accepts every program this document declares well-formed.
2. Rejects every program this document declares ill-formed.
3. Produces runtime behavior consistent with the dynamic semantics in this document.
4. Produces deterministic output for the same input, compiler version, and target triple.

Determinism is load-bearing: see `rules.md` §Determinism.

### 0.5 Versioning

The language version is a single integer. `fuse --version` prints `fuse <lang-version> (<compiler-build>)`. This document describes **Fuse language version 1**.

---

## 1. Philosophy and pillars

### 1.1 The three pillars

Fuse rests on three pillars. Every feature in this document can be traced to one or more of them.

**Pillar 1 — Memory safety without garbage collection.**
Programs must not have use-after-free, double-free, data races on non-atomic memory, or wild pointer dereferences, except inside an explicit `unsafe { }` block. This is achieved without a tracing garbage collector and without a borrow checker, using a combination of ownership keywords, compile-time liveness analysis, and a destructor protocol. See §6.

**Pillar 2 — Concurrency safety without a borrow checker.**
Shared state is reached through `Shared[T]` with compile-time lock-order (`@rank(N)`) checks. Message-passing uses `Chan[T]`. Thread spawning uses `spawn`. No `async`/`await`. No hidden scheduler. See §13.

**Pillar 3 — Developer experience as a first-class constraint.**
The call site is always diagnosable. `mutref` at the call site tells the reader which arguments get mutated. `unsafe { }` at the call site tells the reader where the rules weaken. `?` at the call site tells the reader where errors propagate. A developer scanning a function body can predict its effects without jumping to definitions.

These pillars are the tiebreaker. When two designs compete, the one that better serves the pillars wins, even if the other is simpler or more familiar.

### 1.2 Language DNA

Fuse inherits:

- **Module layout, traits, and `implements` at the declaration site** from the Rust family.
- **Value-types-by-default with opt-in auto-derivation** from the Mojo-style `@value struct`.
- **Readable keyword-driven syntax** from Python (but with required semicolons and explicit braces).
- **Ownership discipline with manual destructors** from C++ RAII, cleaned up.
- **Channel-based concurrency** from Go and Occam, but without green threads on day one.

Fuse rejects:

- Exceptions. Errors are values (`Result[T, E]`).
- Null. Absence is `Option[T]`.
- Implicit conversion between numeric types.
- Implicit copy of non-`@value` data.
- `async`/`await`. Concurrency is structured through `spawn`, `Chan`, and `Shared`.
- A hidden runtime. The runtime surface is a finite list of C entry points (§15).

### 1.3 What Fuse is not

Fuse is not a scripting language. Programs are compiled ahead of time to native code and then executed. There is no REPL that interprets source directly; the `fuse repl` subcommand (§17) drives a JIT-compiled session but still runs compiled code.

Fuse is not a garbage-collected language. There is no tracing GC, no reference counting in the default case, and no hidden allocation at assignment.

Fuse is not a dynamically typed language. Every expression has a statically known type by the end of the checker pass.

Fuse is not a dependency-heavy language. A conforming distribution depends only on: a C11 compiler reachable through the `CC` environment variable or as `cc` on `PATH`, a libc, and a POSIX-like threads implementation on non-Windows targets (the Windows target uses Win32 primitives directly).

---

## 2. Source files and modules

### 2.1 File encoding and extension

Fuse source files are UTF-8 encoded, have the extension `.fuse`, and use LF line endings. CRLF line endings are accepted on input and normalized to LF. A byte-order mark at the start of a file is rejected.

### 2.2 Source file structure

A source file has this shape, in order:

1. Zero or one **module attribute block** (`#![...]` lines).
2. Zero or one **module doc comment** (consecutive `//!` lines).
3. Zero or more **imports** (`import` and `pub import` statements).
4. Zero or more **top-level declarations** (functions, structs, traits, constants, `extern fn` blocks).

Anything outside this order is a parse error.

### 2.3 Modules and the `src/` layout

A Fuse package has a `src/` directory at its root. The file `src/main.fuse` is the package entry point if it defines a `pub fn main() -> Int { ... }`. Otherwise the package is a library.

Modules correspond one-to-one with files. An import `import a.b.c` resolves to `src/a/b/c.fuse` relative to the package root. There are no `mod.fuse` files, no implicit parent modules, and no multi-file modules. One module, one file.

### 2.4 `import` and `pub import`

```fuse
import core.string;            // makes the `string` module visible as `string`
import core.string as s;       // aliased import
import core.list.{push, pop};  // selective import of named items
pub import core.result;        // re-export for consumers of this module
```

An `import` brings names into the importing module's scope. A `pub import` additionally re-exports those names so that a consumer of the importing module sees them as if they had been defined there. Re-export is explicit: there is no implicit inheritance of `pub` items from imports.

Cyclic imports are a compile-time error. The module graph is a DAG.

### 2.5 Visibility

A declaration without `pub` is private to its module. A declaration with `pub` is visible to any module that imports it. There is no `pub(crate)`, `pub(super)`, or other scoped visibility. The only two levels are **module-private** and **public**.

### 2.6 Module doc comments

A module doc comment is one or more consecutive lines beginning with `//!`, placed at the top of the file after the optional attribute block and before the first import. Tooling extracts module doc comments for `fuse doc`.

```fuse
//! String manipulation primitives.
//! This module provides UTF-8 string operations that do not require
//! OS access.
```

### 2.7 Module attributes

A module attribute begins with `#![` and ends with `]`. The attribute block, if present, is the first non-blank content in the file. Recognized attributes:

- `#![forbid(unsafe)]` — fails compilation if the module contains any `unsafe { }` block.
- `#![no_std]` — reserved, currently rejected (the `core` tier is always available).
- `#![deny(warnings)]` — escalates every warning in this module to a hard error.

Unknown attributes are a compile-time error. There is no extensibility hatch.

### 2.8 No trailing commas

Function call arguments, function parameters, struct literals, and list literals reject trailing commas. The rationale is that trailing commas mask arity mistakes; requiring an explicit last element forces the reader to see the final item.

---

## 3. Lexical structure

### 3.1 Comments

- Line comment: `// ...` to end of line.
- Block comment: `/* ... */`. Block comments nest.
- Doc comment (item): `/// ...` attaches to the next declaration.
- Doc comment (module): `//!` at the top of the file, as in §2.6.

Comments are whitespace for the parser; they are not a token kind.

### 3.2 Identifiers

An identifier begins with an ASCII letter or underscore, followed by ASCII letters, digits, or underscores. Unicode identifiers are not permitted. The single underscore `_` is a pattern wildcard, not an identifier.

Identifiers are case-sensitive. By convention:

- Types and traits use `UpperCamelCase`.
- Functions, variables, parameters, and fields use `lowerCamelCase`.
- Constants use `SCREAMING_SNAKE_CASE`.
- Modules use `lower_snake_case`.

Case conventions are not enforced by the compiler, but `fuse fmt` will rewrite a file that violates them if `--fix` is passed.

### 3.3 Keywords

The following words are **reserved** and may not be used as identifiers:

```
and       as        break     case      class     const     continue
data      default   do        else      enum      extern    false
fn        for       if        impl      implements import    in
let       loop      match     move      mut       mutref    not
or        owned     pub       ref       return    self      Self
spawn     struct    trait     true      type      unsafe    use
var       where     while     with
```

The following are also reserved for future use and currently produce a dedicated "reserved keyword" diagnostic when used:

```
async     await     yield     try       catch     throw     finally
macro     typeof    sizeof    alignof   static    dyn       union
select
```

`Vec` and `@simd` are reserved type and attribute names respectively, described in §18.

### 3.4 Literals

**Integer literals.**

```
0          42         1_000_000      0xFF       0b1010     0o777
42i32      42u64      42usize
```

An integer literal with no suffix has type `Int`. Suffixes `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`, `usize`, `isize` fix the type. Underscores are permitted between digits but not at the start, end, or immediately after the base prefix.

**Floating-point literals.**

```
1.0      1.0e3      1.0e-3      .5       // rejected: must start with digit
1.0f32   1.0f64
```

A floating-point literal with no suffix has type `Float`. Suffixes `f32` and `f64` fix the type.

**Boolean literals.** `true` and `false`, type `Bool`.

**Character literals.** Single-quoted, UTF-8 encoded: `'a'`, `'\n'`, `'\u{1F600}'`. Type `Char`, representing a Unicode scalar value in `0..=0x10FFFF` excluding the surrogate range.

**String literals.** Double-quoted, UTF-8: `"hello"`, `"line\nbreak"`, `"\u{1F600}"`. Type `String`. Raw string literals use `r"..."` or `r#"..."#` for embedded quotes, without escape processing.

**The `none` literal.** Writes an empty `Option[T]` where `T` is inferred from context. Type `Option[_]`. See §5.8.

**The unit literal.** `()` is the only value of type `()`, called the unit type. It is returned by functions that have no meaningful return value.

### 3.5 Operators and punctuation

The following multi-character tokens are recognized:

```
==  !=  <=  >=  <<  >>  &&  ||  ->  =>  ..  ..=  ::  :=  +=  -=  *=  /=  %=
&=  |=  ^=  <<= >>= ?.  ??  #!  //  /*  */  ///  //!
```

Single-character operators and punctuation:

```
+ - * / % & | ^ ~ < > = ! ? . , ; : ( ) [ ] { } @ #
```

The `!` character appears only as part of `!=`, as the Never type marker (§4.5), as part of `#!` or `//!`, or in macro-like attribute positions that are currently reserved. It is **not** a boolean negation operator; use `not` for that (§7.4). The consequence is that `!x` is a parse error, which forces the reader to use the more readable `not x`.

### 3.6 Whitespace, newlines, and semicolons

Whitespace separates tokens and is otherwise insignificant. Statements are terminated by `;`. Newlines have no syntactic meaning; there is no automatic semicolon insertion.

This rule is deliberate. Automatic semicolon insertion makes error recovery worse and produces subtle parse changes when code is reformatted. A required `;` at the end of every statement is cheap and unambiguous.

---

## 4. Primitive types

### 4.1 Integer types

The signed integer types are `I8`, `I16`, `I32`, `I64`, `ISize`. The unsigned integer types are `U8`, `U16`, `U32`, `U64`, `USize`. The width-less types `Int` and `UInt` are aliases for the target's natural word size: 32 bits on 32-bit targets, 64 bits on 64-bit targets, always matching `ISize` and `USize`.

All integer types have:

- Defined overflow behavior. By default, arithmetic overflow panics in debug builds and wraps in release builds. The library functions `checkedAdd`, `saturatingAdd`, `wrappingAdd`, etc. make the choice explicit.
- No implicit conversion to or from other numeric types. `let x: I64 = some_i32` is a compile error; use `some_i32.toI64()`.
- Bit width exposed by `I32.BITS`, `I64.BITS`, etc.
- Min and max exposed by `I32.MIN`, `I32.MAX`, etc.

### 4.2 Floating-point types

`F32` (IEEE 754 binary32) and `F64` (IEEE 754 binary64). `Float` is an alias for `F64`. Operations follow IEEE 754 including NaN and infinity. Comparison uses total order when requested via the `Comparable` trait on `F32`/`F64` (total order distinguishes NaN bit patterns); the default `<`/`>` operators follow IEEE partial order, which is not total.

### 4.3 `Bool`

Exactly two values: `true` and `false`. One byte in memory. Not interconvertible with integers without explicit `.toInt()`.

### 4.4 `Char` and `String`

`Char` holds a single Unicode scalar value. `String` is a heap-allocated, UTF-8-encoded, immutable-by-default byte sequence. A `String` knows its length in bytes at O(1); its length in characters is O(n). See §16 for the operations available.

`String` is **not** a primitive in the sense of being hardcoded into the compiler. It is a stdlib type defined in `core.string`. The compiler treats string literals by emitting calls into the `core.string` constructor. This is intentional: it forces the language to support whatever machinery strings need, rather than special-casing.

### 4.5 `!` — the Never type

`!` is the type of expressions that never produce a value: `return x`, `panic("...")`, `loop { }` with no `break`, and any function whose body diverges on every path. The type system treats `!` as a subtype of every type, so an expression of type `!` can appear wherever any other type is expected.

`!` exists as a type marker only; it does not have a value. It is written as the single token `!` and appears in return positions: `fn panic(msg: String) -> ! { ... }`. The token `!` has no other meaning in a type context; see §3.5 for how this is kept unambiguous.

### 4.6 The unit type

`()` is the type whose only value is `()`. A function with no `->` clause implicitly returns `()`. A block whose final expression is a statement (ends with `;`) has type `()`.

### 4.7 `Ptr[T]` — FFI-only raw pointer

`Ptr[T]` is an opaque raw pointer to `T`, used only at the FFI boundary. It has no methods in safe code. Dereferencing a `Ptr[T]` requires `unsafe { }`. `Ptr[T]` carries no ownership information; the programmer is responsible for every lifetime decision. See §14.

---

## 5. Compound types

### 5.1 `struct` — the opaque struct

```fuse
struct FileHandle {
    fd: I32,
    path: String,
}
```

An opaque `struct` is a nominal product type. It has no auto-generated methods. The programmer must hand-write any of `__copyinit__`, `__moveinit__`, `__del__`, `__eq__`, `__hash__`, or any trait implementation. A plain `struct` does **not** auto-implement `Equatable`, `Hashable`, `Comparable`, `Printable`, or `Debuggable`; if the programmer tries to use such a `struct` where those traits are required, the compiler emits an error that directs them to one of the two remedies: use `@value struct` or `data class`, or implement the trait manually.

Plain `struct` is the right choice for types that wrap a resource with a non-trivial lifecycle (file handles, sockets, mutexes) where an automatically generated copy constructor would be wrong.

### 5.2 `@value struct` — the value-type struct

```fuse
@value struct Point {
    x: F64,
    y: F64,
}
```

A `@value struct` gets auto-generated `__copyinit__` (field-wise copy), `__moveinit__` (field-wise move), and `__del__` (field-wise destruction in reverse declaration order). It is eligible for auto-derivation of the Core traits listed in §11.6 if all its fields are.

`@value` is the right choice for types that behave like mathematical values: their semantics are captured by their field values and they have no external resource to clean up.

### 5.3 `data class` — the data class

```fuse
data class User(name: String, age: Int)
```

A `data class` is a `@value struct` with:

- **Public positional fields** declared in the header.
- **Auto-generated `toString`** derived from the field values.
- **Auto-generated `==` and `!=`** via the `Equatable` trait.
- **Auto-generated positional constructor** `User("Alice", 30)`.

A `data class` cannot declare custom methods in its body — it is a pure data container. If you need methods, use `@value struct`.

### 5.4 Tuples

```fuse
let p: (I32, String) = (42, "hello");
let (a, b) = p;
let first = p.0;
```

A tuple is a structural product type. Tuple types are equal if their component types are equal in order. Tuple elements are accessed by zero-indexed positional access (`.0`, `.1`, etc.). Tuples with zero or one element are not written as tuples: the zero-tuple is the unit type `()`, and a one-element tuple is just the element. Tuples larger than twelve elements are rejected — use a `struct`.

### 5.5 Arrays and slices

```fuse
let a: [I32; 4] = [1, 2, 3, 4];
let s: [I32] = a[1..3];    // slice
```

A **fixed-size array** `[T; N]` has a compile-time known length `N` and is allocated inline in its owner. A **slice** `[T]` is a pointer-plus-length view into a contiguous sequence; it does not own its data and must be bounded by the lifetime of whatever does.

Slices are the first-class way to pass contiguous data to functions. The length is available as `slice.len`.

### 5.6 Function types

```fuse
let f: (I32, I32) -> I32 = add;
```

A function type has the form `(T1, T2, ..., Tn) -> R`. Ownership keywords on parameters are part of the type: `(mutref Buffer, I32) -> ()` is a different type from `(ref Buffer, I32) -> ()`.

### 5.7 `Option[T]`

```fuse
enum Option[T] {
    Some(T),
    None,
}
```

`Option[T]` is the standard way to represent "a value of type `T` or nothing". The literal `none` produces `Option.None` with `T` inferred from context. The shorthand `Some(x)` and `None` are in the prelude.

### 5.8 `Result[T, E]`

```fuse
enum Result[T, E] {
    Ok(T),
    Err(E),
}
```

`Result[T, E]` is the standard way to represent "either success with a value, or failure with an error". The `?` operator (§7.11) short-circuits on `Err`. `Result` and its variants are in the prelude.

### 5.9 Enums

```fuse
enum Shape {
    Circle(F64),
    Rectangle(F64, F64),
    Square(F64),
}
```

An `enum` is a tagged union (sum type). Variants may be empty (`None`), have positional payloads (`Circle(F64)`), or be struct-like with named fields (`Rectangle { w: F64, h: F64 }`). The compiler guarantees that a `match` over an enum is exhaustive.

The tag representation is an implementation detail; programs must not assume a particular layout. FFI code that needs to interoperate with C-style enums uses `extern enum` (§14).

### 5.10 Generics

Types and functions may be parameterized by type parameters.

```fuse
fn head[T](xs: ref List[T]) -> Option[T] { ... }

@value struct Pair[A, B] {
    first: A,
    second: B,
}
```

Type parameters may have trait bounds:

```fuse
fn max[T: Comparable](a: T, b: T) -> T { ... }
```

Multiple bounds use `+`:

```fuse
fn dedup[T: Equatable + Hashable](xs: ref List[T]) -> List[T] { ... }
```

A `where` clause may be used for longer bound lists:

```fuse
fn frobnicate[T, U](x: T, y: U) -> U
    where T: Printable, U: Default + Equatable { ... }
```

Generic instantiation is monomorphization: each distinct set of type arguments produces a distinct compiled function. There is no type erasure and no runtime dispatch for generic calls.

### 5.11 Reserved: `Vec[T, N]`

`Vec[T, N]` is reserved for fixed-width SIMD vector types (e.g. `Vec[F32, 4]`). It is a lexer keyword on day one but emits a "not yet implemented" error if actually used. See §18.

---

## 6. Ownership and memory model

### 6.1 Overview

Fuse manages memory without a garbage collector and without a borrow checker. The mechanism has four pieces:

1. **Four ownership keywords** — `ref`, `mutref`, `owned`, `move` — that annotate how a value flows across a function boundary.
2. **A liveness analysis** — ASAP (As Soon As Possible) destruction — computed once per HIR function and consulted by every later pass.
3. **A destructor protocol** — `__copyinit__` / `__moveinit__` / `__del__` — that every type must either provide or decline.
4. **A closure escape rule** — escaping closures must explicitly `move` their captures — that closes the one case the first three cannot handle.

Together these are sufficient to guarantee memory safety for all twenty-three normal allocation patterns. The twenty-fourth case — raw FFI pointers — is the documented exception that requires `unsafe { }`.

### 6.2 The four ownership keywords

**`ref T` — shared borrow.** The caller retains ownership; the callee gets read-only access. A `ref` parameter cannot be assigned to and its fields cannot be modified. The value pointed to must outlive the call.

```fuse
fn sumAll(xs: ref List[I32]) -> I32 { ... }  // reads xs, does not consume it
```

**`mutref T` — mutable borrow.** The caller retains ownership; the callee gets read-write access. A `mutref` parameter can be assigned to and its fields can be modified, but the parameter itself cannot be dropped.

```fuse
fn sort(xs: mutref List[I32]) -> () { ... }  // sorts in place
```

**A `mutref` parameter MUST be annotated `mutref` at the call site as well.**

```fuse
let mut xs = List[I32].new();
sort(mutref xs);    // required
sort(xs);           // compile error: "sort expects mutref; annotate the call site"
```

This is load-bearing for Pillar 3. Reading the call site tells you which arguments are mutated.

**`owned T` — ownership transfer.** The caller gives the value to the callee. After the call, the caller may not use the value unless the callee returns it. This is the implicit default for `self` on `__del__` and for return values.

```fuse
fn consume(x: owned Widget) -> () { ... }
```

**`move` — explicit move at a use site.** The `move` keyword appears at a **use** site, not a declaration. It tells the compiler to transfer ownership from the named binding to the expression.

```fuse
let w = Widget.new();
consume(move w);   // after this line, w is no longer usable
```

The compiler's liveness analysis will automatically insert `move` at the **last use** of a binding when the target parameter is `owned T`. The explicit `move` keyword is required only when:

1. The move is not at the last use (the programmer wants to force-move earlier).
2. The binding is captured by a closure that escapes its scope (§6.6).
3. The target is an `owned` field in a struct literal.

### 6.3 Parameter defaults

When a parameter has no ownership annotation, the default depends on its type:

- If the type is a primitive (integer, float, bool, char, `Ptr[T]`), the default is **by-value copy**. Primitives are always trivially copyable.
- If the type is `@value struct` or `data class`, the default is **by-value copy**. The auto-generated `__copyinit__` is called.
- If the type is a plain `struct`, the default is **`ref`**. The caller retains ownership; the callee cannot modify.
- If the type is a collection (`List[T]`, `Map[K, V]`, `String`), the default is **`ref`**. Passing a collection by value requires explicit `owned` or `move`.

The default was chosen to make the common case minimal noise while keeping the behavior predictable: small things copy, big things borrow. When in doubt, annotate explicitly.

### 6.4 ASAP destruction

A value's destructor runs **immediately after its last use**, not at the end of its enclosing block. This is called ASAP destruction.

```fuse
fn example() -> () {
    let a = openFile("a.txt");   // a is live
    let b = openFile("b.txt");   // b is live
    let content = a.readAll();   // a's last use is here
                                 // a.__del__() runs immediately
    b.write(content);            // b's last use is here
                                 // b.__del__() runs immediately
                                 // content.__del__() runs immediately
}
```

The order of destruction at a single site is the reverse of the order of the last uses, so that a value that depends on another is destroyed first. When two destructors have equivalent positions, the order is declaration-reverse.

ASAP destruction is computed by a single liveness pass during HIR lowering. The result is attached as a per-node `LiveAfter` attribute and consulted by every later pass. There is no second liveness computation in the codegen.

### 6.5 The destructor protocol

A type that owns resources implements the destructor protocol by defining one or more of the following methods:

```fuse
impl Widget {
    fn __copyinit__(self: ref Widget) -> Widget { ... }   // called when a copy is needed
    fn __moveinit__(self: owned Widget) -> Widget { ... } // called when ownership transfers
    fn __del__(self: owned Widget) -> () { ... }          // called at the end of life
}
```

A `@value struct` gets all three auto-generated. A plain `struct` gets none auto-generated; the programmer MUST decide. A `struct` that leaves `__copyinit__` undefined cannot be copied; the compiler rejects any code that would require a copy.

The destructor protocol is symmetric: if you write `__copyinit__` you usually also write `__del__`. The compiler does not require this, but the linter warns.

### 6.6 The escaping-closure rule

A closure captures its environment implicitly by reference when it is non-escaping:

```fuse
fn forEach(xs: ref List[I32], f: (ref I32) -> ()) -> () { ... }
forEach(ref xs, |x| print(x));  // fine — closure does not escape
```

A closure that **escapes** its defining scope — returned, stored in a struct, passed to `spawn`, or sent on a `Chan` — MUST explicitly `move` each captured binding:

```fuse
fn later(x: owned String) -> () -> () {
    return move |_: ()| print(x);   // `move` makes the capture explicit
}
```

Without the `move` keyword, the compiler rejects the closure with a diagnostic that names the captured binding and recommends adding `move`.

The rationale: a closure that outlives its defining scope cannot borrow locals from that scope. Requiring `move` makes the ownership transfer visible at the closure site rather than inferred silently. This is the one allocation case the other mechanisms cannot handle on their own.

### 6.7 `unsafe { }` blocks

The `unsafe { }` block is the escape hatch. Inside `unsafe { }`, the programmer may:

- Dereference a `Ptr[T]`.
- Call an `extern fn`.
- Call a Fuse function that is itself marked `unsafe fn`.
- Perform a `transmute` (reserved).

The `unsafe` block does **not** disable type checking, ownership checking on safe code, or trait checking. It disables only the four safety rules above, and only inside the block.

```fuse
let p: Ptr[U8] = extern_get_buffer();
let first = unsafe { *p };   // raw dereference
```

An `unsafe { }` block appears at every point where the safety rules weaken. This is load-bearing for Pillar 3: a reader can `grep unsafe` to see the full attack surface of a module.

A module marked `#![forbid(unsafe)]` fails to compile if any `unsafe { }` block appears in it.

### 6.8 `#![forbid(unsafe)]` and the stdlib

The three stdlib tiers are constrained:

- **Core**: no `unsafe { }` except in the small set of files that bridge to the runtime (documented by name in `repository-layout.md`).
- **Full**: `unsafe { }` allowed where the OS surface requires it (file handles, socket FDs). Every `unsafe { }` in Full must have a preceding comment stating the invariant that justifies it.
- **Ext**: same rules as Full.

User code is expected to use `#![forbid(unsafe)]` as the default and relax it only where truly necessary.

### 6.9 Summary of the memory safety guarantee

A program that compiles without `unsafe { }` blocks and without FFI is guaranteed to have:

- No use-after-free.
- No double-free.
- No data race on non-atomic memory.
- No wild pointer dereference.
- No uninitialized read.
- No buffer overflow on safe slices.
- No null dereference (there are no nulls).

This guarantee does not extend into `unsafe { }` blocks or across FFI boundaries, where the programmer is responsible.

---

## 7. Expressions

### 7.1 Overview

Fuse is an expression-oriented language. A block is an expression whose value is the value of its final expression (or `()` if the final statement ends in `;`). An `if` is an expression. A `match` is an expression. A loop can produce a value via `break val`.

### 7.2 Literals as expressions

Every literal form from §3.4 is an expression.

### 7.3 Arithmetic and numeric operators

```
+  -  *  /  %
```

Binary arithmetic requires both operands to have the same numeric type. There is no implicit promotion between widths or between signed and unsigned. `/` on integers is truncating; `%` follows the sign of the dividend.

Unary minus `-` applies to signed integers and floats. Unary plus is not a valid operator.

### 7.4 Logical operators and `not`

```
and   or   not
```

`and` and `or` are short-circuiting; both operands must be `Bool`. `not` is unary prefix: `not done`, `not x.isEmpty()`.

The keyword `not` is used rather than `!` so that the character `!` remains unambiguous (§3.5). The keywords `and`/`or` are used rather than `&&`/`||` to make logical flow read as prose. The symbols `&&` and `||` are currently accepted as synonyms for `and` and `or` in expressions, and `fuse fmt` normalizes to the keywords.

### 7.5 Comparison operators

```
==  !=  <  <=  >  >=
```

Comparison requires both operands to implement `Equatable` (for `==`, `!=`) or `Comparable` (for `<`, `<=`, `>`, `>=`). Comparison between different types is a compile error.

### 7.6 Bitwise operators

```
&  |  ^  ~  <<  >>
```

Bitwise operators apply only to integer types. `~` is unary (bitwise NOT). Shift by an amount greater than or equal to the operand's bit width is defined: the result is zero for `<<` and `>>` on unsigned types, and arithmetic (sign-extended) for `>>` on signed types. The amount must be non-negative.

### 7.7 Assignment

```
=  +=  -=  *=  /=  %=  &=  |=  ^=  <<=  >>=
```

Assignment is a **statement**, not an expression. It has no value. Chained assignment (`a = b = c`) is a parse error.

The compound forms (`+=` etc.) are syntactic sugar for `x = x op rhs` with one evaluation of `x`.

### 7.8 Field access

```
point.x
user.name
```

Field access on a `ref` or `mutref` binding is a compile-time shortcut that does not copy the value. Field access on an `owned` binding that appears at the last use transfers ownership of the field (partial move).

### 7.9 Method calls

```
list.push(42)
buf.readAll()
```

The receiver's ownership is determined by the method signature. `self: ref Self` is the most common; `self: mutref Self` is required for mutation; `self: owned Self` consumes the receiver.

Uniform function call syntax is not supported. A method is called on its receiver, full stop.

### 7.10 Index

```
arr[3]
map[key]
slice[1..4]
```

Indexing desugars to a method call on the `Index` or `IndexMut` trait (defined in the Core tier). `arr[i]` on a `mutref` binding is an `IndexMut` call; on a `ref` binding it is an `Index` call.

Range syntax `a..b` produces a `Range[Int]`; `a..=b` produces a `RangeInclusive[Int]`. These are used both for slicing and for `for` loops (§8.5).

### 7.11 `?` — error propagation

The postfix `?` operator short-circuits on `Err` or `None`:

```fuse
fn readInt(path: ref String) -> Result[Int, IoError] {
    let content = readFile(path)?;     // if Err, return Err(it)
    let n = parseInt(ref content)?;    // if Err, return Err(it)
    return Ok(n);
}
```

`x?` desugars to:

```fuse
match x {
    case Ok(v) => v,
    case Err(e) => return Err(e.into()),
}
```

(or the `Option` analogue: `Some(v)` / `None => return None`). The `.into()` call performs an error conversion via the `From[E]` trait if the function's return error type is not `E` but some supertype; if `From[E]` is not implemented, it is a compile error.

`?` is only usable in a function whose return type is itself a `Result` or `Option` of a compatible shape.

### 7.12 Block expressions

A block `{ stmt; stmt; expr }` has the type of its final expression. A block `{ stmt; stmt; }` (ending in a semicolon) has type `()`. Blocks create a fresh scope.

### 7.13 `if` expressions

```fuse
let msg = if done { "yes" } else { "no" };
```

An `if` with no `else` branch has type `()`; its then-branch must also have type `()`. An `if` with an `else` branch requires both branches to unify to a single type (§9).

There is no ternary operator; `if`-as-expression covers that use case.

### 7.14 `match` expressions

See §9 for full pattern grammar. A `match` is an expression; its arms must all unify to a single type.

### 7.15 Struct literals

```fuse
let p = Point { x: 1.0, y: 2.0 };
let u = User("Alice", 30);     // data class positional constructor
```

Struct literals provide all fields in any order; missing fields are a compile error. There is no field update syntax (`..other`) on day one.

### 7.16 Closure literals

```fuse
|x: I32| x + 1
|x, y| x + y       // parameter types inferred from context
move |x| x + base  // `move` captures, see §6.6
```

A closure literal has a function type (§5.6). Parameter types may be inferred when the closure appears in a context that fixes them (e.g. an argument to `forEach`).

---

## 8. Statements and control flow

### 8.1 `let` and `var`

```fuse
let x = 42;           // immutable binding
let x: I32 = 42;      // immutable with explicit type
var y = 0;            // mutable binding
var y: I32 = 0;       // mutable with explicit type
```

`let` creates an immutable binding. `var` creates a mutable binding. The binding keyword is required; there is no `:=` shorthand in statement position (the token `:=` is reserved for future use).

Shadowing is allowed; a new `let` or `var` with the same name introduces a fresh binding and the previous one is no longer accessible by name.

### 8.2 Assignment statements

```fuse
y = 7;
point.x = 3.14;
buf[0] = 'x';
```

Assignment is a statement. The left-hand side must be a place expression: a `var` binding, a field access of a mutable binding, or an index into a mutable binding.

### 8.3 `if` / `else if` / `else`

```fuse
if cond {
    ...
} else if other {
    ...
} else {
    ...
}
```

Braces are required. There is no single-statement `if` without braces.

### 8.4 `while`

```fuse
while cond {
    ...
}
```

`while` loops have type `()`; they cannot produce a value. Use `loop` with `break val` if a loop must produce a value.

### 8.5 `for`

```fuse
for x in 0..10 {
    print(x);
}

for item in ref list {
    process(item);
}
```

`for P in E` iterates over any expression `E` whose type implements the `Sequence` trait (§11.6). The pattern `P` may be a simple binding, a tuple pattern, or a struct pattern.

Iteration over a collection defaults to borrowing: `for x in list` binds `x` as a `ref` into `list`. To consume the collection, write `for x in move list`.

### 8.6 `loop`

```fuse
let result = loop {
    let n = next();
    if n == 0 { break 42; }
};
```

`loop { ... }` is an infinite loop. Its type is the type of its `break expr` expressions unified together, or `!` if there is no `break` with a value. `break` with no value gives the loop type `()`.

### 8.7 `break` and `continue`

`break` exits the nearest enclosing loop. `break expr` exits with a value, permitted only inside `loop` (not `while` or `for`, whose types are fixed at `()`).

`continue` restarts the nearest enclosing loop at its next iteration.

Labels (`'outer: loop { ... break 'outer; }`) are not supported on day one. If nested breaking is required, refactor into a helper function.

### 8.8 `return`

```fuse
return;         // only valid in `-> ()` functions
return expr;
```

`return` exits the current function. Its type is `!` (§4.5).

### 8.9 Block statements

A block `{ stmt; stmt; }` is itself a statement when used in statement position. It introduces a new scope and is useful for limiting the lifetime of temporaries:

```fuse
{
    let tmp = expensive();
    use(tmp);
}   // tmp destroyed here by ASAP, though also at the closing brace
```

---

## 9. Pattern matching

### 9.1 `match` expression

```fuse
match shape {
    case Circle(r) => pi * r * r,
    case Rectangle(w, h) => w * h,
    case Square(s) => s * s,
}
```

A `match` expression evaluates its scrutinee once and tries each arm in order. The first arm whose pattern matches runs. The `match` expression has the type obtained by unifying all arm bodies under the rules of §9.3.

A `match` must be **exhaustive**: the set of patterns must cover every possible value of the scrutinee's type. Non-exhaustive `match` is a compile error with a diagnostic that names a missing pattern.

A `match` must not be **redundant**: each arm must be reachable. An unreachable arm is a hard error, not a warning, because in practice an unreachable arm almost always indicates a bug.

### 9.2 Pattern grammar

- **Literal pattern**: `42`, `"hello"`, `true`, `'x'`.
- **Wildcard**: `_` matches anything and binds nothing.
- **Binding**: `x` binds the matched value to `x`.
- **Enum variant**: `Circle(r)`, `None`, `Point { x, y }`.
- **Tuple**: `(a, b, c)`.
- **Struct**: `Point { x, y }` or `Point { x, .. }` (rest wildcard).
- **Or-pattern**: `A | B | C` matches any alternative; all alternatives must bind the same names with the same types.
- **Range**: `1..=9` matches integers in that inclusive range.
- **Guard**: `case Some(n) if n > 0 => ...`.

### 9.3 Arm unification rules (U1–U7)

When a `match` has N arms, the compiler must produce a single type for the whole expression. The rules are:

**U1.** If all arms have the same type `T`, the match has type `T`.

**U2.** An arm of type `!` (diverges) is ignored for type computation. A match all of whose arms are `!` has type `!`.

**U3.** An arm of type `()` forces all other non-`!` arms to be `()`. A type mismatch under this rule is a hard error.

**U4.** If the non-`!` arms contain both `Option[T]` and `T`, the unification is `Option[T]` and the bare-`T` arm is implicitly wrapped in `Some(...)`. This is the only implicit wrap the compiler performs.

**U5.** If two arms have numerically different integer types, the unification fails. The programmer must convert explicitly.

**U6.** If arms have the same base type but different ownership (`ref T` vs `owned T`), the unification is `owned T` and the `ref` arms force a copy (if `T: @value`) or a compile error (if not).

**U7.** Otherwise, the unification fails and the compiler reports the first pair of arms that disagree. The diagnostic names both types and the arm locations.

The seven rules are deliberately tight. The goal is that a programmer can read them and predict the result of any non-trivial `match` without needing to run the compiler.

### 9.4 Guards

A guard is an arbitrary `Bool` expression attached to a pattern with `if`:

```fuse
case Some(n) if n > 0 => ...,
case Some(_) => ...,
case None => ...,
```

Guards do **not** contribute to exhaustiveness checking. The compiler assumes a guarded arm might fail and requires the match to be exhaustive without relying on the guard.

### 9.5 Irrefutable patterns

A pattern that cannot fail to match (e.g. a simple binding, a tuple destructure) is called irrefutable. Irrefutable patterns are allowed in `let` and `for`:

```fuse
let (a, b) = getPair();
for (key, value) in map { ... }
```

A refutable pattern in `let` is a compile error; use `match` instead.

---

## 10. Functions and closures

### 10.1 Function declarations

```fuse
fn add(a: I32, b: I32) -> I32 {
    return a + b;
}

pub fn greet(name: ref String) -> () {
    print("Hello, ");
    print(name);
}
```

A function has a name, a parameter list, an optional return type (`()` if omitted), and a body block. `pub fn` makes the function visible outside the module.

### 10.2 Parameters

Each parameter has the form `name: ownership? Type`. The ownership keyword is one of `ref`, `mutref`, `owned`, or omitted (in which case the default of §6.3 applies).

Default parameter values are not supported on day one; use overloads or builder structs.

### 10.3 Return types

```fuse
fn f() -> I32 { ... }        // returns I32
fn g() -> () { ... }         // returns unit
fn h() { ... }               // omitted return type, equivalent to -> ()
fn panic(msg: String) -> ! { ... }  // diverges
```

A function with `-> !` must have every code path end in `return` of a non-returning expression, `panic`, or an infinite loop with no `break`. The checker verifies this.

### 10.4 Generic functions

```fuse
fn swap[T](a: mutref T, b: mutref T) -> () {
    let tmp = move *a;
    *a = move *b;
    *b = move tmp;
}
```

Type parameters appear in square brackets after the function name. Bounds use the syntax of §5.10.

### 10.5 Methods

Methods are functions defined inside an `impl` block for a type:

```fuse
impl Point {
    pub fn new(x: F64, y: F64) -> Point {
        return Point { x: x, y: y };
    }

    pub fn distance(self: ref Point, other: ref Point) -> F64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        return (dx * dx + dy * dy).sqrt();
    }
}
```

The first parameter of a method is `self`, with its own ownership keyword. A method with no `self` parameter is an associated function and is called with `TypeName.method(args)` (note the dot, not `::`).

### 10.6 Closures

A closure is an anonymous function literal. It has one of two kinds:

- **Non-escaping closure**: lives only as long as its call's activation record. May capture locals by `ref` or `mutref` implicitly.
- **Escaping closure**: returned, stored, or sent to another thread. MUST use `move` on each capture (§6.6).

The compiler decides which kind a given closure is by looking at whether it flows into a position that outlives the current scope. If the programmer wants to force an escape-safe closure even when it would otherwise be non-escaping, they write `move |args| body`.

Closure parameter types are inferred from context where possible. If a closure appears in a context that does not determine its parameter types, they must be annotated explicitly.

---

## 11. Traits

### 11.1 Trait declarations

A trait describes a set of methods that a type can implement:

```fuse
pub trait Printable {
    fn print(self: ref Self) -> ();
}
```

`Self` inside a trait refers to the implementing type. Method signatures may refer to `Self`, to associated types, and to the type parameters of the trait itself.

### 11.2 Default methods

A trait may provide default implementations that implementing types may override:

```fuse
pub trait Greeter {
    fn name(self: ref Self) -> String;
    fn greeting(self: ref Self) -> String {
        return "Hello, " + self.name();
    }
}
```

### 11.3 `implements`

A type implements a trait with an `impl ... implements ...` block:

```fuse
impl Point implements Printable {
    fn print(self: ref Point) -> () {
        print("(");
        self.x.print();
        print(", ");
        self.y.print();
        print(")");
    }
}
```

The `implements` keyword is required. There is no blanket `impl Trait for Type` syntax without `implements`; the word is load-bearing because it makes the relationship searchable.

A single `impl` block can implement at most one trait. A type that implements multiple traits uses multiple `impl` blocks.

### 11.4 Trait objects

Trait objects are not supported on day one. All generic dispatch is monomorphized (§5.10). The keyword `dyn` is reserved for future use.

### 11.5 Associated types

```fuse
pub trait Sequence {
    type Item;
    fn next(self: mutref Self) -> Option[Self.Item];
}
```

An associated type is referenced as `Trait.TypeName` or `Self.TypeName`. Associated types are resolved at monomorphization.

### 11.6 The Core trait set

The following traits are defined in `core.traits` and are available on day one. These are called the **Core trait set**.

**`Equatable`** — equality.
```fuse
pub trait Equatable {
    fn equals(self: ref Self, other: ref Self) -> Bool;
}
```
Provides the `==` and `!=` operators via the compiler.

**`Hashable`** — hashing, for use as a `Map` key.
```fuse
pub trait Hashable implements Equatable {
    fn hashInto(self: ref Self, h: mutref Hasher) -> ();
}
```
Every `Hashable` must also be `Equatable`, and `a == b` must imply `a.hashInto(h) == b.hashInto(h)`.

**`Comparable`** — total order.
```fuse
pub trait Comparable implements Equatable {
    fn compare(self: ref Self, other: ref Self) -> Ordering;
}
```
Provides the `<`, `<=`, `>`, `>=` operators via the compiler. `Ordering` is an enum with variants `Less`, `Equal`, `Greater`.

**`Printable`** — user-facing string conversion, for `print` and format.
```fuse
pub trait Printable {
    fn print(self: ref Self, out: mutref StringBuilder) -> ();
}
```

**`Debuggable`** — developer-facing string conversion, for assertions and debug logs.
```fuse
pub trait Debuggable {
    fn debug(self: ref Self, out: mutref StringBuilder) -> ();
}
```

**`Sequence`** — iteration protocol (§8.5).

**`Index[K]` / `IndexMut[K]`** — indexing (§7.10).

**`Default`** — provides `Type.default() -> Self` for types with a meaningful zero value.

**`From[T]` / `Into[T]`** — infallible conversion. `Into` is auto-derived from `From`.

**`TryFrom[T, E]` / `TryInto[T, E]`** — fallible conversion.

**Serialization traits** (`Serializable`, `Encodable`, `Decodable`) are **not** in the Core set; they live in the Ext tier. Core is OS-free and does not depend on any particular serialization format.

### 11.7 Auto-generation from field metadata

A `@value struct` and a `data class` automatically get implementations of:

- `Equatable` — field-wise `equals`.
- `Hashable` — field-wise `hashInto`, if every field is `Hashable`.
- `Comparable` — lexicographic order over fields, if every field is `Comparable`.
- `Printable` — field-wise printing in declaration order.
- `Debuggable` — field-wise debug in declaration order.

A plain `struct` gets **none** of these auto-generated. Attempting to use a plain `struct` where one of these traits is required produces an error of the form:

```
error: type `FileHandle` is a plain `struct` and does not auto-implement `Hashable`.
note: to auto-generate the Core trait set, declare `FileHandle` as `@value struct` or `data class`.
note: to implement `Hashable` by hand, add `impl FileHandle implements Hashable { ... }`.
```

The rule — interfaces for behavior, decorators for directives, never mix — is absolute. A plain `struct` is opaque because the programmer has not declared it safe for auto-trait behavior.

### 11.8 Operator sugar

Some operators are sugar for trait method calls:

| Operator        | Trait             | Method            |
|-----------------|-------------------|-------------------|
| `a == b`        | `Equatable`       | `a.equals(ref b)` |
| `a != b`        | `Equatable`       | `not a.equals(ref b)` |
| `a < b`         | `Comparable`      | `a.compare(ref b) == Ordering.Less` |
| `a <= b`        | `Comparable`      | `a.compare(ref b) != Ordering.Greater` |
| `a[i]` (read)   | `Index[K]`        | `a.get(ref i)` |
| `a[i] = v`      | `IndexMut[K]`     | `a.set(mutref i, v)` |
| `for x in e`    | `Sequence`        | iterator protocol |
| `a?`            | `Try`             | short-circuit protocol |

Operator overloading for user-defined types is achieved by implementing the relevant trait. There is no independent operator-overloading mechanism.

---

## 12. Error handling

### 12.1 No exceptions, no null

Fuse does not have exceptions. A function that can fail returns `Result[T, E]`. A function that can return a missing value returns `Option[T]`. There is no `null`, no `nil`, and no implicit `Optional<T>` unwrapping.

### 12.2 `Result[T, E]`

Defined in §5.8. The idiomatic use is:

```fuse
fn parseConfig(path: ref String) -> Result[Config, ConfigError] {
    let raw = readFile(path)?;
    let parsed = parseToml(ref raw)?;
    let cfg = Config.fromToml(parsed)?;
    return Ok(cfg);
}
```

### 12.3 `Option[T]`

Defined in §5.7. Used for absence rather than failure.

### 12.4 The `?` operator

Defined in §7.11.

### 12.5 Panics

A **panic** is an unrecoverable error. It aborts the current thread after running destructors on its local stack. Panics are used for bugs and invariant violations — not for failures the caller might reasonably want to handle.

```fuse
fn getFirst(xs: ref List[I32]) -> I32 {
    if xs.isEmpty() {
        panic("getFirst called on empty list");
    }
    return xs[0];
}
```

The function `panic(msg: String) -> !` is in the prelude. It prints `msg` to standard error, unwinds the current thread's stack running destructors, and aborts the thread. Whether the process continues depends on whether any other thread is running; if the panicking thread is the main thread, the process exits with a non-zero status.

There is no `catch`. There is no `recover`. Panics are not a flow-control mechanism. If you want handleable failures, use `Result`.

### 12.6 `debugAssert` and `assert`

```fuse
assert(x > 0, "x must be positive");
debugAssert(invariant(), "invariant broken");
```

`assert(cond, msg)` panics if `cond` is false, in every build. `debugAssert(cond, msg)` panics if `cond` is false only in debug builds; in release builds it is a no-op. Both are in the prelude.

---

## 13. Concurrency

### 13.1 The three-tier model

Fuse concurrency has three concepts and no others:

1. **`Chan[T]`** — typed, bounded channels for message passing.
2. **`Shared[T]` + `@rank(N)`** — compile-time-ranked mutexes for shared state.
3. **`spawn`** — OS thread creation.

Everything else about concurrency either builds on these three or is rejected. There is no `async`/`await`, no executor, no global scheduler, no `Future`, no `Promise`.

### 13.2 `Chan[T]` — channels

```fuse
let ch: Chan[I32] = Chan.new(capacity: 16);

spawn(move |_| {
    for i in 0..100 {
        ch.send(i);
    }
    ch.close();
});

while let Some(v) = ch.recv() {
    print(v);
}
```

A `Chan[T]` is a typed, bounded, multi-producer multi-consumer channel. Operations:

- `send(value: owned T)` — block until space is available, then transfer ownership of `value` into the channel. Returns `Result[(), ChannelClosed]`.
- `recv() -> Option[T]` — block until a value is available, then remove and return it. Returns `None` once the channel is closed and drained.
- `tryRecv() -> Option[T]` — non-blocking recv.
- `trySend(value: owned T) -> Result[(), TrySendError[T]]` — non-blocking send; returns the value back on failure.
- `close()` — close the channel. Subsequent `send` fails; subsequent `recv` drains remaining values then returns `None`.
- `len() -> USize`, `capacity() -> USize`, `isClosed() -> Bool` — inspection.

`Chan[T]` is itself `Shared` internally; it is safe to pass by `ref` across threads without any additional wrapping.

### 13.3 `Shared[T]` and `@rank(N)`

A `Shared[T]` wraps a value of type `T` with a mutex:

```fuse
@rank(1)
let counter: Shared[I64] = Shared.new(0);

spawn(move |_| {
    counter.with(mutref |n| *n += 1);
});
```

The `with` method takes a closure that gets called with a mutref to the inner value. The mutex is held for the duration of the closure and released on return.

`@rank(N)` is a compile-time annotation that names the mutex's rank. Inside a `Shared[T].with(...)` body, the only mutexes that may be acquired are those of strictly lower rank. The compiler enforces this statically: attempting to acquire a rank-5 mutex while holding a rank-3 mutex is a compile error.

This is the **compile-time deadlock prevention** mechanism. It is not a runtime check. The ranking is a total order declared per `Shared[T]` binding (usually a global or a field in a struct). For nested locks within a single struct, annotate each `Shared[T]` field with its rank.

### 13.4 `spawn` — thread creation

```fuse
let handle: ThreadHandle = spawn(move |_| {
    doWork();
    return 42;
});

let result: I32 = handle.join();
```

`spawn(f: owned (() -> T)) -> ThreadHandle[T]` creates a new OS thread running `f`. The returned handle can be `join`ed to retrieve the return value. Handles can be `detach`ed to let the thread run without being joined.

On day one, every `spawn` creates a full OS thread via `fuse_rt_thread_create` (see §15). There is no green-thread scheduler, no thread pool, and no work-stealing runtime. A program that spawns thousands of threads will actually create thousands of kernel threads and will pay the corresponding memory cost. This is deliberate: the simple model is correct first, the sophisticated model can come later behind a ship gate (§18).

### 13.5 Memory model

The Fuse memory model is defined in terms of C11 `<stdatomic.h>`:

- **Within a single thread**, program order holds as written.
- **Across threads**, only `Shared[T]` lock acquisitions, `Chan[T]` send/recv pairs, and explicit `Atomic[T]` operations create synchronization edges.
- All Fuse-visible synchronization is **sequentially consistent** at those edges. The compiler is free to reorder within a thread up to that constraint.

Non-atomic shared access between threads outside of `Shared[T]` / `Chan[T]` / `Atomic[T]` is **undefined behavior**. The type system prevents it in safe code: `Shared[T]` is the only way to share a mutable value, and the compiler rejects capturing a non-`Shared` mutable binding into a `spawn`.

### 13.6 `Atomic[T]`

For performance-sensitive code, `Atomic[T]` wraps a primitive type (`I32`, `I64`, `U32`, `U64`, `USize`, `Bool`) with atomic operations:

```fuse
let flag: Atomic[Bool] = Atomic.new(false);
flag.store(true, Ordering.Release);
let cur = flag.load(Ordering.Acquire);
```

Memory orderings are spelled `Ordering.Relaxed`, `Ordering.Acquire`, `Ordering.Release`, `Ordering.AcqRel`, `Ordering.SeqCst`. The semantics match the C11 model.

`Atomic[T]` is a primitive type in the stdlib, and its operations are emitted inline using C11 `<stdatomic.h>`. There is no `fuse_rt_atomic_*` runtime function.

### 13.7 What is not in day one

- **`select` statement** for waiting on multiple channels. Reserved. See §18.
- **Green threads** / user-space scheduler. Reserved. See §18.
- **Thread pools** with work stealing. A user library may build one on top of `spawn`; it is not in the stdlib on day one.
- **Async / await.** Permanently rejected. Do not propose it.

---

## 14. Foreign function interface

### 14.1 Goal

The FFI lets Fuse code call C functions and lets C code call Fuse functions that have been marked `@export`. The FFI is the only way to reach outside the language; it is also the only place where the safety rules weaken. The five FFI rules ensure every weakening is visible.

### 14.2 The five FFI rules

**Rule 1 — Primitive types only at the boundary.**
Only the following types may appear in `extern fn` signatures or `@export` signatures: `I8`, `I16`, `I32`, `I64`, `U8`, `U16`, `U32`, `U64`, `ISize`, `USize`, `F32`, `F64`, `Bool`, `Char`, `Ptr[T]` (where `T` is itself FFI-legal or a primitive), and `()`. `String`, `List[T]`, `@value struct`, and all other Fuse-native types are forbidden at the boundary. To pass a string to C, the programmer converts it to a `Ptr[U8]` plus length via `string.asCBuf()` explicitly.

**Rule 2 — `unsafe { }` required at every call site.**
Every call to an `extern fn` is a compile error unless it appears inside an `unsafe { }` block. This is not a stylistic preference — it is a structural rule enforced by the checker.

**Rule 3 — Fuse generates the matching C header.**
When a module declares `extern fn`s or `@export`s, the compiler generates a header file containing the corresponding C declarations. The programmer does **not** write the header by hand. The header is regenerated on every build. If a downstream C file `#include`s a manually-maintained header that drifts from the Fuse source, the build fails when linking.

**Rule 4 — `_Static_assert` on primitive sizes at the FFI file top.**
The compiler emits a set of `_Static_assert` declarations at the top of every FFI-generated C file asserting that `sizeof(int8_t) == 1`, `sizeof(int32_t) == 4`, `sizeof(void*) == FUSE_PTR_SIZE`, etc. A host compiler with a different primitive size fails to compile the FFI glue, halting the build.

**Rule 5 — No implicit ownership transfer across FFI.**
`Ptr[T]` carries no ownership. If a C function takes a pointer and is supposed to free it, that is a contract the Fuse programmer documents in comments and honors by calling the appropriate deallocation function. The compiler does not track ownership across FFI. If the programmer wants to hand off an owned Fuse value to C, they first convert it to a raw pointer via an explicit `unsafe` operation that returns `Ptr[T]`; the original Fuse value is then logically consumed. Getting this wrong is a programmer bug.

### 14.3 `extern fn`

```fuse
extern fn c_open(path: Ptr[U8], flags: I32, mode: I32) -> I32;
extern fn c_read(fd: I32, buf: Ptr[U8], count: USize) -> ISize;
extern fn c_close(fd: I32) -> I32;

pub fn openOrFail(path: ref String) -> Result[I32, IoError] {
    let cpath = path.asCString();      // yields Ptr[U8]
    let fd = unsafe { c_open(cpath.ptr(), O_RDONLY, 0) };
    if fd < 0 {
        return Err(IoError.fromErrno());
    }
    return Ok(fd);
}
```

`extern fn` declarations appear at the top level of a module. The compiler does not produce an implementation for them; the linker is expected to resolve them against a C object file.

### 14.4 `@export`

```fuse
@export("fuse_add")
pub fn add(a: I32, b: I32) -> I32 {
    return a + b;
}
```

An `@export(name)` attribute marks a Fuse function as callable from C under the given external name. The compiler generates a C header with a declaration matching the Fuse signature. Inside the function, normal Fuse safety rules apply. The compiler emits no unwinding across the export boundary: if the function panics, the panic is converted to an `abort()` at the FFI edge.

`@export` is useful for embedding a Fuse library inside a C or C++ application, or for compiling Fuse to a shared library.

### 14.5 `#![forbid(unsafe)]` interaction

A module with `#![forbid(unsafe)]` cannot contain `extern fn` declarations or `unsafe { }` blocks. It may still contain `@export` declarations, because `@export` does not weaken safety inside the function body.

---

## 15. Runtime surface contract

### 15.1 What the runtime is

The Fuse runtime is a small body of C11 code — in the order of 500 to 700 lines of source — that provides a fixed set of entry points the compiler may call. The runtime has a stable name prefix `fuse_rt_` and a stable ABI. A conforming implementation provides exactly these entry points and no more.

The runtime surface is **closed**. The compiler may not invent new runtime calls beyond this list. If a feature needs a new runtime entry, the feature is blocked on updating this document first.

### 15.2 The entry point list

The runtime provides approximately forty entry points, grouped into eleven categories. The exact names and signatures are frozen as part of the language:

**Memory (5):**
- `fuse_rt_alloc(size: usize) -> *void`
- `fuse_rt_alloc_aligned(size: usize, align: usize) -> *void`
- `fuse_rt_realloc(ptr: *void, new_size: usize) -> *void`
- `fuse_rt_free(ptr: *void) -> ()`
- `fuse_rt_oom() -> !` — called when allocation fails; aborts the process.

**Panic (3):**
- `fuse_rt_panic(msg: *const u8, len: usize) -> !`
- `fuse_rt_panic_with_loc(msg: *const u8, len: usize, file: *const u8, line: u32) -> !`
- `fuse_rt_abort() -> !`

**Raw I/O (4):**
- `fuse_rt_stdout_write(buf: *const u8, len: usize) -> isize`
- `fuse_rt_stderr_write(buf: *const u8, len: usize) -> isize`
- `fuse_rt_stdin_read(buf: *u8, cap: usize) -> isize`
- `fuse_rt_stdin_eof() -> bool`

**Process (3):**
- `fuse_rt_exit(code: i32) -> !`
- `fuse_rt_argc() -> i32`
- `fuse_rt_argv(i: i32) -> *const u8`

**Raw file I/O (5):**
- `fuse_rt_file_open(path: *const u8, path_len: usize, flags: i32) -> i32`
- `fuse_rt_file_read(fd: i32, buf: *u8, cap: usize) -> isize`
- `fuse_rt_file_write(fd: i32, buf: *const u8, len: usize) -> isize`
- `fuse_rt_file_close(fd: i32) -> i32`
- `fuse_rt_file_seek(fd: i32, offset: i64, whence: i32) -> i64`

**Threads (4):**
- `fuse_rt_thread_create(f: *void, arg: *void) -> u64` — returns an opaque handle.
- `fuse_rt_thread_join(handle: u64, out_result: *void) -> i32`
- `fuse_rt_thread_detach(handle: u64) -> i32`
- `fuse_rt_thread_yield() -> ()`

**Mutex (3):**
- `fuse_rt_mutex_init(m: *void) -> i32`
- `fuse_rt_mutex_lock(m: *void) -> i32`
- `fuse_rt_mutex_unlock(m: *void) -> i32`

**RW-lock (5):**
- `fuse_rt_rwlock_init(l: *void) -> i32`
- `fuse_rt_rwlock_read_lock(l: *void) -> i32`
- `fuse_rt_rwlock_read_unlock(l: *void) -> i32`
- `fuse_rt_rwlock_write_lock(l: *void) -> i32`
- `fuse_rt_rwlock_write_unlock(l: *void) -> i32`

**Condvar (4):**
- `fuse_rt_cond_init(c: *void) -> i32`
- `fuse_rt_cond_wait(c: *void, m: *void) -> i32`
- `fuse_rt_cond_signal(c: *void) -> i32`
- `fuse_rt_cond_broadcast(c: *void) -> i32`

**TLS (2):**
- `fuse_rt_tls_get(key: u32) -> *void`
- `fuse_rt_tls_set(key: u32, value: *void) -> i32`

**Time (3):**
- `fuse_rt_time_now_nanos() -> u64` — monotonic clock.
- `fuse_rt_wall_now_nanos() -> i64` — wall-clock time.
- `fuse_rt_sleep_nanos(ns: u64) -> ()`

The exact count may vary by one or two as the compiler author tunes the surface. What is non-negotiable is that the surface is **enumerated**, closed, and prefixed `fuse_rt_`.

### 15.3 What is NOT in the runtime

The following are emitted **inline** by the compiler, not through the runtime:

- **Atomics.** C11 `<stdatomic.h>` is used directly. There is no `fuse_rt_atomic_*`.
- **`memcpy`, `memset`, `memcmp`.** The compiler emits libc calls directly.
- **Arithmetic overflow checks.** Inline conditional branches, not runtime calls.
- **Slice bounds checks.** Inline conditional branches.

The following are **library code written in Fuse on top of the runtime**, not runtime calls themselves:

- **Strings.** `core.string` is written in Fuse. It uses `fuse_rt_alloc` / `fuse_rt_free` for storage.
- **Lists.** `core.list` is written in Fuse.
- **Maps.** `core.map` is written in Fuse.
- **Hashing.** `core.hash` is written in Fuse.
- **Formatting.** `core.fmt` is written in Fuse.

This design avoids the trap of "a runtime that grows unboundedly because every new feature finds an excuse to add a runtime call." The runtime is a narrow waist; the rest of the stdlib sits above it.

### 15.4 Runtime conformance

A conforming runtime implementation:

1. Provides every entry point listed in §15.2 with the listed signature.
2. Has no global state visible to Fuse programs outside of what is documented.
3. Does not call `longjmp` or otherwise unwind across Fuse frames.
4. Uses only `pthread` primitives (or their Win32 equivalents) and libc.
5. Is built with the same C11 compiler the compiler uses for the target.

---

## 16. Standard library structure

### 16.1 The three tiers

The Fuse standard library is split into three tiers, corresponding to three layers of dependencies on the outside world.

**Core — OS-free.**
- No file I/O.
- No network.
- No process or environment access.
- No threading primitives exposed (though the lock primitives that `Shared[T]` builds on are here).
- No clock access.

Core provides: primitive type methods, `Option`, `Result`, `String`, `List[T]`, `Map[K, V]`, `Set[T]`, `hash`, `fmt`, `math`, `iter`, the trait set of §11.6, and `Atomic[T]`.

Core programs can run on any target, including freestanding environments with minimal runtime support.

**Full — standard OS surface.**
- File I/O (`fs.File`, `fs.Dir`).
- Process control (`os.Process`, `os.Env`).
- Standard I/O (`io.stdin`, `io.stdout`, `io.stderr`).
- Time (`time.Instant`, `time.Duration`, `time.WallClock`).
- Threading (`thread.spawn`, `thread.Handle`, `sync.Mutex`, `sync.RwLock`, `sync.Cond`, `sync.Once`).
- Concurrency primitives (`Chan[T]`, `Shared[T]`).

Full programs run on any hosted target (Linux, macOS, Windows, WASI).

**Ext — opt-in.**
- JSON (`ext.json`).
- Serialization traits (`Serializable`, `Encodable`, `Decodable`).
- Regular expressions (`ext.regex`).
- Compression, cryptography, higher-level networking, etc.

Ext modules are imported individually. They may have more substantial runtime costs, slower compile times, or larger binary impact.

### 16.2 The prelude

A tiny set of names is imported automatically into every module:

- Types: `Int`, `UInt`, `I8`–`I64`, `U8`–`U64`, `ISize`, `USize`, `F32`, `F64`, `Bool`, `Char`, `String`, `Option`, `Result`, `Ordering`.
- Variant constructors: `Some`, `None`, `Ok`, `Err`.
- Functions: `print`, `eprint`, `panic`, `assert`, `debugAssert`.
- Traits: `Equatable`, `Hashable`, `Comparable`, `Printable`, `Debuggable`, `Default`, `From`, `Into`, `TryFrom`, `TryInto`, `Sequence`.

Anything outside the prelude requires an explicit `import`. There is no auto-import of `core.list` or `core.map`; the programmer writes `import core.list.List;` and `import core.map.Map;` explicitly.

### 16.3 `Map[K, V]` — insertion order

`Map[K: Hashable, V]` preserves insertion order by default. `keys()`, `values()`, and `entries()` yield elements in the order they were inserted. Removal preserves the order of the remaining elements.

For the unordered or sorted-by-key views:

```fuse
map.sortedKeys() -> Sequence[ref K]
map.unorderedKeys() -> Sequence[ref K]
```

The default being insertion-ordered reflects a judgment about what programs most often need: reproducible iteration that matches the code's order of operations, without having to think about it.

### 16.4 Stdlib policy

The stdlib is held to a stricter standard than user code:

- **No workarounds.** Any compiler bug found while writing the stdlib is a compiler bug, fixed in the compiler, not worked around in the library. Zero exceptions.
- **No `unsafe { }` outside of documented bridge files.** See §6.8.
- **No `Ptr[T]` in public APIs.** Ptr is for FFI; user-facing APIs use typed references.
- **Every public function has a doc comment.** `fuse doc` must produce a complete reference.

These rules exist because the stdlib is the first real test of the language's expressiveness. If the stdlib needs a workaround, the language needs a change.

---

## 17. Compilation, targets, and the CLI

### 17.1 The `fuse` CLI

The Fuse distribution ships a single executable called `fuse`. It has exactly nine subcommands:

| Subcommand  | Purpose |
|-------------|---------|
| `fuse build`   | Compile a package to an artifact. |
| `fuse run`     | Compile and immediately execute a package. |
| `fuse check`   | Type-check and analyze without producing an artifact. |
| `fuse test`    | Run the package's tests. |
| `fuse fmt`     | Format source files in place. |
| `fuse doc`     | Generate HTML documentation from doc comments. |
| `fuse repl`    | Start an interactive REPL. |
| `fuse version` | Print compiler and language version. |
| `fuse help`    | Print help for the CLI or a subcommand. |

No tenth subcommand. No plugin mechanism. No subcommand aliases. The set is closed.

### 17.2 Flag discipline

The CLI uses long flags (`--target linux-amd64`) and short flags where they are standard (`-v` for verbose, `-o` for output). The flag parser is hand-rolled and accepts only the documented flags. Unknown flags produce a non-zero exit and a diagnostic naming the flag.

Every subcommand accepts `--format=json` to produce machine-readable output. This is not an afterthought; it is required of every subcommand.

### 17.3 Supported target triples

On day one, the compiler supports six target triples:

- `linux-amd64`
- `linux-arm64`
- `macos-amd64`
- `macos-arm64`
- `windows-amd64`
- `wasm32-wasi`

The native target is detected automatically; `--target` selects another. Cross-compilation requires the corresponding `cc` to be available:

- Linux and macOS cross-builds use the system `cc` with `--target=<triple>` where appropriate.
- Windows cross-builds use `clang` with `--target=x86_64-pc-windows-msvc` and the `lld-link` linker (or MSVC `cl.exe` on a Windows host).
- WASI cross-builds use `clang --target=wasm32-wasi` together with the WASI SDK. WASM is a compilation **target**, not a separate backend: the compiler still emits C11 that the WASI-capable `clang` then lowers to WASM. See §17.6.

### 17.4 The build pipeline

```
 Fuse source (.fuse)
       |
       v
  [ lex ]      — tokens
       |
       v
  [ parse ]    — AST
       |
       v
  [ resolve ]  — HIR (typed, scoped, liveness annotated)
       |
       v
  [ check ]    — HIR with full metadata
       |
       v
  [ lower ]    — MIR (flat, explicit Drop/Move)
       |
       v
  [ codegen ]  — C11 source
       |
       v
  [ cc ]       — object files
       |
       v
  [ link ]     — final artifact
```

The three IRs (AST, HIR, MIR) are described in the Implementation Plan. From the language user's perspective, the relevant facts are:

- The compiler emits C11, not assembly, not LLVM IR, not a bytecode.
- The C compiler is invoked as a subprocess; the user does not need to install anything beyond `cc`.
- The final artifact is a native executable (or shared library with `--crate-type=cdylib`).

### 17.5 Output artifacts

- **Native executable** (default): produced by `fuse build` on `main.fuse`.
- **Static library** (`.a`, `.lib`): produced by `fuse build --crate-type=staticlib`.
- **Shared library** (`.so`, `.dylib`, `.dll`): produced by `fuse build --crate-type=cdylib`.
- **Object file** (for integrating with a larger C build): produced by `fuse build --crate-type=obj`.

WASM targets produce `.wasm` files via the WASI-capable `clang`.

### 17.6 WASM is a target, not a backend

The compiler does not have a separate WASM code generator. The flow is:

```
Fuse source -> MIR -> C11 -> (clang --target=wasm32-wasi) -> .wasm
```

This means the Fuse compiler needs only one backend — the C11 emitter — and WASM support follows automatically as long as the host has `clang` with WASM support installed. The tradeoff is that WASM builds depend on the WASI SDK being available at build time. The alternative (a separate WASM backend inside the compiler) was rejected because it doubles the size of the compiler for a target that is well-served by clang's existing WASM support.

### 17.7 Build determinism

A build produces **byte-identical output** given the same inputs, compiler version, and target triple. Determinism is achieved by:

- Deterministic iteration order in every compiler pass (no hash-table randomization in the IR).
- Deterministic mangling of symbol names.
- Normalized path strings in debug info.
- `SOURCE_DATE_EPOCH` support for any embedded timestamp.

A user who runs `fuse build` twice on a clean checkout MUST get identical bytes. This is a test gate in CI.

### 17.8 Compilation errors

Errors have structure:

```
error[E0042]: type mismatch in function argument
  --> src/main.fuse:12:19
   |
12 |     let n = add(1, "two");
   |                   ^^^^^ expected `I32`, found `String`
   |
help: convert the string to an integer explicitly:
   |
12 |     let n = add(1, parseInt(ref "two")?);
   |                    ^^^^^^^^^^^^^^^^^^^^
```

Every error:

1. Has an **error code** (e.g. `E0042`) that indexes a long-form explanation retrievable via `fuse help error E0042`.
2. Has **source location** with line and column.
3. Has a **rendered source snippet** with the offending span underlined.
4. Has, wherever feasible, a **`help` block** suggesting a fix.

Warnings have the same structure with `warning[W...]:` instead of `error[E...]:`. A warning never silently becomes an error; an escalation requires `#![deny(warnings)]`.

Errors are first-class. A compiler that produces a working binary with a confusing error on some other input is not acceptable.

### 17.9 Incremental compilation

On day one, the compiler performs **file-level incremental compilation**: if a source file and its dependencies have not changed since the last successful build, its compilation output is reused. Finer-grained incrementality (per-function) is not in day one.

---

## 18. Reserved but not shipped

The following language elements are reserved — they cannot be used as identifiers, and the parser knows about them — but are not implemented on day one. A program that uses a reserved feature gets a diagnostic saying so, not a generic parse error.

### 18.1 `Vec[T, N]` and `@simd`

`Vec[T, N]` is the type name for a fixed-width SIMD vector of `N` elements of type `T`. `@simd` is the attribute that marks a loop as a target for SIMD vectorization. Both are reserved on day one; actual SIMD code generation is a later wave. The reservation exists so that the day-one grammar accepts them and tooling can surface the "not yet implemented" message cleanly.

### 18.2 `select` on channels

A `select` statement that waits on multiple channels is reserved. Day-one programs must use explicit `tryRecv` polling or dedicated helper threads. The keyword `select` is reserved at the lexer level.

### 18.3 Green threads

A user-space scheduler with M:N green threads is reserved. Day-one `spawn` creates a true OS thread. When green threads ship, `spawn` will acquire a `--scheduler` knob; existing programs will continue to compile unchanged.

### 18.4 `async`/`await`

**Permanently rejected.** The keywords `async`, `await`, `yield`, `try`, `catch`, `throw`, and `finally` are reserved only to give a helpful error if a programmer tries to use them. They will never be implemented. Do not propose them.

### 18.5 Macros, `typeof`, `sizeof`, `alignof`

Reserved. The rationale for deferring macros is that they are a major design commitment that should not be rushed. `typeof`, `sizeof`, `alignof` are reserved partly because they are natural names and partly because a compile-time evaluator (which they require) is a larger feature than day one can afford.

### 18.6 Trait objects (`dyn`)

Reserved. On day one, generic dispatch is monomorphized; there is no runtime dispatch through a vtable. When `dyn Trait` ships, it will be additive to the current story, not a replacement.

---

## 19. Appendix A — Keyword list

### 19.1 Active keywords (reserved, used)

```
and        as         break      case       class      const      continue
data       default    do         else       enum       extern     false
fn         for        if         impl       implements import     in
let        loop       match      move       mut        mutref     not
or         owned      pub        ref        return     self       Self
spawn      struct     trait      true       type       unsafe     use
var        where      while      with
```

### 19.2 Reserved for future use

```
async      await      yield      try        catch      throw      finally
macro      typeof     sizeof     alignof    static     dyn        union
select
```

### 19.3 Reserved type and attribute names

- `Vec` — reserved type constructor for fixed-width SIMD vectors.
- `@simd` — reserved attribute for SIMD vectorization hints.
- `@rank` — **active** attribute for lock ordering.
- `@value` — **active** attribute for value-type struct opt-in.
- `@export` — **active** attribute for FFI export.
- `@override` — **active** attribute for trait method overrides.

---

## 20. Appendix B — Operator precedence

Precedence from **highest** (binds tightest) to **lowest**:

| Level | Operators / forms                                  | Associativity |
|-------|---------------------------------------------------|---------------|
| 14    | `f(args)`, `x.field`, `x[i]`, `x?`                | left          |
| 13    | `- x` (unary), `~ x`, `not x`                     | right         |
| 12    | `*`, `/`, `%`                                     | left          |
| 11    | `+`, `-`                                          | left          |
| 10    | `<<`, `>>`                                        | left          |
| 9     | `&`                                               | left          |
| 8     | `^`                                               | left          |
| 7     | `\|`                                              | left          |
| 6     | `==`, `!=`, `<`, `<=`, `>`, `>=`                  | non-assoc     |
| 5     | `and`                                             | left          |
| 4     | `or`                                              | left          |
| 3     | `..`, `..=`                                       | non-assoc     |
| 2     | `=`, `+=`, `-=`, `*=`, `/=`, `%=`, ...            | right (stmt)  |
| 1     | block-level statements                            | —             |

Comparison operators are non-associative: `a < b < c` is a parse error. The programmer must write `a < b and b < c`.

Assignment is a statement (level 2) and cannot appear in expression position.

---

## 21. Appendix C — Grammar summary

The grammar below is an informal EBNF sketch. It is sufficient to disambiguate the language, but a full formal grammar lives next to the parser implementation and is treated as authoritative when the two disagree.

```ebnf
file            ::= attr_block? module_doc? import* top_decl*

attr_block      ::= "#!" "[" attr ("," attr)* "]"
attr            ::= IDENT ("(" attr_arg_list ")")?

module_doc      ::= ("//!" LINE_TEXT)+

import          ::= "pub"? "import" module_path ("as" IDENT | "." "{" ident_list "}")? ";"
module_path     ::= IDENT ("." IDENT)*

top_decl        ::= fn_decl | struct_decl | enum_decl | trait_decl | impl_block
                  | const_decl | extern_block | type_alias

fn_decl         ::= "pub"? "unsafe"? "fn" IDENT generics? "(" params? ")"
                    ("->" type)? where_clause? block
params          ::= param ("," param)*
param           ::= IDENT ":" ownership? type
ownership       ::= "ref" | "mutref" | "owned"
generics        ::= "[" type_param ("," type_param)* "]"
type_param      ::= IDENT (":" bound ("+" bound)*)?
where_clause    ::= "where" where_pred ("," where_pred)*
where_pred      ::= type ":" bound ("+" bound)*
bound           ::= type_path

struct_decl     ::= attr_list? "pub"? ("struct" | "data" "class") IDENT generics? struct_body
struct_body     ::= "{" (field ("," field)* )? "}"
                  | "(" (field_pos ("," field_pos)* )? ")"   -- data class positional form
field           ::= "pub"? IDENT ":" type
field_pos       ::= IDENT ":" type | type

enum_decl       ::= "pub"? "enum" IDENT generics? "{" variant ("," variant)* "}"
variant         ::= IDENT ( "(" type ("," type)* ")" | "{" field ("," field)* "}" )?

trait_decl      ::= "pub"? "trait" IDENT generics? ("implements" bound ("+" bound)*)?
                    "{" trait_item* "}"
trait_item      ::= fn_sig ";" | fn_decl | assoc_type
assoc_type      ::= "type" IDENT (":" bound ("+" bound)*)? ";"

impl_block      ::= "impl" generics? type ("implements" type_path)? "{" fn_decl* "}"

const_decl      ::= "pub"? "const" IDENT ":" type "=" expr ";"
type_alias      ::= "pub"? "type" IDENT generics? "=" type ";"

extern_block    ::= "extern" "{" extern_item* "}"
extern_item     ::= "fn" IDENT "(" params? ")" ("->" type)? ";"

type            ::= type_path generics_args?
                  | "Ptr" "[" type "]"
                  | "[" type ("," INT_LIT)? ";" INT_LIT "]"
                  | "(" type ("," type)* ")"
                  | fn_type
                  | "!"
fn_type         ::= "(" (ownership? type ("," ownership? type)*)? ")" "->" type

stmt            ::= let_stmt | var_stmt | expr ";" | assign_stmt | item_decl
let_stmt        ::= "let" pattern (":" type)? "=" expr ";"
var_stmt        ::= "var" pattern (":" type)? "=" expr ";"
assign_stmt     ::= place_expr assign_op expr ";"
assign_op       ::= "=" | "+=" | "-=" | "*=" | "/=" | "%="
                  | "&=" | "|=" | "^=" | "<<=" | ">>="

expr            ::= literal
                  | IDENT
                  | block
                  | if_expr
                  | match_expr
                  | loop_expr
                  | while_expr
                  | for_expr
                  | return_expr
                  | break_expr
                  | continue_expr
                  | call_expr
                  | field_expr
                  | index_expr
                  | unary_expr
                  | binary_expr
                  | closure_expr
                  | struct_lit
                  | tuple_lit
                  | array_lit
                  | "move" expr

block           ::= "{" stmt* expr? "}"

if_expr         ::= "if" expr block ("else" "if" expr block)* ("else" block)?
match_expr      ::= "match" expr "{" match_arm ("," match_arm)* "}"
match_arm       ::= "case" pattern ("if" expr)? "=>" expr
loop_expr       ::= "loop" block
while_expr      ::= "while" expr block
for_expr        ::= "for" pattern "in" expr block
return_expr     ::= "return" expr?
break_expr      ::= "break" expr?
continue_expr   ::= "continue"

closure_expr    ::= "move"? "|" closure_params? "|" expr
closure_params  ::= closure_param ("," closure_param)*
closure_param   ::= IDENT (":" type)?

pattern         ::= "_" | literal | IDENT | variant_pat | tuple_pat
                  | struct_pat | range_pat | or_pat
variant_pat     ::= type_path ("(" pattern ("," pattern)* ")")?
tuple_pat       ::= "(" pattern ("," pattern)* ")"
struct_pat      ::= type_path "{" (field_pat ("," field_pat)* )? (".." )? "}"
field_pat       ::= IDENT (":" pattern)?
range_pat       ::= literal ".." literal | literal "..=" literal
or_pat          ::= pattern "|" pattern
```

This grammar is illustrative and intentionally informal. A contributor implementing the parser follows this as a starting point and resolves ambiguities by referring to the prose sections above.

---

*End of language guide.*
