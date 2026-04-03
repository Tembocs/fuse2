use cranelift_ffi::{
    cranelift_ffi_function_free, cranelift_ffi_function_new, cranelift_ffi_module_free,
    cranelift_ffi_module_new, cranelift_ffi_version,
};

#[test]
fn ffi_surface_is_callable() {
    assert_eq!(cranelift_ffi_version(), 1);
    let module = cranelift_ffi_module_new();
    assert!(!module.is_null());
    let function = cranelift_ffi_function_new();
    assert!(!function.is_null());
    unsafe {
        cranelift_ffi_function_free(function);
        cranelift_ffi_module_free(module);
    }
}
