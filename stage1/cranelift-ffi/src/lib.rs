//! Cranelift FFI — C-compatible wrapper around Cranelift's code generation API.
//!
//! Stage 2 (the self-hosted Fuse compiler) calls these functions via `extern fn`
//! declarations to generate native machine code. All handles are opaque pointers.
//! All functions use C calling convention.
//!
//! **ABI contract:** Fuse's uniform ABI passes all values as `FuseHandle`
//! (pointer-sized). Functions that conceptually return integers must box them
//! via `fuse_int()`. Functions that return opaque handles return raw pointers
//! (which are pointer-sized and pass through the ABI unchanged).

use std::fs;
use std::path::Path;
use std::slice;

use cranelift_codegen::ir::{self, types, AbiParam, Function, UserFuncName};
use cranelift_codegen::settings;
use cranelift_module::{default_libcall_names, DataDescription, FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use fuse_runtime::{FuseHandle, fuse_int, fuse_unit};

// ---------------------------------------------------------------------------
// Opaque handle types
// ---------------------------------------------------------------------------

/// Wraps an ObjectModule and tracks declared function IDs by integer index.
pub struct FfiModule {
    module: ObjectModule,
    pointer_type: ir::Type,
    /// Maps integer ID -> FuncId for declared functions.
    func_ids: Vec<FuncId>,
    /// Maps integer ID -> DataId for declared data objects.
    data_ids: Vec<cranelift_module::DataId>,
}

/// Wraps a codegen::Context for building a single function.
pub struct FfiContext {
    ctx: cranelift_codegen::Context,
}

/// Wraps a Signature (parameter + return types for a function).
pub struct FfiSignature {
    sig: ir::Signature,
}

// ---------------------------------------------------------------------------
// Type ID mapping
//
// Fuse code passes integer type IDs across FFI. This maps them to
// Cranelift IR types.
//   0 = I8
//   1 = I32
//   2 = I64
//   3 = F64
//   4 = pointer (same as I64 on 64-bit)
// ---------------------------------------------------------------------------

fn type_from_id(id: i64, pointer_type: ir::Type) -> ir::Type {
    match id {
        0 => types::I8,
        1 => types::I32,
        2 => types::I64,
        3 => types::F64,
        4 => pointer_type,
        _ => pointer_type,
    }
}

// ---------------------------------------------------------------------------
// Linkage mapping
//   0 = Import
//   1 = Local
//   2 = Export
// ---------------------------------------------------------------------------

fn linkage_from_id(id: i64) -> Linkage {
    match id {
        0 => Linkage::Import,
        1 => Linkage::Local,
        2 => Linkage::Export,
        _ => Linkage::Local,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

unsafe fn str_from_raw<'a>(ptr: FuseHandle, len: FuseHandle) -> &'a str {
    // ptr is a FuseHandle wrapping the raw pointer address as Int
    let p = to_i64(ptr) as *const u8;
    let n = to_i64(len) as usize;
    let bytes = unsafe { slice::from_raw_parts(p, n) };
    std::str::from_utf8(bytes).unwrap_or("<invalid-utf8>")
}

/// Extract a raw i64 from a FuseHandle wrapping an Int.
fn to_i64(handle: FuseHandle) -> i64 {
    fuse_runtime::extract_int(handle)
}

/// Box an i64 as a FuseHandle wrapping an Int.
fn from_i64(value: i64) -> FuseHandle {
    unsafe { fuse_int(value) }
}

/// Wrap a raw pointer as a FuseHandle<Int> (storing the address as i64).
/// This ensures the pointer survives in the Fuse runtime without being
/// dereferenced as a FuseValue.
fn from_ptr<T>(ptr: *mut T) -> FuseHandle {
    from_i64(ptr as i64)
}

/// Extract a raw pointer from a FuseHandle<Int>.
fn to_ptr<T>(handle: FuseHandle) -> *mut T {
    to_i64(handle) as *mut T
}

/// Return a unit FuseHandle (for void-returning functions).
fn unit() -> FuseHandle {
    unsafe { fuse_unit() }
}

// =========================================================================
// Phase W0.1 — Module & Context Management
// =========================================================================

/// Return FFI version. Callers can check compatibility.
#[unsafe(no_mangle)]
pub extern "C" fn cranelift_ffi_version() -> FuseHandle {
    from_i64(2)
}

/// Create a new ObjectModule targeting the host native ISA.
/// Returns an opaque module handle as FuseHandle<Int> (address).
#[unsafe(no_mangle)]
pub extern "C" fn cranelift_ffi_module_new() -> FuseHandle {
    let isa_builder = cranelift_native::builder().expect("native ISA available");
    let isa = isa_builder
        .finish(settings::Flags::new(settings::builder()))
        .expect("native ISA flags");
    let pointer_type = isa.pointer_type();
    let builder =
        ObjectBuilder::new(isa, "fuse_stage2", default_libcall_names()).expect("object builder");
    let module = Box::new(FfiModule {
        module: ObjectModule::new(builder),
        pointer_type,
        func_ids: Vec::new(),
        data_ids: Vec::new(),
    });
    from_ptr(Box::into_raw(module))
}

/// Destroy a module handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_module_free(module: FuseHandle) -> FuseHandle {
    let ptr: *mut FfiModule = to_ptr(module);
    if !ptr.is_null() {
        unsafe { drop(Box::from_raw(ptr)) };
    }
    unit()
}

/// Create a new codegen Context for building one function.
#[unsafe(no_mangle)]
pub extern "C" fn cranelift_ffi_context_new() -> FuseHandle {
    let ctx = Box::new(FfiContext {
        ctx: cranelift_codegen::Context::new(),
    });
    from_ptr(Box::into_raw(ctx))
}

/// Destroy a context handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_context_free(ctx: FuseHandle) -> FuseHandle {
    let ptr: *mut FfiContext = to_ptr(ctx);
    if !ptr.is_null() {
        unsafe { drop(Box::from_raw(ptr)) };
    }
    unit()
}

/// Return the pointer type width in bytes (8 on 64-bit).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_module_target_pointer_type(
    module: FuseHandle,
) -> FuseHandle {
    let m = unsafe { &*to_ptr::<FfiModule>(module) };
    from_i64(m.pointer_type.bytes() as i64)
}

/// Declare a function in the module. Returns an integer ID (as FuseHandle<Int>).
///
/// `name_ptr` — raw pointer to UTF-8 name bytes (Ptr).
/// `name_len` — length as FuseHandle<Int>.
/// `sig` — signature handle (Ptr).
/// `linkage` — 0=Import, 1=Local, 2=Export (as FuseHandle<Int>).
///
/// Returns function integer ID (>= 0) on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_module_declare_function(
    module: FuseHandle,
    name_ptr: FuseHandle,
    name_len: FuseHandle,
    sig: FuseHandle,
    linkage: FuseHandle,
) -> FuseHandle {
    let m = unsafe { &mut *to_ptr::<FfiModule>(module) };
    let name = unsafe { str_from_raw(name_ptr, name_len) };
    let s = unsafe { &*to_ptr::<FfiSignature>(sig) };
    let link = linkage_from_id(to_i64(linkage));
    match m.module.declare_function(name, link, &s.sig) {
        Ok(func_id) => {
            // Check if this func_id is already tracked.
            if let Some(idx) = m.func_ids.iter().position(|id| *id == func_id) {
                return from_i64(idx as i64);
            }
            let idx = m.func_ids.len();
            m.func_ids.push(func_id);
            from_i64(idx as i64)
        }
        Err(_) => from_i64(-1),
    }
}

/// Define a function body. The context must have been populated by a
/// FunctionBuilder (see W0.3). Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_module_define_function(
    module: FuseHandle,
    func_id_index: FuseHandle,
    ctx: FuseHandle,
) -> FuseHandle {
    let m = unsafe { &mut *to_ptr::<FfiModule>(module) };
    let c = unsafe { &mut *to_ptr::<FfiContext>(ctx) };
    let idx = to_i64(func_id_index) as usize;
    let func_id = match m.func_ids.get(idx) {
        Some(id) => *id,
        None => return from_i64(-1),
    };
    match m.module.define_function(func_id, &mut c.ctx) {
        Ok(_) => from_i64(0),
        Err(_) => from_i64(-1),
    }
}

/// Finalize the module and write the object file to disk.
///
/// `path_ptr` — raw pointer to UTF-8 path bytes (Ptr).
/// `path_len` — length as FuseHandle<Int>.
///
/// Returns 0 on success, -1 on error.
/// **Consumes the module** — the handle is invalid after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_module_finish(
    module: FuseHandle,
    path_ptr: FuseHandle,
    path_len: FuseHandle,
) -> FuseHandle {
    let m = unsafe { Box::from_raw(to_ptr::<FfiModule>(module)) };
    let path_str = unsafe { str_from_raw(path_ptr, path_len) };
    let product = m.module.finish();
    let bytes = match product.emit() {
        Ok(bytes) => bytes,
        Err(_) => return from_i64(-1),
    };
    if let Some(parent) = Path::new(path_str).parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::write(path_str, &bytes) {
        Ok(_) => from_i64(0),
        Err(_) => from_i64(-1),
    }
}

// =========================================================================
// Phase W0.2 — Signature & Type Building
// =========================================================================

/// Create a new function signature.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_signature_new(
    module: FuseHandle,
) -> FuseHandle {
    let m = unsafe { &*to_ptr::<FfiModule>(module) };
    from_ptr(Box::into_raw(Box::new(FfiSignature {
        sig: m.module.make_signature(),
    })))
}

/// Destroy a signature handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_signature_free(sig: FuseHandle) -> FuseHandle {
    let ptr: *mut FfiSignature = to_ptr(sig);
    if !ptr.is_null() {
        unsafe { drop(Box::from_raw(ptr)) };
    }
    unit()
}

/// Add a parameter type to the signature.
/// `type_id` — 0=I8, 1=I32, 2=I64, 3=F64, 4=pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_signature_add_param(
    sig: FuseHandle,
    type_id: FuseHandle,
    module: FuseHandle,
) -> FuseHandle {
    let s = unsafe { &mut *to_ptr::<FfiSignature>(sig) };
    let m = unsafe { &*to_ptr::<FfiModule>(module) };
    s.sig.params.push(AbiParam::new(type_from_id(to_i64(type_id), m.pointer_type)));
    unit()
}

/// Add a return type to the signature.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_signature_add_return(
    sig: FuseHandle,
    type_id: FuseHandle,
    module: FuseHandle,
) -> FuseHandle {
    let s = unsafe { &mut *to_ptr::<FfiSignature>(sig) };
    let m = unsafe { &*to_ptr::<FfiModule>(module) };
    s.sig.returns.push(AbiParam::new(type_from_id(to_i64(type_id), m.pointer_type)));
    unit()
}

/// Clone a signature for reuse.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_signature_clone(
    sig: FuseHandle,
) -> FuseHandle {
    let s = unsafe { &*to_ptr::<FfiSignature>(sig) };
    from_ptr(Box::into_raw(Box::new(FfiSignature {
        sig: s.sig.clone(),
    })))
}
