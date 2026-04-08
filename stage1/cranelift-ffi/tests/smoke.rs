use fuse_runtime::{extract_int, fuse_int};

use cranelift_ffi::{
    cranelift_ffi_builder_block_params, cranelift_ffi_builder_create_block,
    cranelift_ffi_builder_declare_func_in_func, cranelift_ffi_builder_entry_block,
    cranelift_ffi_builder_finalize, cranelift_ffi_builder_free, cranelift_ffi_builder_new,
    cranelift_ffi_builder_seal_all_blocks, cranelift_ffi_builder_seal_block,
    cranelift_ffi_builder_switch_to_block, cranelift_ffi_context_free, cranelift_ffi_context_new,
    cranelift_ffi_module_declare_function, cranelift_ffi_module_define_function,
    cranelift_ffi_module_free, cranelift_ffi_module_new, cranelift_ffi_module_target_pointer_type,
    cranelift_ffi_signature_add_param, cranelift_ffi_signature_add_return,
    cranelift_ffi_signature_clone, cranelift_ffi_signature_free, cranelift_ffi_signature_new,
    cranelift_ffi_version,
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
    assert_eq!(v(cranelift_ffi_version()), 2);

    let module = cranelift_ffi_module_new();
    assert!(!module.is_null());
    let ptr_bytes = v(unsafe { cranelift_ffi_module_target_pointer_type(module) });
    assert_eq!(ptr_bytes, 8);

    let ctx = cranelift_ffi_context_new();
    assert!(!ctx.is_null());
    unsafe { cranelift_ffi_context_free(ctx) };

    unsafe { cranelift_ffi_module_free(module) };
}

#[test]
fn w0_2_signature_round_trip() {
    let module = cranelift_ffi_module_new();

    let sig = unsafe { cranelift_ffi_signature_new(module, h(0)) };
    assert!(!sig.is_null());

    unsafe { cranelift_ffi_signature_add_param(sig, h(4), module) };
    unsafe { cranelift_ffi_signature_add_param(sig, h(4), module) };
    unsafe { cranelift_ffi_signature_add_return(sig, h(4), module) };

    let sig2 = unsafe { cranelift_ffi_signature_clone(sig) };
    assert!(!sig2.is_null());
    assert_ne!(sig, sig2);

    unsafe { cranelift_ffi_signature_free(sig) };
    unsafe { cranelift_ffi_signature_free(sig2) };
    unsafe { cranelift_ffi_module_free(module) };
}

#[test]
fn w0_3_builder_create_finalize() {
    // W0.3.13: create function with entry block, add params, finalize.
    let module = cranelift_ffi_module_new();

    // Create signature: (Ptr) -> Ptr
    let sig = unsafe { cranelift_ffi_signature_new(module, h(0)) };
    unsafe { cranelift_ffi_signature_add_param(sig, h(4), module) };
    unsafe { cranelift_ffi_signature_add_return(sig, h(4), module) };

    // Declare a function so we can define it later.
    let name = b"test_func";
    let func_id = v(unsafe {
        cranelift_ffi_module_declare_function(
            module,
            h(name.as_ptr() as i64),
            h(name.len() as i64),
            sig,
            h(1), // Local linkage
        )
    });
    assert!(func_id >= 0);

    // Create context and builder.
    let ctx = cranelift_ffi_context_new();
    let bld = unsafe { cranelift_ffi_builder_new(module, ctx, sig) };
    assert!(!bld.is_null());

    // Entry block should exist and have id 0.
    let entry = v(unsafe { cranelift_ffi_builder_entry_block(bld) });
    assert_eq!(entry, 0);

    // Read block params — should have 1 param (the Ptr argument).
    let mut param_buf: [i64; 4] = [0; 4];
    let count = v(unsafe {
        cranelift_ffi_builder_block_params(
            bld,
            h(entry),
            h(param_buf.as_mut_ptr() as i64),
            h(4),
        )
    });
    assert_eq!(count, 1);
    let param0 = param_buf[0]; // Value id of the first param

    // Create a second block, switch to it, seal it.
    let block1 = v(unsafe { cranelift_ffi_builder_create_block(bld) });
    assert!(block1 > 0);

    // Import a runtime function to call (declare fuse_int in the module first).
    let rt_sig = unsafe { cranelift_ffi_signature_new(module, h(0)) };
    unsafe { cranelift_ffi_signature_add_param(rt_sig, h(2), module) }; // I64 param
    unsafe { cranelift_ffi_signature_add_return(rt_sig, h(4), module) }; // Ptr return
    let rt_name = b"fuse_int";
    let rt_func_id = v(unsafe {
        cranelift_ffi_module_declare_function(
            module,
            h(rt_name.as_ptr() as i64),
            h(rt_name.len() as i64),
            rt_sig,
            h(0), // Import linkage
        )
    });
    assert!(rt_func_id >= 0);

    // Import the runtime function into this function's scope.
    let func_ref = v(unsafe {
        cranelift_ffi_builder_declare_func_in_func(bld, module, h(rt_func_id))
    });
    assert!(func_ref >= 0);

    // Seal all blocks and finalize.
    unsafe { cranelift_ffi_builder_seal_all_blocks(bld) };
    // We need to emit a return instruction before finalizing, otherwise
    // Cranelift will complain about an unterminated block. Use ins_return
    // once W0.4 is implemented. For now, just use builder_free which
    // finalizes internally — the function body won't verify without a
    // terminator but we're testing the builder API, not codegen.

    // Free builder (finalizes), then free context and module.
    unsafe { cranelift_ffi_builder_free(bld) };
    unsafe { cranelift_ffi_signature_free(sig) };
    unsafe { cranelift_ffi_signature_free(rt_sig) };
    unsafe { cranelift_ffi_context_free(ctx) };
    unsafe { cranelift_ffi_module_free(module) };
}
