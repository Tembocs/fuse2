# Fuse Standard Library — Implementation Specification

---

## ⚠ Compiler Bug Policy — Read This First

**Any bug discovered in the Stage 1 compiler while implementing these libraries must be fixed in the compiler. No exceptions.**

The standard library is not a workaround surface. It is a stress test. When a library implementation triggers a compiler bug — a wrong codegen output, a crash, an incorrect ownership error, a missed edge case in the checker — the correct response is:

1. **Stop.** Do not restructure the library code to avoid the bug.
2. **Reproduce it minimally.** Write the smallest possible `.fuse` program that triggers the bug and add it to `tests/fuse/core/` or `tests/fuse/full/` as a `_rejected.fuse` or snapshot test.
3. **Fix the compiler.** The bug lives in Stage 1 — in the lexer, parser, checker, lowering pass, or Cranelift backend. Find it and fix it there.
4. **Verify the fix.** Confirm the minimal reproduction now passes. Confirm no existing tests regressed.
5. **Resume the library.** Only then return to the library implementation, writing it the natural way.

This policy exists because the entire purpose of building the standard library before self-hosting is to discover compiler bugs under real-world usage conditions. A library that is written around a compiler bug does not expose the bug — it buries it. That buried bug will surface later, during self-hosting, in a much harder context to debug. The standard library must be written the way any Fuse developer would naturally write it. If that natural code breaks the compiler, the compiler is wrong.

**Cutting corners in the library to avoid a compiler bug is not a solution. It is a debt that will be collected during Stage 2, with interest.**

The only permitted deviation from natural Fuse code in a library implementation is when the language guide itself specifies a limitation (e.g. "variadic functions not yet supported"). In that case, document the limitation explicitly with a `// TODO(compiler):` comment at the call site, and file it as a known gap. A known, documented gap is acceptable. A hidden workaround is not.

---

> **For AI agents and implementors reading this document:**
> This is the authoritative specification for the Fuse standard library as implemented in Stage 1. Every module listed here is either already partially present in the runtime or must be written in Fuse and compiled by the Stage 1 `fusec` compiler. The organisation follows the Core/Full/Ext tier model from the language guide. Each module entry contains: purpose, complete public API with exact signatures, error types, usage examples, and implementation notes for the Stage 1 Rust backend.
>
> **The rule of tiers:**
> - `stdlib/core/` — Pure computation, no OS interaction, no FFI to external systems. Must work in the Stage 0 Python interpreter. Every function here is deterministic and side-effect-free except where noted.
> - `stdlib/full/` — Requires FFI, OS syscalls, async, or concurrency. Stage 1 and Stage 2 only.
> - `stdlib/ext/` — Optional, heavyweight, or opinionated. Not bundled. Installed per project.
>
> **Implementation contract:** Every function signature in this document is final. Changing a signature requires updating this document first, then the guide, then the implementation. No implementation detail may contradict a signature stated here.

---

## Table of Contents

### Core (`stdlib/core/`)
1. [result.fuse](#1-resultfuse)
2. [option.fuse](#2-optionfuse)
3. [list.fuse](#3-listfuse)
4. [map.fuse](#4-mapfuse)
5. [set.fuse](#5-setfuse)
6. [string.fuse](#6-stringfuse)
7. [int.fuse](#7-intfuse)
8. [float.fuse](#8-floatfuse)
9. [bool.fuse](#9-boolfuse)
10. [math.fuse](#10-mathfuse)
11. [fmt.fuse](#11-fmtfuse)

### Full (`stdlib/full/`)
12. [io.fuse](#12-iofuse)
13. [path.fuse](#13-pathfuse)
14. [os.fuse](#14-osfuse)
15. [env.fuse](#15-envfuse)
16. [sys.fuse](#16-sysfuse)
17. [time.fuse](#17-timefuse)
18. [random.fuse](#18-randomfuse)
19. [process.fuse](#19-processfuse)
20. [net.fuse](#20-netfuse)
21. [json.fuse](#21-jsonfuse)
22. [chan.fuse](#22-chanfuse)
23. [shared.fuse](#23-sharedfuse)
24. [timer.fuse](#24-timerfuse)
25. [simd.fuse](#25-simdfuse)
26. [http.fuse](#26-httpfuse)

### Ext (`stdlib/ext/`)
27. [test.fuse](#27-testfuse)
28. [log.fuse](#28-logfuse)
29. [regex.fuse](#29-regexfuse)
30. [json_schema.fuse](#30-json_schemafuse)
31. [yaml.fuse](#31-yamlfuse)
32. [toml.fuse](#32-tomlfuse)
33. [crypto.fuse](#33-cryptofuse)
34. [http_server.fuse](#34-http_serverfuse)

---

## Conventions Used in This Document

**Signatures** follow Fuse syntax exactly. `ref self` means the method does not take ownership. `mutref self` means it modifies the receiver. `owned self` means it consumes the receiver.

**Error strings** in `Result<T, String>` are human-readable messages suitable for display. They follow the pattern `"module: description"` (e.g. `"io: file not found: /etc/foo"`).

**`IOError`** is the canonical error type for all I/O operations. It is a `data class` defined in `io.fuse` and re-exported where needed.

**Index bounds:** All index operations (`get(i)`, `charAt(i)`, etc.) that access a position that does not exist return a panic by default unless the method is documented as returning `Option`. There is no silent out-of-bounds.

---

## Part 1 — `stdlib/core/`

---

### 1. `result.fuse`

**Purpose:** The `Result<T, E>` type is built into the language and does not need to be imported. This module provides convenience methods on `Result` values via extension functions. Import when you need the helper methods.

```fuse
import stdlib.core.result
```

#### API

```fuse
// Unwrap the Ok value or panic with message
fn Result<T, E>.unwrap(owned self) -> T

// Unwrap the Ok value or return a default
fn Result<T, E>.unwrapOr(owned self, default: T) -> T

// Unwrap the Ok value or compute a default from the error
fn Result<T, E>.unwrapOrElse(owned self, f: fn(E) -> T) -> T

// True if Ok
fn Result<T, E>.isOk(ref self) -> Bool

// True if Err
fn Result<T, E>.isErr(ref self) -> Bool

// Map Ok value through a function; Err passes through unchanged
fn Result<T, E>.map(owned self, f: fn(T) -> U) -> Result<U, E>

// Map Err value through a function; Ok passes through unchanged
fn Result<T, E>.mapErr(owned self, f: fn(E) -> F) -> Result<T, F>

// Flatten Result<Result<T, E>, E> into Result<T, E>
fn Result<Result<T, E>, E>.flatten(owned self) -> Result<T, E>

// Convert Ok value to Option<T>, discarding the error
fn Result<T, E>.ok(owned self) -> Option<T>

// Convert Err value to Option<E>, discarding the Ok value
fn Result<T, E>.err(owned self) -> Option<E>
```

#### Example

```fuse
import stdlib.core.result

fn parsePort(s: String) -> Result<Int, String> {
  val n = s.parseInt()?
  if n < 1 or n > 65535 { return Err(f"port out of range: {n}") }
  Ok(n)
}

@entrypoint
fn main() {
  val port = parsePort("8080").unwrapOr(3000)
  println(f"port: {port}")

  val r = parsePort("bad")
  println(f"isErr: {r.isErr()}")
}
```

**Implementation note:** `Result<T, E>` is built into the runtime as `ValueKind::Result`. All extension functions are pure Fuse — no additional Rust is required. The `map` and `mapErr` methods require the compiler to support function values as arguments (first-class `fn` types). If first-class functions are not yet supported, stub those two methods and document the limitation.

---

### 2. `option.fuse`

**Purpose:** Extension methods on `Option<T>`. The type itself is built-in and needs no import.

```fuse
import stdlib.core.option
```

#### API

```fuse
// Unwrap Some or panic
fn Option<T>.unwrap(owned self) -> T

// Unwrap Some or return a default
fn Option<T>.unwrapOr(owned self, default: T) -> T

// Unwrap Some or compute a default
fn Option<T>.unwrapOrElse(owned self, f: fn() -> T) -> T

// True if Some
fn Option<T>.isSome(ref self) -> Bool

// True if None
fn Option<T>.isNone(ref self) -> Bool

// Map Some value through a function; None passes through
fn Option<T>.map(owned self, f: fn(T) -> U) -> Option<U>

// Return self if Some; return other otherwise (lazy: other is a function)
fn Option<T>.orElse(owned self, f: fn() -> Option<T>) -> Option<T>

// Flatten Option<Option<T>> into Option<T>
fn Option<Option<T>>.flatten(owned self) -> Option<T>

// Convert to Result: Some(v) -> Ok(v), None -> Err(err)
fn Option<T>.okOr(owned self, err: E) -> Result<T, E>

// Filter: return None if predicate fails, Some(v) otherwise
fn Option<T>.filter(owned self, f: fn(ref T) -> Bool) -> Option<T>
```

#### Example

```fuse
import stdlib.core.option

fn findUser(id: Int) -> Option<String> {
  if id == 1 { Some("alice") } else { None }
}

@entrypoint
fn main() {
  val name = findUser(1).unwrapOr("unknown")
  println(name)   // alice

  val missing = findUser(99).map(fn(s) => s.toUpper())
  println(missing.isSome())  // false
}
```

**Implementation note:** Pure Fuse extension functions. Same caveat on first-class `fn` as `result.fuse`.

---

### 3. `list.fuse`

**Purpose:** Extension methods on `List<T>`. The type is built-in. This module provides the full functional and mutating API.

```fuse
import stdlib.core.list
```

#### API — Query

```fuse
fn List<T>.len(ref self) -> Int
fn List<T>.isEmpty(ref self) -> Bool
fn List<T>.get(ref self, index: Int) -> Option<T>        // None if out of bounds
fn List<T>.first(ref self) -> Option<T>
fn List<T>.last(ref self) -> Option<T>
fn List<T>.contains(ref self, item: T) -> Bool           // requires T == T
fn List<T>.indexOf(ref self, item: T) -> Option<Int>     // first matching index or None
fn List<T>.count(ref self, f: fn(ref T) -> Bool) -> Int  // count matching items
fn List<T>.any(ref self, f: fn(ref T) -> Bool) -> Bool
fn List<T>.all(ref self, f: fn(ref T) -> Bool) -> Bool
```

#### API — Transformation (non-mutating, returns new list)

```fuse
fn List<T>.map(ref self, f: fn(T) -> U) -> List<U>
fn List<T>.filter(ref self, f: fn(ref T) -> Bool) -> List<T>
fn List<T>.flatMap(ref self, f: fn(T) -> List<U>) -> List<U>
fn List<T>.reduce(ref self, init: U, f: fn(U, T) -> U) -> U
fn List<T>.sorted(ref self) -> List<T>                   // requires T comparable
fn List<T>.sortedBy(ref self, f: fn(ref T, ref T) -> Int) -> List<T>  // f returns -1, 0, 1
fn List<T>.reversed(ref self) -> List<T>
fn List<T>.unique(ref self) -> List<T>                   // remove duplicates, preserve order
fn List<T>.slice(ref self, start: Int, end: Int) -> List<T>  // [start, end) half-open
fn List<T>.take(ref self, n: Int) -> List<T>             // first n items
fn List<T>.drop(ref self, n: Int) -> List<T>             // skip first n items
fn List<T>.zip(ref self, other: ref List<U>) -> List<(T, U)>
fn List<T>.flatten(ref self) -> List<U>                  // T must be List<U>
fn List<T>.concat(ref self, other: ref List<T>) -> List<T>
fn List<T>.join(ref self, sep: String) -> String         // T must be String
```

#### API — Mutation (modifies in place)

```fuse
fn List<T>.push(mutref self, item: T)
fn List<T>.pop(mutref self) -> Option<T>
fn List<T>.insert(mutref self, index: Int, item: T)
fn List<T>.removeAt(mutref self, index: Int) -> Option<T>
fn List<T>.removeWhere(mutref self, f: fn(ref T) -> Bool)
fn List<T>.clear(mutref self)
fn List<T>.sortInPlace(mutref self)
fn List<T>.reverseInPlace(mutref self)
```

#### API — Construction

```fuse
fn List<T>.new() -> List<T>
fn List.of(items: T...) -> List<T>     // variadic construction
fn List.repeat(item: T, n: Int) -> List<T>
fn List.range(start: Int, end: Int) -> List<Int>      // [start, end) exclusive
fn List.rangeClosed(start: Int, end: Int) -> List<Int> // [start, end] inclusive
```

#### Example

```fuse
import stdlib.core.list

@entrypoint
fn main() {
  val nums = List.range(0, 10)           // [0, 1, 2, ..., 9]
  val evens = nums.filter(fn(n) => n % 2 == 0)
  val doubled = evens.map(fn(n) => n * 2)
  println(doubled.join(", "))            // 0, 4, 8, 12, 16

  var items: List<String> = []
  items.push("alpha")
  items.push("beta")
  items.push("gamma")
  println(items.sorted().join(", "))     // alpha, beta, gamma

  val top = items.sortedBy(fn(a, b) => a.len() - b.len()).take(2)
  println(top.join(" "))                 // beta alpha
}
```

**Implementation note:** `List<T>` is backed by `ValueKind::List(Vec<FuseHandle>)` in the runtime. All mutating methods modify the underlying `Vec` in place via the existing `fuse_list_*` family in `value.rs`. Non-mutating transformation methods must allocate a new `List` — they are best implemented in pure Fuse over the existing primitives, calling `push` in a loop. The `sorted` and `sortedBy` methods require the runtime to expose a comparison FFI or be written in Fuse using a simple sort algorithm (insertion sort is acceptable for a first pass). The tuple type `(T, U)` used by `zip` must be representable as a two-field `data class` if tuples are not natively supported yet — document that limitation explicitly.

---

### 4. `map.fuse`

**Purpose:** Extension methods on `Map<K, V>`. The type and its core methods (`set`, `get`, `remove`, `len`, `isEmpty`, `contains`, `keys`, `values`, `entries`) are built-in. This module adds higher-level helpers.

```fuse
import stdlib.core.map
```

#### API

```fuse
fn Map<K, V>.new() -> Map<K, V>     // already built-in; re-exported for explicit use

// Functional helpers
fn Map<K, V>.getOrDefault(ref self, key: K, default: V) -> V
fn Map<K, V>.getOrInsert(mutref self, key: K, default: V) -> V  // inserts if missing
fn Map<K, V>.mapValues(ref self, f: fn(V) -> U) -> Map<K, U>
fn Map<K, V>.filter(ref self, f: fn(ref K, ref V) -> Bool) -> Map<K, V>
fn Map<K, V>.merge(ref self, other: ref Map<K, V>) -> Map<K, V>  // other wins on conflict
fn Map<K, V>.forEach(ref self, f: fn(ref K, ref V))

// Conversion
fn Map<K, V>.toList(ref self) -> List<(K, V)>    // same as .entries() but named consistently
fn Map<K, V>.invert(ref self) -> Map<V, K>       // requires V to be a valid key type
```

#### Example

```fuse
import stdlib.core.map

@entrypoint
fn main() {
  var scores = Map::<String, Int>.new()
  scores.set("alice", 90)
  scores.set("bob", 75)

  val s = scores.getOrDefault("eve", 0)
  println(f"eve: {s}")       // eve: 0

  val doubled = scores.mapValues(fn(v) => v * 2)
  for entry in doubled.entries() {
    println(f"{entry.0}: {entry.1}")
  }
}
```

**Implementation note:** All methods implemented in pure Fuse over the built-in `Map` primitives. No additional Rust is required.

---

### 5. `set.fuse`

**Purpose:** `Set<T>` — an unordered collection of unique values. This type is not built into the language; it is implemented in Fuse over `Map<T, Bool>`.

```fuse
import stdlib.core.set.{Set}
```

#### Type Definition

```fuse
// Implemented internally as Map<T, Bool>
data class Set<T>(val inner: Map<T, Bool>)
```

#### API

```fuse
fn Set<T>.new() -> Set<T>
fn Set<T>.of(items: T...) -> Set<T>         // variadic construction
fn Set<T>.fromList(items: List<T>) -> Set<T>

// Query
fn Set<T>.contains(ref self, item: T) -> Bool
fn Set<T>.len(ref self) -> Int
fn Set<T>.isEmpty(ref self) -> Bool
fn Set<T>.toList(ref self) -> List<T>       // order unspecified

// Mutation
fn Set<T>.add(mutref self, item: T)
fn Set<T>.remove(mutref self, item: T) -> Bool  // true if item was present
fn Set<T>.clear(mutref self)

// Set operations (return new Set, do not modify self)
fn Set<T>.union(ref self, other: ref Set<T>) -> Set<T>
fn Set<T>.intersect(ref self, other: ref Set<T>) -> Set<T>
fn Set<T>.difference(ref self, other: ref Set<T>) -> Set<T>       // self - other
fn Set<T>.symmetricDiff(ref self, other: ref Set<T>) -> Set<T>    // (A ∪ B) - (A ∩ B)
fn Set<T>.isSubsetOf(ref self, other: ref Set<T>) -> Bool
fn Set<T>.isSupersetOf(ref self, other: ref Set<T>) -> Bool
fn Set<T>.isDisjoint(ref self, other: ref Set<T>) -> Bool         // no common elements

// Iteration
fn Set<T>.forEach(ref self, f: fn(ref T))
fn Set<T>.filter(ref self, f: fn(ref T) -> Bool) -> Set<T>
fn Set<T>.map(ref self, f: fn(T) -> U) -> Set<U>
```

#### Example

```fuse
import stdlib.core.set.{Set}

@entrypoint
fn main() {
  var a = Set.of("alpha", "beta", "gamma")
  var b = Set.of("beta", "gamma", "delta")

  val common = a.intersect(ref b)
  println(f"common: {common.len()}")      // 2

  a.add("delta")
  println(f"contains delta: {a.contains("delta")}")  // true

  val all = a.union(ref b)
  println(f"union size: {all.len()}")    // 4
}
```

**Implementation note:** Entirely pure Fuse built on `Map<T, Bool>`. No Rust changes required. The `T...` variadic syntax in `of` depends on whether variadic functions are supported in Stage 1; if not, provide `fromList` and document that `of` is a future addition.

---

### 6. `string.fuse`

**Purpose:** Extension methods on `String`. The built-in `String` type already supports `len()`, `charAt(i)`, `substring(start, end)`, `toUpper()`, `isEmpty()`, `parseInt()`, `parseFloat()`, and f-string interpolation. This module adds the rest.

```fuse
import stdlib.core.string
```

#### API

```fuse
// Case
fn String.toLower(ref self) -> String
fn String.toUpper(ref self) -> String     // already built-in; re-exported
fn String.capitalize(ref self) -> String  // first char upper, rest lower

// Search and test
fn String.contains(ref self, sub: String) -> Bool
fn String.startsWith(ref self, prefix: String) -> Bool
fn String.endsWith(ref self, suffix: String) -> Bool
fn String.indexOf(ref self, sub: String) -> Option<Int>      // byte index of first match
fn String.lastIndexOf(ref self, sub: String) -> Option<Int>

// Transformation
fn String.trim(ref self) -> String           // removes leading and trailing whitespace
fn String.trimStart(ref self) -> String
fn String.trimEnd(ref self) -> String
fn String.replace(ref self, from: String, to: String) -> String  // replaces all occurrences
fn String.replaceFirst(ref self, from: String, to: String) -> String
fn String.split(ref self, sep: String) -> List<String>
fn String.splitLines(ref self) -> List<String>   // splits on \n and \r\n
fn String.repeat(ref self, n: Int) -> String
fn String.padStart(ref self, width: Int, ch: String) -> String  // ch must be single char
fn String.padEnd(ref self, width: Int, ch: String) -> String
fn String.reverse(ref self) -> String

// Conversion
fn String.toInt(ref self) -> Result<Int, String>     // alias for parseInt; cleaner name
fn String.toFloat(ref self) -> Result<Float, String> // alias for parseFloat
fn String.toBool(ref self) -> Result<Bool, String>   // "true"/"false" only, case-insensitive
fn String.toBytes(ref self) -> List<Int>             // UTF-8 byte values
fn String.chars(ref self) -> List<String>            // Unicode codepoints as single-char strings
fn String.len(ref self) -> Int                       // byte length (already built-in)
fn String.charCount(ref self) -> Int                 // Unicode codepoint count (may differ from len)

// Comparison
fn String.compareTo(ref self, other: String) -> Int  // lexicographic: -1, 0, 1

// Construction
fn String.fromBytes(bytes: List<Int>) -> Result<String, String>  // from UTF-8 bytes
fn String.fromChar(code: Int) -> String                          // from Unicode codepoint
```

#### Example

```fuse
import stdlib.core.string

@entrypoint
fn main() {
  val s = "  Hello, World!  "
  println(s.trim())                     // "Hello, World!"
  println(s.trim().toLower())           // "hello, world!"
  println("fuse".repeat(3))            // "fusefusefuse"
  println("42".toInt().unwrapOr(0))    // 42
  println("abc".contains("bc"))        // true

  val parts = "a,b,c".split(",")
  println(parts.len())                 // 3

  println("hi".padEnd(6, "."))        // "hi...."
}
```

**Implementation note:** `toUpper` and `isEmpty` are already in the runtime (`fuse_string_to_upper`, `fuse_string_is_empty`). `toLower` needs a new FFI entry: `fuse_rt_string_to_lower`. `split` must be implemented in pure Fuse using `indexOf` and `substring`. `chars` must correctly handle multi-byte UTF-8 by iterating codepoints, not bytes — requires runtime support (`fuse_rt_string_chars` returning `ValueKind::List` of single-char strings). All other methods can be pure Fuse using `len`, `charAt`, `substring`, and `contains`.

---

### 7. `int.fuse`

**Purpose:** Extension methods on `Int`.

```fuse
import stdlib.core.int
```

#### API

```fuse
// Arithmetic helpers
fn Int.abs(ref self) -> Int
fn Int.min(ref self, other: Int) -> Int
fn Int.max(ref self, other: Int) -> Int
fn Int.clamp(ref self, low: Int, high: Int) -> Int   // max(low, min(self, high))
fn Int.pow(ref self, exp: Int) -> Int                // exp must be >= 0
fn Int.gcd(ref self, other: Int) -> Int
fn Int.lcm(ref self, other: Int) -> Int

// Predicates
fn Int.isEven(ref self) -> Bool
fn Int.isOdd(ref self) -> Bool
fn Int.isPositive(ref self) -> Bool    // > 0
fn Int.isNegative(ref self) -> Bool    // < 0
fn Int.isZero(ref self) -> Bool

// Conversion
fn Int.toFloat(ref self) -> Float
fn Int.toString(ref self) -> String
fn Int.toBits(ref self) -> String         // binary string, e.g. "101010"
fn Int.toHex(ref self) -> String          // lowercase hex, e.g. "2a"
fn Int.toOctal(ref self) -> String

// Parsing
fn Int.parse(s: String) -> Result<Int, String>          // decimal
fn Int.parseHex(s: String) -> Result<Int, String>       // hex string without "0x" prefix
fn Int.parseBinary(s: String) -> Result<Int, String>    // binary string without "0b" prefix

// Constants
val Int.MIN: Int    // -9223372036854775808
val Int.MAX: Int    //  9223372036854775807
```

#### Example

```fuse
import stdlib.core.int

@entrypoint
fn main() {
  println((-5).abs())           // 5
  println(12.gcd(8))            // 4
  println(255.toHex())          // "ff"
  println(Int.parse("42").unwrapOr(0))  // 42
  println(7.clamp(0, 5))        // 5
}
```

**Implementation note:** `abs`, `min`, `max`, `pow` are pure arithmetic expressible in Fuse. `gcd` can be implemented as Euclid's algorithm in Fuse. `toFloat` requires a runtime cast FFI (`fuse_rt_int_to_float`). `toString` is already implicitly available via f-strings; make it explicit. `toHex`, `toBits`, `toOctal` are pure Fuse string-building over `%` and `/`.

---

### 8. `float.fuse`

**Purpose:** Extension methods on `Float`.

```fuse
import stdlib.core.float
```

#### API

```fuse
// Arithmetic helpers
fn Float.abs(ref self) -> Float
fn Float.min(ref self, other: Float) -> Float
fn Float.max(ref self, other: Float) -> Float
fn Float.clamp(ref self, low: Float, high: Float) -> Float
fn Float.pow(ref self, exp: Float) -> Float
fn Float.sqrt(ref self) -> Float
fn Float.floor(ref self) -> Float
fn Float.ceil(ref self) -> Float
fn Float.round(ref self) -> Float            // round half-away from zero
fn Float.trunc(ref self) -> Float            // truncate toward zero
fn Float.fract(ref self) -> Float            // fractional part: self - trunc(self)

// Predicates
fn Float.isNaN(ref self) -> Bool
fn Float.isInfinite(ref self) -> Bool
fn Float.isFinite(ref self) -> Bool
fn Float.isPositive(ref self) -> Bool        // > 0.0, not NaN
fn Float.isNegative(ref self) -> Bool        // < 0.0, not NaN

// Comparison (handle NaN correctly)
fn Float.approxEq(ref self, other: Float, epsilon: Float) -> Bool  // |self - other| < epsilon

// Conversion
fn Float.toInt(ref self) -> Int              // truncates toward zero
fn Float.toString(ref self) -> String
fn Float.toStringFixed(ref self, decimals: Int) -> String   // e.g. 3.14159.toStringFixed(2) = "3.14"

// Parsing
fn Float.parse(s: String) -> Result<Float, String>

// Constants
val Float.NAN: Float
val Float.INFINITY: Float
val Float.NEG_INFINITY: Float
val Float.PI: Float         // 3.141592653589793
val Float.E: Float          // 2.718281828459045
val Float.EPSILON: Float    // 2.220446049250313e-16 (machine epsilon)
```

#### Example

```fuse
import stdlib.core.float

@entrypoint
fn main() {
  println(3.7.floor())                 // 3.0
  println((-2.3).abs())               // 2.3
  println(2.0.pow(10.0))              // 1024.0
  println(3.14159.toStringFixed(2))   // "3.14"
  println(Float.PI)                   // 3.141592653589793
  println(0.1 + 0.2 == 0.3)          // false
  println((0.1 + 0.2).approxEq(0.3, Float.EPSILON * 4.0))  // true
}
```

**Implementation note:** All math operations must be backed by FFI to Rust's `f64` methods (`floor`, `ceil`, `sqrt`, `powi`, `powf`, `abs`, `round`, `trunc`, `fract`, `is_nan`, `is_infinite`, `is_finite`). Add `fuse_rt_float_*` FFI functions for each. The constants are declared as `val` in the module and initialized from Rust's `f64::NAN`, `f64::INFINITY`, `std::f64::consts::PI`, etc.

---

### 9. `bool.fuse`

**Purpose:** Extension methods on `Bool`. Minimal — booleans are simple.

```fuse
import stdlib.core.bool
```

#### API

```fuse
fn Bool.not(ref self) -> Bool           // same as !self but chainable
fn Bool.toString(ref self) -> String    // "true" or "false"
fn Bool.toInt(ref self) -> Int          // 1 if true, 0 if false
```

**Implementation note:** Entirely pure Fuse. No Rust changes needed.

---

### 10. `math.fuse`

**Purpose:** Free mathematical functions not attached to a type. All pure. No side effects.

```fuse
import stdlib.core.math
```

#### API

```fuse
// Trigonometry (arguments in radians)
fn math.sin(x: Float) -> Float
fn math.cos(x: Float) -> Float
fn math.tan(x: Float) -> Float
fn math.asin(x: Float) -> Float
fn math.acos(x: Float) -> Float
fn math.atan(x: Float) -> Float
fn math.atan2(y: Float, x: Float) -> Float

// Exponential and logarithm
fn math.exp(x: Float) -> Float        // e^x
fn math.exp2(x: Float) -> Float       // 2^x
fn math.ln(x: Float) -> Float         // natural log; returns NaN for x <= 0
fn math.log2(x: Float) -> Float
fn math.log10(x: Float) -> Float
fn math.log(x: Float, base: Float) -> Float

// Roots and powers
fn math.sqrt(x: Float) -> Float
fn math.cbrt(x: Float) -> Float       // cube root
fn math.hypot(x: Float, y: Float) -> Float   // sqrt(x^2 + y^2) without overflow

// Rounding (same as Float extension methods, provided as free functions too)
fn math.floor(x: Float) -> Float
fn math.ceil(x: Float) -> Float
fn math.round(x: Float) -> Float
fn math.trunc(x: Float) -> Float
fn math.abs(x: Float) -> Float
fn math.absInt(x: Int) -> Int

// Min / max (overloaded for Int and Float)
fn math.minFloat(a: Float, b: Float) -> Float
fn math.maxFloat(a: Float, b: Float) -> Float
fn math.minInt(a: Int, b: Int) -> Int
fn math.maxInt(a: Int, b: Int) -> Int
fn math.clampFloat(x: Float, low: Float, high: Float) -> Float
fn math.clampInt(x: Int, low: Int, high: Int) -> Int

// Number theory
fn math.gcd(a: Int, b: Int) -> Int
fn math.lcm(a: Int, b: Int) -> Int
fn math.isPrime(n: Int) -> Bool
fn math.factorial(n: Int) -> Int      // panics if n < 0 or n > 20

// Conversion
fn math.degreesToRadians(deg: Float) -> Float
fn math.radiansToDegrees(rad: Float) -> Float

// Constants (re-exported from Float for convenience)
val math.PI: Float
val math.E: Float
val math.TAU: Float      // 2 * PI
val math.SQRT2: Float
val math.LN2: Float
val math.LN10: Float
```

#### Example

```fuse
import stdlib.core.math

@entrypoint
fn main() {
  val angle = math.degreesToRadians(45.0)
  println(math.sin(angle))              // ~0.7071
  println(math.hypot(3.0, 4.0))        // 5.0
  println(math.isPrime(97))            // true
  println(math.log(1024.0, 2.0))       // 10.0
}
```

**Implementation note:** All functions are backed by Rust's `f64` methods via FFI. Add a `fuse_rt_math_*` family. `isPrime` and `factorial` are pure Fuse using loops. `gcd` uses Euclid's algorithm in Fuse.

---

### 11. `fmt.fuse`

**Purpose:** String formatting utilities beyond f-string interpolation. Produces formatted strings for display. All functions are pure.

```fuse
import stdlib.core.fmt
```

#### API

```fuse
// Number formatting
fn fmt.decimal(n: Float, places: Int) -> String  // fixed decimal places
fn fmt.scientific(n: Float, places: Int) -> String  // e.g. "1.23e+10"
fn fmt.percent(n: Float, places: Int) -> String  // e.g. "98.60%"
fn fmt.thousands(n: Int) -> String               // e.g. 1234567 -> "1,234,567"
fn fmt.thousandsFloat(n: Float, places: Int) -> String  // e.g. "1,234.56"
fn fmt.hex(n: Int) -> String                     // lowercase hex without prefix
fn fmt.hexUpper(n: Int) -> String
fn fmt.binary(n: Int) -> String                  // binary without prefix
fn fmt.octal(n: Int) -> String

// String alignment and padding
fn fmt.padLeft(s: String, width: Int) -> String           // space-padded
fn fmt.padRight(s: String, width: Int) -> String
fn fmt.padCenter(s: String, width: Int) -> String
fn fmt.padLeftWith(s: String, width: Int, ch: String) -> String  // ch is one character
fn fmt.padRightWith(s: String, width: Int, ch: String) -> String
fn fmt.truncate(s: String, maxLen: Int) -> String         // truncates at maxLen, no ellipsis
fn fmt.truncateEllipsis(s: String, maxLen: Int) -> String // truncates and appends "..."

// Table formatting (simple fixed-width columns)
fn fmt.columns(rows: List<List<String>>, widths: List<Int>) -> String

// Repetition
fn fmt.repeatChar(ch: String, n: Int) -> String      // ch must be one character
fn fmt.ruler(width: Int) -> String                   // string of '-' repeated width times
```

#### Example

```fuse
import stdlib.core.fmt

@entrypoint
fn main() {
  println(fmt.decimal(3.14159, 2))     // "3.14"
  println(fmt.thousands(1000000))      // "1,000,000"
  println(fmt.percent(0.986, 1))       // "98.6%"
  println(fmt.padLeft("hi", 10))       // "        hi"
  println(fmt.truncateEllipsis("hello world", 8))  // "hello..."
  println(fmt.hex(255))                // "ff"
}
```

**Implementation note:** Entirely pure Fuse, built on `String` methods, arithmetic, and loops. No Rust changes needed. `fmt.columns` uses `padRight` and joins columns with a space separator.

---

## Part 2 — `stdlib/full/`

---

### 12. `io.fuse`

**Purpose:** File I/O — reading and writing files, working with stdin/stdout/stderr beyond `println`. All I/O operations are synchronous in Core; async variants are not provided here (use `http.fuse` and `net.fuse` for async I/O).

```fuse
import stdlib.full.io.{IOError, File, readFile, writeFile}
```

#### Error Type

```fuse
pub data class IOError(val message: String, val code: Int)
// code: 0 = generic, 1 = not found, 2 = permission denied,
//       3 = already exists, 4 = is a directory, 5 = not a directory,
//       6 = disk full, 7 = interrupted
```

#### API — Free functions

```fuse
// Whole-file operations
pub fn readFile(path: String) -> Result<String, IOError>
pub fn readFileBytes(path: String) -> Result<List<Int>, IOError>
pub fn writeFile(path: String, content: String) -> Result<(), IOError>
pub fn writeFileBytes(path: String, bytes: List<Int>) -> Result<(), IOError>
pub fn appendFile(path: String, content: String) -> Result<(), IOError>

// stdin
pub fn readLine() -> Result<String, IOError>    // reads one line from stdin, strips trailing \n
pub fn readAll() -> Result<String, IOError>     // reads stdin until EOF
```

#### API — `File` (buffered, incremental access)

```fuse
pub enum OpenMode { Read, Write, Append, ReadWrite }

pub struct File {
  // opaque — do not access fields directly
}

pub fn File.open(path: String, mode: OpenMode) -> Result<File, IOError>
pub fn File.create(path: String) -> Result<File, IOError>   // write, truncate, create if missing

pub fn File.readLine(mutref self) -> Result<Option<String>, IOError>  // None at EOF
pub fn File.readChunk(mutref self, maxBytes: Int) -> Result<List<Int>, IOError>  // empty at EOF
pub fn File.readAll(mutref self) -> Result<String, IOError>

pub fn File.write(mutref self, content: String) -> Result<(), IOError>
pub fn File.writeBytes(mutref self, bytes: List<Int>) -> Result<(), IOError>
pub fn File.flush(mutref self) -> Result<(), IOError>

pub fn File.seek(mutref self, offset: Int) -> Result<(), IOError>   // from file start
pub fn File.pos(ref self) -> Result<Int, IOError>                   // current position
pub fn File.size(ref self) -> Result<Int, IOError>

pub fn File.close(owned self) -> Result<(), IOError>
pub fn File.__del__(owned self)   // closes automatically on ASAP destruction
```

#### Example

```fuse
import stdlib.full.io.{readFile, writeFile, File, OpenMode, IOError}

@entrypoint
fn main() {
  // Whole-file read
  match readFile("config.txt") {
    Ok(contents) => println(f"read {contents.len()} bytes")
    Err(e)       => println(f"error: {e.message}")
  }

  // Write file
  writeFile("out.txt", "hello\n")?

  // Buffered incremental read
  val file = File.open("data.csv", OpenMode.Read)?
  defer file.close()

  loop {
    match file.readLine()? {
      Some(line) => println(line)
      None       => break
    }
  }
}
```

**Implementation note:** Backed by FFI to Rust's `std::fs` and `std::io`. Add `fuse_rt_io_*` family. `File` is a `struct` wrapping an opaque `Ptr` (a heap-allocated Rust `BufReader` or `BufWriter`). The `__del__` method must call `fuse_rt_file_close` to prevent file handle leaks. `readLine` strips `\r\n` on Windows and `\n` on all platforms.

---

### 13. `path.fuse`

**Purpose:** Path construction, inspection, and normalisation. **Pure string manipulation** — no filesystem access. Platform-aware: uses `\` on Windows, `/` everywhere else. `path.fuse` never touches the filesystem; use `os.fuse` for that.

```fuse
import stdlib.full.path
```

#### API

```fuse
// Construction
pub fn path.join(base: String, parts: String...) -> String  // join with OS separator
pub fn path.fromParts(parts: List<String>) -> String

// Inspection
pub fn path.basename(p: String) -> String        // "foo/bar/baz.txt" -> "baz.txt"
pub fn path.stem(p: String) -> String            // "foo/bar/baz.txt" -> "baz"
pub fn path.extension(p: String) -> Option<String>  // "baz.txt" -> Some("txt"); "baz" -> None
pub fn path.parent(p: String) -> Option<String>     // "foo/bar/baz" -> Some("foo/bar"); "/" -> None
pub fn path.components(p: String) -> List<String>   // splits on separator, drops empty
pub fn path.isAbsolute(p: String) -> Bool
pub fn path.isRelative(p: String) -> Bool

// Transformation
pub fn path.normalize(p: String) -> String       // resolve "." and ".." components
pub fn path.withExtension(p: String, ext: String) -> String   // replace or add extension
pub fn path.withBasename(p: String, name: String) -> String   // replace last component
pub fn path.toAbsolute(p: String) -> Result<String, String>   // resolves relative to cwd

// Constants
pub val path.SEPARATOR: String    // "/" on Unix, "\" on Windows
```

#### Example

```fuse
import stdlib.full.path

@entrypoint
fn main() {
  val p = path.join("/home/user", "projects", "fuse", "main.fuse")
  println(p)                         // /home/user/projects/fuse/main.fuse
  println(path.basename(p))         // main.fuse
  println(path.stem(p))             // main
  println(path.extension(p))        // Some("fuse")
  println(path.parent(p))           // Some("/home/user/projects/fuse")
  println(path.normalize("a/b/../c/./d"))  // a/c/d
}
```

**Implementation note:** Entirely pure Fuse string manipulation except `toAbsolute` (which needs the cwd from `sys.fuse`) and the `SEPARATOR` constant (needs platform detection via a single FFI call `fuse_rt_path_separator` returning `"/"` or `"\\"`, called once at module init). `normalize` implements a simple stack-based `..` resolution algorithm in Fuse.

---

### 14. `os.fuse`

**Purpose:** Filesystem operations — querying, creating, moving, and deleting files and directories. All operations are synchronous and return `Result`.

```fuse
import stdlib.full.os
import stdlib.full.io.{IOError}
```

#### Type Definitions

```fuse
pub enum EntryKind { File, Directory, Symlink, Other }

pub data class DirEntry(
  val name: String,
  val path: String,
  val kind: EntryKind,
  val size: Int,          // bytes; 0 for directories
  val modifiedAt: Int     // Unix timestamp (seconds)
)

pub data class FileInfo(
  val path: String,
  val kind: EntryKind,
  val size: Int,
  val modifiedAt: Int,
  val createdAt: Int,
  val isReadOnly: Bool
)
```

#### API

```fuse
// Querying
pub fn os.exists(path: String) -> Bool
pub fn os.isFile(path: String) -> Bool
pub fn os.isDir(path: String) -> Bool
pub fn os.stat(path: String) -> Result<FileInfo, IOError>
pub fn os.readDir(path: String) -> Result<List<DirEntry>, IOError>
pub fn os.readDirRecursive(path: String) -> Result<List<DirEntry>, IOError>

// Creating
pub fn os.mkdir(path: String) -> Result<(), IOError>             // fails if exists
pub fn os.mkdirAll(path: String) -> Result<(), IOError>          // no-op if exists
pub fn os.createFile(path: String) -> Result<(), IOError>        // empty file, fails if exists

// Copying and moving
pub fn os.copyFile(src: String, dst: String) -> Result<(), IOError>
pub fn os.copyDir(src: String, dst: String) -> Result<(), IOError>   // recursive
pub fn os.rename(src: String, dst: String) -> Result<(), IOError>    // atomic on same filesystem
pub fn os.move(src: String, dst: String) -> Result<(), IOError>      // rename with copy fallback

// Deleting
pub fn os.removeFile(path: String) -> Result<(), IOError>
pub fn os.removeDir(path: String) -> Result<(), IOError>         // fails if not empty
pub fn os.removeDirAll(path: String) -> Result<(), IOError>      // recursive, dangerous

// Symlinks
pub fn os.createSymlink(src: String, dst: String) -> Result<(), IOError>
pub fn os.readSymlink(path: String) -> Result<String, IOError>   // returns target path

// Permissions
pub fn os.setReadOnly(path: String, readonly: Bool) -> Result<(), IOError>

// Temp files
pub fn os.tempDir() -> String        // system temp directory path
pub fn os.tempFile(prefix: String) -> Result<String, IOError>   // creates and returns path
pub fn os.tempDirCreate(prefix: String) -> Result<String, IOError>
```

#### Example

```fuse
import stdlib.full.os

@entrypoint
fn main() {
  if !os.exists("output") {
    os.mkdirAll("output")?
  }

  val entries = os.readDir(".")?
  for entry in entries {
    println(f"{entry.name} ({entry.kind})")
  }

  os.copyFile("src/main.fuse", "backup/main.fuse")?
  os.removeDirAll("tmp")?
}
```

**Implementation note:** All functions backed by FFI to Rust's `std::fs`. `DirEntry` and `FileInfo` are `data class` values populated from Rust struct fields. `readDirRecursive` can be implemented in Fuse over `readDir` using a loop and a stack. `move` tries `rename` first and falls back to `copyDir` + `removeDirAll` if they are on different filesystems.

---

### 15. `env.fuse`

**Purpose:** Environment variable access.

```fuse
import stdlib.full.env
```

#### API

```fuse
pub fn env.get(name: String) -> Option<String>
pub fn env.getOrDefault(name: String, default: String) -> String
pub fn env.set(name: String, value: String) -> Result<(), String>
pub fn env.remove(name: String) -> Result<(), String>
pub fn env.all() -> Map<String, String>      // snapshot of entire environment
pub fn env.has(name: String) -> Bool
```

#### Example

```fuse
import stdlib.full.env

@entrypoint
fn main() {
  val home = env.getOrDefault("HOME", "/tmp")
  println(f"home: {home}")

  match env.get("SECRET_KEY") {
    Some(k) => println(f"key length: {k.len()}")
    None    => println("no key set")
  }
}
```

**Implementation note:** Backed by `std::env::var`, `std::env::set_var`, `std::env::vars` via `fuse_rt_env_*` FFI. `all()` returns a snapshot; mutations after the snapshot are not reflected.

---

### 16. `sys.fuse`

**Purpose:** Process-level information — arguments, exit, current working directory.

```fuse
import stdlib.full.sys
```

#### API

```fuse
pub fn sys.args() -> List<String>     // command-line arguments, including argv[0]
pub fn sys.exit(code: Int) -> !       // never returns; exits with given code
pub fn sys.cwd() -> Result<String, String>    // current working directory
pub fn sys.setCwd(path: String) -> Result<(), String>
pub fn sys.pid() -> Int               // current process ID
pub fn sys.platform() -> String       // "linux", "macos", "windows"
pub fn sys.arch() -> String           // "x86_64", "aarch64"
pub fn sys.numCpus() -> Int           // logical CPU count
pub fn sys.memTotal() -> Int          // total system RAM in bytes (approximate)
```

#### Example

```fuse
import stdlib.full.sys

@entrypoint
fn main() {
  val args = sys.args()
  if args.len() < 2 {
    println("usage: program <input>")
    sys.exit(1)
  }
  println(f"running on {sys.platform()} ({sys.arch()})")
  println(f"pid: {sys.pid()}")
}
```

**Implementation note:** Backed by `std::env::args`, `std::process::exit`, `std::env::current_dir`, `std::env::set_current_dir`, `num_cpus` (already available in Cargo). `sys.exit` has return type `!` (the never type) — the compiler must support this annotation. If not yet supported, document that the caller must treat any code after `sys.exit` as unreachable.

---

### 17. `time.fuse`

**Purpose:** Real-world clock, timestamps, dates, and durations. Distinct from `timer.fuse` (which is async sleep/timeout). This module is synchronous.

```fuse
import stdlib.full.time.{Instant, Duration, DateTime}
```

#### Type Definitions

```fuse
// Opaque monotonic timestamp for measuring elapsed time
pub data class Instant(val nanos: Int)

// Duration in nanoseconds
pub data class Duration(val nanos: Int)

// Calendar date and time (UTC)
pub data class DateTime(
  val year: Int,
  val month: Int,    // 1–12
  val day: Int,      // 1–31
  val hour: Int,     // 0–23
  val minute: Int,   // 0–59
  val second: Int,   // 0–59
  val nanoSecond: Int,
  val unixSeconds: Int   // seconds since 1970-01-01T00:00:00Z
)
```

#### API — `Instant`

```fuse
pub fn Instant.now() -> Instant
pub fn Instant.elapsed(ref self) -> Duration       // time since self
pub fn Instant.durationSince(ref self, earlier: ref Instant) -> Duration
```

#### API — `Duration`

```fuse
pub fn Duration.fromNanos(nanos: Int) -> Duration
pub fn Duration.fromMicros(micros: Int) -> Duration
pub fn Duration.fromMillis(millis: Int) -> Duration
pub fn Duration.fromSecs(secs: Int) -> Duration
pub fn Duration.fromMins(mins: Int) -> Duration

pub fn Duration.toNanos(ref self) -> Int
pub fn Duration.toMicros(ref self) -> Int
pub fn Duration.toMillis(ref self) -> Int
pub fn Duration.toSecs(ref self) -> Int

pub fn Duration.add(ref self, other: ref Duration) -> Duration
pub fn Duration.sub(ref self, other: ref Duration) -> Duration
pub fn Duration.mul(ref self, factor: Int) -> Duration
pub fn Duration.toString(ref self) -> String    // e.g. "1.234s", "456ms", "789µs"
```

#### API — `DateTime`

```fuse
pub fn DateTime.now() -> DateTime                   // current UTC time
pub fn DateTime.fromUnix(secs: Int) -> DateTime     // from Unix timestamp
pub fn DateTime.parse(s: String) -> Result<DateTime, String>  // ISO 8601 only
pub fn DateTime.toString(ref self) -> String        // ISO 8601: "2024-01-15T10:30:00Z"
pub fn DateTime.toDate(ref self) -> String          // "2024-01-15"
pub fn DateTime.toTime(ref self) -> String          // "10:30:00"
pub fn DateTime.add(ref self, d: ref Duration) -> DateTime
pub fn DateTime.sub(ref self, d: ref Duration) -> DateTime
pub fn DateTime.diff(ref self, other: ref DateTime) -> Duration  // absolute difference
pub fn DateTime.dayOfWeek(ref self) -> Int          // 0 = Monday, 6 = Sunday
pub fn DateTime.isLeapYear(ref self) -> Bool
```

#### Example

```fuse
import stdlib.full.time.{Instant, Duration, DateTime}

@entrypoint
fn main() {
  val start = Instant.now()
  // ... some work ...
  val elapsed = start.elapsed()
  println(f"elapsed: {elapsed.toString()}")

  val now = DateTime.now()
  println(f"UTC: {now.toString()}")

  val week = Duration.fromSecs(7 * 24 * 3600)
  val nextWeek = now.add(ref week)
  println(f"next week: {nextWeek.toDate()}")
}
```

**Implementation note:** `Instant.now()` uses `std::time::Instant` via FFI. `DateTime.now()` uses `std::time::SystemTime`. All internal arithmetic is done in Fuse on integer nanoseconds. The `parse` function handles ISO 8601 only — no locale-aware parsing in this version.

---

### 18. `random.fuse`

**Purpose:** Pseudo-random number generation seeded from OS entropy. Not cryptographically secure — see `crypto.fuse` for that.

```fuse
import stdlib.full.random.{Rng}
```

#### Type Definition

```fuse
pub struct Rng { }  // opaque state
```

#### API

```fuse
// Construction
pub fn Rng.new() -> Rng           // seeded from OS entropy (non-deterministic)
pub fn Rng.seeded(seed: Int) -> Rng  // deterministic, for tests

// Integer
pub fn Rng.nextInt(mutref self) -> Int
pub fn Rng.nextIntRange(mutref self, low: Int, high: Int) -> Int  // [low, high)
pub fn Rng.nextBool(mutref self) -> Bool

// Float
pub fn Rng.nextFloat(mutref self) -> Float      // [0.0, 1.0)
pub fn Rng.nextFloatRange(mutref self, low: Float, high: Float) -> Float  // [low, high)

// Normal distribution
pub fn Rng.nextGaussian(mutref self, mean: Float, stddev: Float) -> Float

// Collections
pub fn Rng.shuffle(mutref self, mutref list: List<T>)
pub fn Rng.choose(mutref self, ref list: List<T>) -> Option<T>  // None if empty
pub fn Rng.sample(mutref self, ref list: List<T>, n: Int) -> List<T>  // n unique items

// Convenience (uses a shared global Rng — not thread-safe, use your own Rng in concurrent code)
pub fn random.int() -> Int
pub fn random.intRange(low: Int, high: Int) -> Int
pub fn random.float() -> Float
pub fn random.bool() -> Bool
```

#### Example

```fuse
import stdlib.full.random.{Rng}

@entrypoint
fn main() {
  var rng = Rng.seeded(42)
  println(rng.nextIntRange(1, 7))   // die roll: deterministic

  var items = ["alpha", "beta", "gamma", "delta"]
  rng.shuffle(mutref items)
  println(items.join(", "))

  match rng.choose(ref items) {
    Some(s) => println(f"chosen: {s}")
    None    => println("empty")
  }
}
```

**Implementation note:** Back `Rng` by Rust's `rand::rngs::SmallRng` (seeded) and `rand::thread_rng()` (entropy) via FFI. The global convenience functions use a thread-local `SmallRng` initialized from OS entropy. `nextGaussian` uses Box-Muller transform implemented in Fuse over `nextFloat`.

---

### 19. `process.fuse`

**Purpose:** Spawning and interacting with child processes. Distinct from `spawn` (which creates Fuse coroutines).

```fuse
import stdlib.full.process.{Command, Output, ProcessError}
```

#### Type Definitions

```fuse
pub data class ProcessError(val message: String, val code: Int)

pub data class Output(
  val stdout: String,
  val stderr: String,
  val exitCode: Int,
  val success: Bool
)

pub struct Command { }  // builder pattern, opaque state
```

#### API — `Command` builder

```fuse
pub fn Command.new(program: String) -> Command
pub fn Command.arg(mutref self, arg: String) -> mutref Command     // returns self for chaining
pub fn Command.args(mutref self, args: List<String>) -> mutref Command
pub fn Command.env(mutref self, key: String, val: String) -> mutref Command
pub fn Command.envClear(mutref self) -> mutref Command             // clear all env vars
pub fn Command.cwd(mutref self, dir: String) -> mutref Command
pub fn Command.stdin(mutref self, input: String) -> mutref Command // pipe string to stdin

// Execution
pub fn Command.run(owned self) -> Result<Output, ProcessError>     // wait for completion
pub fn Command.status(owned self) -> Result<Int, ProcessError>     // exit code only
pub fn Command.output(owned self) -> Result<Output, ProcessError>  // alias for run
```

#### API — Free functions

```fuse
pub fn process.run(program: String, args: List<String>) -> Result<Output, ProcessError>
pub fn process.shell(cmd: String) -> Result<Output, ProcessError>  // runs via sh/cmd.exe
```

#### Example

```fuse
import stdlib.full.process.{Command, process}

@entrypoint
fn main() {
  val out = Command.new("git")
    .arg("log")
    .arg("--oneline")
    .arg("-10")
    .run()?

  if out.success {
    println(out.stdout)
  } else {
    println(f"git failed (code {out.exitCode}): {out.stderr}")
  }

  // Simple form
  val result = process.run("ls", ["-la"])?
  println(result.stdout)
}
```

**Implementation note:** Backed by Rust's `std::process::Command` via FFI. The `Command` struct wraps a heap-allocated Rust `std::process::Command`. `stdin` pipes a string via `Stdio::piped()`. `process.shell` uses `sh -c` on Unix and `cmd.exe /C` on Windows.

---

### 20. `net.fuse`

**Purpose:** Low-level TCP and UDP networking. For HTTP, use `http.fuse`. This module provides raw socket access.

```fuse
import stdlib.full.net.{TcpStream, TcpListener, UdpSocket, NetError}
```

#### Error Type

```fuse
pub data class NetError(val message: String, val code: Int)
// code: 0 = generic, 1 = connection refused, 2 = timeout,
//       3 = address in use, 4 = broken pipe, 5 = not connected
```

#### API — TCP client (`TcpStream`)

```fuse
pub fn TcpStream.connect(addr: String, port: Int) -> Result<TcpStream, NetError>
pub fn TcpStream.connectTimeout(addr: String, port: Int, timeoutMs: Int) -> Result<TcpStream, NetError>

pub fn TcpStream.read(mutref self, maxBytes: Int) -> Result<List<Int>, NetError>
pub fn TcpStream.readExact(mutref self, n: Int) -> Result<List<Int>, NetError>
pub fn TcpStream.readLine(mutref self) -> Result<Option<String>, NetError>  // None at EOF
pub fn TcpStream.readAll(mutref self) -> Result<String, NetError>

pub fn TcpStream.write(mutref self, data: String) -> Result<Int, NetError>    // returns bytes written
pub fn TcpStream.writeBytes(mutref self, data: List<Int>) -> Result<Int, NetError>
pub fn TcpStream.flush(mutref self) -> Result<(), NetError>

pub fn TcpStream.setReadTimeout(mutref self, ms: Int) -> Result<(), NetError>   // 0 = no timeout
pub fn TcpStream.setWriteTimeout(mutref self, ms: Int) -> Result<(), NetError>
pub fn TcpStream.localAddr(ref self) -> Result<String, NetError>   // "127.0.0.1:54321"
pub fn TcpStream.peerAddr(ref self) -> Result<String, NetError>

pub fn TcpStream.close(owned self) -> Result<(), NetError>
pub fn TcpStream.__del__(owned self)   // closes on ASAP destruction
```

#### API — TCP server (`TcpListener`)

```fuse
pub fn TcpListener.bind(addr: String, port: Int) -> Result<TcpListener, NetError>
pub fn TcpListener.accept(mutref self) -> Result<TcpStream, NetError>   // blocks until connection
pub fn TcpListener.localAddr(ref self) -> Result<String, NetError>
pub fn TcpListener.close(owned self) -> Result<(), NetError>
pub fn TcpListener.__del__(owned self)
```

#### API — UDP (`UdpSocket`)

```fuse
pub fn UdpSocket.bind(addr: String, port: Int) -> Result<UdpSocket, NetError>
pub fn UdpSocket.sendTo(mutref self, data: List<Int>, addr: String, port: Int) -> Result<Int, NetError>
pub fn UdpSocket.recvFrom(mutref self, maxBytes: Int) -> Result<(List<Int>, String, Int), NetError>  // (data, addr, port)
pub fn UdpSocket.setBroadcast(mutref self, enabled: Bool) -> Result<(), NetError>
pub fn UdpSocket.close(owned self) -> Result<(), NetError>
pub fn UdpSocket.__del__(owned self)
```

#### Example

```fuse
import stdlib.full.net.{TcpStream, TcpListener}

@entrypoint
fn main() {
  // Simple TCP client
  val stream = TcpStream.connect("example.com", 80)?
  stream.write("GET / HTTP/1.0\r\nHost: example.com\r\n\r\n")?
  val response = stream.readAll()?
  println(f"got {response.len()} bytes")
  stream.close()?
}
```

**Implementation note:** Backed by Rust's `std::net` via FFI. `TcpStream` and `TcpListener` wrap heap-allocated Rust objects behind `Ptr`. The `__del__` on each closes the underlying socket automatically. For concurrent server use, combine with `spawn` from Fuse Full.

---

### 21. `json.fuse`

**Purpose:** JSON serialisation and deserialisation. The JSON value model is a tagged union. No reflection or code generation — you map manually between `JsonValue` and your domain types.

```fuse
import stdlib.full.json.{JsonValue, JsonError, parse, stringify}
```

#### Type Definitions

```fuse
pub data class JsonError(val message: String, val line: Int, val col: Int)

pub enum JsonValue {
  Null,
  Bool(Bool),
  Number(Float),
  Str(String),
  Array(List<JsonValue>),
  Object(Map<String, JsonValue>)
}
```

#### API

```fuse
// Parsing
pub fn json.parse(s: String) -> Result<JsonValue, JsonError>
pub fn json.parseFile(path: String) -> Result<JsonValue, JsonError>

// Serialisation
pub fn json.stringify(value: ref JsonValue) -> String                        // compact
pub fn json.stringifyPretty(value: ref JsonValue, indent: Int) -> String     // indented

// JsonValue helpers
pub fn JsonValue.isNull(ref self) -> Bool
pub fn JsonValue.isBool(ref self) -> Bool
pub fn JsonValue.isNumber(ref self) -> Bool
pub fn JsonValue.isString(ref self) -> Bool
pub fn JsonValue.isArray(ref self) -> Bool
pub fn JsonValue.isObject(ref self) -> Bool

pub fn JsonValue.asBool(ref self) -> Option<Bool>
pub fn JsonValue.asNumber(ref self) -> Option<Float>
pub fn JsonValue.asInt(ref self) -> Option<Int>        // truncates, None if not a number
pub fn JsonValue.asString(ref self) -> Option<String>
pub fn JsonValue.asArray(ref self) -> Option<List<JsonValue>>
pub fn JsonValue.asObject(ref self) -> Option<Map<String, JsonValue>>

// Object access helpers
pub fn JsonValue.get(ref self, key: String) -> Option<JsonValue>  // None if not Object or key missing
pub fn JsonValue.getPath(ref self, path: List<String>) -> Option<JsonValue>  // nested access

// Construction helpers
pub fn JsonValue.object(entries: List<(String, JsonValue)>) -> JsonValue
pub fn JsonValue.array(items: List<JsonValue>) -> JsonValue
```

#### Example

```fuse
import stdlib.full.json.{JsonValue, json}

@entrypoint
fn main() {
  val raw = "{\"name\": \"alice\", \"age\": 30, \"active\": true}"
  val v = json.parse(raw)?

  val name = v.get("name")?.asString() ?: "unknown"
  val age  = v.get("age")?.asInt() ?: 0
  println(f"name={name} age={age}")

  // Build JSON
  val out = JsonValue.object([
    ("status", JsonValue.Str("ok")),
    ("count",  JsonValue.Number(42.0))
  ])
  println(json.stringify(ref out))
}
```

**Implementation note:** The parser is a hand-written recursive descent parser in Fuse, operating over a `String` with an index cursor (no FFI needed). The `stringify` methods are pure Fuse string builders. `parseFile` calls `readFile` from `io.fuse`. The `getPath` method is pure Fuse, iterating the path list and calling `get` at each step.

---

### 22. `chan.fuse`

**Purpose:** `Chan<T>` API — typed channels for inter-task communication. The runtime type is built-in.

```fuse
import stdlib.full.chan
```

#### API

```fuse
pub fn Chan<T>.bounded(capacity: Int) -> (Chan<T>, Chan<T>)    // (sender, receiver)
pub fn Chan<T>.unbounded() -> (Chan<T>, Chan<T>)

pub fn Chan<T>.send(mutref self, value: T) -> Result<(), String>  // Err if channel closed
pub fn Chan<T>.recv(mutref self) -> Result<T, String>             // Err if channel closed and empty
pub fn Chan<T>.tryRecv(mutref self) -> Option<T>                  // non-blocking
pub fn Chan<T>.close(owned self)
pub fn Chan<T>.isClosed(ref self) -> Bool
pub fn Chan<T>.len(ref self) -> Int                               // items currently queued
pub fn Chan<T>.cap(ref self) -> Option<Int>                       // None for unbounded
```

#### Example

```fuse
import stdlib.full.chan

@entrypoint
fn main() {
  val (tx, rx) = Chan::<Int>.bounded(10)

  spawn async {
    var i = 0
    while i < 5 {
      tx.send(i)?
      i = i + 1
    }
    tx.close()
  }

  loop {
    match rx.recv() {
      Ok(v)  => println(f"got: {v}")
      Err(_) => break
    }
  }
}
```

**Implementation note:** `Chan<T>` is backed by `ValueKind::Channel` in the runtime. `send` and `recv` are already partially implemented. `close` marks the channel as closed; subsequent `recv` on an empty closed channel returns `Err`. `tryRecv` is a non-blocking `recv` — returns `None` immediately if no item is available.

---

### 23. `shared.fuse`

**Purpose:** `Shared<T>` — a `@rank`-annotated concurrent mutable value backed by an RwLock.

```fuse
import stdlib.full.shared
```

#### API

```fuse
pub fn Shared<T>.new(value: T) -> Shared<T>

pub fn Shared<T>.read(ref self) -> ref T        // blocks until read lock acquired
pub fn Shared<T>.write(mutref self) -> mutref T  // blocks until write lock acquired
pub fn Shared<T>.tryWrite(mutref self, timeoutMs: Int) -> Result<mutref T, String>
pub fn Shared<T>.tryRead(ref self, timeoutMs: Int) -> Result<ref T, String>
```

**Note:** `@rank` annotation is required at the declaration site by the compiler. Missing `@rank` on a `Shared<T>` is a compile error. Lock acquisition order must be ascending by rank — violations are a compile error.

---

### 24. `timer.fuse`

**Purpose:** Async sleep and timeouts.

```fuse
import stdlib.full.timer.{Timer, Timeout}
```

#### API

```fuse
pub fn Timer.sleep(ms: Int) -> ()           // async: suspends current task
pub fn Timer.sleepSecs(secs: Float) -> ()   // async: suspends current task

pub data class Timeout(val ms: Int)
pub fn Timeout.ms(ms: Int) -> Timeout
pub fn Timeout.secs(secs: Float) -> Timeout
pub fn Timeout.never() -> Timeout
```

**Usage:** `Timer.sleep` and `Timer.sleepSecs` must be called from within an `async` context — i.e., inside a `spawn async { }` block or a `suspend` function.

---

### 25. `simd.fuse`

**Purpose:** `SIMD<T, N>` — hardware vector operations.

```fuse
import stdlib.full.simd
```

#### API

```fuse
pub fn SIMD<T, N>.broadcast(value: T) -> SIMD<T, N>
pub fn SIMD<T, N>.fromList(items: List<T>) -> SIMD<T, N>   // items.len() must equal N
pub fn SIMD<T, N>.toList(ref self) -> List<T>

pub fn SIMD<T, N>.add(ref self, other: ref SIMD<T, N>) -> SIMD<T, N>
pub fn SIMD<T, N>.sub(ref self, other: ref SIMD<T, N>) -> SIMD<T, N>
pub fn SIMD<T, N>.mul(ref self, other: ref SIMD<T, N>) -> SIMD<T, N>
pub fn SIMD<T, N>.div(ref self, other: ref SIMD<T, N>) -> SIMD<T, N>
pub fn SIMD<T, N>.sum(ref self) -> T
pub fn SIMD<T, N>.dot(ref self, other: ref SIMD<T, N>) -> T
pub fn SIMD<T, N>.min(ref self, other: ref SIMD<T, N>) -> SIMD<T, N>
pub fn SIMD<T, N>.max(ref self, other: ref SIMD<T, N>) -> SIMD<T, N>
pub fn SIMD<T, N>.abs(ref self) -> SIMD<T, N>
pub fn SIMD<T, N>.sqrt(ref self) -> SIMD<T, N>    // T must be Float or Float32
pub fn SIMD<T, N>.get(ref self, index: Int) -> T
pub fn SIMD<T, N>.len(ref self) -> Int             // always N
```

---

### 26. `http.fuse`

**Purpose:** HTTP client — `GET`, `POST`, `PUT`, `DELETE`, and custom requests. No HTTP server (see `http_server.fuse` in Ext).

```fuse
import stdlib.full.http.{HttpClient, Request, Response, HttpError}
```

#### Type Definitions

```fuse
pub data class HttpError(val message: String, val code: Int)
// code: 0 = generic, 1 = timeout, 2 = dns failure, 3 = connection refused, 4 = tls error

pub data class Response(
  val status: Int,
  val headers: Map<String, String>,
  val body: String
)

pub struct HttpClient { }   // opaque, holds connection pool
```

#### API

```fuse
// Default client (no connection pooling, one-off requests)
pub fn http.get(url: String) -> Result<Response, HttpError>
pub fn http.post(url: String, body: String) -> Result<Response, HttpError>
pub fn http.postJson(url: String, body: String) -> Result<Response, HttpError>  // sets Content-Type: application/json
pub fn http.put(url: String, body: String) -> Result<Response, HttpError>
pub fn http.delete(url: String) -> Result<Response, HttpError>

// Configurable client
pub fn HttpClient.new() -> HttpClient
pub fn HttpClient.withTimeout(mutref self, ms: Int) -> mutref HttpClient
pub fn HttpClient.withHeader(mutref self, key: String, val: String) -> mutref HttpClient
pub fn HttpClient.withBasicAuth(mutref self, user: String, pass: String) -> mutref HttpClient
pub fn HttpClient.withBearerToken(mutref self, token: String) -> mutref HttpClient

pub fn HttpClient.get(ref self, url: String) -> Result<Response, HttpError>
pub fn HttpClient.post(ref self, url: String, body: String) -> Result<Response, HttpError>
pub fn HttpClient.postJson(ref self, url: String, body: String) -> Result<Response, HttpError>
pub fn HttpClient.put(ref self, url: String, body: String) -> Result<Response, HttpError>
pub fn HttpClient.delete(ref self, url: String) -> Result<Response, HttpError>

// Response helpers
pub fn Response.ok(ref self) -> Bool               // status in 200–299
pub fn Response.json(ref self) -> Result<JsonValue, JsonError>   // parses body as JSON
```

#### Example

```fuse
import stdlib.full.http.{http, HttpClient}
import stdlib.full.json.{json}

@entrypoint
fn main() {
  val resp = http.get("https://httpbin.org/get")?
  println(f"status: {resp.status}")

  val data = resp.json()?
  val origin = data.get("origin")?.asString() ?: "unknown"
  println(f"your ip: {origin}")

  // Authenticated client
  val client = HttpClient.new()
    .withBearerToken("my-secret-token")
    .withTimeout(5000)

  val r = client.postJson("https://api.example.com/items", "{\"name\":\"fuse\"}")?
  println(f"created: {r.status}")
}
```

**Implementation note:** Backed by Rust's `reqwest` crate (blocking client) via FFI. `reqwest` is already a reasonable choice because it handles TLS (via `rustls`), redirects, and connection pooling cleanly. The `async` variants of `reqwest` are not used here — the synchronous API is sufficient and simpler to FFI-wrap for Stage 1. Async HTTP can be a future addition.

---

## Part 3 — `stdlib/ext/`

Ext modules live in `stdlib/ext/`. They are not imported by default. Each is an optional dependency. They are implemented in Fuse over FFI to Rust crates or as pure Fuse libraries.

---

### 27. `test.fuse`

**Purpose:** Test assertion utilities. Intended for use inside `*_test.fuse` files.

```fuse
import stdlib.ext.test.{assert, assertEq, assertNe, assertPanics, fail}
```

#### API

```fuse
pub fn assert(cond: Bool, message: String)
pub fn assertEq(a: T, b: T, message: String)     // T must support ==
pub fn assertNe(a: T, b: T, message: String)
pub fn assertOk(r: Result<T, E>)
pub fn assertErr(r: Result<T, E>)
pub fn assertSome(o: Option<T>)
pub fn assertNone(o: Option<T>)
pub fn assertApprox(a: Float, b: Float, epsilon: Float)
pub fn fail(message: String) -> !                 // unconditional test failure
pub fn skip(message: String) -> !                 // marks test as skipped

// Test grouping
pub fn describe(name: String, f: fn())            // logical grouping, printed in output
```

**Implementation note:** On assertion failure, print a formatted message to stderr including the assertion type, the values (via `toString`), and the caller-provided message, then call `sys.exit(1)`. `assertPanics` requires the ability to catch a panic — defer this until the panic/exception model is settled.

---

### 28. `log.fuse`

**Purpose:** Structured, levelled logging.

```fuse
import stdlib.ext.log.{Logger, log}
```

#### API

```fuse
pub enum Level { Trace, Debug, Info, Warn, Error }

pub struct Logger { }  // opaque

pub fn Logger.new() -> Logger
pub fn Logger.withLevel(mutref self, level: Level) -> mutref Logger
pub fn Logger.withPrefix(mutref self, prefix: String) -> mutref Logger
pub fn Logger.toFile(mutref self, path: String) -> mutref Logger
pub fn Logger.toStderr(mutref self) -> mutref Logger   // default

pub fn Logger.trace(ref self, msg: String)
pub fn Logger.debug(ref self, msg: String)
pub fn Logger.info(ref self, msg: String)
pub fn Logger.warn(ref self, msg: String)
pub fn Logger.error(ref self, msg: String)

// Global logger (defaults to Info level, stderr)
pub fn log.trace(msg: String)
pub fn log.debug(msg: String)
pub fn log.info(msg: String)
pub fn log.warn(msg: String)
pub fn log.error(msg: String)
pub fn log.setLevel(level: Level)
```

**Output format:** `2024-01-15T10:30:00Z [INFO] prefix: message\n`

---

### 29. `regex.fuse`

**Purpose:** Regular expression matching. Not bundled because regex engines are heavyweight.

```fuse
import stdlib.ext.regex.{Regex, Match, RegexError}
```

#### API

```fuse
pub data class RegexError(val message: String)
pub data class Match(val text: String, val start: Int, val end: Int)

pub struct Regex { }   // compiled pattern, opaque

pub fn Regex.compile(pattern: String) -> Result<Regex, RegexError>
pub fn Regex.isMatch(ref self, text: String) -> Bool
pub fn Regex.find(ref self, text: String) -> Option<Match>          // first match
pub fn Regex.findAll(ref self, text: String) -> List<Match>
pub fn Regex.replace(ref self, text: String, replacement: String) -> String    // first match
pub fn Regex.replaceAll(ref self, text: String, replacement: String) -> String
pub fn Regex.split(ref self, text: String) -> List<String>
pub fn Regex.captures(ref self, text: String) -> Option<List<String>>  // capture groups
```

**Implementation note:** Backed by Rust's `regex` crate. The `Regex` struct wraps a heap-allocated compiled `regex::Regex`. Compilation is expensive — cache compiled patterns.

---

### 30. `json_schema.fuse`

**Purpose:** JSON Schema validation (draft 7). Validates a `JsonValue` against a schema. Ext because it is rarely needed in general code but essential for API boundary validation.

```fuse
import stdlib.ext.json_schema.{Schema, ValidationError}
```

#### API

```fuse
pub data class ValidationError(val path: String, val message: String)
pub struct Schema { }

pub fn Schema.compile(schema: ref JsonValue) -> Result<Schema, String>
pub fn Schema.validate(ref self, value: ref JsonValue) -> Result<(), List<ValidationError>>
pub fn Schema.isValid(ref self, value: ref JsonValue) -> Bool
```

---

### 31. `yaml.fuse`

**Purpose:** YAML parsing and serialisation. The YAML value model mirrors `JsonValue` (YAML is a superset of JSON).

```fuse
import stdlib.ext.yaml.{YamlValue, YamlError, yaml}
```

#### API

```fuse
pub data class YamlError(val message: String, val line: Int, val col: Int)

pub enum YamlValue {
  Null,
  Bool(Bool),
  Int(Int),
  Float(Float),
  Str(String),
  Seq(List<YamlValue>),
  Map(Map<String, YamlValue>)
}

pub fn yaml.parse(s: String) -> Result<YamlValue, YamlError>
pub fn yaml.parseFile(path: String) -> Result<YamlValue, YamlError>
pub fn yaml.stringify(value: ref YamlValue) -> String
pub fn yaml.stringifyPretty(value: ref YamlValue) -> String
```

**Implementation note:** Backed by Rust's `serde_yaml` crate.

---

### 32. `toml.fuse`

**Purpose:** TOML parsing and serialisation. Common for configuration files.

```fuse
import stdlib.ext.toml.{TomlValue, TomlError, toml}
```

#### API

```fuse
pub data class TomlError(val message: String, val line: Int, val col: Int)

pub enum TomlValue {
  Bool(Bool),
  Int(Int),
  Float(Float),
  Str(String),
  DateTime(String),
  Array(List<TomlValue>),
  Table(Map<String, TomlValue>)
}

pub fn toml.parse(s: String) -> Result<TomlValue, TomlError>
pub fn toml.parseFile(path: String) -> Result<TomlValue, TomlError>
pub fn toml.stringify(value: ref TomlValue) -> String
```

**Implementation note:** Backed by Rust's `toml` crate.

---

### 33. `crypto.fuse`

**Purpose:** Cryptographic primitives. Ext because security-sensitive code must be versioned and audited independently of the language runtime.

```fuse
import stdlib.ext.crypto.{hash, hmac, rand}
```

#### API

```fuse
// Hashing
pub fn hash.sha256(data: String) -> String        // hex-encoded hash
pub fn hash.sha256Bytes(data: List<Int>) -> List<Int>
pub fn hash.sha512(data: String) -> String
pub fn hash.md5(data: String) -> String           // for legacy use only — not secure
pub fn hash.blake3(data: String) -> String

// HMAC
pub fn hmac.sha256(key: String, data: String) -> String   // hex-encoded

// Constant-time comparison (prevents timing attacks)
pub fn crypto.constantTimeEq(a: String, b: String) -> Bool

// Cryptographically secure random bytes
pub fn rand.bytes(n: Int) -> List<Int>
pub fn rand.hex(n: Int) -> String     // n random bytes, hex encoded (len = 2n)
pub fn rand.uuid4() -> String         // random UUID v4
```

**Implementation note:** Backed by Rust's `sha2`, `hmac`, `md5`, `blake3`, and `getrandom` crates.

---

### 34. `http_server.fuse`

**Purpose:** HTTP server. Ext because a server is an opinionated, stateful component that belongs outside the core runtime.

```fuse
import stdlib.ext.http_server.{Server, Router, Request, Response, Handler}
```

#### API

```fuse
pub data class Request(
  val method: String,
  val path: String,
  val headers: Map<String, String>,
  val query: Map<String, String>,
  val body: String
)

pub data class Response(
  val status: Int,
  val headers: Map<String, String>,
  val body: String
)

pub fn Response.ok(body: String) -> Response
pub fn Response.json(body: String) -> Response       // sets Content-Type
pub fn Response.status(code: Int, body: String) -> Response
pub fn Response.redirect(url: String) -> Response

pub struct Router { }
pub fn Router.new() -> Router
pub fn Router.get(mutref self, path: String, handler: fn(ref Request) -> Response) -> mutref Router
pub fn Router.post(mutref self, path: String, handler: fn(ref Request) -> Response) -> mutref Router
pub fn Router.put(mutref self, path: String, handler: fn(ref Request) -> Response) -> mutref Router
pub fn Router.delete(mutref self, path: String, handler: fn(ref Request) -> Response) -> mutref Router
pub fn Router.use(mutref self, middleware: fn(ref Request, fn(ref Request) -> Response) -> Response) -> mutref Router

pub struct Server { }
pub fn Server.new(router: Router) -> Server
pub fn Server.withPort(mutref self, port: Int) -> mutref Server
pub fn Server.withHost(mutref self, host: String) -> mutref Server
pub fn Server.withThreads(mutref self, n: Int) -> mutref Server
pub fn Server.listen(owned self) -> Result<(), String>    // blocks; call from main
```

#### Example

```fuse
import stdlib.ext.http_server.{Router, Server, Request, Response}

@entrypoint
fn main() {
  var router = Router.new()
    .get("/", fn(req) => Response.ok("hello from fuse"))
    .get("/ping", fn(req) => Response.json("{\"pong\": true}"))

  Server.new(router)
    .withPort(8080)
    .listen()?
}
```

**Implementation note:** Backed by Rust's `tiny_http` or `hyper` (blocking variant) via FFI. Path pattern matching (`:param` segments) can be added in a later iteration — the first version supports only literal paths.

---

## Implementation Order

The following order minimises blocking dependencies. Each tier depends on the previous.

### Wave 1 — Core (no dependencies except language primitives)
`result` → `option` → `bool` → `int` → `float` → `math` → `fmt` → `string` → `list` → `map` → `set`

### Wave 2 — Full I/O and System
`io` → `path` → `os` → `env` → `sys` → `time` → `random` → `process`

### Wave 3 — Full Networking and Data
`net` → `json` → `http`

### Wave 4 — Full Concurrency (already partially present)
`chan` → `shared` → `timer` → `simd`

### Wave 5 — Ext
`test` → `log` → `regex` → `toml` → `yaml` → `json_schema` → `crypto` → `http_server`

---

## FFI Naming Convention

All Rust FFI functions added to `fuse-runtime` follow the pattern:

```
fuse_rt_{module}_{operation}
```

Examples:
- `fuse_rt_string_to_lower`
- `fuse_rt_float_sqrt`
- `fuse_rt_io_read_file`
- `fuse_rt_net_tcp_connect`
- `fuse_rt_time_instant_now`

Every new FFI function must:
1. Be declared `#[unsafe(no_mangle)] pub unsafe extern "C" fn` in `fuse-runtime/src/ffi.rs`.
2. Take and return `FuseHandle` for all Fuse-typed arguments, and raw `Int`/`Float`/`Bool` for primitives.
3. Have a corresponding `extern fn` declaration at the top of the relevant `.fuse` module.
4. Be documented with its signature and ownership semantics in a comment above the Rust declaration.

---

## Test File Requirements

Every stdlib module must ship with a corresponding test file:

```
tests/fuse/stdlib/core/list_test.fuse
tests/fuse/stdlib/core/string_test.fuse
tests/fuse/stdlib/full/io_test.fuse
...
```

Each test file must:
- Import `stdlib.ext.test.{assertEq, assert, assertOk}`
- Have a single `@entrypoint fn main()` that runs all assertions
- Test every public function with at least one happy-path case and one edge case
- Test every documented edge case explicitly

Test files whose names end in `_rejected.fuse` verify that misuse (wrong types, missing `@rank`, etc.) produces the correct compile error.

---

*End of Fuse Standard Library Specification*
