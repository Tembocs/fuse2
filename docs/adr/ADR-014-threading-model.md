# ADR-014 Threading model — single-threaded Stage 1, OS threads Stage 2, green threads post-Stage 2

## Status
Accepted

## Context

Fuse's concurrency model is built on three primitives: `spawn` (fire-and-forget tasks), `Chan<T>` (typed channels), and `Shared<T>` (rank-checked shared state). The language guide specifies these at the API level, but the **runtime execution model** — how `spawn` actually runs code — is an architectural decision with deep consequences for the runtime, codegen, and ABI.

The core tension: Fuse promises **concurrency safety without a borrow checker** via compile-time ownership and rank checking. But the *runtime* must eventually execute concurrent tasks on real hardware threads. The question is when and how.

## Decision

Three phases, each building on the last:

### Phase 1: Single-threaded (Stage 1 — current)

Stage 1 is single-threaded. There are no real locks, no thread spawning, and no OS-level synchronization in the Fuse runtime.

- **`Shared<T>`** — `read()` returns a clone, `write()` returns the live handle. No `RwLock`, no `Mutex`. The `@rank` system is enforced at compile time only.
- **`Chan<T>`** — backed by plain `VecDeque`. No synchronization primitives. `send()` and `recv()` are direct queue operations.
- **`spawn`** — parsed and type-checked, but not compiled to actual thread creation. The evaluator (`--run`) executes spawn bodies inline.
- **`FuseHandle` (`*mut FuseValue`)** — raw pointer, not `Send`, not `Sync`.

This is correct for Stage 1's purpose: validate the language design, compile correct programs, and bootstrap Stage 2. All concurrency safety is verified at compile time. The runtime does not need to enforce it because there is only one thread.

### Phase 2: OS threads (Stage 2)

When Stage 2 adds real concurrency:

- **`spawn`** creates an OS thread (`std::thread::spawn` equivalent).
- **`Shared<T>`** gains a real `RwLock`. `read()` acquires a read lock, `write()` acquires a write lock. The `@rank` compile-time ordering prevents deadlocks.
- **`Chan<T>`** gains `Mutex` + `Condvar` (or equivalent lock-free queue) for thread-safe send/recv with blocking.
- **Pool size** — unbounded. Developer's responsibility, matching Go's goroutine model.

The **FFI surface stays identical**: `fuse_shared_new`, `fuse_shared_read`, `fuse_shared_write`, `fuse_chan_send`, `fuse_chan_recv` — same function signatures, same `FuseHandle` parameters. Only the internal implementation changes.

### Phase 3: Green threads (post-Stage 2, performance optimization)

Replace OS threads with lightweight M:N scheduled green threads:

- **`spawn`** creates a green thread on a runtime-managed thread pool (small number of OS threads, large number of green threads).
- **Work-stealing scheduler** distributes tasks across OS threads.
- **Stack size** — small initial allocation (e.g., 4KB), grown on demand.
- **`Chan<T>`** operations can suspend the green thread without blocking the OS thread.

The `spawn { }` API does not change. This is a transparent runtime swap. User code is unaware of the scheduling model.

## FFI stability contract

The runtime FFI surface must remain stable across all three phases:

```
fuse_shared_new(value: FuseHandle) -> FuseHandle
fuse_shared_read(shared: FuseHandle) -> FuseHandle
fuse_shared_write(shared: FuseHandle) -> FuseHandle
fuse_shared_try_read(shared: FuseHandle, timeout_ms: i64) -> FuseHandle
fuse_shared_try_write(shared: FuseHandle, timeout_ms: i64) -> FuseHandle
fuse_chan_new(capacity: i64) -> FuseHandle
fuse_chan_send(chan: FuseHandle, value: FuseHandle)
fuse_chan_recv(chan: FuseHandle) -> FuseHandle
fuse_chan_close(chan: FuseHandle)
```

Stage 2 compiled binaries must link against the same runtime symbols whether the runtime uses OS threads or green threads. The scheduling model is a runtime implementation detail, not a language-level concept.

## Rejected alternatives

**Async/await:** Rejected in W0.6 and removed from the language. Reasons:
- Function coloring — splits the ecosystem into sync and async halves.
- Viral propagation — one async function forces callers to be async.
- Hidden state machines — compiler-generated state machines are hard to debug.
- Fuse's model is simpler: every function is synchronous. Concurrency is a call-site decision via `spawn`.

**Immediate OS threading in Stage 1:** Rejected because:
- Adds complexity to a bootstrap compiler that will be replaced by Stage 2.
- `FuseHandle` (`*mut FuseValue`) is not thread-safe — would require either a tagged value representation or `Arc` wrapping, both significant undertakings.
- Compile-time rank checking is sufficient to validate the concurrency design without real threads.
- Stage 1's job is correctness, not concurrent execution.

**Green threads in Stage 2:** Rejected as premature. OS threads are simple, correct, and proven. Green threads require:
- A runtime scheduler (work-stealing, I/O event loop integration).
- Stack management (growable stacks, stack overflow detection).
- Cooperative yield points (inserted by the compiler or at function calls).
- Performance data showing OS thread overhead is actually a bottleneck.

Green threads are deferred until profiling data from real Stage 2 programs justifies the complexity.

**Actor model:** Rejected because:
- Channels are more flexible — actors enforce one-mailbox-per-entity, channels allow arbitrary topologies.
- Fuse already has `spawn` + `Chan<T>` which subsumes the actor pattern.
- Users can build actors from channels if desired.

**`select` expression:** Deferred to post-Stage 2. Requires runtime scheduler integration for efficient channel multiplexing. Current workaround: use separate `spawn` blocks per channel, or polling with `try_recv`.

**Joinable spawn handles (`SpawnHandle<T>`):** Deferred. Fire-and-forget + channels provides equivalent capability. Adding `SpawnHandle<T>` requires runtime task tracking and result storage, which is scheduler infrastructure that doesn't exist yet.

## Consequences

- **Stage 1 is safe to use for all non-concurrent programs.** Concurrent programs type-check and ownership-check correctly but execute sequentially.
- **Stage 2 migration is additive** — the runtime gains real synchronization primitives behind the same FFI surface. No codegen changes, no ABI breaks.
- **Green thread migration is transparent** — user code does not change. The `spawn` API is the same whether it creates an OS thread or a green thread.
- **`@rank` validation has long-term value** — it prevents deadlocks regardless of the scheduling model. The compile-time check works with OS threads, green threads, or any future model.
- **No function coloring** — Fuse will never have `async fn`. Concurrency is always `spawn` + channels/shared-state. This is a permanent design constraint.
- **Testing concurrent behavior** in Stage 1 requires the evaluator (`--run`) with sequential execution. True concurrent tests (race conditions, deadlock scenarios) must wait for Stage 2 OS threads.
