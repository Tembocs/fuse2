# Fuse Language Guide v2

> **For AI agents reading this document:**
> This is the single source of truth for the Fuse programming language — its specification, architecture, implementation plan, and test contract. Every section is self-contained. Code examples are complete and runnable. The pattern is: concept, code, rules, edge cases. No section depends on a later section.
>
> This document supersedes `fuse-language-guide.md`, `self_hosting.md`, `fuse-implementation-plan.md`, and `fuse-repository-layout.md`.

---

## Table of Contents

### Part 1: Language Specification
1.1 What Is Fuse
1.2 Language DNA
1.3 Fuse Core and Fuse Full
1.4 Variables and Types
1.5 Functions
1.6 Control Flow
1.7 Data Types
1.8 Pattern Matching
1.9 Ownership
1.10 Memory Model
1.11 Error Handling
1.12 Modules and Imports
1.13 Extension Functions
1.14 Generics
1.15 String Operations
1.16 FFI
1.17 Concurrency (Fuse Full)
1.18 SIMD (Fuse Full)

### Part 2: Architecture
2.1-2.7

### Part 3: Implementation Plan
3.1-3.9

### Part 4: Test Suite
4.1-4.5

### Part 5: Design Decisions (ADRs)
5.1-5.11

---

## 1.1 What Is Fuse

Fuse is a general-purpose programming language designed around three non-negotiable properties:

- **Memory safety without a garbage collector.** Memory is reclaimed deterministically at the last use of a value, not at an arbitrary collection cycle.
- **Concurrency safety without a borrow checker.** Ownership is declared through four readable keywords. The compiler enforces the rules; the developer does not negotiate with a lifetime system.
- **Developer experience as a first-class concern.** Every keyword, annotation, and error message is chosen so that reading code aloud produces a correct description of what it does.

Fuse is not a research language. It is designed to be implemented, self-hosted, and used to build production systems.

> Fuse is a statically typed, compiled language that combines the ownership model of Mojo, the error handling of Rust, the concurrency model of Go, the null safety of Kotlin, the readability of Python, and the type expressiveness of TypeScript — with a developer experience that ties them together into a single coherent whole.

---

## 1.2 Language DNA

Every feature in Fuse has been proven in production at scale. Fuse does not experiment — it integrates.

| Source | What Fuse takes |
|---|---|
| **Mojo** | `owned`/`mutref`/`ref` argument conventions, ASAP destruction, `@value` auto-lifecycle, `SIMD<T,N>` primitives |
| **Rust** | `Result<T,E>`, `Option<T>`, `?` error propagation, exhaustive `match` |
| **Kotlin** | `val`/`var` type inference, Elvis operator `?:`, optional chaining `?.`, `data class`, scope functions (`let`, `also`, `takeIf`) |
| **C#** | LINQ-style method chains (`.map`, `.filter`, `.sorted`) |
| **Python** | `f"..."` string interpolation, list comprehensions, `@decorator` syntax |
| **Go** | `spawn` (goroutines), `defer`, typed channels (`Chan<T>`) |
| **TypeScript** | Union types (`A \| B \| C`), optional chaining `?.`, `interface` constraints |

---

## 1.3 Fuse Core and Fuse Full

Fuse is divided into two tiers. **Fuse Core** is the minimal subset sufficient to write a compiler. **Fuse Full** adds concurrency and SIMD on top of Core.

### Fuse Core — included

- `fn`, `struct`, `data class`, `enum`, `@value`, `@entrypoint`
- `val`, `var`, type inference
- `ref`, `mutref`, `owned`, `move`
- `Result<T,E>`, `Option<T>`, `match`, `when`, `?`
- `if`/`else`, `for`/`in`, `while`, `loop`, `break`, `continue`, `return`
- `List<T>`, `Map<K, V>`, `String`, `Int`, `Float`, `Bool`
- `f"..."` interpolation, `?.` chaining, `?:` Elvis
- `defer`
- Extension functions, expression-body functions, block expressions
- Integer division (truncating), float division
- Modules and imports

### Fuse Full — not in Core

- `spawn`, `Chan<T>`, `Shared<T>`, `@rank`
- `SIMD<T,N>`
- `interface`/`trait` polymorphism

**Rule:** Implement Core first. A working Core interpreter validates the language design before concurrency complexity is introduced.

Fuse Core is stable. The Stage 0 Python interpreter implements the complete Core feature set needed by the revised phases 1-5 delivery. 25 Core tests plus the milestone program pass. The `tests/fuse/full/` files remain Phase 1 contract artifacts and are not part of the runnable Stage 0 suite.

---

## 1.4 Variables and Types

Variables are declared with `val` (immutable) or `var` (mutable). The compiler infers types from initializers. Explicit type annotations are optional.

### Primitive types

| Type | Description | Size |
|---|---|---|
| `Int` | 64-bit signed integer | 8 bytes |
| `Float` | 64-bit IEEE 754 double | 8 bytes |
| `Bool` | `true` or `false` | 1 byte |
| `String` | UTF-8 text, dynamically sized | pointer + length |

### Compound types

| Type | Description |
|---|---|
| `List<T>` | Dynamically-sized ordered collection of elements of type `T` |
| `Map<K, V>` | Key-value collection with unique keys, unordered |
| `Option<T>` | Either `Some(value)` or `None` — nullable types without null |
| `Result<T, E>` | Either `Ok(value)` or `Err(error)` — fallible operations without exceptions |

### Code example

```fuse
// val is immutable — cannot be reassigned
val name = "Amara"
val score = 95.5
val active = true
val items = [1, 2, 3]

// var is mutable — can be reassigned
var count = 0
count = count + 1

// explicit type annotations (optional — compiler infers)
val ratio: Float = 98.6
var names: List<String> = []

// Map<K, V> — key-value collection
var scores = Map::<String, Int>.new()
scores.set("alice", 95)
scores.set("bob", 87)
val s = scores.get("alice")       // Option<Int> → Some(95)
val missing = scores.get("eve")   // Option<Int> → None
```

### Rules

- `val` is the default. Use `var` only when the value changes.
- The compiler infers types from initializers. Explicit annotations are optional but permitted.
- Every variable must be initialized at declaration. There are no uninitialized variables.
- `Int` is 64-bit signed integer. Integer overflow is undefined behavior in release mode; it traps in debug mode.
- `Float` is 64-bit IEEE 754 double-precision floating point.
- `Bool` is `true` or `false`. There is no implicit conversion from integers to booleans.
- `String` is UTF-8 text. String literals use double quotes: `"hello"`.
- `List<T>` is a dynamically-sized ordered collection. List literals use square brackets: `[1, 2, 3]`.
- `Map<K, V>` is an unordered key-value collection with unique keys. Created with `Map::<K, V>.new()`. Keys must support equality (`==`). `.get(key)` returns `Option<V>`, not a raw value — no null, no panic on missing key.

### Map methods

| Method | Signature | Description |
|---|---|---|
| `Map::<K, V>.new()` | `() -> Map<K, V>` | Create empty map |
| `.set(key, value)` | `(K, V) -> ()` | Insert or update entry |
| `.get(key)` | `(K) -> Option<V>` | Lookup — returns `Some(v)` or `None` |
| `.contains(key)` | `(K) -> Bool` | Check if key exists |
| `.remove(key)` | `(K) -> Option<V>` | Remove and return value |
| `.len()` | `() -> Int` | Number of entries |
| `.isEmpty()` | `() -> Bool` | True if no entries |
| `.keys()` | `() -> List<K>` | All keys (unordered) |
| `.values()` | `() -> List<V>` | All values (unordered) |
| `.entries()` | `() -> List<(K, V)>` | All key-value pairs |

```fuse
// Map example — symbol table for a compiler
var symbols = Map::<String, Int>.new()
symbols.set("x", 42)
symbols.set("y", 7)

match symbols.get("x") {
  Some(val) => println(f"x = {val}")
  None      => println("x not found")
}

println(f"has y: {symbols.contains("y")}")   // true
println(f"count: {symbols.len()}")           // 2

for key in symbols.keys() {
  println(f"  {key}")
}

symbols.remove("x")
println(f"after remove: {symbols.len()}")    // 1
```

---

## 1.5 Functions

Functions are declared with `fn`. Parameters have names, types, and optional ownership conventions. The return type follows `->`. The last expression in a block is the return value.

### Code examples

```fuse
fn add(a: Int, b: Int) -> Int {
  a + b    // last expression is the return value
}

// expression body for one-liners
fn double(n: Int) -> Int => n * 2

// explicit return for early exit
fn abs(n: Int) -> Int {
  if n < 0 { return -n }
  n
}

// functions with ownership conventions
fn greet(ref user: User) -> String {
  f"Hello, {user.name}"
}

fn process(mutref data: List<Int>) {
  data.push(42)
}

fn consume(owned conn: Connection) {
  // conn will be destroyed when this function ends
}
```

### Rules

- Parameters default to pass-by-value (the compiler chooses ref or copy as an optimization).
- Ownership conventions (`ref`, `mutref`, `owned`) are explicit when the function needs a specific guarantee about the argument. See section 1.9 for full ownership semantics.
- The last expression in a block is the return value. No `return` keyword is needed.
- `return` is for early exit only. Using `return` for the final expression is legal but not idiomatic.
- Functions that return nothing have no `->` annotation. The return type is implicit `Unit`.
- Expression-body syntax (`=> expr`) is shorthand for `{ expr }` when the body is a single expression.
- Function names use `camelCase`. Type names use `PascalCase`.

### The `@entrypoint` decorator

Every Fuse program must have exactly one entry point, marked with `@entrypoint`:

```fuse
@entrypoint
fn main() {
  println("Hello, world!")
}
```

The `@entrypoint` function takes no arguments and returns `Unit`. It is the first user code that executes.

---

## 1.6 Control Flow

Fuse provides five control flow constructs: `if`/`else`, `for`/`in`, `while`, `loop`, and `match`/`when` (see section 1.8 for pattern matching). Three keywords modify loop behavior: `break`, `continue`, and `return`.

### if/else

`if`/`else` is an expression. It returns a value.

```fuse
// if/else — expression (returns a value)
val status = if score > 90 { "excellent" } else { "good" }
```

When used as an expression, both branches must produce the same type. When used as a statement, the `else` branch is optional.

### for/in

`for`/`in` iterates over a `List`. There is no C-style `for(;;)`.

```fuse
// for/in — iterate over a list
for item in items {
  println(item)
}

// for with index tracking
var i = 0
for item in items {
  println(f"[{i}] {item}")
  i = i + 1
}
```

### while

`while` runs the body while the condition is true.

```fuse
// while — loop while condition is true
var count = 0
while count < 10 {
  println(f"count = {count}")
  count = count + 1
}
```

### loop

`loop` runs forever until `break` or `return` exits it.

```fuse
// loop — infinite loop, exit with break or return
loop {
  val line = readLine()
  if line == "quit" { break }
  println(f"you said: {line}")
}
```

### break

`break` exits the innermost loop, while, or for. It carries no value.

```fuse
// break — exits the innermost loop, while, or for
for item in items {
  if item == target { break }
  process(item)
}
```

### continue

`continue` skips to the next iteration of the innermost loop, while, or for.

```fuse
// continue — skips to the next iteration
for item in items {
  if item.isEmpty() { continue }
  process(item)
}
```

### break and continue in while and loop

`break` and `continue` work identically in `for`, `while`, and `loop`:

```fuse
var n = 0
while n < 100 {
  n = n + 1
  if n % 2 == 0 { continue }
  if n > 50 { break }
  println(f"odd: {n}")
}
```

### return

`return` exits the enclosing function, optionally with a value.

```fuse
fn findFirst(items: List<String>, target: String) -> Option<Int> {
  var i = 0
  for item in items {
    if item == target { return Some(i) }
    i = i + 1
  }
  None
}
```

### Rules

- `if`/`else` is an expression — it returns a value. When used as an expression, both branches must be present and must produce the same type.
- `for x in list` iterates over `List`. There is no C-style `for(;;)`.
- `while condition { body }` runs the body while the condition is true.
- `loop { body }` runs forever until `break` or `return`.
- `break` exits the innermost `loop`, `while`, or `for`. It carries no value.
- `continue` skips to the next iteration of the innermost `loop`, `while`, or `for`.
- `return` exits the enclosing function, optionally with a value.
- There is no `do-while`. Use `loop` with a conditional `break` at the end.

### Edge cases

- `break` and `continue` are compile errors outside a `loop`, `while`, or `for`.
- Nested loops: `break` and `continue` affect only the innermost loop. There are no labeled breaks.
- An `if` without `else` used as an expression is a compile error.
- An empty `for` body is legal: `for item in items { }` does nothing but iterates.

---

## 1.7 Data Types

Fuse provides four ways to define composite types: `struct`, `data class`, `enum`, and the `@value` decorator.

### struct

A `struct` is a named record with fields and methods. Fields are `val` (immutable) or `var` (mutable).

```fuse
struct Server {
  val host: String
  var port: Int

  fn address(ref self) -> String {
    f"{self.host}:{self.port}"
  }
}
```

### @value

The `@value` decorator auto-generates copy, move, and destructor lifecycle methods for a struct or data class.

```fuse
@value
struct Connection {
  val dsn: String
  var pending: Int

  fn __del__(owned self) {
    println(f"[del] Connection closed: {self.dsn}")
  }
}
```

When `@value` is applied:
- `__copyinit__` is generated (deep copy of all fields).
- `__moveinit__` is generated (move all fields, invalidate source).
- `__del__` is generated (destroy all fields). If you define `__del__` manually, the auto-generated version is replaced.

### data class

A `data class` is a `struct` with `@value` plus auto-generated structural equality (`==`, `!=`) and `toString()`.

```fuse
data class Point(val x: Int, val y: Int)

// usage:
val p1 = Point(1, 2)
val p2 = Point(1, 2)
println(p1 == p2)    // true (structural equality)
println(p1)          // Point(1, 2)
```

`data class` uses a compact syntax: fields are declared in the parenthesized parameter list. This is equivalent to defining a struct with those fields plus `@value` plus equality and toString.

### enum

An `enum` is a tagged union. Variants can carry data (like Rust enums / algebraic data types).

```fuse
enum Status {
  Ok,
  Warn(String),
  Err(String)
}

val s = Status.Warn("elevated")
match s {
  Status.Ok       => println("all good")
  Status.Warn(w)  => println(f"warning: {w}")
  Status.Err(e)   => println(f"error: {e}")
}
```

### Built-in enums

Two enums are built into the language and do not need to be imported:

```fuse
enum Result<T, E> { Ok(T), Err(E) }
enum Option<T> { Some(T), None }
```

These are used pervasively for error handling (section 1.11) and nullable values.

### Rules

- `struct` fields are `val` (immutable) or `var` (mutable). Methods receive `self` with an ownership convention: `ref self`, `mutref self`, or `owned self`.
- `data class` generates `==`, `!=`, and `toString()` from fields. All data class fields participate in equality.
- `@value` generates `__copyinit__`, `__moveinit__`, and `__del__`. Define only `__del__` when custom cleanup is needed; the other two are always auto-generated.
- `enum` variants can carry zero or more data fields. Variants without data are unit variants (e.g., `Ok`, `None`).
- `data` is a contextual keyword. It is only special when followed by `class`. The identifier `data` can be used as a variable name elsewhere.
- Constructors use the type name as a function: `Point(1, 2)`, `Server { host: "localhost", port: 8080 }`. Data classes use positional syntax; structs use named-field syntax.

---

## 1.8 Pattern Matching

Fuse provides two pattern matching constructs: `match` (exhaustive, matches against a subject value) and `when` (condition-based, no subject).

### match

`match` destructures a value against patterns. The compiler enforces exhaustiveness: every possible case must be handled.

```fuse
// match on Result
match result {
  Ok(value)             => println(f"got: {value}")
  Err("not found")      => println("missing")
  Err(e)                => println(f"error: {e}")
}

// match on enum variants
match status {
  Status.Ok            => println("ok")
  Status.Warn(msg)     => println(f"warn: {msg}")
  Status.Err(msg)      => println(f"err: {msg}")
}

// match on tuples
match (lang, title) {
  ("sw", Some(t)) => f"Karibu, {t} {name}!"
  ("sw", None)    => f"Karibu, {name}!"
  (_,    Some(t)) => f"Welcome, {t} {name}."
  _               => f"Hello, {name}."
}

// match on literals
match code {
  200 => "ok"
  404 => "not found"
  _   => "unknown"
}
```

### when

`when` is for condition-based branching. It has no subject value. Each arm is a boolean condition.

```fuse
val label = when {
  score >= 90 => "excellent"
  score >= 70 => "good"
  score >= 50 => "pass"
  else        => "fail"
}
```

### Patterns supported

| Pattern | Example | Matches |
|---|---|---|
| Literal | `200`, `"hello"`, `true` | Exact value |
| Variable binding | `x`, `msg` | Any value, bound to the name |
| Constructor | `Ok(x)`, `Some(val)` | Enum variant, binds inner data |
| Qualified constructor | `Status.Warn(msg)` | Specific variant of a specific enum |
| Tuple | `(a, b)`, `("sw", None)` | Tuple with matching structure |
| Wildcard | `_` | Any value, not bound to a name |

### Rules

- `match` is exhaustive. The compiler rejects a `match` that does not cover all possible values of the subject type.
- The wildcard `_` covers all remaining cases. Placing `_` before other arms makes those later arms unreachable (compile warning).
- `when` is for condition-based branching with no subject to match against.
- `when` requires an `else` arm unless all conditions are provably exhaustive.
- Both `match` and `when` are expressions. They return a value. When used as an expression, all arms must produce the same type.
- Arms are evaluated top to bottom. The first matching arm wins.
- Each arm body can be a single expression (after `=>`) or a block (`=> { ... }`).

### Edge cases

- A `match` on a `Bool` with `true` and `false` arms is exhaustive without `_`.
- A `match` on an `enum` with one arm per variant is exhaustive without `_`.
- A `match` on `Int` or `String` requires `_` because these types have unbounded values.
- Nested patterns are supported: `Some(Ok(value))` destructures through both `Option` and `Result`.

---

## 1.9 Ownership

Ownership is the core of Fuse's memory safety model. Instead of a garbage collector or a borrow checker with lifetimes, Fuse uses four keywords that express the full spectrum of value access:

```
ref  ->  mutref  ->  owned  ->  move
read it    change it    own it     transfer it
```

### ref — read-only access

`ref` provides read-only access to a value. The callee cannot modify it. The caller retains ownership. Multiple `ref` borrows can exist simultaneously.

```fuse
fn displayUser(ref user: User) {
  println(f"Name: {user.name}")
  println(f"Score: {user.score}")
  // cannot modify user — ref is read-only
}
```

### mutref — mutable access

`mutref` provides mutable access. The callee can modify the value and the caller sees the changes. The `mutref` keyword must appear at both the parameter declaration and the call site.

```fuse
fn addScore(mutref user: User, points: Int) {
  user.score = user.score + points
}

// call site must be explicit about mutation:
addScore(mutref player, 10)
//       ^^^^^^ signals: this will be modified
```

The call-site annotation is a deliberate design choice. Reading the call site tells you which arguments will be modified, without looking at the function signature.

### owned — full ownership

`owned` transfers ownership to the callee. The callee decides when the value is destroyed.

```fuse
fn closeConnection(owned conn: Connection) {
  println(f"Closing: {conn.dsn}")
  // conn is destroyed at end of function (__del__ fires)
}
```

### move — transfer at call site

`move` transfers ownership at the call site. After `move`, the compiler forbids any use of the variable in the current scope.

```fuse
closeConnection(move conn)
// conn is gone — using it here is a compile error
```

### Default convention

When no convention is specified, the compiler treats the parameter as pass-by-value. For read-only access, this is effectively `ref`. The compiler is free to choose the most efficient strategy (reference or copy) as an implementation detail.

```fuse
fn greet(name: String) -> String {  // implicitly ref
  f"Hello, {name}"
}
```

### Complete example

This program demonstrates all four ownership conventions in a single flow:

```fuse
data class Resource(val name: String, var uses: Int) {
  fn __del__(owned self) {
    println(f"[del] Resource released: {self.name}")
  }
}

fn inspect(ref r: Resource) {
  println(f"Name: {r.name}, uses: {r.uses}")
}

fn increment(mutref r: Resource) {
  r.uses = r.uses + 1
}

fn consume(owned r: Resource) {
  println(f"Consuming: {r.name} (used {r.uses} times)")
}

@entrypoint
fn main() {
  var r = Resource("file.txt", 0)
  inspect(ref r)                // read — r is unchanged
  increment(mutref r)           // mutate — r.uses is now 1
  increment(mutref r)           // mutate — r.uses is now 2
  consume(move r)               // transfer — r is gone
  // r cannot be used here
}
```

Expected output:
```
Name: file.txt, uses: 0
Name: file.txt, uses: 2
Consuming: file.txt (used 2 times)
[del] Resource released: file.txt
```

### Convention summary

| Convention | Where written | Mutates caller's value | Transfers ownership | Runtime cost |
|---|---|---|---|---|
| `ref` | parameter | no | no | zero (pointer) |
| `mutref` | parameter + call site | yes | no | zero (pointer) |
| `owned` | parameter | n/a | callee decides | move or copy |
| `move` | call site only | n/a | yes, enforced by compiler | zero (pointer transfer) |

### Rules

- `ref` is the default. Multiple concurrent `ref` borrows of the same value are always safe.
- `mutref` must be explicit at both the parameter declaration and the call site. While a `mutref` borrow is active, no other borrows (ref or mutref) of the same value are permitted.
- `owned` means the callee takes full ownership. Whether the value is moved or copied depends on the call site (`move` for transfer, default for copy).
- After `move`, the compiler forbids any use of the moved-from variable in the current scope. This is a hard compile error, not a warning.
- There is no garbage collector. There is no borrow checker. There are no lifetime annotations.
- The four keywords (`ref`, `mutref`, `owned`, `move`) carry all information needed for memory safety. The ownership model is complete with these four concepts.

---

## 1.10 Memory Model

Fuse uses **ASAP (As Soon As Possible) destruction**. Values are destroyed at their last use, not at the end of their lexical scope. This is deterministic and predictable.

### ASAP destruction

When a value's last use occurs, the compiler inserts a destructor call immediately after that statement. If the value has a `__del__` method, it fires at that point.

```fuse
@value
data class Sensor(val id: String) {
  fn __del__(owned self) {
    println(f"[del] Destroyed: {self.id}")
  }
}

@entrypoint
fn main() {
  val a = Sensor("A")
  val b = Sensor("B")
  println(f"Using A: {a.id}")
  // a's last use was the line above — a is destroyed HERE
  println(f"Using B: {b.id}")
  // b's last use was the line above — b is destroyed HERE
  println("Done")
}
```

Expected output:
```
Using A: A
[del] Destroyed: A
Using B: B
[del] Destroyed: B
Done
```

Note: `"Done"` prints after both destructors because both `a` and `b` were last used before the final `println`.

### defer

`defer` schedules a statement to run at function exit, after all ASAP destruction. Multiple `defer` statements execute in LIFO (last-in, first-out) order.

```fuse
fn example() {
  defer println("cleanup 1")
  defer println("cleanup 2")
  defer println("cleanup 3")
  println("working...")
}
```

Expected output:
```
working...
cleanup 3
cleanup 2
cleanup 1
```

`defer` is useful for cleanup that must happen regardless of how the function exits (normal return, early return, or error propagation).

### Destruction order within a function

The destruction order follows these rules, applied strictly:

1. **Statements execute sequentially.** Each statement runs to completion before the next begins.
2. **ASAP destruction fires after each statement.** After a statement completes, if any value's last reference was in that statement and the value has a `__del__` method, the destructor fires immediately.
3. **Remaining values are destroyed.** After all statements have executed, any values still alive (because their last use was the final statement) are destroyed.
4. **`defer` callbacks fire last, in LIFO order.** After all ASAP destruction is complete, `defer` callbacks execute in reverse registration order.

### Interaction with ownership

- Variables transferred via `move` are NOT destroyed in the caller's scope. The callee is now responsible for destruction.
- Variables passed via `ref` or `mutref` are NOT destroyed by the callee. The caller retains ownership.
- Variables passed via `owned` (without `move` at call site) are copied. The caller's copy follows normal ASAP rules; the callee's copy is destroyed in the callee.
- Variables referenced in a `defer` block are kept alive until the defer executes. ASAP destruction does not fire before the defer for values the defer captures.

### Rules

- No GC. No borrow checker. No reference counting. Values are destroyed at last use.
- `@value` auto-generates `__copyinit__`, `__moveinit__`, and `__del__`. All three are generated; you only need to manually define `__del__` when custom cleanup is required.
- `defer` runs at function exit, after ASAP destruction, in LIFO order.
- ASAP destruction is a compiler analysis. The compiler tracks the last use of every value and inserts destructor calls at the earliest safe point.
- A value used in a conditional branch is considered "last used" at the branch point if it is not used after the branch. The compiler is conservative: if any branch uses the value, it is kept alive through all branches.

### Edge cases

- A value created but never used is destroyed immediately after creation.
- A value used only in its initializer (`val x = expensive()` followed by no uses of `x`) is destroyed after the initialization statement.
- A value captured by a closure extends its lifetime to match the closure's lifetime.
- `defer` does not affect ASAP analysis of values not referenced by the defer. Only values explicitly mentioned in the defer body are kept alive.

---

## 1.11 Error Handling

Fuse has no null, no unchecked exceptions, and no silent failures. Every fallible operation returns `Result<T, E>`. Every potentially absent value is `Option<T>`. The `?` operator, optional chaining (`?.`), and the Elvis operator (`?:`) eliminate boilerplate while keeping error paths explicit.

### Result<T, E>

```fuse
// Result<T, E> — every fallible operation returns one
fn divide(a: Float, b: Float) -> Result<Float, String> {
  if b == 0.0 { return Err("division by zero") }
  Ok(a / b)
}

// ? operator — unwrap Ok/Some or return Err/None immediately
fn calculate(x: Float, y: Float) -> Result<String, String> {
  val result = divide(x, y)?
  Ok(f"result: {result}")
}

// match on Result
@entrypoint
fn main() {
  match calculate(10.0, 3.0) {
    Ok(s)  => println(s)
    Err(e) => println(f"Error: {e}")
  }
  match calculate(10.0, 0.0) {
    Ok(s)  => println(s)
    Err(e) => println(f"Error: {e}")
  }
}
```

Expected output:

```
result: 3.3333333333333335
Error: division by zero
```

### Option<T> and optional chaining

```fuse
// Option<T> — explicit absence, no null
data class Profile(val displayName: String, val locale: String)
data class User(val name: String, val profile: Option<Profile>)

// optional chaining (?.) — short-circuits to None
fn getLocale(user: User) -> String {
  val locale = user.profile?.locale ?: "en"
  locale
}

// Elvis operator (?:) — unwrap or use fallback
fn getDisplayName(user: User) -> String {
  user.profile?.displayName ?: user.name
}
```

### Rules

- Every fallible function returns `Result<T,E>`.
- Every nullable value is `Option<T>`.
- There is no null, no unchecked exception, no silent failure.
- `?` on a `Result`: unwraps `Ok(v)` to `v`, or returns `Err(e)` from the function.
- `?` on an `Option`: unwraps `Some(v)` to `v`, or returns `None` from the function.
- `?.` chains through optional values — short-circuits to `None`.
- `?:` provides a fallback value when the left side is `None`.
- `match` is exhaustive on `Result` and `Option` — all variants must be handled.

### Edge cases

- `?` can only be used in a function whose return type is `Result` or `Option`. Using `?` in a function that returns `Int` is a compile error.
- `?.` produces an `Option` — you cannot chain `?.` and then use the result as a bare value without unwrapping.
- `?:` evaluates the right-hand side lazily — the fallback expression is not evaluated if the left side is `Some`.

---

## 1.12 Modules and Imports

Every `.fuse` file is a module. The file name is the module name. Items are private by default; use `pub` to export them. Import paths use dots, not slashes.

### Module definition and usage

```fuse
// file: src/lexer/token.fuse
pub enum Tok {
  Fn, Val, Var, If, Else, For, While, Loop, Break, Continue,
  Return, Match, When, Enum, Struct, Data, Class,
  Ref, Mutref, Owned, Move, Defer, Extern, Pub, Import,
  Ident(String), IntLit(Int), FloatLit(Float), StrLit(String),
  Plus, Minus, Star, Slash, Eq, EqEq, BangEq, Lt, Gt, LtEq, GtEq,
  LParen, RParen, LBrace, RBrace, LBracket, RBracket,
  Arrow, FatArrow, Dot, Comma, Colon, Question, At,
  Eof
}

pub data class Token(val ty: Tok, val line: Int, val col: Int)

pub fn isKeyword(s: String) -> Bool {
  // ...
}

// file: src/parser/parser.fuse
import lexer.token.{Token, Tok}

pub fn parse(tokens: List<Token>) -> Program {
  // can use Token and Tok directly
}

// file: src/main.fuse
import lexer.token
import parser.parser.{parse}

@entrypoint
fn main() {
  val tokens = tokenize(source)   // from token module
  val program = parse(tokens)     // imported directly
}
```

### Module resolution rules

- `import a.b.c` maps to `src/a/b/c.fuse` relative to the project root.
- `import a.b.{X, Y}` imports specific items from `src/a/b.fuse`.
- `import a.b` imports the module — access items as `b.X`, `b.Y`.
- `pub` makes items visible outside their module. Items without `pub` are module-private.
- Circular imports are a compile error.
- Each `.fuse` file is one module. The file name is the module name.

### Rules

- All items are private by default. Use `pub` to export.
- `pub` can be applied to: `fn`, `struct`, `data class`, `enum`, `val`, `var`.
- The `import` statement must appear at the top of the file, before any declarations.
- Module paths use dots (`.`), not slashes.

### Edge cases

- A module with no `pub` items can still be imported — it simply contributes nothing to the importing scope. This is not an error, but the compiler may emit a warning.
- If two imports bring in the same name, the compiler reports an ambiguity error. Resolve by using qualified access (`module.Name`) instead of destructured imports.
- Nested directories map directly to nested module paths: `src/a/b/c.fuse` is `a.b.c`.

### Future: Package Management

A package manager is a post-self-hosting concern. The module system (`import`/`pub`) is the foundation it will be built on. The package manager will not be designed until Stage 2 is complete and developers are writing real Fuse libraries — its design depends on decisions about versioning, registry hosting, native dependencies, and cross-compilation that cannot be answered responsibly until the language is stable and in use. Cargo, Go modules, and npm are living proof that a package manager designed too early creates permanent ecosystem debt, while one designed after language maturity becomes an asset.

---

## 1.13 Extension Functions

Extension functions add methods to existing types without modifying their definition. They use dot syntax with an explicit receiver and are resolved at compile time.

```fuse
// extend existing types with new methods
fn String.scream(ref self) -> String {
  self.toUpper() + "!"
}

fn Int.isEven(ref self) -> Bool {
  self % 2 == 0
}

fn List.sum(ref self) -> Int {
  var total = 0
  for item in self {
    total = total + item
  }
  total
}

@entrypoint
fn main() {
  println("hello".scream())     // HELLO!
  println(f"4 even? {4.isEven()}")  // 4 even? true
  println(f"sum: {[1,2,3].sum()}")  // sum: 6
}
```

### Rules

- Extension functions use dot syntax: `fn TypeName.methodName(ref self, ...)`.
- `self` is the receiver — it follows the same ownership conventions (`ref`, `mutref`, `owned`).
- Extension functions cannot access private fields of the type.
- Extension functions are resolved at compile time (no dynamic dispatch).
- Multiple extensions can be defined for the same type in different modules.

### Edge cases

- If an extension function has the same name as a built-in method, the built-in method takes priority. The extension is shadowed — no error, but the compiler may emit a warning.
- Extension functions must be in scope (imported) to be callable. Defining an extension in module `a` does not make it available in module `b` unless `b` imports it.
- Extension functions on generic types use the same type parameter syntax: `fn List<T>.first(ref self) -> Option<T>`.

---

## 1.14 Generics

Fuse supports generic types and functions with angle-bracket syntax. Generics are monomorphized at compile time — there is no runtime cost and no type erasure.

```fuse
// generic enum
enum Result<T, E> {
  Ok(T),
  Err(E)
}

enum Option<T> {
  Some(T),
  None
}

// generic data class
data class Pair<A, B>(val first: A, val second: B)

// generic function
fn first<T>(items: List<T>) -> Option<T> {
  if items.isEmpty() { return None }
  Some(items.get(0))
}

// generic struct
struct Stack<T> {
  var items: List<T>
  
  fn push(mutref self, item: T) {
    self.items.push(item)
  }
  
  fn pop(mutref self) -> Option<T> {
    if self.items.isEmpty() { return None }
    Some(self.items.last())
  }
}
```

### Rules

- Type parameters are written in angle brackets: `<T>`, `<T, E>`, `<A, B>`.
- Generic types are monomorphized at compile time (no runtime cost).
- The compiler infers type arguments where possible.

### Edge cases

- Monomorphization means each unique instantiation produces a separate copy of the function or type in the compiled output. This increases binary size but eliminates dynamic dispatch overhead.
- A generic function with no callers is not monomorphized and produces no code.
- Type parameters cannot be shadowed: `fn foo<T>(x: T) -> T` inside a `struct Bar<T>` uses the same `T` — redefining it is a compile error.

---

## 1.15 String Operations

Strings in Fuse are immutable sequences of bytes. All modification operations return new strings. Parsing operations return `Result`, not exceptions.

```fuse
// f-string interpolation
val name = "Amara"
val greeting = f"Hello, {name}!"
val math = f"2 + 2 = {2 + 2}"

// string methods
val s = "Hello, World"
println(s.len())              // 12
println(s.charAt(0))          // H
println(s.substring(0, 5))    // Hello
println(s.contains("World"))  // true
println(s.startsWith("Hello"))// true
println(s.split(", "))        // [Hello, World]
println(s.trim())             // Hello, World
println(s.toUpper())          // HELLO, WORLD
println(s.toLower())          // hello, world
println(s.replace("World", "Fuse")) // Hello, Fuse

// char code operations
val ch = "A"
println(ch.charCodeAt(0))     // 65
println(fromCharCode(66))     // B

// parsing
match parseInt("42") {
  Ok(n)  => println(f"got: {n}")
  Err(e) => println(f"bad: {e}")
}
```

### Rules

- Strings are immutable. Operations return new strings.
- `len()` returns byte length (not character count).
- `charAt(i)` returns the character at byte index `i`.
- f-strings can contain any expression inside `{...}`.
- `parseInt` and `parseFloat` return `Result` (not exceptions).

### Edge cases

- `charAt` on an index beyond `len() - 1` is a runtime panic. Use `len()` to bounds-check first.
- `substring(start, end)` uses half-open range: `[start, end)`. If `end > len()`, the result is truncated to `len()`.
- f-strings nest: `f"outer {f"inner {x}"}"` is valid. The inner f-string is evaluated first.
- Empty string `""` has `len() == 0`. Calling `charAt(0)` on it panics.

---

## 1.16 FFI (Foreign Function Interface)

FFI allows Fuse code to call functions defined in other languages (typically C or the Fuse runtime). Foreign functions bypass Fuse's ownership and type checking — the developer is responsible for correctness.

```fuse
// declare external functions
extern fn fuse_rt_println(val: Ptr) -> ()
extern fn fuse_rt_int(v: Int) -> Ptr
extern fn fuse_rt_str(ptr: Ptr, len: Int) -> Ptr
extern fn fuse_rt_add(a: Ptr, b: Ptr) -> Ptr

// extern blocks group related declarations
extern "fuse-runtime" {
  fn fuse_rt_println(val: Ptr) -> ()
  fn fuse_rt_int(v: Int) -> Ptr
  fn fuse_rt_add(a: Ptr, b: Ptr) -> Ptr
}
```

### FFI types

- `Ptr` — 64-bit raw pointer (opaque handle to native memory).
- `Byte` — 8-bit unsigned byte for raw buffers.
- `Int`, `Float`, `Bool` — passed as raw values, no boxing.

### Rules

- Foreign functions bypass Fuse's ownership and type checking.
- The developer is responsible for: correct argument types, memory management of raw pointers, thread safety.
- FFI is a boundary, not an escape hatch. Wrap foreign calls in safe Fuse functions.
- `extern fn` is for individual declarations. `extern "lib" { ... }` groups related ones.
- The library name is a documentation hint — the linker resolves symbols.

### Edge cases

- Calling an `extern fn` with the wrong number or type of arguments is undefined behavior. The compiler trusts the declaration.
- `Ptr` values cannot be dereferenced in Fuse. They can only be passed to other `extern fn` calls or stored.
- An `extern fn` that returns `Ptr` transfers ownership of that pointer to the Fuse caller. If the foreign library expects the caller to free it, the caller must call the appropriate foreign free function.

---

## 1.17 Concurrency (Fuse Full)

Fuse provides a three-tier concurrency model. Each tier has a clear use case. The compiler enforces correct usage at compile time where possible.

### Tier 1 — Channels (preferred, zero locks)

Channels are the default communication primitive. They require no locks and no shared state.

```fuse
val (tx, rx) = Chan::<Int>.bounded(10)

spawn {
  tx.send(42)
}

val value = rx.recv()?
println(f"got: {value}")
```

### Tier 2 — Shared<T> with @rank (when sharing is necessary)

When multiple tasks must share a live mutable value, use `Shared<T>`. Every `Shared<T>` must be annotated with `@rank` to enforce a global lock acquisition order at compile time.

```fuse
@rank(1) val config  = Shared::new(Config.load())
@rank(2) val db      = Shared::new(Db.open("localhost"))
@rank(3) val metrics = Shared::new(Vec::<Metric>.new())

fn update() {
  val ref    cfg  = config.read()     // rank 1
  val mutref conn = db.write()        // rank 2 > 1 — ok
  val mutref m    = metrics.write()   // rank 3 > 2 — ok
  m.push(fetchRow(ref conn, ref cfg))
}

fn broken() {
  val mutref m    = metrics.write()   // rank 3
  val mutref conn = db.write()        // rank 2 < 3 — COMPILE ERROR
}
```

### Tier 3 — try_write (dynamic lock order)

When the lock order is truly dynamic (e.g., locking items from a list), use `try_write` with a timeout.

```fuse
fn dynamicUpdate(resources: List<Shared<Resource>>) -> Result<(), LockError> {
  val guards = [r.try_write(Timeout.ms(50))? for r in resources]
  for mutref g in guards { g.flush() }
  Ok(())
}
```

### Decision hierarchy

```
Does data need to flow between tasks?
  → Yes → Tier 1: Chan<T>

Must multiple tasks share a live mutable value?
  → Yes → Tier 2: Shared<T> + @rank

Is the lock order truly dynamic?
  → Yes → Tier 3: try_write(timeout)
```

### Spawn capture rules

- `spawn move { ... }` — task takes ownership of captured values (ok).
- `spawn ref { ... }` — task gets read-only access to captured values (ok).
- `spawn { mutref x }` — COMPILE ERROR (no mutable borrows across spawn).

### Rules

- `@rank` is mandatory on every `Shared<T>` — missing rank is a compile error.
- Locks must be acquired in ascending rank order — violations are a compile error.
- Same rank means independent — safe to acquire in any order.
- Guards are released by ASAP destruction — no forgotten unlocks.

### Edge cases

- A `Shared<T>` with `@rank(0)` can only be acquired first. No other lock can be held when acquiring rank 0.
- `try_write` returns `Err(LockError::Timeout)` if the lock cannot be acquired within the timeout. The caller must handle this.
- Spawned tasks that capture `ref` values extend the lifetime of those values until the task completes.

### How I/O works without async/await

Fuse does not have `async`/`await`. Functions that do I/O are normal functions. The caller decides whether to run them concurrently:

```fuse
// Normal function — no async keyword needed
fn fetchUser(id: Int) -> Result<User, NetworkError> {
    val resp = Http.get(f"/users/{id}")?
    match resp.status {
        200 => Ok(resp.json::<User>()?)
        _   => Err(NetworkError.Http(resp.status))
    }
}

// Sequential — just call it
@entrypoint
fn main() {
    val user = fetchUser(42)?
    println(user.name)
}

// Concurrent — spawn it, communicate via channel
@entrypoint
fn main() {
    val (tx, rx) = Chan::<Result<User, NetworkError>>.bounded(1)
    spawn move { tx.send(fetchUser(42)) }
    val user = rx.recv()??
    println(user.name)
}
```

There is no function coloring. Every function is the same. Concurrency is a call-site decision, not a function declaration decision.

### Patterns

**Timeout:**

```fuse
fn fetchWithTimeout(id: Int) -> Result<User, String> {
    val (tx, rx) = Chan::<Result<User, NetworkError>>.bounded(1)
    spawn move { tx.send(fetchUser(id)) }

    match rx.recv_timeout(Timeout.secs(5)) {
        Ok(result) => result.mapErr(fn(e: NetworkError) -> String { e.toString() })
        Err(_)     => Err("request timed out")
    }
}
```

**Worker pool:**

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

---

## 1.18 SIMD (Fuse Full)

SIMD (Single Instruction, Multiple Data) operations allow data-parallel computation on fixed-size vectors. Fuse exposes SIMD through a typed, lane-count-aware API that maps to platform intrinsics via Cranelift.

```fuse
// SIMD operations on vectors
val data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
val sum = SIMD::<Float32, 8>.sum(data)
val avg = sum / data.len().toFloat()
println(f"average: {avg}")
```

### Rules

- `SIMD<T, N>` where `T` is the element type and `N` is the lane count.
- Supported types: `Float32`, `Float64`, `Int32`, `Int64`.
- Lane counts must be powers of 2: 2, 4, 8, 16.
- SIMD operations are mapped to platform intrinsics via Cranelift.
- Fallback scalar implementation when hardware SIMD is unavailable.

### Edge cases

- If the input list length is not a multiple of the lane count, the trailing elements are processed with the scalar fallback.
- `SIMD<T, N>` with an unsupported lane count (e.g., 3, 5) is a compile error.
- `SIMD<String, 4>` is a compile error — only numeric primitive types are supported.
- On platforms without SIMD hardware support, all operations silently fall back to scalar loops with no API change required.

---

# Part 2: Architecture

## 2.1 Repository Layout

One repository, three stages, one test suite. The stages do not share source code — each is an independent implementation. They share test cases, the language guide, and standard library definitions.

```
fuse/
├── docs/                    # All documentation
│   ├── guide/               # Language guide (this document)
│   └── adr/                 # Architecture Decision Records
├── tests/fuse/              # Shared test suite
│   ├── milestone/           # four_functions.fuse
│   ├── core/                # Fuse Core tests (ownership, memory, errors, types)
│   └── full/                # Fuse Full tests (concurrency, simd)
├── stdlib/                  # Standard library written in Fuse
│   ├── core/                # Available in all stages
│   └── full/                # Stage 1+ only
├── examples/                # Standalone learning programs
├── stage0/                  # Python tree-walking interpreter
│   └── src/                 # lexer.py, parser.py, checker.py, evaluator.py, etc.
├── stage1/                  # Rust compiler + Cranelift backend
│   ├── fusec/               # Compiler binary crate
│   │   └── src/             # lexer/, parser/, ast/, hir/, checker/, codegen/
│   ├── fuse-runtime/        # Runtime library linked into compiled binaries
│   └── cranelift-ffi/       # C-compatible Cranelift wrappers (for Stage 2)
└── stage2/                  # Self-hosting compiler written in Fuse
    └── src/                 # The Fuse compiler in Fuse
```

Rules:
- Nothing in stage1/ depends on stage0/
- Nothing in stage2/ depends on stage0/ or stage1/ source (only on compiled binaries)
- A test passing in Stage 0 must produce identical output in Stage 1 and Stage 2
- The guide precedes implementation. If behavior is not in the guide, it does not exist

## 2.2 Stage 0 — Python Interpreter

Purpose: Validate language semantics. Does ref/mutref enforce correctly? Does match exhaust correctly?

Architecture: `Source → Lexer → Parser → AST → Checker → Tree-walking Evaluator`

Files:
| File | Responsibility |
|---|---|
| `main.py` | CLI entry point |
| `lexer.py` | Source text → token stream |
| `fuse_token.py` | Token type definitions |
| `parser.py` | Token stream → AST |
| `ast_nodes.py` | AST node dataclasses |
| `checker.py` | Ownership enforcement, match exhaustiveness |
| `evaluator.py` | Tree-walking execution |
| `environment.py` | Scope and binding management |
| `values.py` | Runtime value representations |
| `errors.py` | Error types and formatting |

Build/run commands:
```bash
cd stage0
python src/main.py <file.fuse>          # run a program
python src/main.py --check <file.fuse>  # check without running
python tests/run_tests.py               # run test suite
```

## 2.3 Stage 1 — Rust Compiler

Purpose: Produce native binaries. Semantics proven in Stage 0; Stage 1 is about performance and compilation.

Architecture: `Source → Lexer → Parser → AST → HIR Lowering → Checker → Cranelift Codegen → Object File → Linker → Native Binary`

Workspace: Cargo workspace with three crates:
1. `fusec` — the compiler binary
2. `fuse-runtime` — runtime library linked into every compiled program
3. `cranelift-ffi` — C-compatible wrappers for Cranelift (used by Stage 2 via FFI)

Key files in fusec:
| Directory | Responsibility |
|---|---|
| `lexer/` | Tokenizer (mirrors Stage 0) |
| `parser/` | Recursive descent parser |
| `ast/` | AST node definitions |
| `hir/` | High-level IR nodes + AST→HIR lowering |
| `checker/` | Types, ownership, exhaustiveness, rank, spawn |
| `codegen/` | HIR → Cranelift IR, value layout, ABI |
| `eval.rs` | Tree-walking evaluator (for testing, fallback) |

Build/run commands:
```bash
cd stage1
cargo build --release                                    # build compiler
cargo run --bin fusec -- <file.fuse>                     # run (interpret)
cargo run --bin fusec -- --compile <file.fuse> -o out    # compile to native
cargo test                                               # run test suite
```

## 2.4 Stage 2 — Self-Hosting Compiler

Purpose: Write the Fuse compiler in Fuse Core. Proves the language is complete.

Architecture: Same pipeline as Stage 1, but written in Fuse calling Cranelift via FFI.

`Source → Lexer → Parser → AST → Codegen (via Cranelift FFI) → Object File → Linker → Native Binary`

The Stage 2 compiler uses `extern fn` declarations to call Cranelift wrapper functions (from cranelift-ffi crate) and runtime functions (from fuse-runtime crate).

Bootstrap chain:
```
Step 1: fusec (Stage 1) compiles stage2/src/main.fuse → fusec2-bootstrap
Step 2: fusec2-bootstrap compiles stage2/src/main.fuse → fusec2-stage2  
Step 3: fusec2-stage2 compiles stage2/src/main.fuse → fusec2-verified
Step 4: sha256(fusec2-stage2) == sha256(fusec2-verified)  ← reproducibility proof
```

## 2.5 Compilation Pipeline

Detail of each stage in the pipeline:

1. **Lexer**: Source text → Token stream. Tracks line/column for errors. Handles: keywords, identifiers, literals (int, float, string, f-string, bool), operators, delimiters, comments.

2. **Parser**: Token stream → AST. Recursive descent. Expression precedence: assignment < elvis < or < and < not < comparison < addition < multiplication < unary < postfix < primary. Postfix: field access `.`, optional chain `?.`, question `?`, call `()`, lambda `{}`.

3. **HIR Lowering**: AST → HIR. Attaches type information to every expression. Two-pass: collect signatures first, then lower bodies. This is where type inference results are resolved.

4. **Checker**: Validates HIR. Ownership enforcement (ref/mutref/owned/move), match exhaustiveness, @rank ordering, spawn capture rules.

5. **Codegen**: HIR → Cranelift IR → Object file. Maps Fuse types to machine types. Emits function prologues/epilogues, branch blocks for control flow, ASAP destruction calls, mutref writeback, defer cleanup.

6. **Linker**: Object file + fuse-runtime library → native executable. Uses rustc as linker driver on MSVC, gcc/cc on Unix.

## 2.6 Runtime Internals

This section documents how the Fuse runtime works — critical for anyone implementing the codegen.

**FuseValue** — the universal runtime value type:
```rust
enum FuseValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    List(Vec<FuseValue>),
    Struct { type_name: String, fields: HashMap<String, FuseValue> },
    Enum { type_name: String, variant: String, data: Box<FuseValue> },
    Fn(fn_pointer),
    Unit,
}
```

All values in compiled code are passed as `*mut FuseValue` (i64 pointers in Cranelift). The runtime functions construct, inspect, and destroy these boxed values.

**Mutref Ref Cells** — how `mutref` parameters work at runtime:
1. Caller creates a ref cell: `fuse_rt_ref_new(value)` — wraps value in a List\<FuseValue\> cell
2. Caller passes the cell to the function
3. Callee unwraps: `fuse_rt_ref_get(cell)` — clones value out of cell
4. Callee works with the clone
5. On return: `fuse_rt_ref_set(cell, updated_value)` — writes back to cell
6. Caller reads back: `fuse_rt_ref_get(cell)` — gets updated value, redefines local variable

CRITICAL: The writeback (step 5) must happen for EVERY return path, including early `return` inside `loop`/`while`/`if`. Failure to write back causes the caller to see stale values.

**Field Access** — how struct fields work:
- `fuse_rt_field(obj, field_name_ptr, field_name_len)` — reads a field, returns cloned value
- `fuse_rt_set_field(obj, field_name_ptr, field_name_len, value)` — modifies field in place
- Fields are stored in a HashMap\<String, FuseValue\> inside the Struct variant

**Runtime Function Categories:**
| Category | Examples |
|---|---|
| Value construction | `fuse_rt_int(v)`, `fuse_rt_str(ptr, len)`, `fuse_rt_bool(v)` |
| Arithmetic | `fuse_rt_add(a, b)`, `fuse_rt_sub(a, b)`, `fuse_rt_mul(a, b)` |
| Comparison | `fuse_rt_eq(a, b)`, `fuse_rt_lt(a, b)`, `fuse_rt_ge(a, b)` |
| String ops | `fuse_rt_str_char_at(s, i)`, `fuse_rt_str_split(s, delim)` |
| List ops | `fuse_rt_list_push(list, val)`, `fuse_rt_list_get(list, i)` |
| Result/Option | `fuse_rt_ok(v)`, `fuse_rt_err(v)`, `fuse_rt_some(v)`, `fuse_rt_none()` |
| Struct ops | `fuse_rt_struct_new(name, len)`, `fuse_rt_field(obj, name, len)` |
| Map ops | `fuse_rt_map_new()`, `fuse_rt_map_set(map, k, v)`, `fuse_rt_map_get(map, k)` |
| I/O | `fuse_rt_println(v)`, `fuse_rt_read_file(path, len)` |
| Ref cells | `fuse_rt_ref_new(v)`, `fuse_rt_ref_get(cell)`, `fuse_rt_ref_set(cell, v)` |

## 2.7 Linker Integration

On Windows MSVC:
- Uses `rustc` as linker driver with `--crate-type=bin` and `#![no_main]` stub
- Passes object file and runtime library via `-C link-arg=`
- Stack size: `/STACK:8388608` (8MB for compiler workloads)

On Linux/macOS:
- Uses `cc`/`gcc` directly
- Links object file + runtime library + platform libs (`-lpthread -ldl -lm`)
- Stack size: `-Wl,-z,stacksize=8388608` (Linux) or `-Wl,-stack_size,0x800000` (macOS)

---

# Part 3: Implementation Plan

Each phase has an entry condition, deliverables, done-when criteria, and known pitfalls.

## 3.1 Phase 1 — Test Suite

One job: Write every .fuse test file and expected output before implementation.

Entry condition: Language guide is written.

Deliverables: All test files in tests/fuse/core/ and tests/fuse/full/ with EXPECTED OUTPUT or EXPECTED ERROR comments.

Done when: Every test file exists with expected output. Verified by manual reading.

## 3.2 Phase 2 — Stage 0 Lexer & Parser

One job: Turn source text into an AST.

Entry condition: Phase 1 complete.

Deliverables: lexer.py, parser.py, ast_nodes.py, fuse_token.py.

Done when: All Core test files parse without error.

## 3.3 Phase 3 — Stage 0 Ownership Checker

One job: Enforce ownership rules and reject invalid programs.

Entry condition: Phase 2 complete.

Deliverables: checker.py

What it enforces:
- ref: cannot assign through or move from
- mutref: can modify, cannot move
- owned: full rights
- move: marks consumed, subsequent use is compile error
- match: exhaustive (all variants covered)
- val: immutable after declaration

Done when: All error test files produce correct errors. All valid files pass.

## 3.4 Phase 4 — Stage 0 Evaluator

One job: Execute Fuse Core programs.

Entry condition: Phase 3 complete.

Deliverables: evaluator.py, values.py, environment.py

Milestone: `four_functions.fuse` runs with correct output.

Done when: All tests/fuse/core/ tests pass. Milestone program passes.

## 3.5 Phase 5 — Language Stabilization

One job: Fix every gap Phase 4 revealed. Freeze Fuse Core.

Entry condition: Phase 4 complete.

Done when: Guide accurately describes every behavior. No ambiguity remains. Core is frozen.

## 3.6 Phase 6 — Stage 1 Frontend

One job: Reproduce lexer, parser, checker in Rust.

Entry condition: Phase 5 complete.

Deliverables: Rust lexer/, parser/, ast/, hir/, checker/ in stage1/fusec/src/.

Done when: `fusec --check <file>` accepts valid Core files and rejects error files with matching messages.

## 3.7 Phase 7 — Stage 1 Backend

One job: Generate native binaries from Fuse Core via Cranelift.

Entry condition: Phase 6 complete.

Deliverables: codegen/cranelift.rs, codegen/layout.rs, fuse-runtime/ library.

Known pitfalls (discovered during Stage D attempt):

**Pitfall 1: mutref_cells timing.** The mutref ref-cell list must be populated BEFORE compiling the function body. If populated after, explicit `return` statements inside loops won't write back mutref parameters. The caller sees stale values, causing infinite loops.

```rust
// WRONG — sets mutref_cells after body compilation
let result = ctx.stmts(body);
ctx.mutref_cells = mutref_cells;  // too late for return inside body

// CORRECT — set before body compilation
ctx.mutref_cells = mutref_cells;  // available to return statements
let result = ctx.stmts(body);
```

**Pitfall 2: and/or SSA corruption.** Short-circuit boolean expressions (`and`/`or`) create Cranelift branch blocks. In large programs (100+ functions), these blocks can corrupt SSA variable tracking when used inside functions with loops and mutref parameters. Workaround: extract complex boolean conditions into helper functions. Root cause: likely a block-sealing issue in Cranelift variable resolution. Must be investigated and fixed.

**Pitfall 3: UTF-8 byte indexing.** `String.len()` returns byte length. `charAt(i)` uses byte indexing. Multi-byte characters cause panics at non-boundary bytes. Fix: make charAt return the character containing the byte, or return a replacement character for non-boundary bytes.

**Pitfall 4: Stack size.** Default 1MB stack is insufficient for compiler workloads. Set 8MB via linker flags.

Milestone: All tests/fuse/core/ compile to native binaries with output matching Stage 0.

Done when: Stage 0 and Stage 1 produce identical output for every Core test.

## 3.8 Phase 8 — Fuse Full

One job: Add concurrency and SIMD.

Entry condition: Phase 7 complete.

Deliverables: chan.rs, shared.rs, SIMD intrinsics, rank/spawn checker passes wired to runtime.

Done when: All tests/fuse/full/ pass. @rank violations, spawn capture violations produce correct errors.

## 3.9 Phase 9 — Self-Hosting

One job: Write the Fuse compiler in Fuse Core. Bootstrap.

Entry condition: Phase 8 complete. Stage 1 can compile any Fuse Core program.

Sub-stages:
- Stage A: Native codegen in Stage 1 (prerequisite — already done in Phase 7)
- Stage B: FFI and Cranelift wrappers (extern fn, cranelift-ffi crate)
- Stage C: Write Stage 2 compiler in Fuse (lexer, parser, codegen via FFI)
- Stage D: Bootstrap and verify (compile self, reproducibility check)

Stage 2 constraints:
- Fuse Core only (no concurrency)
- Use `while`/`break`/`continue` for all iteration (no recursion for loops)
- Use modules to split across files (no 2000-line monoliths)
- Avoid `and`/`or` in functions with loops+mutref until SSA bug is fixed

Bootstrap chain:
```bash
# Step 1: Stage 1 compiles Stage 2
fusec --compile stage2/src/main.fuse -o fusec2-bootstrap

# Step 2: fusec2-bootstrap compiles itself
./fusec2-bootstrap stage2/src/main.fuse -o fusec2-stage2

# Step 3: fusec2-stage2 compiles itself again
./fusec2-stage2 stage2/src/main.fuse -o fusec2-verified

# Step 4: Reproducibility check
sha256sum fusec2-stage2 fusec2-verified
# Both hashes must match
```

Done when: Reproducibility check passes. Fuse compiles itself.

---

# Part 4: Test Suite

## 4.1 Test Organization

```
tests/fuse/
├── milestone/
│   └── four_functions.fuse    # The canonical program
├── core/                      # Fuse Core tests
│   ├── ownership/             # ref, mutref, owned, move
│   ├── memory/                # ASAP destruction, defer
│   ├── errors/                # Result, Option, ?, match
│   └── types/                 # val/var, data class, enum, for, while, etc.
└── full/                      # Fuse Full tests
    ├── concurrency/           # channels, shared state, @rank
    └── simd/                  # SIMD operations
```

## 4.2 Test Contract

The fundamental rule: **Stage 0 output = Stage 1 output = Stage 2 output.**

If any stage produces different output for the same test, that stage has a bug.

## 4.3 How to Run Tests

```bash
# Stage 0
cd stage0 && python tests/run_tests.py

# Stage 1
cd stage1 && cargo test

# Stage 2
./fusec2 --check tests/fuse/core/types/val_immutable.fuse  # check mode
./fusec2 tests/fuse/core/types/for_loop.fuse -o test && ./test  # compile + run
```

## 4.4 How to Add Tests

1. Create a .fuse file in the appropriate tests/fuse/ subdirectory
2. Add EXPECTED OUTPUT or EXPECTED ERROR comment at the top
3. Run against all implemented stages to verify
4. Expected output must match byte-for-byte

Format:
```fuse
// EXPECTED OUTPUT:
// line 1
// line 2

@entrypoint
fn main() {
  println("line 1")
  println("line 2")
}
```

Error tests:
```fuse
// EXPECTED ERROR:
// error: cannot reassign to `name` — declared as `val`

@entrypoint
fn main() {
  val name = "Amara"
  name = "Bayo"       // error
}
```

## 4.5 Error Tests

Files with `EXPECTED ERROR` are run in check mode. The test runner verifies:
1. The program is rejected (non-zero exit)
2. The error message matches the expected text
3. The line/column in the error matches

---

# Part 5: Design Decisions (ADRs)

## 5.1 ADR-001: `ref` not `borrowed`

Decision: Read-only convention is `ref`, not `borrowed`.

Rationale: `ref` and `mutref` share a visible prefix. The relationship is immediate.

Rejected: `borrowed` (no relationship to `mutref`), `ro` (too abbreviated), `read` (too verbose).

## 5.2 ADR-002: `mutref` not `inout`

Decision: Mutable reference is `mutref`, not `inout`.

Rationale: `mutref` is self-documenting — mutable reference. `inout` is audio engineering jargon.

Rejected: `inout` (Mojo), `mut` (implies ownership), `rw` (too abbreviated).

## 5.3 ADR-003: `move` not `^`

Decision: Ownership transfer is `move value`, not `value^`.

Rationale: A keyword reads as intent. `shutdown(move conn)` can be read aloud. `conn^` needs a legend.

Rejected: `^` (Mojo), `give` (informal), `transfer` (verbose), `own` (ambiguous direction).

## 5.4 ADR-004: `@rank` mandatory

Decision: `Shared<T>` without `@rank(N)` is a compile error, not a warning.

Rationale: Optional safety annotations get skipped under pressure. Compile error = never unguarded.

Rejected: Lint warning (ignored), runtime detection (too late), no enforcement (defeats purpose).

## 5.5 ADR-005: Three-tier deadlock prevention

Decision: Channels → @rank → try_write, in a mandatory hierarchy.

Rationale: A hierarchy with a clear default (channels) means the lowest-friction path is safest.

Rejected: Single approach (too restrictive), advisory only (no enforcement), runtime cycle detection.

## 5.6 ADR-006: Stage 0 in Python

Decision: First interpreter is Python.

Rationale: Stage 0 answers "are semantics correct?" Python focuses on that without codegen overhead.

Rejected: Rust (conflates validation with codegen), TypeScript, Haskell.

## 5.7 ADR-007: Cranelift not LLVM

Decision: Stage 1 uses Cranelift.

Rationale: Simpler API, faster compilation, sufficient for Stage 1. LLVM added later if needed.

Rejected: LLVM (too complex for Stage 1), QBE (less mature), custom backend.

## 5.8 ADR-008: No timelines

Decision: No milestone has a date.

Rationale: A language is complete when correct, not when a calendar says so.

Rejected: Milestone dates, sprint planning, release schedule.

## 5.9 ADR-009: `data` contextual keyword

Decision: `data` is only special before `class`. Usable as identifier elsewhere.

Rationale: `processMetrics(mutref data: List<Metric>)` — `data` is a natural parameter name.

Rejected: Reserving `data` (breaks natural naming), using `record` instead.

## 5.10 ADR-010: ASAP destruction semantics

Decision: `__del__` fires at last use. `defer` fires after, in LIFO order.

Rationale: Deterministic, predictable from reading code. Lock guards release immediately.

Rejected: Scope-end destruction, reference counting, tracing GC.

## 5.11 ADR-011: `extern fn` for FFI

Decision: Foreign functions use `extern fn`, not annotations.

Rationale: `extern` is universally understood across C, Rust, Go, C#.

Rejected: `@foreign` (implies body exists), `native fn` (ambiguous), `#[link]` (Rust-specific).

---

> This document is the single source of truth for the Fuse programming language.
> It supersedes all previous documentation. When the guide and the code disagree,
> fix the code.
