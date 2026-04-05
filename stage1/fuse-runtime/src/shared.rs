// ---------------------------------------------------------------------------
// Stage 1 Shared<T> model — plain-value-with-rank-checked-access
// ---------------------------------------------------------------------------
//
// Stage 1 is single-threaded.  There is no real RwLock and no OS-level
// synchronisation.  The concurrency safety of Shared<T> rests entirely on
// **compile-time rank checking** performed by the checker (`checker/mod.rs`):
//
//   - Every `Shared<T>` binding must carry a `@rank(N)` annotation.
//   - Locks must be acquired in ascending rank order.
//   - Same rank means independent — safe to acquire in any order.
//   - Write guard held across `await` produces a compile warning.
//
// At runtime the distinction between `read()` and `write()` is:
//
//   **read()** — returns a *clone* (snapshot) of the inner value.
//               The caller gets an independent copy; subsequent writes to
//               the Shared storage do not mutate the snapshot.
//
//   **write()** — returns the *live inner handle*.
//                Mutations through this handle are immediately visible to
//                future read() / write() calls.
//
// This means:
//   - No aliasing between a read snapshot and the live storage (H2.4).
//   - Multiple reads return equal-valued but independent copies (H1.4).
//   - Write-then-read shows mutation visibility (H1.5).
//   - ASAP destruction of the Shared wrapper releases the inner value and
//     fires its `__del__` destructor if one exists (H1.7).
//
// When Stage 1 gains real multi-threaded execution (post-Phase 9), this
// module will grow an actual RwLock.  The public FFI surface stays the same.
// ---------------------------------------------------------------------------

use crate::value::{fuse_shared_new, fuse_shared_read, fuse_shared_write, FuseHandle};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_shared_runtime_new(value: FuseHandle) -> FuseHandle {
    unsafe { fuse_shared_new(value) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_shared_runtime_read(shared: FuseHandle) -> FuseHandle {
    unsafe { fuse_shared_read(shared) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_shared_runtime_write(shared: FuseHandle) -> FuseHandle {
    unsafe { fuse_shared_write(shared) }
}
