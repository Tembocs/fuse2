# Fuse — Interfaces & Concurrency Model Specification

> **Status:** Design complete. Implementation not started.
> **Prerequisite:** Hardening H0 complete. H0.6 (async removal) complete.
> **Scope:** 2 major features, 8 implementation phases, ~60 tasks.
> **Authority:** This document is the authoritative specification for
> Fuse's interface system and concurrency model. The language guide
> (`docs/fuse-language-guide-2.md`) must be updated to reflect this
> spec before implementation begins.
>
> **This document does not tolerate ambiguity.** Every design decision
> is explicit. Every implementation task is granular. Every open
> question is listed and must be resolved before the relevant phase
> begins.

---

## Mandatory Rules

> **Before starting any phase in this document, re-read:**
>
> 1. The **Language Philosophy** section in `docs/fuse-hardening-plan.md`
> 2. The **Mandatory Rules** section in `docs/fuse-hardening-plan.md`
> 3. The specific files listed in each phase's `Before starting` block
> 4. The `docs/fuse-language-guide-2.md` sections referenced in the phase
>
> All rules from the hardening plan apply here without exception:
> Rule 1 (Read Before You Build), Rule 2 (No TODO, No Defer),
> Rule 3 (Zero Regressions), Rule 4 (Vigilance), Rule 5 (Completion Standard).

---

## Status Key

- `[ ]` — not started
- `[~]` — in progress
- `[x]` — done
- `[!]` — blocked (must state what blocks it)

---

## Task Summary

| Section | Name | Phases | Tasks | Depends On |
|---------|------|--------|-------|------------|
| I | Interface System | 6 | ~45 | Hardening H1 (structs, generics) |
| II | Concurrency Model | 2 | ~15 | Hardening H0.6 (async removal) |
| **Total** | | **8** | **~60** | |

---

# Part I — Interface System

## 1. Design Overview

Fuse interfaces define behavioral contracts. A type satisfies an
interface by declaring `implements InterfaceName` and providing
all required methods as extension methods.

### 1.1 Declaration

```fuse
interface Printable {
    fn toString(ref self) -> String
}
```

An interface is a named collection of function signatures. It has:
- A name (PascalCase, like types)
- Zero or more function signatures (no bodies)
- Optional type parameters (generic interfaces)

Interfaces cannot contain:
- Fields or stored state
- Constants or associated types (future consideration)
- Constructors

### 1.2 Conformance

Conformance is always explicit. A type declares which interfaces it
implements:

```fuse
data class Point(x: Float, y: Float) implements Printable
```

The declaration `implements Printable` is a compile-time contract.
The compiler verifies that all methods declared in `Printable` are
implemented for `Point`. If any method is missing, the compiler emits
a clear error listing the missing methods.

Multiple interfaces are comma-separated:

```fuse
data class Point(x: Float, y: Float) implements Printable, Hashable, Equatable
```

### 1.3 Implementation via Extension Methods

Method bodies are provided through extension methods — the same
mechanism already used throughout the Fuse stdlib:

```fuse
fn Point.toString(ref self) -> String {
    f"({self.x}, {self.y})"
}
```

There is no `impl X for Y` block. There is no special syntax for
interface implementations vs regular extension methods. The extension
method *is* the implementation. The compiler matches extension methods
to interface requirements by:

1. Name match
2. Parameter count match (including `self`)
3. Parameter type compatibility
4. Return type compatibility
5. Ownership convention compatibility (`ref self` in interface requires
   `ref self` or more restrictive in implementation)

### 1.4 Default Methods

Default methods are provided by writing extension methods on the
interface itself:

```fuse
fn Printable.debugPrint(ref self) {
    println(f"[DEBUG] {self.toString()}")
}
```

Any type that implements `Printable` gets `debugPrint` for free.
A type can override a default method by providing its own extension
method with the same signature.

Default method resolution order:
1. Type's own extension method (highest priority)
2. Interface's default method (fallback)

### 1.5 Generic Bounds

Type parameters can be constrained to require interface conformance:

```fuse
fn printAll<T: Printable>(items: List<T>) {
    for item in items {
        println(item.toString())
    }
}
```

Multiple bounds use `+` syntax:

```fuse
fn process<T: Printable + Hashable>(item: T) { ... }
```

The compiler verifies at the call site that the concrete type
argument satisfies all bounds:

```fuse
printAll([Point(1.0, 2.0), Point(3.0, 4.0)])  // OK — Point implements Printable
printAll([42, 43])                              // OK if Int implements Printable
printAll([SomeType()])                          // ERROR: SomeType does not implement Printable
```

### 1.6 Interface Composition

Interfaces can extend other interfaces:

```fuse
interface Comparable : Equatable {
    fn compareTo(ref self, ref other: Self) -> Int
}
```

A type implementing `Comparable` must provide methods for both
`Comparable` AND `Equatable`. The compiler checks all inherited
requirements transitively.

Multiple parent interfaces:

```fuse
interface Serializable : Printable, Hashable {
    fn serialize(ref self) -> List<Int>
}
```

Diamond inheritance is resolved trivially — interfaces have no state,
so there is nothing to conflict. If `A : B, C` and both `B` and `C`
extend `D`, the methods of `D` are required once.

### 1.7 Ownership in Interface Methods

Interface methods declare ownership conventions on `self`:

```fuse
interface Container {
    fn size(ref self) -> Int          // shared borrow — read only
    fn clear(mutref self)             // exclusive borrow — mutation
    fn consume(owned self) -> List<T> // takes ownership — destroys self
}
```

The implementing extension method must match or be more restrictive:
- Interface says `ref self` → implementation may use `ref self`
- Interface says `mutref self` → implementation must use `mutref self`
- Interface says `owned self` → implementation must use `owned self`

A method declared `ref self` in the interface CANNOT be implemented
with `mutref self` — that would widen the requirement, forcing callers
with only a shared borrow to fail.

### 1.8 Generic Interfaces

Interfaces themselves can be generic:

```fuse
interface Convertible<T> {
    fn convert(ref self) -> T
}

data class Celsius(value: Float) implements Convertible<Fahrenheit>

fn Celsius.convert(ref self) -> Fahrenheit {
    Fahrenheit(self.value * 9.0 / 5.0 + 32.0)
}
```

A type can implement the same generic interface for multiple type
arguments:

```fuse
data class Temperature(value: Float) implements Convertible<Celsius>, Convertible<Fahrenheit>
```

### 1.9 What Interfaces Are NOT

| Not supported | Reason |
|---|---|
| Structural typing ("duck typing") | Conformance is always explicit. No implicit satisfaction. |
| Runtime interface casting (`as Printable`) | Fuse is statically dispatched. No vtables (initially). |
| Associated types | Adds complexity. Use generic interfaces instead. |
| Default field values | Interfaces have no fields. |
| `impl` blocks | Use extension methods. |
| Marker interfaces (no methods) | Under consideration. Not in initial implementation. |

---

## 2. Language Comparison

| Feature | Fuse | Rust | Go | Kotlin | Swift |
|---|---|---|---|---|---|
| Declaration | `interface` | `trait` | `interface` | `interface` | `protocol` |
| Conformance | explicit `implements` | explicit `impl T for S` | implicit (structural) | explicit `: Interface` | explicit `: Protocol` |
| Implementation | extension methods | `impl` block | methods on struct | class body | extension or class body |
| Default methods | extension on interface | default in trait | ✗ | default in interface | extension on protocol |
| Generic bounds | `T: Interface` | `T: Trait` | `[T Interface]` | `T : Interface` | `T: Protocol` |
| Composition | `: Parent` | `: Supertrait` | embedding | `: Parent` | `: Parent` |
| Dispatch | static (initially) | static + `dyn` | dynamic (vtable) | dynamic (vtable) | static + existential |

**Fuse's approach combines:**
- Kotlin/Swift's explicit conformance (not Go's implicit satisfaction)
- Rust's static dispatch (no vtable overhead initially)
- Swift's protocol extensions as default methods
- Fuse's own extension method syntax (no new implementation blocks)

---

## 3. New Keywords

| Keyword | Usage | Example |
|---|---|---|
| `interface` | Declare an interface | `interface Printable { ... }` |
| `implements` | Declare conformance | `data class X() implements Y` |

These are new reserved words. They must be added to the lexer's
keyword table. Any existing variable or function named `interface`
or `implements` will become a compile error.

---

## 4. Open Questions (Must Resolve Before Implementation)

| # | Question | Options | Impact |
|---|---|---|---|
| 1 | Should `enum` types implement interfaces? | Yes (useful for Printable on enums) / No (defer) | Parser: `enum X implements Y` syntax |
| 2 | Can module-level `pub fn` satisfy an interface method? | Yes (more flexible) / No (must be extension method) | Checker: method resolution logic |
| 3 | Runtime interface check (`is Printable`)? | Defer to post-Stage 2 (requires runtime type info) | No immediate impact |
| 4 | Marker interfaces (no methods)? | Useful for `Send`, `Sync` equivalents | Checker validation of empty interfaces |
| 5 | Dynamic dispatch (`dyn Interface`)? | Defer post-Stage 2. Static dispatch first. | No vtable generation needed initially |

> **Rule:** Questions 1-4 must be answered before Phase I.1 begins.
> Question 5 is deferred — static dispatch only in initial implementation.

---

## 5. Implementation Phases

### Phase I.1 — Lexer & Parser: Interface Declaration

> **MANDATORY:** Before starting this phase, read:
>
> - `stage1/fusec/src/lexer/token.rs` — `TokenKind` enum, `keyword_kind()` function
> - `stage1/fusec/src/lexer/lexer.rs` — keyword tokenization
> - `stage1/fusec/src/parser/parser.rs` — `parse_declaration()` dispatch (line ~81),
>   `parse_data_class()` (line ~276)
> - `stage1/fusec/src/ast/nodes.rs` — `Statement` enum (line ~120),
>   `DataClassDecl` (line ~85)

- [ ] **I.1.1** Add `Interface` and `Implements` variants to `TokenKind` in `lexer/token.rs`.
- [ ] **I.1.2** Add `"interface" => TokenKind::Interface` and
      `"implements" => TokenKind::Implements` to `keyword_kind()` in `lexer/token.rs`.
- [ ] **I.1.3** Add `InterfaceDecl` struct to `ast/nodes.rs`:
      ```rust
      pub struct InterfaceDecl {
          pub name: String,
          pub type_params: Vec<String>,
          pub parents: Vec<String>,           // composed interfaces
          pub methods: Vec<FunctionSignature>, // signatures only, no body
          pub span: Span,
      }
      ```
- [ ] **I.1.4** Add `FunctionSignature` struct to `ast/nodes.rs` (if not already present):
      ```rust
      pub struct FunctionSignature {
          pub name: String,
          pub params: Vec<Parameter>,
          pub return_type: Option<TypeAnnotation>,
          pub span: Span,
      }
      ```
- [ ] **I.1.5** Add `Interface(InterfaceDecl)` variant to `Statement` enum in `ast/nodes.rs`.
- [ ] **I.1.6** Add `implements: Vec<String>` field to `DataClassDecl` in `ast/nodes.rs`.
      Default to empty vec for backward compatibility.
- [ ] **I.1.7** Implement `parse_interface()` in `parser.rs`:
      - Consume `TokenKind::Interface`
      - Parse name (identifier)
      - Parse optional type params `<T, U>`
      - Parse optional parents `: Parent1, Parent2`
      - Parse `{` block of function signatures `}`
      - Each signature: `fn name(params) -> ReturnType` (no body)
- [ ] **I.1.8** Add `TokenKind::Interface` dispatch in `parse_declaration()` to call
      `parse_interface()`.
- [ ] **I.1.9** Extend `parse_data_class()` to parse optional `implements X, Y` after
      the field list `)` and before the body or end of declaration.
- [ ] **I.1.10** Add test: `interface_parse_basic.fuse` — parse a simple interface with
       two method signatures. Use `// EXPECTED OUTPUT` with no runtime behavior
       (just verify it compiles without parse error).
- [ ] **I.1.11** Add test: `interface_parse_with_parents.fuse` — parse an interface with
       `: Parent` composition. Verify no parse error.
- [ ] **I.1.12** Add test: `interface_parse_generic.fuse` — parse `interface Convertible<T>`.
       Verify no parse error.

---

### Phase I.2 — HIR Lowering: Interface Nodes

> **MANDATORY:** Before starting this phase, read:
>
> - `stage1/fusec/src/hir/nodes.rs` — HIR node definitions
> - `stage1/fusec/src/hir/lower.rs` — AST-to-HIR lowering functions
> - `stage1/fusec/src/ast/nodes.rs` — the new `InterfaceDecl` and updated `DataClassDecl`

- [ ] **I.2.1** Add `InterfaceDecl` to HIR nodes in `hir/nodes.rs` (mirror the AST
      structure, or re-export the AST type if HIR re-uses AST types directly).
- [ ] **I.2.2** Add `Interface(InterfaceDecl)` variant to HIR `Statement` enum.
- [ ] **I.2.3** Add `implements: Vec<String>` field to HIR `DataClassDecl` (mirror AST).
- [ ] **I.2.4** Add lowering for `Statement::Interface(decl)` in `lower.rs` — lower
      each method signature's parameter types and return type.
- [ ] **I.2.5** Update `DataClassDecl` lowering to carry the `implements` list through.
- [ ] **I.2.6** Verify: `cargo build` succeeds with no warnings. Run existing test suite.

---

### Phase I.3 — Checker: Interface Registration & Conformance Validation

> **MANDATORY:** Before starting this phase, read:
>
> - `stage1/fusec/src/checker/mod.rs` — `ModuleInfo`, function registration,
>   `extension_functions`, `static_functions`
> - `stage1/fusec/src/checker/types.rs` — type representation
> - `stage1/fusec/src/checker/ownership.rs` — ownership convention checking
> - The **Design Overview** section (§1) of this document

- [ ] **I.3.1** Add `interfaces: HashMap<String, InterfaceInfo>` to `ModuleInfo`
      in `checker/mod.rs`. `InterfaceInfo` contains:
      ```rust
      pub struct InterfaceInfo {
          pub name: String,
          pub type_params: Vec<String>,
          pub parents: Vec<String>,
          pub methods: Vec<FunctionSignature>,
          pub span: Span,
      }
      ```
- [ ] **I.3.2** Add `implements: HashMap<String, Vec<String>>` to `ModuleInfo`
      (maps type name → list of interface names it implements).
- [ ] **I.3.3** Register interfaces during the checker's declaration pass: when
      encountering `Statement::Interface(decl)`, store in `ModuleInfo.interfaces`.
- [ ] **I.3.4** Register conformance during the checker's declaration pass: when
      encountering a `DataClassDecl` with non-empty `implements`, store in
      `ModuleInfo.implements`.
- [ ] **I.3.5** Add `resolve_interface(name: &str) -> Option<&InterfaceInfo>` lookup
      to checker.
- [ ] **I.3.6** Implement interface parent resolution: when registering an interface
      with parents (`: Parent1, Parent2`), verify each parent exists. Collect
      all inherited method signatures transitively.
- [ ] **I.3.7** Implement conformance checking: after all functions and extension methods
      are registered, for each type that `implements` an interface:
      - Collect all required methods (including inherited from parents)
      - For each required method, find a matching extension method on the type
      - Match by: name, parameter count, parameter types, return type, ownership convention
      - If any method is missing, emit error: `"Type 'X' declares 'implements Y' but does not implement method 'Z(ref self) -> ReturnType'"`
- [ ] **I.3.8** Implement ownership convention checking in conformance:
      - `ref self` in interface → implementation must use `ref self`
      - `mutref self` in interface → implementation must use `mutref self`
      - `owned self` in interface → implementation must use `owned self`
      - Mismatch → error: `"Method 'X.method' uses 'mutref self' but interface 'Y' requires 'ref self'"`
- [ ] **I.3.9** Implement generic bound checking: when a function has `<T: Interface>`,
      at the call site, verify the concrete type argument has `implements Interface`
      declared. If not, emit error: `"Type 'X' does not implement interface 'Y' required by bound 'T: Y'"`
- [ ] **I.3.10** Add test: `interface_missing_method.fuse` — declare `implements Printable`
       but do not provide `toString`. Expect `// EXPECTED ERROR` with clear message.
- [ ] **I.3.11** Add test: `interface_wrong_ownership.fuse` — interface declares `ref self`,
       implementation uses `mutref self`. Expect `// EXPECTED ERROR`.
- [ ] **I.3.12** Add test: `interface_bound_satisfied.fuse` — call `printAll<T: Printable>`
       with a type that implements Printable. Expect `// EXPECTED OUTPUT`.
- [ ] **I.3.13** Add test: `interface_bound_violated.fuse` — call `printAll<T: Printable>`
       with a type that does NOT implement Printable. Expect `// EXPECTED ERROR`.
- [ ] **I.3.14** Add test: `interface_composition.fuse` — interface `Comparable : Equatable`,
       type implements `Comparable`, verify both `compareTo` and `equals` are required.

---

### Phase I.4 — Checker: Default Methods

> **MANDATORY:** Before starting this phase, read:
>
> - `stage1/fusec/src/checker/mod.rs` — extension method registration
> - Phase I.3 implementation (all tasks must be complete)
> - The **Default Methods** section (§1.4) of this document

- [ ] **I.4.1** Detect default methods: when an extension method's receiver type
      matches an interface name (e.g., `fn Printable.debugPrint(ref self)`),
      register it as a default method on the interface, not as a regular
      extension method.
- [ ] **I.4.2** Store default methods in `InterfaceInfo`:
      ```rust
      pub default_methods: Vec<hir::FunctionDecl>,
      ```
- [ ] **I.4.3** During conformance checking, if a required method is not found on the
      type BUT a default method exists on the interface, mark it as satisfied.
      The default method will be used at codegen time.
- [ ] **I.4.4** Override resolution: if a type provides its own extension method that
      matches a default method's signature, the type's method takes priority.
      Do NOT report a duplicate error.
- [ ] **I.4.5** Add test: `interface_default_method.fuse` — interface with default
      method, type does not override it. Call the default method. Expect output.
- [ ] **I.4.6** Add test: `interface_default_method_override.fuse` — interface with
      default method, type DOES override it. Call the method. Expect overridden output.

---

### Phase I.5 — Codegen: Interface Method Dispatch

> **MANDATORY:** Before starting this phase, read:
>
> - `stage1/fusec/src/codegen/object_backend.rs` — `declare_user_surface`,
>   `emit_object`, extension method compilation, `compile_call`
> - Phase I.3 and I.4 implementation (all tasks must be complete)
> - The **Key Codegen Patterns** section in `.github/copilot-instructions.md`

- [ ] **I.5.1** No new code generation for interface declarations — interfaces produce
      no machine code themselves. They are compile-time-only constructs.
      Verify: interface declarations are skipped in `declare_user_surface` and
      `emit_object` without error.
- [ ] **I.5.2** Implement default method compilation: for each type that implements
      an interface, if the type uses a default method (not overridden), generate
      a forwarding symbol `fuse_ext_{module}_{Type}__{method}` that calls the
      default method's compiled body.
- [ ] **I.5.3** Verify extension method dispatch: when calling `x.toString()` where
      `x: T` and `T: Printable`, the call resolves to `fuse_ext_{module}_{T}__toString`.
      This should already work via existing extension method compilation. Verify with test.
- [ ] **I.5.4** Implement generic bound dispatch: when compiling a call inside a
      function with `<T: Printable>`, method calls on `T` must resolve to the
      concrete type's extension method at the call site (monomorphization is
      already type-erased, so this should work — verify).
- [ ] **I.5.5** Add test: `interface_codegen_basic.fuse` — full end-to-end: declare
      interface, implement on data class, call method, print result.
      Expect `// EXPECTED OUTPUT` with correct value.
- [ ] **I.5.6** Add test: `interface_codegen_default.fuse` — type uses default method.
      Compile, run, verify default method output.
- [ ] **I.5.7** Add test: `interface_codegen_generic_bound.fuse` — function with
      `<T: Printable>`, call with concrete type. Verify correct output.

---

### Phase I.6 — Language Guide Update

> **MANDATORY:** Before starting this phase, read:
>
> - `docs/fuse-language-guide-2.md` — full document
> - All previous interface phases (I.1–I.5) must be complete

- [ ] **I.6.1** Add new section to `fuse-language-guide-2.md`: "§X.X Interfaces"
      covering declaration, conformance, implementation, default methods.
- [ ] **I.6.2** Add new section: "§X.X Generic Bounds" covering `T: Interface`
      syntax and multiple bounds.
- [ ] **I.6.3** Add new section: "§X.X Interface Composition" covering `: Parent`
      syntax and diamond inheritance.
- [ ] **I.6.4** Update the keyword table in the language guide to include `interface`
      and `implements`.
- [ ] **I.6.5** Review all existing guide sections for consistency with interface
      design. Update any conflicting text.

---

# Part II — Concurrency Model

## 6. Design Overview

Fuse's concurrency model is spawn-only. There is no `async`/`await`.
There are no colored functions. Concurrency is a call-site decision,
not a function declaration decision.

### 6.1 Why No async/await

| Problem in Rust/C# | How Fuse avoids it |
|---|---|
| **Function coloring** — async and sync are different worlds, traits must be duplicated (`Read` vs `AsyncRead`) | No coloring. Every function is the same. Concurrency is a call-site decision. |
| **Lifetime infection** — futures borrow arguments, producing `Pin<Box<dyn Future + Send + 'a>>` | No lifetimes. ASAP destruction + ownership conventions handle it. |
| **No built-in runtime** — Rust has `async`/`await` but requires Tokio/async-std/smol | Fuse ships one lightweight runtime. No ecosystem split. |
| **suspend/resume complexity** — stackless coroutines require state machine transformation | No coroutines. Spawn creates a real task. No hidden state machines. |

### 6.2 The Three Tiers

**Tier 1 — Channels (preferred, zero locks)**

```fuse
val (tx, rx) = Chan::<Int>.bounded(10)

spawn {
    tx.send(42)
}

val value = rx.recv()?
println(f"got: {value}")
```

**Tier 2 — Shared<T> with @rank (when sharing is necessary)**

```fuse
@rank(1) val config  = Shared.new(Config.load())
@rank(2) val db      = Shared.new(Db.open("localhost"))

fn update() {
    val ref    cfg  = config.read()   // rank 1
    val mutref conn = db.write()      // rank 2 > 1 — ok
}
```

**Tier 3 — try_write (dynamic lock order)**

```fuse
fn dynamicUpdate(resources: List<Shared<Resource>>) -> Result<Unit, LockError> {
    val guards = [r.try_write(Timeout.ms(50))? for r in resources]
    for mutref g in guards { g.flush() }
    Ok(())
}
```

### 6.3 Spawn Rules

| Syntax | Meaning |
|---|---|
| `spawn { ... }` | Create a new concurrent task |
| `spawn move { ... }` | Task takes ownership of captured values |
| `spawn ref { ... }` | Task gets read-only access to captured values |
| `spawn { mutref x }` | **COMPILE ERROR** — no mutable borrows across spawn boundaries |

Spawned tasks extend the lifetime of `ref` captures until the task
completes. This is safe because `ref` is shared-immutable — multiple
tasks can read concurrently without data races.

### 6.4 How I/O Works Without async/await

Functions that do I/O are normal functions. The caller decides whether
to run them concurrently:

```fuse
// Normal function — no async keyword
fn fetchUser(id: Int) -> Result<User, NetworkError> {
    val resp = Http.get(f"/users/{id}")?
    match resp.status {
        200 => Ok(resp.json::<User>()?)
        _   => Err(NetworkError.Http(resp.status))
    }
}

// Sequential — just call it
fn main() {
    val user = fetchUser(42)?
    println(user.name)
}

// Concurrent — spawn it, communicate via channel
fn main() {
    val (tx, rx) = Chan::<Result<User, NetworkError>>.bounded(1)
    spawn move { tx.send(fetchUser(42)) }
    val user = rx.recv()??
    println(user.name)
}
```

### 6.5 Patterns

**Timeout:**

```fuse
fn fetchWithTimeout(id: Int) -> Result<User, String> {
    val (tx, rx) = Chan::<Result<User, NetworkError>>.bounded(1)
    spawn move { tx.send(fetchUser(id)) }

    match rx.recv_timeout(Timeout.secs(5)) {
        Ok(result) => result.mapErr(fn(e) { e.toString() })
        Err(_)     => Err("request timed out")
    }
}
```

**Worker Pool:**

```fuse
fn processItems(items: List<Item>) -> List<Result<Output, Error>> {
    val (work_tx, work_rx) = Chan::<Item>.bounded(100)
    val (result_tx, result_rx) = Chan::<Result<Output, Error>>.bounded(100)

    for _ in range(0, 4) {
        spawn move {
            for item in work_rx {
                result_tx.send(process(item))
            }
        }
    }

    for item in items { work_tx.send(item) }
    work_tx.close()

    val results = List::<Result<Output, Error>>.new()
    for _ in items { results.push(result_rx.recv()?) }
    results
}
```

**Select (Multiple Channels):**

```fuse
fn mergeStreams(rx1: Chan<Int>, rx2: Chan<String>) {
    loop {
        match select {
            val n from rx1 => println(f"Int: {n}")
            val s from rx2 => println(f"String: {s}")
            closed         => break
        }
    }
}
```

> **Open question:** `select` syntax needs design. Options:
> `match select { }` (consistent with match), `select { case }` (Go-style).
> Decision must be made before `select` is implemented.

**Pipeline:**

```fuse
fn pipeline(input: Chan<RawData>) -> Chan<ProcessedData> {
    val (tx, rx) = Chan::<ProcessedData>.bounded(50)

    spawn move {
        for raw in input {
            val parsed = parse(raw)
            val validated = validate(parsed)
            tx.send(transform(validated))
        }
        tx.close()
    }

    rx
}
```

### 6.6 What This Model Does NOT Include

| Feature | Reason |
|---|---|
| `async fn` | Removed. Functions are functions. |
| `await` | Removed. No futures to await. |
| `suspend` | Removed. No coroutine state machines. |
| Thread pinning | Not needed. Add if SIMD/GPU demands it post-Stage 2. |
| Structured concurrency (nurseries) | Interesting but unproven at scale. Revisit post-Stage 2. |

---

## 7. Concurrency Open Questions (Must Resolve Before Implementation)

| # | Question | Options | Impact |
|---|---|---|---|
| 1 | What is the exact `select` syntax? | `match select { }` vs `select { case }` | Parser: new expression form |
| 2 | Should `spawn` return a joinable handle? | `val h = spawn { ... }; h.join()?` vs fire-and-forget | Runtime: handle type, join semantics |
| 3 | Spawn pool size limit? | Bounded (prevent fork bombs) vs unbounded (developer's responsibility) | Runtime: task scheduler |
| 4 | Runtime scheduling model? | Green threads (Go-style) vs OS threads | Runtime: significant implementation difference |

> **Rule:** Questions 1-2 must be answered before concurrency codegen work.
> Questions 3-4 are runtime design decisions — resolve before runtime implementation.

---

## 8. Implementation Phases

### Phase II.1 — Async/Await/Suspend Removal

> **This phase is tracked as Phase H0.6 in `docs/fuse-hardening-plan.md`.**
> See that document for the full task list with 30 exact changes across
> 10 files and the mandatory execution order.
>
> **Prerequisite:** H0 complete. This phase must be done BEFORE any
> concurrency model implementation.

Summary of changes (detailed in hardening plan H0.6):

| File | Changes | Lines removed |
|---|---|---|
| `lexer/token.rs` | 2 changes | ~6 |
| `ast/nodes.rs` | 5 changes | ~12 |
| `parser/parser.rs` | 5 changes | ~20 |
| `checker/mod.rs` | 4 changes | ~18 |
| `codegen/object_backend.rs` | 4 changes | ~6 |
| `evaluator.rs` | 4 changes | ~6 |
| `main.rs` | 1 change | ~4 |
| `stage0/fuse_token.py` | 1 change | ~3 |
| `tests/fuse/full/async/` | Delete 6 files | ~120 |
| `fuse-language-guide-2.md` | 4 changes | ~50 rewritten |
| **Total** | **30 changes** | **~125 removed + ~50 rewritten** |

Execution order:
1. AST first (remove types)
2. All consumers (parser, checker, codegen, evaluator, main)
3. Lexer (remove token kinds)
4. Tests (delete `tests/fuse/full/async/`)
5. Stage 0 (remove from `fuse_token.py`)
6. Spec (rewrite language guide section 1.18)

---

### Phase II.2 — Language Guide: Concurrency Section Rewrite

> **MANDATORY:** Before starting this phase, read:
>
> - `docs/fuse-language-guide-2.md` — section 1.17 (Concurrency), section 1.18 (Async)
> - Phase II.1 must be complete (async removed)

- [ ] **II.2.1** Rewrite section 1.17 in `fuse-language-guide-2.md`:
      - Title: "Concurrency (spawn + channels)"
      - Content: spawn syntax, channel creation, send/recv, ownership rules
      - Include: spawn rules table from §6.3 of this document
- [ ] **II.2.2** Replace section 1.18 ("Async") with a note:
      > "Fuse does not have `async`/`await`. Concurrency is expressed through
      > `spawn` blocks and channels. See §1.17 Concurrency."
- [ ] **II.2.3** Remove `async`/`await`/`suspend` from the Fuse Full feature listing
      in the language guide header.
- [ ] **II.2.4** Update any `spawn async { }` examples to `spawn { }` throughout
      the language guide.
- [ ] **II.2.5** Add concurrency patterns section to language guide: timeout, worker pool,
      pipeline (from §6.5 of this document).
- [ ] **II.2.6** Review all guide sections for stale async/await references and remove them.

---

# Appendix A — Comparison with Rust Traits

```rust
// Rust — trait + impl block
trait Printable {
    fn to_string(&self) -> String;
    fn debug_print(&self) {  // default method
        println!("[DEBUG] {}", self.to_string());
    }
}

impl Printable for Point {
    fn to_string(&self) -> String {
        format!("({}, {})", self.x, self.y)
    }
}
```

```fuse
// Fuse — interface + extension methods
interface Printable {
    fn toString(ref self) -> String
}

fn Printable.debugPrint(ref self) {  // default method
    println(f"[DEBUG] {self.toString()}")
}

data class Point(x: Float, y: Float) implements Printable

fn Point.toString(ref self) -> String {
    f"({self.x}, {self.y})"
}
```

Key differences:
- Fuse has no `impl` blocks — extension methods serve the same purpose
- Default methods in Fuse are extension methods on the interface name
- Fuse uses `ref self` / `mutref self` instead of `&self` / `&mut self`
- Conformance is on the type declaration (`implements`), not in a separate block

---

# Appendix B — Comparison with Go Interfaces

```go
// Go — implicit structural typing
type Printable interface {
    ToString() string
}

// Point satisfies Printable implicitly — no declaration needed
func (p Point) ToString() string {
    return fmt.Sprintf("(%f, %f)", p.X, p.Y)
}
```

```fuse
// Fuse — explicit conformance
interface Printable {
    fn toString(ref self) -> String
}

data class Point(x: Float, y: Float) implements Printable

fn Point.toString(ref self) -> String {
    f"({self.x}, {self.y})"
}
```

Key difference: Fuse requires explicit `implements`. This is intentional —
explicit conformance catches errors at declaration time rather than at
use time. If you forget to implement a method, the error points to the
`implements` declaration, not to a function call three files away.

---

*Document created from design sessions. Source: `interfaces_n_others.md` §1–§6.*
*Last updated: 2026-04-06*
