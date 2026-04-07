# ADR-013 Compile-time reflection — interfaces for behavior, decorators for directives

## Status
Accepted

## Context
Runtime reflection (inspecting types, fields, and methods at runtime) is a common language feature that enables serialization, test discovery, debug printing, dependency injection, and plugin systems. However, it carries significant performance costs:

- **Metadata storage** — binary must embed type names, field names, field types, and method signatures, bloating binary size 10-30%.
- **Prevents optimization** — the compiler cannot dead-code-eliminate fields, devirtualize calls, or inline methods for types that might be reflected upon at runtime.
- **Forces boxing** — reflecting on a value requires a uniform representation; you cannot reflect on a bare `i64` in a register.
- **Inhibits monomorphization** — generic code using reflection cannot be specialized per type.
- **Runtime type checks** — `is`, `as`, type switches require discriminant checks at runtime.

These costs directly conflict with Fuse's three non-negotiable properties: memory safety without GC (ASAP destruction requires knowing when values die, not preserving them for reflection), concurrency safety without a borrow checker (unboxed values stay in registers), and developer experience (fast binaries).

Meanwhile, every use case that motivates runtime reflection has a compile-time alternative that produces **zero runtime cost**.

## Decision

Fuse uses **compile-time reflection only**. No type metadata is emitted in the binary. There is no runtime reflection API.

The two mechanisms are:

1. **Interfaces** — define behavioral contracts. When a type says `implements SomeInterface`, the compiler auto-generates the required methods from field metadata if possible. If auto-generation is not possible (custom logic needed), the user writes the methods manually. Interfaces are used for: equality, hashing, comparison, serialization, encoding, debugging — anything that defines "what methods does this type have."

2. **Decorators** — compiler directives that do not define behavioral contracts. They instruct the compiler about lifecycle, linking, optimization, testing, and tooling. Decorators are used for: `@value`, `@entrypoint`, `@export`, `@test`, `@inline`, `@builder`, etc.

**The key distinction: interfaces define behavior. Decorators configure the compiler.** There are no behavioral decorators like `@equatable` or `@hashable` — those responsibilities belong to interfaces.

## How it works

### `implements` triggers auto-generation

When a `data class` or `@value struct` declares `implements`, the compiler knows every field name, type, and offset at compile time. It generates the required methods automatically:

```fuse
data class Key(name: String, id: Int) implements Hashable
```

The compiler generates (no user code required):

```fuse
// Satisfies Equatable (parent of Hashable)
fn Key.eq(ref self, ref other: Key) -> Bool {
    self.name == other.name && self.id == other.id
}

// Satisfies Hashable
fn Key.hash(ref self) -> Int {
    var h = 17
    h = h * 31 + self.name.hash()
    h = h * 31 + self.id.hash()
    h
}
```

**The `implements` clause is the only signal.** No decorator needed. The compiler sees the interface, knows the fields, and generates conformance.

### Auto-generation rules

| Type | Auto-generation | Rationale |
|---|---|---|
| `data class` | Always available | Fields are fully declared in the parameter list; compiler has complete knowledge |
| `@value struct` | Always available | `@value` guarantees lifecycle methods exist; fields are known |
| Plain `struct` (no `@value`) | Not available — user must write methods manually | Without `@value`, the compiler cannot assume field layout for codegen |
| `enum` | Available for some interfaces (e.g., `Printable`, `Serializable`) | Variants are known at compile time; field-wise generation applies per variant |

If a type declares `implements` but auto-generation is not possible, and the user has not written the required methods, the checker emits a clear error:

```
error: `CustomKey` implements `Hashable` but does not provide `hash(ref self) -> Int`
  → either add `@value` to enable auto-generation, or implement the method manually
```

### Manual implementation is always an option

```fuse
struct CustomKey {
    val data: List<Int>
}

// Manually implement Hashable — no auto-generation, full control
fn CustomKey.eq(ref self, ref other: CustomKey) -> Bool {
    self.data == other.data
}

fn CustomKey.hash(ref self) -> Int {
    var h = 0
    for item in self.data {
        h = h * 31 + item
    }
    h
}
```

### Override auto-generated methods

If a type qualifies for auto-generation but the user writes a method manually, the manual method takes precedence:

```fuse
data class Point(x: Int, y: Int) implements Hashable

// Override: use only x for hashing (custom logic)
fn Point.hash(ref self) -> Int {
    self.x
}
// eq() is still auto-generated from both fields
```

## Stdlib interfaces

These interfaces belong in `stdlib/core/` and form the foundation for generic programming:

```fuse
// stdlib/core — always available

interface Equatable {
    fn eq(ref self, ref other: Self) -> Bool
}
// Default: fn Equatable.ne(ref self, ref other: Self) -> Bool { !self.eq(other) }

interface Hashable : Equatable {
    fn hash(ref self) -> Int
}

interface Comparable : Equatable {
    fn compareTo(ref self, ref other: Self) -> Int
}
// Defaults: lt, gt, le, ge derived from compareTo
fn Comparable.lt(ref self, ref other: Self) -> Bool { self.compareTo(other) < 0 }
fn Comparable.gt(ref self, ref other: Self) -> Bool { self.compareTo(other) > 0 }
fn Comparable.le(ref self, ref other: Self) -> Bool { self.compareTo(other) <= 0 }
fn Comparable.ge(ref self, ref other: Self) -> Bool { self.compareTo(other) >= 0 }

interface Printable {
    fn toString(ref self) -> String
}

interface Debuggable : Printable {
    fn debugString(ref self) -> String
}

// stdlib/ext or stdlib/full — available when imported

interface Serializable {
    fn toJSON(ref self) -> String
}
// Static method for deserialization (no self):
// fn Type.fromJSON(ref s: String) -> Result<Type, Error>

interface Encodable {
    fn encode(ref self, mutref encoder: Encoder)
}

interface Decodable {
    // Static method: fn Type.decode(mutref decoder: Decoder) -> Result<Type, Error>
}
```

### Auto-generation per interface

| Interface | Auto-generated method | Field-wise logic |
|---|---|---|
| `Equatable` | `eq(ref self, ref other: Self) -> Bool` | `self.f1 == other.f1 && self.f2 == other.f2 && ...` |
| `Hashable` | `hash(ref self) -> Int` (+ `eq` from parent) | `h = h * 31 + self.f1.hash()` for each field |
| `Comparable` | `compareTo(ref self, ref other: Self) -> Int` (+ `eq`) | Compare fields in declaration order; first non-zero wins |
| `Printable` | `toString(ref self) -> String` | `TypeName(f1, f2, ...)` — already done by `data class` |
| `Debuggable` | `debugString(ref self) -> String` (+ `toString`) | `TypeName { f1: v1, f2: v2 }` — with field names |
| `Serializable` | `toJSON(ref self) -> String`, `fromJSON(...)` | JSON object with field names as keys |
| `Encodable` | `encode(ref self, mutref encoder: Encoder)` | Encode fields in declaration order |
| `Decodable` | `decode(mutref decoder: Decoder) -> Result<Self, Error>` | Decode fields in declaration order |

## Decorators — compiler directives only

Decorators remain for things that are **not behavioral contracts**:

| Decorator | Target | Purpose |
|---|---|---|
| `@value` | struct, data class | Auto-generate `__copyinit__`, `__moveinit__`, `__del__` lifecycle methods |
| `@entrypoint` | function | Mark program entry point (`fuse_user_entry` symbol) |
| `@export(name)` | function | Set custom C-ABI linker symbol |
| `@deprecated(msg)` | function, type | Emit warning at call sites |
| `@rank(n)` | variable (Shared) | Compile-time lock ordering for deadlock prevention |
| `@test` | function | Register as test function for test runner |
| `@inline` | function | Inlining hint to Cranelift backend |
| `@unsafe` | function | Bypass ownership checks for FFI |
| `@builder` | struct, data class | Generate fluent builder: `Type.builder().f1(v).f2(v).build()` |
| `@ignore(reason)` | function (`@test`) | Skip test with reason |

**None of these define behavioral contracts. They are instructions to the compiler.**

## Complete example

```fuse
import core/map

// Hashable enables use as Map key
// Comparable enables sorted collections
// Debuggable enables rich debug output
data class Employee(
    name: String,
    id: Int,
    salary: Float
) implements Hashable, Comparable, Debuggable

@entrypoint
fn main() {
    val alice = Employee("Alice", 1, 95000.0)
    val bob = Employee("Bob", 2, 87000.0)

    // Equatable (auto-generated via Hashable parent)
    println(alice == bob)              // false

    // Hashable (auto-generated) — works as Map key
    var directory = Map::<Employee, String>.new()
    directory.set(alice, "Engineering")

    // Comparable (auto-generated) — sortable
    val team = [bob, alice]
    val sorted = team.sorted()         // sorted by name, then id, then salary

    // Debuggable (auto-generated)
    println(alice.debugString())       // Employee { name: "Alice", id: 1, salary: 95000.0 }

    // Printable (auto-generated via Debuggable parent)
    println(alice.toString())          // Employee("Alice", 1, 95000.0)
}
```

No decorators needed for any of the behavioral interfaces. Just `implements`.

## Implementation priority

| Phase | Stdlib interfaces | Notes |
|---|---|---|
| Already done | `Printable` (in guide, section 1.19) | Used in interface examples and tests |
| Phase 1 | `Equatable`, `Hashable` | Needed for HashMap-backed `Map<K,V>` and `Set<K>` |
| Phase 2 | `Comparable`, `Debuggable` | Sorting, rich debug output |
| Phase 3 | `Serializable`, `Encodable`, `Decodable` | Requires stdlib `json`/`toml` modules |

## Rejected alternatives

**Runtime reflection (`@reflect`):** Rejected because:
- Forces boxing for reflected types — incompatible with unboxed primitive strategy
- Creates two tiers of types (reflected vs. not) that interact poorly with generics
- Every use case has a zero-cost compile-time alternative
- Can be reconsidered post-stage-2 if a genuine need (plugin systems) surfaces

**Behavioral decorators (`@equatable`, `@hashable`, etc.):** Initially considered, then rejected. The `implements` clause already signals what the type needs. Adding a decorator on top is redundant — `@hashable data class Key(...) implements Hashable` says the same thing twice. Interfaces are the single mechanism for behavioral contracts.

**Derive macros (Rust-style proc macros):** Rejected because proc macros are a separate compilation unit with their own toolchain complexity. The compiler itself performs codegen from field metadata — no external toolchain needed.

**Structural typing / duck typing:** Rejected because Fuse uses nominal typing. `implements` is explicit. The compiler generates real methods on the declared type.

## Consequences

- **Zero runtime cost** for all reflection use cases. No metadata in the binary.
- **Single mechanism for behavior** — `implements` is the only way to declare behavioral contracts. No confusion between decorators and interfaces.
- **Decorators stay focused** — `@value`, `@entrypoint`, `@export`, `@test`, `@inline`, `@builder`, etc. are compiler directives, not behavioral contracts. The two systems do not overlap.
- **Auto-generation is opt-in** — only types that say `implements` get generated methods. Plain structs without `implements` are untouched.
- **Manual override** — users can always write methods manually to override auto-generated behavior.
- **Full optimization** — generated methods inline, specialize, and dead-code-eliminate like hand-written code.
- **Boilerplate elimination** — `implements Serializable` on a 20-field type replaces 40+ lines of hand-written encode/decode.
- **Clear errors** — if auto-generation is not possible (plain struct without `@value`), the checker tells the user exactly what to do.
