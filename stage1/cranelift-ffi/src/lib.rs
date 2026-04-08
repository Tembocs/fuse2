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

use cranelift_codegen::ir::{self, types, AbiParam, Block};
use cranelift_codegen::settings;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{default_libcall_names, FuncId, Linkage, Module};
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

/// Wraps a FunctionBuilder for building a single function's body.
///
/// This is a self-referential struct: `builder` borrows from `builder_ctx`
/// and from the `FfiContext.ctx.func` passed at creation. The caller must
/// ensure the `FfiContext` outlives this `FfiBuilder`.
///
/// `entry_block` caches the entry block created during `builder_new`.
pub struct FfiBuilder {
    /// Owned FunctionBuilderContext — must not move after builder is created.
    builder_ctx: *mut FunctionBuilderContext,
    /// The FunctionBuilder with erased lifetime (borrows builder_ctx + ctx.func).
    builder: *mut FunctionBuilder<'static>,
    /// The entry block, created during new().
    entry_block: Block,
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
///
/// `call_conv` — calling convention as FuseHandle<Int>:
///   0 = default (module's native convention, typically SystemV or WindowsFastcall)
///
/// Currently only the default convention is used. The parameter exists for
/// future extensibility (e.g. tail-call convention).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_signature_new(
    module: FuseHandle,
    _call_conv: FuseHandle,
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

// =========================================================================
// Phase W0.3 — Function Builder & Blocks
// =========================================================================

/// Create a FunctionBuilder from a context and signature.
///
/// Sets up the context's function with the given signature and creates an
/// entry block with parameters matching the signature. The caller must keep
/// `ctx` alive until after `builder_finalize` or `builder_free`.
///
/// Returns an opaque builder handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_builder_new(
    module: FuseHandle,
    ctx: FuseHandle,
    sig: FuseHandle,
) -> FuseHandle {
    let _m = unsafe { &*to_ptr::<FfiModule>(module) };
    let c = unsafe { &mut *to_ptr::<FfiContext>(ctx) };
    let s = unsafe { &*to_ptr::<FfiSignature>(sig) };

    // Set up the function in the context with the given signature.
    c.ctx.func = ir::Function::with_name_signature(
        ir::UserFuncName::default(),
        s.sig.clone(),
    );

    // Heap-allocate the FunctionBuilderContext so it stays pinned.
    let builder_ctx = Box::into_raw(Box::new(FunctionBuilderContext::new()));

    // Create FunctionBuilder borrowing ctx.func and builder_ctx.
    // Safety: we erase lifetimes — the caller must keep `ctx` alive until
    // finalize/free, and we drop builder before builder_ctx in free().
    let builder_ref: &'static mut FunctionBuilderContext =
        unsafe { &mut *builder_ctx };
    let func_ref: &'static mut ir::Function =
        unsafe { std::mem::transmute(&mut c.ctx.func) };
    let mut builder = Box::new(FunctionBuilder::new(func_ref, builder_ref));

    // Create entry block with function params.
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    let builder_ptr = Box::into_raw(builder);
    from_ptr(Box::into_raw(Box::new(FfiBuilder {
        builder_ctx,
        builder: builder_ptr,
        entry_block: entry,
    })))
}

/// Finalize and destroy a builder.
///
/// Calls `builder.finalize()` then drops builder and builder context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_builder_free(
    bld: FuseHandle,
) -> FuseHandle {
    let ptr = to_ptr::<FfiBuilder>(bld);
    if !ptr.is_null() {
        let ffi_bld = unsafe { Box::from_raw(ptr) };
        // Drop builder first (it borrows builder_ctx).
        if !ffi_bld.builder.is_null() {
            let mut builder = unsafe { Box::from_raw(ffi_bld.builder) };
            builder.finalize();
        }
        // Then drop the builder context.
        if !ffi_bld.builder_ctx.is_null() {
            unsafe { drop(Box::from_raw(ffi_bld.builder_ctx)) };
        }
    }
    unit()
}

/// Create a new block. Returns block id as integer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_builder_create_block(
    bld: FuseHandle,
) -> FuseHandle {
    let ffi_bld = unsafe { &*to_ptr::<FfiBuilder>(bld) };
    let builder = unsafe { &mut *ffi_bld.builder };
    let block = builder.create_block();
    from_i64(block.as_u32() as i64)
}

/// Switch the builder's current block.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_builder_switch_to_block(
    bld: FuseHandle,
    block: FuseHandle,
) -> FuseHandle {
    let ffi_bld = unsafe { &*to_ptr::<FfiBuilder>(bld) };
    let builder = unsafe { &mut *ffi_bld.builder };
    builder.switch_to_block(Block::from_u32(to_i64(block) as u32));
    unit()
}

/// Seal a block (declare that no more predecessors will be added).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_builder_seal_block(
    bld: FuseHandle,
    block: FuseHandle,
) -> FuseHandle {
    let ffi_bld = unsafe { &*to_ptr::<FfiBuilder>(bld) };
    let builder = unsafe { &mut *ffi_bld.builder };
    builder.seal_block(Block::from_u32(to_i64(block) as u32));
    unit()
}

/// Seal all blocks at once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_builder_seal_all_blocks(
    bld: FuseHandle,
) -> FuseHandle {
    let ffi_bld = unsafe { &*to_ptr::<FfiBuilder>(bld) };
    let builder = unsafe { &mut *ffi_bld.builder };
    builder.seal_all_blocks();
    unit()
}

/// Append a block parameter. Returns the Value id as integer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_builder_append_block_param(
    bld: FuseHandle,
    block: FuseHandle,
    type_id: FuseHandle,
    module: FuseHandle,
) -> FuseHandle {
    let ffi_bld = unsafe { &*to_ptr::<FfiBuilder>(bld) };
    let m = unsafe { &*to_ptr::<FfiModule>(module) };
    let builder = unsafe { &mut *ffi_bld.builder };
    let ty = type_from_id(to_i64(type_id), m.pointer_type);
    let val = builder.append_block_param(Block::from_u32(to_i64(block) as u32), ty);
    from_i64(val.as_u32() as i64)
}

/// Fill `out` array with the Value ids for a block's parameters.
/// Returns the number of values written (capped at `max`).
///
/// `out` — pointer to an array of i64 (FuseHandle-sized slots).
/// `max` — maximum number of entries to write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_builder_block_params(
    bld: FuseHandle,
    block: FuseHandle,
    out: FuseHandle,
    max: FuseHandle,
) -> FuseHandle {
    let ffi_bld = unsafe { &*to_ptr::<FfiBuilder>(bld) };
    let builder = unsafe { &*ffi_bld.builder };
    let params = builder.block_params(Block::from_u32(to_i64(block) as u32));
    let max_n = to_i64(max) as usize;
    let out_ptr = to_i64(out) as *mut i64;
    let count = params.len().min(max_n);
    for i in 0..count {
        unsafe { *out_ptr.add(i) = params[i].as_u32() as i64 };
    }
    from_i64(count as i64)
}

/// Return the entry block id (created during builder_new).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_builder_entry_block(
    bld: FuseHandle,
) -> FuseHandle {
    let ffi_bld = unsafe { &*to_ptr::<FfiBuilder>(bld) };
    from_i64(ffi_bld.entry_block.as_u32() as i64)
}

/// Finalize the builder without destroying it.
///
/// After this call, the context is ready for `module_define_function`.
/// The builder handle must still be freed with `builder_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_builder_finalize(
    bld: FuseHandle,
) -> FuseHandle {
    let ffi_bld = unsafe { &mut *to_ptr::<FfiBuilder>(bld) };
    if !ffi_bld.builder.is_null() {
        let builder = unsafe { Box::from_raw(ffi_bld.builder) };
        builder.finalize();
        // Mark as null so builder_free doesn't double-finalize.
        ffi_bld.builder = std::ptr::null_mut();
    }
    unit()
}

/// Import a module-level function into the current function for calling.
/// Returns a FuncRef id as integer.
///
/// `func_id` — the integer index returned by `module_declare_function`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_builder_declare_func_in_func(
    bld: FuseHandle,
    module: FuseHandle,
    func_id: FuseHandle,
) -> FuseHandle {
    let ffi_bld = unsafe { &*to_ptr::<FfiBuilder>(bld) };
    let m = unsafe { &mut *to_ptr::<FfiModule>(module) };
    let builder = unsafe { &mut *ffi_bld.builder };
    let idx = to_i64(func_id) as usize;
    let fid = match m.func_ids.get(idx) {
        Some(id) => *id,
        None => return from_i64(-1),
    };
    let func_ref = m.module.declare_func_in_func(fid, builder.func);
    from_i64(func_ref.as_u32() as i64)
}

/// Fill `out` array with the result Value ids from an instruction.
/// Returns the number of values written (capped at `max`).
///
/// `inst` — the Inst id returned by a call/instruction function.
/// `out` — pointer to an array of i64 slots.
/// `max` — maximum entries to write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cranelift_ffi_builder_inst_results(
    bld: FuseHandle,
    inst: FuseHandle,
    out: FuseHandle,
    max: FuseHandle,
) -> FuseHandle {
    let ffi_bld = unsafe { &*to_ptr::<FfiBuilder>(bld) };
    let builder = unsafe { &*ffi_bld.builder };
    let inst_id = ir::Inst::from_u32(to_i64(inst) as u32);
    let results = builder.inst_results(inst_id);
    let max_n = to_i64(max) as usize;
    let out_ptr = to_i64(out) as *mut i64;
    let count = results.len().min(max_n);
    for i in 0..count {
        unsafe { *out_ptr.add(i) = results[i].as_u32() as i64 };
    }
    from_i64(count as i64)
}
