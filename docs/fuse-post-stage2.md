# Fuse — Post-Stage 2 Roadmap

> **Status:** Planning. Implementation begins after Stage 2 self-hosting.
> **Prerequisite:** Stage 2 compiler (self-hosted) complete and validated.
> **Authority:** This document tracks features explicitly deferred from the
> pre-Stage 2 plan. Each item has a rationale for deferral and conditions
> under which it becomes actionable.
>
> The authoritative language spec is `docs/fuse-language-guide-2.md`.
> All work must conform to the three non-negotiable properties:
>
> 1. **Memory safety without garbage collection.**
> 2. **Concurrency safety without a borrow checker.**
> 3. **Developer experience as a first-class concern.**

---

## During Stage 2

> Items implemented while Stage 2 is being built, as validation exercises
> for the self-hosting compiler. These are applications of Fuse, not
> compiler features.

| Item | Description | Why during Stage 2 |
|------|-------------|-------------------|
| **MCP server** | Model Context Protocol server using JSON-RPC over stdio | Built as a Fuse application using the Stage 2 compiler. Validates the language for real-world protocol implementation. |

---

## Language Features

### Dynamic Dispatch (`dyn Interface`)

**Deferred from:** Wave 5 (Interface System), pre-Stage 2 plan.

**What:** Allow runtime polymorphism via vtable-based dispatch. Syntax:
`fn process(item: dyn Printable)` — accepts any type implementing
`Printable` at runtime.

**Why deferred:** Static dispatch is sufficient for self-hosting and all
pre-Stage 2 work. Dynamic dispatch adds runtime overhead (vtable
indirection), implementation complexity (vtable generation, fat pointers),
and is not needed for any planned use case before Stage 2 completes.

**Unblocked by:** Wave 5 (interfaces) complete. Static dispatch working
and validated. Runtime type metadata infrastructure.

**Implementation sketch:**
- Add `dyn Interface` type syntax to parser
- Generate vtables per (type, interface) pair in codegen
- Fat pointer representation: `(data_ptr, vtable_ptr)`
- Vtable layout: one function pointer per interface method

---

### Runtime Interface Checks (`is Interface`)

**Deferred from:** Wave 5 (Interface System), pre-Stage 2 plan.

**What:** `if value is Printable { ... }` — runtime type check against
an interface.

**Why deferred:** Requires runtime type information (RTTI) embedded in
compiled binaries. Static dispatch and compile-time bounds are sufficient
for all pre-Stage 2 work.

**Unblocked by:** Dynamic dispatch implementation (provides the vtable
infrastructure needed for runtime checks).

---

### Operator Overloading

**Deferred from:** Pre-Stage 2 hardening plan.

**What:** `vec1 + vec2`, `matrix * vector` — custom `+`/`-`/`*`/`==`
dispatch on user-defined types.

**Why deferred:** Significant parser/checker/codegen work. Stage 2 can
be written without it. Should be implemented after hardening validates
the compiler is solid.

**Unblocked by:** Interface system complete (operators map to interface
methods: `Addable`, `Comparable`, etc.).

---

### Fixed-Size Arrays `[T; N]`

**Deferred from:** Pre-Stage 2 hardening plan.

**What:** Contiguous stack-allocated arrays with compile-time known size.

**Why deferred:** FFI struct interop and performance optimization. Not
needed for Stage 2 bootstrap. `List<T>` is sufficient.

**Unblocked by:** Numeric type system (Wave 2) complete. Codegen support
for stack allocation.

---

### Int16 / UInt16

**Deferred from:** Pre-Stage 2 numeric type system (Wave 2).

**What:** 16-bit signed/unsigned integers.

**Why deferred:** Uncommon outside audio processing and legacy formats.
The minimum viable set (Int8, UInt8, Int32, UInt32, UInt64) covers FFI,
WASM, SIMD, and crypto. Add 16-bit types when a concrete FFI use case
demands it.

**Unblocked by:** Wave 2 complete (establishes the pattern for adding
new numeric types).

---

### Select Expression

**Deferred from:** Concurrency model design (interfaces spec Part II).

**What:** Multiplex multiple channel operations in a single expression.

**Proposed syntax (pending final design):**
```fuse
select {
    val n from rx1 => println(f"Int: {n}")
    val s from rx2 => println(f"String: {s}")
    else => break
}
```

**Why deferred:** Requires runtime scheduler integration that does not
exist in Stage 1. Current spawn + channels model is sufficient for all
pre-Stage 2 work.

**Design decision required before implementation:**
- `select { }` as standalone expression vs `match select { }`
- `from` keyword for channel association vs other syntax
- Timeout arm syntax

---

### Joinable Spawn Handles

**Deferred from:** Concurrency model design (interfaces spec Part II).

**What:** `val h = spawn { compute() }; val result = h.join()?` —
spawn returns a handle that can be awaited for the task's result.

**Why deferred:** Current fire-and-forget + channels provides the same
capability. Adding `SpawnHandle<T>` requires runtime task tracking and
result storage. Not needed for self-hosting.

**Unblocked by:** Runtime scheduler improvements. Interface system
(SpawnHandle could implement standard interfaces).

---

### Green Threads

**Deferred from:** Concurrency model design (interfaces spec Part II).

**What:** Replace OS threads with lightweight green threads (M:N
scheduling, like Go goroutines).

**Why deferred:** OS threads are simple, correct, and proven. Green
threads are a performance optimization, not a correctness concern. The
`spawn { }` API does not change — this is a transparent runtime swap.

**Current model:** `spawn` creates an OS thread (Stage 1).
**Future model:** `spawn` creates a green thread on a thread pool.

**Unblocked by:** Stage 2 complete. Performance profiling data showing
OS thread overhead is a bottleneck.

---

## Stdlib Extensions

### Linear Algebra

`stdlib/ext/linalg.fuse` — `Vec2`, `Vec3`, `Vec4`, `Mat4`, `Quaternion`.

**Requires:** Operator overloading + Float32 (Wave 2).

### Structured Concurrency (Nurseries)

Interesting but unproven at scale. Revisit after green threads.

---

## Runtime & Platform

### Browser WASM Target

Separate `fuse-runtime-browser` crate with JS stubs for `fetch`,
`performance.now`, `crypto.getRandomValues`.

**Requires:** WASM target (Wave 8) complete and validated.

### Float16 / BFloat16

Half-precision floats for AI inference.

**Requires:** Float32 (Wave 2) complete, then extend the same pattern.

### Arena Allocation

Batch allocation/deallocation for GB-scale data. Language-level
allocator API.

**Requires:** Mature runtime, performance profiling data.

### GPU Access

FFI to wgpu for Vulkan/Metal/DX12/WebGPU draw calls and compute shaders.

### Tensor Type

N-dimensional arrays with shape, strides.

**Requires:** Float32 + operator overloading.

### Automatic Differentiation

Computational graph recording + backward pass for training.

**Requires:** Tensor type.

### Shader Interop

Compile Fuse subset to SPIR-V or pass WGSL/GLSL strings.

**Requires:** GPU access.

### CUDA/ROCm Native

Direct GPU compute access beyond wgpu.

---

## Metaprogramming

### User-Defined Annotations

Derive macros, custom annotations, annotation composition.

**Requires:** A macro system design. The built-in annotation system
(Wave 4) establishes the syntax; user-defined annotations extend it.

### Runtime Reflection

Querying annotations at runtime. Requires metadata embedding in
compiled binaries and a reflection API.

---

## Stdlib Backlog

> Features identified during stdlib implementation but deferred because
> they require compiler features or design decisions beyond what was
> available at the time.

| Item | Module | Why deferred | Unblocked by |
|------|--------|-------------|-------------|
| **File buffered methods** | `io.fuse` | `readLine`, `readChunk`, `readAll`, `write`, `writeBytes`, `flush`, `seek`, `pos`, `size` — requires struct method dispatch refinement | Wave 1 (structs) |
| **HttpClient builder** | `http.fuse` | Convenience builder needs opaque mutable state | Wave 0 (mutref Self) + Wave 1 (structs) |
| **Response.json()** | `http.fuse` | Needs cross-module import of `json.fuse` | Module system maturity |
| **Router middleware** | `http_server.fuse` | Requires higher-order function composition with current closure ABI | Closure improvements |
| **Map.get / Map.getPath** | `json.fuse` | Need map access by key | Map runtime support |
| **JSON parseFile** | `json.fuse` | Trivial `io.readFile` + `parse` — low priority | Nothing (can be added anytime) |
| **List shuffle / sample** | `random.fuse` | Require list index mutation | List mutation codegen |

---

*Document created: 2026-04-06.*
*Companion document: `docs/fuse-pre-stage2.md`.*
