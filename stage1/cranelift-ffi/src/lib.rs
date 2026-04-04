use cranelift_codegen::ir::Function;
use cranelift_module::default_libcall_names;
use cranelift_object::{ObjectBuilder, ObjectModule};

pub struct CraneliftFfiModule {
    _module: ObjectModule,
}

pub struct CraneliftFfiFunction {
    _function: Function,
}

#[unsafe(no_mangle)]
pub extern "C" fn cranelift_ffi_version() -> u32 {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn cranelift_ffi_module_new() -> *mut CraneliftFfiModule {
    let isa_builder = cranelift_native::builder().expect("native ISA available");
    let isa = isa_builder
        .finish(cranelift_codegen::settings::Flags::new(
            cranelift_codegen::settings::builder(),
        ))
        .expect("native ISA flags");
    let builder =
        ObjectBuilder::new(isa, "fuse_stage1", default_libcall_names()).expect("object builder");
    Box::into_raw(Box::new(CraneliftFfiModule {
        _module: ObjectModule::new(builder),
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_module_free(module: *mut CraneliftFfiModule) {
    if !module.is_null() {
        unsafe { drop(Box::from_raw(module)) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn cranelift_ffi_function_new() -> *mut CraneliftFfiFunction {
    Box::into_raw(Box::new(CraneliftFfiFunction {
        _function: Function::new(),
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_function_free(function: *mut CraneliftFfiFunction) {
    if !function.is_null() {
        unsafe { drop(Box::from_raw(function)) };
    }
}
