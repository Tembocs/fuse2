use fuse_runtime::{extract_int, fuse_int};

// Import all FFI functions used in W0.1 and W0.2.
use cranelift_ffi::{
    cranelift_ffi_context_free, cranelift_ffi_context_new, cranelift_ffi_module_free,
    cranelift_ffi_module_new, cranelift_ffi_module_target_pointer_type, cranelift_ffi_signature_add_param,
    cranelift_ffi_signature_add_return, cranelift_ffi_signature_clone, cranelift_ffi_signature_free,
    cranelift_ffi_signature_new, cranelift_ffi_version,
};

/// Helper: wrap an i64 into a FuseHandle for passing to FFI functions.
fn h(v: i64) -> fuse_runtime::FuseHandle {
    unsafe { fuse_int(v) }
}

/// Helper: extract i64 from a FuseHandle returned by FFI.
fn v(handle: fuse_runtime::FuseHandle) -> i64 {
    extract_int(handle)
}

#[test]
fn w0_1_module_and_context() {
    // Version check.
    assert_eq!(v(cranelift_ffi_version()), 2);

    // Create module, query pointer type.
    let module = cranelift_ffi_module_new();
    assert!(!module.is_null());
    let ptr_bytes = v(unsafe { cranelift_ffi_module_target_pointer_type(module) });
    assert_eq!(ptr_bytes, 8); // 64-bit

    // Create and free context.
    let ctx = cranelift_ffi_context_new();
    assert!(!ctx.is_null());
    unsafe { cranelift_ffi_context_free(ctx) };

    // Free module.
    unsafe { cranelift_ffi_module_free(module) };
}

#[test]
fn w0_2_signature_round_trip() {
    // W0.2.6: create signature with (Ptr, Ptr) -> Ptr, verify round-trip.
    let module = cranelift_ffi_module_new();

    // Create signature with default calling convention (call_conv = 0).
    let sig = unsafe { cranelift_ffi_signature_new(module, h(0)) };
    assert!(!sig.is_null());

    // Add two Ptr params (type_id 4 = pointer).
    unsafe { cranelift_ffi_signature_add_param(sig, h(4), module) };
    unsafe { cranelift_ffi_signature_add_param(sig, h(4), module) };

    // Add one Ptr return.
    unsafe { cranelift_ffi_signature_add_return(sig, h(4), module) };

    // Clone the signature and verify it's a distinct handle.
    let sig2 = unsafe { cranelift_ffi_signature_clone(sig) };
    assert!(!sig2.is_null());
    assert_ne!(sig, sig2);

    // Free both signatures.
    unsafe { cranelift_ffi_signature_free(sig) };
    unsafe { cranelift_ffi_signature_free(sig2) };

    // Free module.
    unsafe { cranelift_ffi_module_free(module) };
}
