use cranelift_codegen::ir::{types, AbiParam, InstBuilder, Signature, UserFuncName};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, Linkage, Module};

pub fn backend_name() -> &'static str {
    "cranelift"
}

pub fn run_host_entry(entry: extern "C" fn() -> i32) -> Result<i32, String> {
    let isa_builder = cranelift_native::builder().map_err(|error| error.to_string())?;
    let isa = isa_builder
        .finish(cranelift_codegen::settings::Flags::new(
            cranelift_codegen::settings::builder(),
        ))
        .map_err(|error| error.to_string())?;
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    builder.symbol("fusec_host_entry", entry as *const u8);
    let mut module = JITModule::new(builder);

    let mut host_sig = Signature::new(CallConv::SystemV);
    host_sig.returns.push(AbiParam::new(types::I32));
    let host_id = module
        .declare_function("fusec_host_entry", Linkage::Import, &host_sig)
        .map_err(|error| error.to_string())?;

    let mut sig = module.make_signature();
    sig.returns.push(AbiParam::new(types::I32));
    let thunk_id = module
        .declare_function("fusec_backend_entry", Linkage::Export, &sig)
        .map_err(|error| error.to_string())?;

    let mut ctx = module.make_context();
    ctx.func.signature = sig;
    ctx.func.name = UserFuncName::user(0, thunk_id.as_u32());
    let mut builder_ctx = FunctionBuilderContext::new();
    {
        let mut function = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let block = function.create_block();
        function.switch_to_block(block);
        function.seal_block(block);
        let local = module.declare_func_in_func(host_id, function.func);
        let call = function.ins().call(local, &[]);
        let result = function.inst_results(call)[0];
        function.ins().return_(&[result]);
        function.finalize();
    }

    module
        .define_function(thunk_id, &mut ctx)
        .map_err(|error| error.to_string())?;
    module.clear_context(&mut ctx);
    module.finalize_definitions().map_err(|error| error.to_string())?;

    let code = module.get_finalized_function(thunk_id);
    let thunk: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    Ok(thunk())
}
