use fuse_runtime::{extract_int, fuse_int};

use cranelift_ffi::{
    cranelift_ffi_builder_block_params, cranelift_ffi_builder_create_block,
    cranelift_ffi_builder_declare_func_in_func, cranelift_ffi_builder_entry_block,
    cranelift_ffi_builder_finalize, cranelift_ffi_builder_free, cranelift_ffi_builder_new,
    cranelift_ffi_builder_seal_all_blocks, cranelift_ffi_builder_seal_block,
    cranelift_ffi_builder_switch_to_block, cranelift_ffi_context_free, cranelift_ffi_context_new,
    cranelift_ffi_context_verify, cranelift_ffi_ins_call, cranelift_ffi_ins_iconst,
    cranelift_ffi_ins_return, cranelift_ffi_builder_inst_results,
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
    let module = cranelift_ffi_module_new();

    // Signature: (Ptr) -> Ptr
    let sig = unsafe { cranelift_ffi_signature_new(module, h(0)) };
    unsafe { cranelift_ffi_signature_add_param(sig, h(4), module) };
    unsafe { cranelift_ffi_signature_add_return(sig, h(4), module) };

    let name = b"test_func";
    let func_id = v(unsafe {
        cranelift_ffi_module_declare_function(
            module,
            h(name.as_ptr() as i64),
            h(name.len() as i64),
            sig,
            h(1),
        )
    });
    assert!(func_id >= 0);

    let ctx = cranelift_ffi_context_new();
    let bld = unsafe { cranelift_ffi_builder_new(module, ctx, sig) };
    assert!(!bld.is_null());

    let entry = v(unsafe { cranelift_ffi_builder_entry_block(bld) });
    assert_eq!(entry, 0);

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

    let block1 = v(unsafe { cranelift_ffi_builder_create_block(bld) });
    assert!(block1 > 0);

    let rt_sig = unsafe { cranelift_ffi_signature_new(module, h(0)) };
    unsafe { cranelift_ffi_signature_add_param(rt_sig, h(2), module) };
    unsafe { cranelift_ffi_signature_add_return(rt_sig, h(4), module) };
    let rt_name = b"fuse_int";
    let rt_func_id = v(unsafe {
        cranelift_ffi_module_declare_function(
            module,
            h(rt_name.as_ptr() as i64),
            h(rt_name.len() as i64),
            rt_sig,
            h(0),
        )
    });
    assert!(rt_func_id >= 0);

    let func_ref = v(unsafe {
        cranelift_ffi_builder_declare_func_in_func(bld, module, h(rt_func_id))
    });
    assert!(func_ref >= 0);

    unsafe { cranelift_ffi_builder_seal_all_blocks(bld) };
    unsafe { cranelift_ffi_builder_free(bld) };
    unsafe { cranelift_ffi_signature_free(sig) };
    unsafe { cranelift_ffi_signature_free(rt_sig) };
    unsafe { cranelift_ffi_context_free(ctx) };
    unsafe { cranelift_ffi_module_free(module) };
}

#[test]
fn w0_4_instructions_build_and_verify() {
    // W0.4.21: Build a function that calls fuse_int(42) and returns the result.
    // Then verify the IR and define the function in the module.
    let module = cranelift_ffi_module_new();

    // Declare runtime function: fuse_int(i64) -> Ptr
    let rt_sig = unsafe { cranelift_ffi_signature_new(module, h(0)) };
    unsafe { cranelift_ffi_signature_add_param(rt_sig, h(2), module) }; // I64
    unsafe { cranelift_ffi_signature_add_return(rt_sig, h(4), module) }; // Ptr
    let rt_name = b"fuse_int";
    let rt_func_id = v(unsafe {
        cranelift_ffi_module_declare_function(
            module,
            h(rt_name.as_ptr() as i64),
            h(rt_name.len() as i64),
            rt_sig,
            h(0), // Import
        )
    });
    assert!(rt_func_id >= 0);

    // Declare our function: () -> Ptr
    let fn_sig = unsafe { cranelift_ffi_signature_new(module, h(0)) };
    unsafe { cranelift_ffi_signature_add_return(fn_sig, h(4), module) }; // Ptr return
    let fn_name = b"make_42";
    let fn_id = v(unsafe {
        cranelift_ffi_module_declare_function(
            module,
            h(fn_name.as_ptr() as i64),
            h(fn_name.len() as i64),
            fn_sig,
            h(1), // Local
        )
    });
    assert!(fn_id >= 0);

    // Build function body.
    let ctx = cranelift_ffi_context_new();
    let bld = unsafe { cranelift_ffi_builder_new(module, ctx, fn_sig) };

    // Import fuse_int into this function.
    let fuse_int_ref = v(unsafe {
        cranelift_ffi_builder_declare_func_in_func(bld, module, h(rt_func_id))
    });
    assert!(fuse_int_ref >= 0);

    // iconst I64, 42
    let forty_two = v(unsafe {
        cranelift_ffi_ins_iconst(bld, h(2), h(42), module) // type_id 2 = I64
    });
    assert!(forty_two >= 0);

    // call fuse_int(42)
    let mut args: [i64; 1] = [forty_two];
    let call_inst = v(unsafe {
        cranelift_ffi_ins_call(
            bld,
            h(fuse_int_ref),
            h(args.as_mut_ptr() as i64),
            h(1),
        )
    });
    assert!(call_inst >= 0);

    // Get call result.
    let mut result_buf: [i64; 1] = [0];
    let nresults = v(unsafe {
        cranelift_ffi_builder_inst_results(
            bld,
            h(call_inst),
            h(result_buf.as_mut_ptr() as i64),
            h(1),
        )
    });
    assert_eq!(nresults, 1);
    let result_val = result_buf[0];

    // return result
    let mut ret_vals: [i64; 1] = [result_val];
    unsafe {
        cranelift_ffi_ins_return(
            bld,
            h(ret_vals.as_mut_ptr() as i64),
            h(1),
        )
    };

    // Seal all blocks and finalize.
    unsafe { cranelift_ffi_builder_seal_all_blocks(bld) };
    unsafe { cranelift_ffi_builder_finalize(bld) };

    // Verify the IR.
    let verify_result = v(unsafe { cranelift_ffi_context_verify(ctx, module) });
    assert_eq!(verify_result, 0, "IR verification failed");

    // Define the function in the module.
    let define_result = v(unsafe {
        cranelift_ffi_module_define_function(module, h(fn_id), ctx)
    });
    assert_eq!(define_result, 0, "Function definition failed");

    // Cleanup.
    unsafe { cranelift_ffi_builder_free(bld) };
    unsafe { cranelift_ffi_signature_free(rt_sig) };
    unsafe { cranelift_ffi_signature_free(fn_sig) };
    unsafe { cranelift_ffi_context_free(ctx) };
    unsafe { cranelift_ffi_module_free(module) };
}
