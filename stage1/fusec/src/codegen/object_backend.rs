use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{types, AbiParam, BlockArg, Function, InstBuilder, UserFuncName, Value};
use cranelift_codegen::settings;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{default_libcall_names, DataDescription, FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::ast::nodes as fa;
use crate::autogen;
use crate::common::{repo_root, resolve_import_path};
use crate::hir::lower_program;
use crate::parser::parse_source;

use super::layout::{self, ProgramLayout};
use super::type_names::{chan_inner_type, option_inner_type, result_err_type, result_ok_type, shared_inner_type, split_tuple_types};

pub fn backend_name() -> &'static str {
    "cranelift-object"
}

/// Map well-known stdlib interface names to their parent interfaces.
fn stdlib_interface_parents(name: &str) -> &'static [&'static str] {
    match name {
        "Hashable" => &["Equatable"],
        "Comparable" => &["Equatable"],
        "Debuggable" => &["Printable"],
        _ => &[],
    }
}

/// Collect an interface and all its ancestor stdlib interfaces.
fn collect_interface_hierarchy(name: &str, out: &mut Vec<String>) {
    if out.iter().any(|n| n == name) {
        return;
    }
    out.push(name.to_string());
    for parent in stdlib_interface_parents(name) {
        collect_interface_hierarchy(parent, out);
    }
}

/// Map well-known stdlib interface names to their module paths.
fn stdlib_interface_module(name: &str) -> Option<&'static str> {
    match name {
        "Equatable" => Some("core.equatable"),
        "Hashable" => Some("core.hashable"),
        "Comparable" => Some("core.comparable"),
        "Printable" => Some("core.printable"),
        "Debuggable" => Some("core.debuggable"),
        _ => None,
    }
}

/// Choose the correct linker symbol for a function declaration.
/// Extension methods include the receiver type to avoid collisions with
/// free functions of the same name in the same module.
/// `@export("name")` overrides the symbol to the given name.
fn symbol_for_function(module_path: &Path, function: &fa::FunctionDecl) -> String {
    if let Some(export) = function.annotations.iter().find(|a| a.is("export")) {
        if let Some(name) = export.string_arg(0) {
            return name.to_string();
        }
    }
    if let Some(receiver) = &function.receiver_type {
        layout::extension_symbol(module_path, receiver, &function.name)
    } else {
        layout::function_symbol(module_path, &function.name)
    }
}

/// Determine linkage for a function. `@export` uses Export linkage.
fn linkage_for_function(function: &fa::FunctionDecl) -> Linkage {
    if function.annotations.iter().any(|a| a.is("export")) {
        Linkage::Export
    } else {
        Linkage::Local
    }
}

/// Split a block's statements into prefix statements and an optional trailing
/// expression.  When the last statement is `Statement::Expr`, the expression is
/// extracted so it can be compiled separately as a block's return value — the
/// same pattern used for function body final-expression extraction (line ~547).
fn split_block_final_expr(stmts: &[fa::Statement]) -> (&[fa::Statement], Option<&fa::Expr>) {
    match stmts.split_last() {
        Some((fa::Statement::Expr(expr_stmt), prefix)) => (prefix, Some(&expr_stmt.expr)),
        _ => (stmts, None),
    }
}

pub fn run_host_entry(entry: extern "C" fn() -> i32) -> Result<i32, String> {
    Ok(entry())
}

pub fn compile_path_to_native(input: &Path, output: &Path) -> Result<(), String> {
    let input = input.canonicalize().unwrap_or_else(|_| input.to_path_buf());
    let session = BuildSession::load(&input)?;
    let object = BackendCompiler::new_native(&session)?.emit_object()?;
    build_wrapper(&input, output, &object)
}

pub fn compile_path_to_ir_text(input: &Path) -> Result<String, String> {
    let input = input.canonicalize().unwrap_or_else(|_| input.to_path_buf());
    let session = BuildSession::load(&input)?;
    BackendCompiler::new_native(&session)?.collect_ir_text()
}

#[derive(Clone)]
struct LoadedModule {
    path: PathBuf,
    imports: Vec<fa::ImportDecl>,
    functions: HashMap<String, fa::FunctionDecl>,
    extensions: HashMap<(String, String), fa::FunctionDecl>,
    statics: HashMap<(String, String), fa::FunctionDecl>,
    data_classes: HashMap<String, fa::DataClassDecl>,
    structs: HashMap<String, fa::StructDecl>,
    enums: HashMap<String, fa::EnumDecl>,
    extern_fns: HashMap<String, fa::ExternFnDecl>,
    consts: HashMap<(String, String), fa::ConstDecl>,
    interface_names: HashSet<String>,
}

struct BuildSession {
    root_path: PathBuf,
    modules: HashMap<PathBuf, LoadedModule>,
    layout: ProgramLayout,
}

impl BuildSession {
    fn load(root_path: &Path) -> Result<Self, String> {
        let mut modules = HashMap::new();
        load_module_recursive(root_path, &mut modules)?;
        let layout = ProgramLayout::new(
            modules
                .values()
                .flat_map(|module| module.data_classes.values().cloned()),
            modules
                .values()
                .flat_map(|module| module.structs.values().cloned()),
        );
        Ok(Self {
            root_path: root_path.to_path_buf(),
            modules,
            layout,
        })
    }

    fn entry_function(&self) -> Result<fa::FunctionDecl, String> {
        self.modules
            .get(&self.root_path)
            .and_then(|module| {
                module
                    .functions
                    .values()
                    .find(|function| {
                        function
                            .annotations
                            .iter()
                            .any(|a| a.is("entrypoint"))
                    })
                    .cloned()
            })
            .ok_or_else(|| format!("missing @entrypoint in `{}`", self.root_path.display()))
    }

    fn resolve_function(&self, name: &str) -> Option<(&Path, &fa::FunctionDecl)> {
        self.modules.values().find_map(|module| {
            module
                .functions
                .get(name)
                .map(|function| (module.path.as_path(), function))
        })
    }

    fn resolve_extension(&self, receiver_type: &str, name: &str) -> Option<(&Path, &fa::FunctionDecl)> {
        let key = (
            layout::canonical_type_name(receiver_type).to_string(),
            name.to_string(),
        );
        self.modules.values().find_map(|module| {
            module
                .extensions
                .get(&key)
                .map(|function| (module.path.as_path(), function))
        })
    }

    fn resolve_static(&self, type_name: &str, name: &str) -> Option<(&Path, &fa::FunctionDecl)> {
        let key = (
            layout::canonical_type_name(type_name).to_string(),
            name.to_string(),
        );
        self.modules.values().find_map(|module| {
            module
                .statics
                .get(&key)
                .map(|function| (module.path.as_path(), function))
        })
    }

    fn resolve_module_function(
        &self,
        calling_module: &Path,
        alias: &str,
        fn_name: &str,
    ) -> Option<(&Path, &fa::FunctionDecl)> {
        let calling = self.modules.get(calling_module)?;
        for import in &calling.imports {
            let import_alias = import.module_path.split('.').next_back()?;
            if import_alias == alias {
                let target = resolve_import_path(calling_module, &import.module_path)?;
                let target = target.canonicalize().unwrap_or(target);
                let target_module = self.modules.get(&target)?;
                if let Some(function) = target_module.functions.get(fn_name) {
                    return Some((target_module.path.as_path(), function));
                }
            }
        }
        None
    }

    fn find_data(&self, type_name: &str) -> Option<(&Path, &fa::DataClassDecl)> {
        let key = layout::canonical_type_name(type_name);
        self.modules.values().find_map(|module| {
            module
                .data_classes
                .get(key)
                .map(|data| (module.path.as_path(), data))
        })
    }

    fn find_struct(&self, type_name: &str) -> Option<(&Path, &fa::StructDecl)> {
        let key = layout::canonical_type_name(type_name);
        self.modules.values().find_map(|module| {
            module
                .structs
                .get(key)
                .map(|s| (module.path.as_path(), s))
        })
    }

    fn find_const(&self, owner: &str, name: &str) -> Option<&fa::ConstDecl> {
        let key = (owner.to_string(), name.to_string());
        self.modules.values().find_map(|module| module.consts.get(&key))
    }

    fn find_extern_fn(&self, name: &str) -> Option<&fa::ExternFnDecl> {
        self.modules.values().find_map(|module| module.extern_fns.get(name))
    }

    fn find_enum(&self, type_name: &str) -> Option<&fa::EnumDecl> {
        let key = layout::canonical_type_name(type_name);
        self.modules.values().find_map(|module| module.enums.get(key))
    }

    fn field_type(&self, type_name: &str, field: &str) -> Option<String> {
        if let Some((_, data)) = self.find_data(type_name) {
            return data.fields.iter().find(|f| f.name == field).and_then(|f| f.type_name.clone());
        }
        if let Some((_, s)) = self.find_struct(type_name) {
            return s.fields.iter().find(|f| f.name == field).and_then(|f| f.type_name.clone());
        }
        None
    }
}

fn load_module_recursive(
    path: &Path,
    modules: &mut HashMap<PathBuf, LoadedModule>,
) -> Result<(), String> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if modules.contains_key(&path) {
        return Ok(());
    }
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?;
    let filename = display_name(&path);
    let parsed = parse_source(&source, &filename).map_err(|diag| diag.render())?;
    let module = lower_program(&parsed, path.clone());
    // Resolve `Self` → concrete type in return types and parameter types,
    // then split extension functions into instance methods (first param is
    // self) and static functions (no self).
    let mut extensions = HashMap::new();
    let mut statics = HashMap::new();
    for ((receiver_type, name), mut function) in module.extension_functions.clone() {
        if let Some(ref mut rt) = function.return_type {
            if rt.contains("Self") {
                *rt = rt.replace("Self", &receiver_type);
            }
        }
        for param in function.params.iter_mut() {
            if let Some(ref mut tn) = param.type_name {
                if tn.contains("Self") {
                    *tn = tn.replace("Self", &receiver_type);
                }
            }
        }
        let is_static = function.params.first().map_or(true, |p| p.name != "self");
        if is_static {
            statics.insert((receiver_type, name), function);
        } else {
            extensions.insert((receiver_type, name), function);
        }
    }
    let mut data_classes: HashMap<String, fa::DataClassDecl> = module
        .data_classes
        .iter()
        .map(|data| (data.name.clone(), data.clone()))
        .collect();
    for (name, data) in data_classes.iter_mut() {
        for method in data.methods.iter_mut() {
            if let Some(ref mut rt) = method.return_type {
                if rt.contains("Self") {
                    *rt = rt.replace("Self", name);
                }
            }
            for param in method.params.iter_mut() {
                if let Some(ref mut tn) = param.type_name {
                    if tn.contains("Self") {
                        *tn = tn.replace("Self", name);
                    }
                }
            }
        }
    }
    // Register struct methods as extensions/statics so member calls resolve.
    for s in &module.structs {
        for method in &s.methods {
            let mut function = method.clone();
            function.receiver_type = Some(s.name.clone());
            if let Some(ref mut rt) = function.return_type {
                if rt.contains("Self") {
                    *rt = rt.replace("Self", &s.name);
                }
            }
            for param in function.params.iter_mut() {
                if let Some(ref mut tn) = param.type_name {
                    if tn.contains("Self") {
                        *tn = tn.replace("Self", &s.name);
                    }
                }
            }
            let is_static = function.params.first().map_or(true, |p| p.name != "self");
            let key = (s.name.clone(), method.name.clone());
            if is_static {
                statics.insert(key, function);
            } else {
                extensions.insert(key, function);
            }
        }
    }
    // Register data class methods as extensions so member calls resolve.
    for data in &module.data_classes {
        for method in &data.methods {
            if method.name == "__del__" {
                continue; // destructors are called through the bridge, not member calls
            }
            let mut function = method.clone();
            function.receiver_type = Some(data.name.clone());
            let is_static = function.params.first().map_or(true, |p| p.name != "self");
            let key = (data.name.clone(), method.name.clone());
            if is_static {
                statics.insert(key, function);
            } else {
                extensions.insert(key, function);
            }
        }
    }
    // Inject default method forwarding: for each type that implements an
    // interface, if the type doesn't have its own extension for a default
    // method, create a synthetic extension entry pointing to the default body.
    let interface_names: HashSet<String> = module.interfaces.iter().map(|i| i.name.clone()).collect();
    // Collect default methods: extension functions whose receiver is an interface name.
    let mut defaults: HashMap<String, Vec<fa::FunctionDecl>> = HashMap::new();
    for ((receiver_type, _name), function) in extensions.iter().chain(statics.iter()) {
        if interface_names.contains(receiver_type) {
            defaults.entry(receiver_type.clone()).or_default().push(function.clone());
        }
    }
    // Collect (type_name, interface_list) pairs from data classes, enums, and structs.
    let implementors: Vec<(String, Vec<String>)> = module.data_classes.iter()
        .filter(|d| !d.implements.is_empty())
        .map(|d| (d.name.clone(), d.implements.clone()))
        .chain(module.enums.iter()
            .filter(|e| !e.implements.is_empty())
            .map(|e| (e.name.clone(), e.implements.clone())))
        .chain(module.structs.iter()
            .filter(|s| !s.implements.is_empty())
            .map(|s| (s.name.clone(), s.implements.clone())))
        .collect();
    for (type_name, iface_names) in &implementors {
        for iface_name in iface_names {
            if let Some(default_fns) = defaults.get(iface_name) {
                for default_fn in default_fns {
                    let key = (type_name.clone(), default_fn.name.clone());
                    if !extensions.contains_key(&key) {
                        let mut forwarded = default_fn.clone();
                        forwarded.receiver_type = Some(type_name.clone());
                        extensions.insert(key, forwarded);
                    }
                }
            }
        }
    }
    // Auto-generate interface methods from field metadata for types that
    // declare `implements` but don't provide the method manually.
    for (type_name, iface_names) in &implementors {
        // Determine type kind and get fields.
        let (type_kind, fields) = if let Some(data) = data_classes.get(type_name) {
            let kind = autogen::classify_type(&data.annotations, false);
            (kind, data.fields.clone())
        } else if let Some(s) = module.structs.iter().find(|s| s.name == *type_name) {
            let kind = autogen::classify_type(&s.annotations, true);
            (kind, s.fields.clone())
        } else {
            continue; // enum auto-gen handled in Phase 6
        };
        // Collect all interfaces to generate for, including parents.
        let mut all_ifaces: Vec<String> = Vec::new();
        for iface_name in iface_names {
            collect_interface_hierarchy(iface_name, &mut all_ifaces);
        }
        for iface_name in &all_ifaces {
            if !autogen::can_auto_generate(type_kind, iface_name) {
                continue;
            }
            let generated: Vec<fa::FunctionDecl> = match iface_name.as_str() {
                "Equatable" => vec![autogen::generate_eq(type_name, &fields)],
                "Hashable" => vec![autogen::generate_hash(type_name, &fields)],
                "Comparable" => vec![autogen::generate_compare_to(type_name, &fields)],
                "Printable" => vec![autogen::generate_to_string(type_name, &fields)],
                "Debuggable" => vec![autogen::generate_debug_string(type_name, &fields)],
                _ => Vec::new(),
            };
            for func in generated {
                let key = (type_name.clone(), func.name.clone());
                if !extensions.contains_key(&key) {
                    extensions.insert(key, func);
                }
            }
        }
    }
    let loaded = LoadedModule {
        path: path.clone(),
        imports: module.imports.clone(),
        functions: module
            .functions
            .iter()
            .map(|function| (function.name.clone(), function.clone()))
            .collect(),
        extensions,
        statics,
        data_classes,
        structs: module
            .structs
            .iter()
            .map(|s| (s.name.clone(), s.clone()))
            .collect(),
        enums: module
            .enums
            .iter()
            .map(|e| (e.name.clone(), e.clone()))
            .collect(),
        extern_fns: module
            .extern_fns
            .iter()
            .map(|e| (e.name.clone(), e.clone()))
            .collect(),
        consts: module
            .consts
            .iter()
            .map(|c| ((c.owner.clone(), c.name.clone()), c.clone()))
            .collect(),
        interface_names,
    };
    modules.insert(path.clone(), loaded);
    for import in &module.imports {
        let target = resolve_import_path(&path, &import.module_path)
            .ok_or_else(|| format!("cannot resolve import `{}`", import.module_path))?;
        load_module_recursive(&target, modules)?;
    }
    // Auto-load stdlib interface modules referenced via `implements`,
    // including parent interfaces (e.g., Hashable → Equatable).
    for (_type_name, iface_names) in &implementors {
        let mut all_ifaces = Vec::new();
        for iface_name in iface_names {
            collect_interface_hierarchy(iface_name, &mut all_ifaces);
        }
        for iface_name in &all_ifaces {
            if let Some(module_path) = stdlib_interface_module(iface_name) {
                if let Some(target) = resolve_import_path(&path, module_path) {
                    if !modules.contains_key(&target) {
                        load_module_recursive(&target, modules)?;
                    }
                }
            }
        }
    }
    Ok(())
}

struct RuntimeFns {
    unit: FuncId,
    int: FuncId,
    float: FuncId,
    bool_: FuncId,
    string_new_utf8: FuncId,
    to_string: FuncId,
    concat: FuncId,
    add: FuncId,
    sub: FuncId,
    mul: FuncId,
    div: FuncId,
    mod_: FuncId,
    eq: FuncId,
    lt: FuncId,
    le: FuncId,
    gt: FuncId,
    ge: FuncId,
    truthy: FuncId,
    extract_int: FuncId,
    println: FuncId,
    none: FuncId,
    some: FuncId,
    option_is_some: FuncId,
    option_unwrap: FuncId,
    ok: FuncId,
    err: FuncId,
    result_is_ok: FuncId,
    result_unwrap: FuncId,
    list_new: FuncId,
    list_push: FuncId,
    list_len: FuncId,
    list_get: FuncId,
    list_get_handle: FuncId,
    rt_list_get: FuncId,
    chan_bounded: FuncId,
    chan_new: FuncId,
    chan_send: FuncId,
    chan_recv: FuncId,
    chan_try_recv: FuncId,
    chan_close: FuncId,
    chan_is_closed: FuncId,
    chan_len: FuncId,
    chan_cap: FuncId,
    shared_new: FuncId,
    shared_read: FuncId,
    shared_write: FuncId,
    shared_try_write: FuncId,
    shared_try_read: FuncId,
    simd_sum: FuncId,
    simd_dot: FuncId,
    simd_add: FuncId,
    simd_sub: FuncId,
    simd_mul: FuncId,
    simd_div: FuncId,
    simd_min: FuncId,
    simd_max: FuncId,
    simd_abs: FuncId,
    simd_sqrt: FuncId,
    simd_broadcast: FuncId,
    simd_get: FuncId,
    simd_len: FuncId,
    simd_extract_raw_f64: FuncId,
    simd_extract_raw_f32: FuncId,
    simd_extract_raw_i64: FuncId,
    simd_extract_raw_i32: FuncId,
    data_new: FuncId,
    data_set_field: FuncId,
    data_get_field: FuncId,
    release: FuncId,
    asap_release: FuncId,
    to_upper: FuncId,
    string_is_empty: FuncId,
    string_char_count: FuncId,
    enum_new: FuncId,
    enum_add_payload: FuncId,
    enum_tag: FuncId,
    enum_payload: FuncId,
    map_new: FuncId,
    map_set: FuncId,
    map_get: FuncId,
    rt_map_get: FuncId,
    map_remove: FuncId,
    map_len: FuncId,
    map_contains: FuncId,
    map_keys: FuncId,
    map_values: FuncId,
    map_entries: FuncId,
    panic: FuncId,
    test_assert_eq: FuncId,
    test_assert_ne: FuncId,
    test_assert_approx: FuncId,
    test_assert_panics: FuncId,
    f32_new: FuncId,
    f32_add: FuncId,
    f32_sub: FuncId,
    f32_mul: FuncId,
    f32_div: FuncId,
    f32_eq: FuncId,
    f32_lt: FuncId,
    f32_le: FuncId,
    f32_gt: FuncId,
    f32_ge: FuncId,
    f32_to_string: FuncId,
    i8: SizedIntFns,
    u8: SizedIntFns,
    i32: SizedIntFns,
    u32: SizedIntFns,
    u64: SizedIntFns,
}

struct SizedIntFns {
    new: FuncId,
    add: FuncId,
    sub: FuncId,
    mul: FuncId,
    div: FuncId,
    mod_: FuncId,
    eq: FuncId,
    lt: FuncId,
    le: FuncId,
    gt: FuncId,
    ge: FuncId,
    to_string: FuncId,
}

struct PendingLambda {
    module_path: PathBuf,
    decl: fa::FunctionDecl,
    captures: Vec<(String, Option<String>)>,
}

struct BackendCompiler<'a> {
    session: &'a BuildSession,
    module: ObjectModule,
    pointer_type: cranelift_codegen::ir::Type,
    runtime: RuntimeFns,
    function_ids: HashMap<String, FuncId>,
    destructor_ids: HashMap<String, FuncId>,
    string_ids: HashMap<String, cranelift_module::DataId>,
    lambda_counter: u32,
    pending_lambdas: Vec<PendingLambda>,
}

#[derive(Clone)]
struct LocalBinding {
    var: Variable,
    ty: Option<String>,
    destroy: bool,
}

#[derive(Clone)]
struct TypedValue {
    value: Value,
    ty: Option<String>,
}

struct LoopFrame {
    break_block: cranelift_codegen::ir::Block,
    continue_block: cranelift_codegen::ir::Block,
}

impl<'a> BackendCompiler<'a> {
    fn new_native(session: &'a BuildSession) -> Result<Self, String> {
        let isa_builder = cranelift_native::builder().map_err(|error| error.to_string())?;
        let isa = isa_builder
            .finish(settings::Flags::new(settings::builder()))
            .map_err(|error| error.to_string())?;
        let mut module = ObjectModule::new(
            ObjectBuilder::new(isa, "fuse_stage1", default_libcall_names())
                .map_err(|error| error.to_string())?,
        );
        let pointer_type = module.target_config().pointer_type();
        let runtime = declare_runtime_functions(&mut module, pointer_type)?;
        let mut compiler = Self {
            session,
            module,
            pointer_type,
            runtime,
            function_ids: HashMap::new(),
            destructor_ids: HashMap::new(),
            string_ids: HashMap::new(),
            lambda_counter: 0,
            pending_lambdas: Vec::new(),
        };
        // Pre-populate function_ids with runtime functions so that extern fn
        // declarations from stdlib modules find them and skip re-declaration.
        let rt = &compiler.runtime;
        for (name, id) in [
            ("fuse_list_new", rt.list_new), ("fuse_list_push", rt.list_push),
            ("fuse_list_len", rt.list_len), ("fuse_list_get", rt.list_get_handle),
            ("fuse_list_get_handle", rt.list_get_handle),
            ("fuse_rt_list_get", rt.rt_list_get),
            ("fuse_map_new", rt.map_new), ("fuse_map_set", rt.map_set),
            ("fuse_map_get", rt.map_get), ("fuse_rt_map_get", rt.rt_map_get),
            ("fuse_map_remove", rt.map_remove),
            ("fuse_map_len", rt.map_len), ("fuse_map_contains", rt.map_contains),
            ("fuse_map_keys", rt.map_keys), ("fuse_map_values", rt.map_values),
            ("fuse_map_entries", rt.map_entries),
            ("fuse_release", rt.release), ("fuse_asap_release", rt.asap_release),
            ("fuse_to_upper", rt.to_upper), ("fuse_string_is_empty", rt.string_is_empty),
            ("fuse_builtin_println", rt.println),
            ("fuse_rt_i8_new", rt.i8.new), ("fuse_rt_i8_add", rt.i8.add), ("fuse_rt_i8_sub", rt.i8.sub),
            ("fuse_rt_i8_mul", rt.i8.mul), ("fuse_rt_i8_div", rt.i8.div), ("fuse_rt_i8_mod", rt.i8.mod_),
            ("fuse_rt_i8_eq", rt.i8.eq), ("fuse_rt_i8_lt", rt.i8.lt), ("fuse_rt_i8_le", rt.i8.le),
            ("fuse_rt_i8_gt", rt.i8.gt), ("fuse_rt_i8_ge", rt.i8.ge), ("fuse_rt_i8_to_string", rt.i8.to_string),
            ("fuse_rt_u8_new", rt.u8.new), ("fuse_rt_u8_add", rt.u8.add), ("fuse_rt_u8_sub", rt.u8.sub),
            ("fuse_rt_u8_mul", rt.u8.mul), ("fuse_rt_u8_div", rt.u8.div), ("fuse_rt_u8_mod", rt.u8.mod_),
            ("fuse_rt_u8_eq", rt.u8.eq), ("fuse_rt_u8_lt", rt.u8.lt), ("fuse_rt_u8_le", rt.u8.le),
            ("fuse_rt_u8_gt", rt.u8.gt), ("fuse_rt_u8_ge", rt.u8.ge), ("fuse_rt_u8_to_string", rt.u8.to_string),
            ("fuse_rt_i32_new", rt.i32.new), ("fuse_rt_i32_add", rt.i32.add), ("fuse_rt_i32_sub", rt.i32.sub),
            ("fuse_rt_i32_mul", rt.i32.mul), ("fuse_rt_i32_div", rt.i32.div), ("fuse_rt_i32_mod", rt.i32.mod_),
            ("fuse_rt_i32_eq", rt.i32.eq), ("fuse_rt_i32_lt", rt.i32.lt), ("fuse_rt_i32_le", rt.i32.le),
            ("fuse_rt_i32_gt", rt.i32.gt), ("fuse_rt_i32_ge", rt.i32.ge), ("fuse_rt_i32_to_string", rt.i32.to_string),
            ("fuse_rt_u32_new", rt.u32.new), ("fuse_rt_u32_add", rt.u32.add), ("fuse_rt_u32_sub", rt.u32.sub),
            ("fuse_rt_u32_mul", rt.u32.mul), ("fuse_rt_u32_div", rt.u32.div), ("fuse_rt_u32_mod", rt.u32.mod_),
            ("fuse_rt_u32_eq", rt.u32.eq), ("fuse_rt_u32_lt", rt.u32.lt), ("fuse_rt_u32_le", rt.u32.le),
            ("fuse_rt_u32_gt", rt.u32.gt), ("fuse_rt_u32_ge", rt.u32.ge), ("fuse_rt_u32_to_string", rt.u32.to_string),
            ("fuse_rt_u64_new", rt.u64.new), ("fuse_rt_u64_add", rt.u64.add), ("fuse_rt_u64_sub", rt.u64.sub),
            ("fuse_rt_u64_mul", rt.u64.mul), ("fuse_rt_u64_div", rt.u64.div), ("fuse_rt_u64_mod", rt.u64.mod_),
            ("fuse_rt_u64_eq", rt.u64.eq), ("fuse_rt_u64_lt", rt.u64.lt), ("fuse_rt_u64_le", rt.u64.le),
            ("fuse_rt_u64_gt", rt.u64.gt), ("fuse_rt_u64_ge", rt.u64.ge), ("fuse_rt_u64_to_string", rt.u64.to_string),
        ] {
            compiler.function_ids.insert(name.to_string(), id);
        }
        compiler.declare_user_surface()?;
        Ok(compiler)
    }

    fn declare_user_surface(&mut self) -> Result<(), String> {
        for loaded in self.session.modules.values() {
            for function in loaded.functions.values() {
                let name = symbol_for_function(loaded.path.as_path(), function);
                let linkage = linkage_for_function(function);
                let func_id = self
                    .module
                    .declare_function(&name, linkage, &self.handle_signature(function.params.len()))
                    .map_err(|error| error.to_string())?;
                self.function_ids.insert(name, func_id);
            }
            for function in loaded.extensions.values() {
                // Skip interface-receiver extensions — they are only used as
                // default method bodies and have been forwarded to concrete types.
                if let Some(recv) = &function.receiver_type {
                    if loaded.interface_names.contains(recv) { continue; }
                }
                let name = symbol_for_function(loaded.path.as_path(), function);
                let linkage = linkage_for_function(function);
                let func_id = self
                    .module
                    .declare_function(&name, linkage, &self.handle_signature(function.params.len()))
                    .map_err(|error| error.to_string())?;
                self.function_ids.insert(name, func_id);
            }
            for function in loaded.statics.values() {
                if let Some(recv) = &function.receiver_type {
                    if loaded.interface_names.contains(recv) { continue; }
                }
                let name = symbol_for_function(loaded.path.as_path(), function);
                let linkage = linkage_for_function(function);
                let func_id = self
                    .module
                    .declare_function(&name, linkage, &self.handle_signature(function.params.len()))
                    .map_err(|error| error.to_string())?;
                self.function_ids.insert(name, func_id);
            }
            for data in loaded.data_classes.values() {
                for method in &data.methods {
                    let name = layout::function_symbol(loaded.path.as_path(), &method.name);
                    let func_id = self
                        .module
                        .declare_function(&name, Linkage::Local, &self.handle_signature(method.params.len()))
                        .map_err(|error| error.to_string())?;
                    self.function_ids.insert(name, func_id);
                }
                if data.methods.iter().any(|method| method.name == "__del__") {
                    let name = layout::destructor_symbol(loaded.path.as_path(), &data.name);
                    let func_id = self
                        .module
                        .declare_function(&name, Linkage::Local, &self.destructor_signature())
                        .map_err(|error| error.to_string())?;
                    self.destructor_ids.insert(name, func_id);
                }
            }
            for s in loaded.structs.values() {
                for method in &s.methods {
                    let name = layout::function_symbol(loaded.path.as_path(), &method.name);
                    let func_id = self
                        .module
                        .declare_function(&name, Linkage::Local, &self.handle_signature(method.params.len()))
                        .map_err(|error| error.to_string())?;
                    self.function_ids.insert(name, func_id);
                }
                if s.methods.iter().any(|method| method.name == "__del__") {
                    let name = layout::destructor_symbol(loaded.path.as_path(), &s.name);
                    let func_id = self
                        .module
                        .declare_function(&name, Linkage::Local, &self.destructor_signature())
                        .map_err(|error| error.to_string())?;
                    self.destructor_ids.insert(name, func_id);
                }
            }
            for extern_fn in loaded.extern_fns.values() {
                // Skip if already declared (runtime functions or another module).
                // Cranelift rejects duplicate declarations with incompatible
                // signatures, so we use try-declare-or-reuse.
                if self.function_ids.contains_key(&extern_fn.name) {
                    continue;
                }
                let sig = self.handle_signature(extern_fn.params.len());
                let func_id = match self.module.declare_function(&extern_fn.name, Linkage::Import, &sig) {
                    Ok(id) => id,
                    Err(_) => {
                        // Already declared by the runtime with a different
                        // signature (e.g. void-returning builtins).  Reuse the
                        // existing declaration via a re-declare with the
                        // runtime's own signature — Cranelift deduplicates when
                        // the signature matches.
                        continue;
                    }
                };
                self.function_ids.insert(extern_fn.name.clone(), func_id);
            }
        }
        self.module
            .declare_function(layout::ENTRY_SYMBOL, Linkage::Export, &self.entry_signature())
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn emit_object(mut self) -> Result<Vec<u8>, String> {
        for loaded in self.session.modules.values() {
            for function in loaded.functions.values() {
                self.compile_function(loaded.path.as_path(), function)?;
            }
            for function in loaded.extensions.values() {
                if let Some(recv) = &function.receiver_type {
                    if loaded.interface_names.contains(recv) { continue; }
                }
                let mut function = function.clone();
                if let Some(receiver_type) = &function.receiver_type {
                    if let Some(param) = function.params.first_mut() {
                        if param.name == "self" && param.type_name.is_none() {
                            param.type_name = Some(receiver_type.clone());
                        }
                    }
                }
                self.compile_function(loaded.path.as_path(), &function)?;
            }
            for function in loaded.statics.values() {
                if let Some(recv) = &function.receiver_type {
                    if loaded.interface_names.contains(recv) { continue; }
                }
                self.compile_function(loaded.path.as_path(), function)?;
            }
            for data in loaded.data_classes.values() {
                for method in &data.methods {
                    let mut method = method.clone();
                    if let Some(param) = method.params.first_mut() {
                        if param.name == "self" && param.type_name.is_none() {
                            param.type_name = Some(data.name.clone());
                        }
                    }
                    self.compile_function(loaded.path.as_path(), &method)?;
                }
                if data.methods.iter().any(|method| method.name == "__del__") {
                    self.compile_destructor(loaded.path.as_path(), data)?;
                }
            }
            for s in loaded.structs.values() {
                for method in &s.methods {
                    let mut method = method.clone();
                    if let Some(param) = method.params.first_mut() {
                        if param.name == "self" && param.type_name.is_none() {
                            param.type_name = Some(s.name.clone());
                        }
                    }
                    self.compile_function(loaded.path.as_path(), &method)?;
                }
                if s.methods.iter().any(|method| method.name == "__del__") {
                    self.compile_struct_destructor(loaded.path.as_path(), s)?;
                }
            }
        }
        self.compile_entry()?;
        let product = self.module.finish();
        product.emit().map_err(|error| error.to_string())
    }

    fn collect_ir_text(mut self) -> Result<String, String> {
        let mut ir_parts = Vec::new();
        for loaded in self.session.modules.values() {
            for function in loaded.functions.values() {
                if let Some(ir) = self.compile_function_to_ir(loaded.path.as_path(), function)? {
                    ir_parts.push(ir);
                }
            }
            for function in loaded.extensions.values() {
                if let Some(recv) = &function.receiver_type {
                    if loaded.interface_names.contains(recv) { continue; }
                }
                let mut function = function.clone();
                if let Some(receiver_type) = &function.receiver_type {
                    if let Some(param) = function.params.first_mut() {
                        if param.name == "self" && param.type_name.is_none() {
                            param.type_name = Some(receiver_type.clone());
                        }
                    }
                }
                if let Some(ir) = self.compile_function_to_ir(loaded.path.as_path(), &function)? {
                    ir_parts.push(ir);
                }
            }
            for function in loaded.statics.values() {
                if let Some(recv) = &function.receiver_type {
                    if loaded.interface_names.contains(recv) { continue; }
                }
                if let Some(ir) = self.compile_function_to_ir(loaded.path.as_path(), function)? {
                    ir_parts.push(ir);
                }
            }
            for data in loaded.data_classes.values() {
                for method in &data.methods {
                    let mut method = method.clone();
                    if let Some(param) = method.params.first_mut() {
                        if param.name == "self" && param.type_name.is_none() {
                            param.type_name = Some(data.name.clone());
                        }
                    }
                    if let Some(ir) = self.compile_function_to_ir(loaded.path.as_path(), &method)? {
                        ir_parts.push(ir);
                    }
                }
            }
            for s in loaded.structs.values() {
                for method in &s.methods {
                    let mut method = method.clone();
                    if let Some(param) = method.params.first_mut() {
                        if param.name == "self" && param.type_name.is_none() {
                            param.type_name = Some(s.name.clone());
                        }
                    }
                    if let Some(ir) = self.compile_function_to_ir(loaded.path.as_path(), &method)? {
                        ir_parts.push(ir);
                    }
                }
            }
        }
        Ok(ir_parts.join("\n"))
    }

    fn compile_function_to_ir(
        &mut self,
        module_path: &Path,
        function: &fa::FunctionDecl,
    ) -> Result<Option<String>, String> {
        let name = symbol_for_function(module_path, function);
        let func_id = match self.function_ids.get(&name) {
            Some(id) => *id,
            None => return Ok(None),
        };
        let sig = self.handle_signature(function.params.len());
        let mut ctx = self.module.make_context();
        ctx.func = Function::with_name_signature(UserFuncName::user(0, func_id.as_u32()), sig);
        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut lowering = LoweringState::new(self, module_path, function);
        for (index, param) in function.params.iter().enumerate() {
            let variable = lowering.new_var(&mut builder, lowering.compiler.pointer_type);
            let value = builder.block_params(entry)[index];
            builder.def_var(variable, value);
            lowering.locals.insert(
                param.name.clone(),
                LocalBinding {
                    var: variable,
                    ty: param.type_name.clone(),
                    destroy: param.convention.as_deref() == Some("owned"),
                },
            );
        }
        let (prefix_statements, final_expr) = match function.body.statements.split_last() {
            Some((fa::Statement::Expr(expr_stmt), prefix)) if function.return_type.is_some() => {
                (prefix, Some(expr_stmt.expr.clone()))
            }
            _ => (function.body.statements.as_slice(), None),
        };
        lowering.compile_statements(&mut builder, prefix_statements)?;
        if !lowering.current_block_is_terminated(&builder) {
            let result = if let Some(expr) = final_expr.as_ref() {
                lowering.compile_expr(&mut builder, expr)?.value
            } else {
                lowering.runtime_nullary(&mut builder, lowering.compiler.runtime.unit)
            };
            if function.return_type.is_none() && final_expr.is_none() {
                lowering.release_remaining(&mut builder);
            }
            builder.ins().return_(&[result]);
        }
        builder.seal_all_blocks();
        builder.finalize();

        // Capture IR text before verification/definition.
        let ir_text = format!("; {name}\n{}", ctx.func.display());
        Ok(Some(ir_text))
    }

    fn handle_signature(&self, arity: usize) -> cranelift_codegen::ir::Signature {
        let mut sig = self.module.make_signature();
        for _ in 0..arity {
            sig.params.push(AbiParam::new(self.pointer_type));
        }
        sig.returns.push(AbiParam::new(self.pointer_type));
        sig
    }

    fn destructor_signature(&self) -> cranelift_codegen::ir::Signature {
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(self.pointer_type));
        sig
    }

    fn entry_signature(&self) -> cranelift_codegen::ir::Signature {
        let mut sig = self.module.make_signature();
        sig.returns.push(AbiParam::new(types::I32));
        sig
    }

    fn string_data_id(&mut self, text: &str) -> Result<cranelift_module::DataId, String> {
        if let Some(id) = self.string_ids.get(text) {
            return Ok(*id);
        }
        let name = format!("fuse_str_{}", self.string_ids.len());
        let id = self
            .module
            .declare_data(&name, Linkage::Local, false, false)
            .map_err(|error| error.to_string())?;
        let mut data = DataDescription::new();
        data.define(text.as_bytes().to_vec().into_boxed_slice());
        self.module
            .define_data(id, &data)
            .map_err(|error| error.to_string())?;
        self.string_ids.insert(text.to_string(), id);
        Ok(id)
    }

    fn compile_function(&mut self, module_path: &Path, function: &fa::FunctionDecl) -> Result<(), String> {
        let name = symbol_for_function(module_path, function);
        let func_id = *self
            .function_ids
            .get(&name)
            .ok_or_else(|| format!("missing function id for `{name}`"))?;
        let sig = self.handle_signature(function.params.len());
        let mut ctx = self.module.make_context();
        ctx.func = Function::with_name_signature(UserFuncName::user(0, func_id.as_u32()), sig);
        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut lowering = LoweringState::new(self, module_path, function);
        for (index, param) in function.params.iter().enumerate() {
            let variable = lowering.new_var(&mut builder, lowering.compiler.pointer_type);
            let value = builder.block_params(entry)[index];
            builder.def_var(variable, value);
            lowering.locals.insert(
                param.name.clone(),
                LocalBinding {
                    var: variable,
                    ty: param.type_name.clone(),
                    destroy: param.convention.as_deref() == Some("owned"),
                },
            );
        }
        let (prefix_statements, final_expr) = match function.body.statements.split_last() {
            Some((fa::Statement::Expr(expr_stmt), prefix)) if function.return_type.is_some() => {
                (prefix, Some(expr_stmt.expr.clone()))
            }
            _ => (function.body.statements.as_slice(), None),
        };
        lowering.compile_statements(&mut builder, prefix_statements)?;
        if !lowering.current_block_is_terminated(&builder) {
            if function.return_type.as_deref() == Some("!") {
                if let Some(expr) = final_expr.as_ref() {
                    lowering.compile_expr(&mut builder, expr)?;
                }
                let panic_fn = lowering.compiler.module.declare_func_in_func(lowering.compiler.runtime.panic, builder.func);
                builder.ins().call(panic_fn, &[]);
                builder.ins().trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
            } else {
                let result = if let Some(expr) = final_expr.as_ref() {
                    lowering.compile_expr(&mut builder, expr)?.value
                } else {
                    lowering.runtime_nullary(&mut builder, lowering.compiler.runtime.unit)
                };
                if function.return_type.is_none() && final_expr.is_none() {
                    lowering.release_remaining(&mut builder);
                }
                builder.ins().return_(&[result]);
            }
        }
        builder.seal_all_blocks();
        builder.finalize();
        let flags = settings::Flags::new(settings::builder());
        if let Err(err) = cranelift_codegen::verify_function(&ctx.func, &flags) {
            return Err(cranelift_codegen::print_errors::pretty_verifier_error(
                &ctx.func,
                None,
                err,
            ));
        }
        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|error| error.to_string())?;

        while let Some(pending) = self.pending_lambdas.pop() {
            self.compile_closure_function(&pending.module_path, &pending.decl, &pending.captures)?;
        }
        Ok(())
    }

    fn compile_closure_function(
        &mut self,
        module_path: &Path,
        function: &fa::FunctionDecl,
        captures: &[(String, Option<String>)],
    ) -> Result<(), String> {
        let name = symbol_for_function(module_path, function);
        let func_id = *self
            .function_ids
            .get(&name)
            .ok_or_else(|| format!("missing function id for `{name}`"))?;
        let sig = self.handle_signature(function.params.len());
        let mut ctx = self.module.make_context();
        ctx.func = Function::with_name_signature(UserFuncName::user(0, func_id.as_u32()), sig);
        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut lowering = LoweringState::new(self, module_path, function);

        // Set up all params (including __env as first)
        for (index, param) in function.params.iter().enumerate() {
            let variable = lowering.new_var(&mut builder, lowering.compiler.pointer_type);
            let value = builder.block_params(entry)[index];
            builder.def_var(variable, value);
            lowering.locals.insert(
                param.name.clone(),
                LocalBinding {
                    var: variable,
                    ty: param.type_name.clone(),
                    destroy: param.convention.as_deref() == Some("owned"),
                },
            );
        }

        // Extract captures from __env (closure list). Index 0 is fn_ptr, captures start at 1.
        if !captures.is_empty() {
            let env_binding = &lowering.locals["__env"];
            let env_val = builder.use_var(env_binding.var);
            for (i, (cap_name, cap_ty)) in captures.iter().enumerate() {
                let idx = lowering.usize_const(&mut builder, (i + 1) as i64);
                let cap_val = lowering.runtime(
                    &mut builder,
                    lowering.compiler.runtime.list_get,
                    &[env_val, idx],
                    lowering.compiler.pointer_type,
                );
                let variable = lowering.new_var(&mut builder, lowering.compiler.pointer_type);
                builder.def_var(variable, cap_val);
                lowering.locals.insert(
                    cap_name.clone(),
                    LocalBinding {
                        var: variable,
                        ty: cap_ty.clone(),
                        destroy: false,
                    },
                );
            }
        }

        let (prefix_statements, final_expr) = match function.body.statements.split_last() {
            Some((fa::Statement::Expr(expr_stmt), prefix)) if function.return_type.is_some() => {
                (prefix, Some(expr_stmt.expr.clone()))
            }
            _ => (function.body.statements.as_slice(), None),
        };
        lowering.compile_statements(&mut builder, prefix_statements)?;
        if !lowering.current_block_is_terminated(&builder) {
            if function.return_type.as_deref() == Some("!") {
                if let Some(expr) = final_expr.as_ref() {
                    lowering.compile_expr(&mut builder, expr)?;
                }
                let panic_fn = lowering.compiler.module.declare_func_in_func(lowering.compiler.runtime.panic, builder.func);
                builder.ins().call(panic_fn, &[]);
                builder.ins().trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
            } else {
                let result = if let Some(expr) = final_expr.as_ref() {
                    lowering.compile_expr(&mut builder, expr)?.value
                } else {
                    lowering.runtime_nullary(&mut builder, lowering.compiler.runtime.unit)
                };
                if function.return_type.is_none() && final_expr.is_none() {
                    lowering.release_remaining(&mut builder);
                }
                builder.ins().return_(&[result]);
            }
        }
        builder.seal_all_blocks();
        builder.finalize();
        let flags = settings::Flags::new(settings::builder());
        if let Err(err) = cranelift_codegen::verify_function(&ctx.func, &flags) {
            return Err(cranelift_codegen::print_errors::pretty_verifier_error(
                &ctx.func,
                None,
                err,
            ));
        }
        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|error| error.to_string())?;

        while let Some(pending) = self.pending_lambdas.pop() {
            self.compile_closure_function(&pending.module_path, &pending.decl, &pending.captures)?;
        }
        Ok(())
    }

    fn compile_destructor(&mut self, module_path: &Path, data: &fa::DataClassDecl) -> Result<(), String> {
        let destructor_method = data
            .methods
            .iter()
            .find(|method| method.name == "__del__")
            .ok_or_else(|| format!("missing destructor method for `{}`", data.name))?;
        let bridge_name = layout::destructor_symbol(module_path, &data.name);
        let bridge_id = *self
            .destructor_ids
            .get(&bridge_name)
            .ok_or_else(|| format!("missing destructor bridge `{bridge_name}`"))?;
        let target_name = layout::function_symbol(module_path, &destructor_method.name);
        let target_id = *self
            .function_ids
            .get(&target_name)
            .ok_or_else(|| format!("missing destructor impl `{target_name}`"))?;
        let mut ctx = self.module.make_context();
        ctx.func = Function::with_name_signature(
            UserFuncName::user(0, bridge_id.as_u32()),
            self.destructor_signature(),
        );
        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let arg = builder.block_params(entry)[0];
        let local = self.module.declare_func_in_func(target_id, builder.func);
        builder.ins().call(local, &[arg]);
        builder.ins().return_(&[]);
        builder.seal_all_blocks();
        builder.finalize();
        self.module
            .define_function(bridge_id, &mut ctx)
            .map_err(|error| error.to_string())
    }

    fn compile_struct_destructor(&mut self, module_path: &Path, s: &fa::StructDecl) -> Result<(), String> {
        let destructor_method = s
            .methods
            .iter()
            .find(|method| method.name == "__del__")
            .ok_or_else(|| format!("missing destructor method for `{}`", s.name))?;
        let bridge_name = layout::destructor_symbol(module_path, &s.name);
        let bridge_id = *self
            .destructor_ids
            .get(&bridge_name)
            .ok_or_else(|| format!("missing destructor bridge `{bridge_name}`"))?;
        let target_name = layout::function_symbol(module_path, &destructor_method.name);
        let target_id = *self
            .function_ids
            .get(&target_name)
            .ok_or_else(|| format!("missing destructor impl `{target_name}`"))?;
        let mut ctx = self.module.make_context();
        ctx.func = Function::with_name_signature(
            UserFuncName::user(0, bridge_id.as_u32()),
            self.destructor_signature(),
        );
        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let arg = builder.block_params(entry)[0];
        let local = self.module.declare_func_in_func(target_id, builder.func);
        builder.ins().call(local, &[arg]);
        builder.ins().return_(&[]);
        builder.seal_all_blocks();
        builder.finalize();
        self.module
            .define_function(bridge_id, &mut ctx)
            .map_err(|error| error.to_string())
    }

    fn compile_entry(&mut self) -> Result<(), String> {
        let entry_func = self.session.entry_function()?;
        let entry_id = self
            .module
            .declare_function(layout::ENTRY_SYMBOL, Linkage::Export, &self.entry_signature())
            .map_err(|error| error.to_string())?;
        let mut ctx = self.module.make_context();
        ctx.func = Function::with_name_signature(
            UserFuncName::user(0, entry_id.as_u32()),
            self.entry_signature(),
        );
        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let target = *self
            .function_ids
            .get(&symbol_for_function(self.session.root_path.as_path(), &entry_func))
            .ok_or_else(|| format!("missing entrypoint function `{}`", entry_func.name))?;
        let local = self.module.declare_func_in_func(target, builder.func);
        let call = builder.ins().call(local, &[]);
        let result = builder.inst_results(call)[0];
        let release = self.module.declare_func_in_func(self.runtime.release, builder.func);
        builder.ins().call(release, &[result]);
        let zero = builder.ins().iconst(types::I32, 0);
        builder.ins().return_(&[zero]);
        builder.seal_all_blocks();
        builder.finalize();
        self.module
            .define_function(entry_id, &mut ctx)
            .map_err(|error| error.to_string())
    }
}

fn declare_runtime_functions(
    module: &mut ObjectModule,
    pointer_type: cranelift_codegen::ir::Type,
) -> Result<RuntimeFns, String> {
    fn sig(
        module: &mut ObjectModule,
        params: &[cranelift_codegen::ir::Type],
        returns: &[cranelift_codegen::ir::Type],
    ) -> cranelift_codegen::ir::Signature {
        let mut sig = module.make_signature();
        for ty in params {
            sig.params.push(AbiParam::new(*ty));
        }
        for ty in returns {
            sig.returns.push(AbiParam::new(*ty));
        }
        sig
    }

    fn declare(
        module: &mut ObjectModule,
        name: &str,
        params: &[cranelift_codegen::ir::Type],
        returns: &[cranelift_codegen::ir::Type],
    ) -> Result<FuncId, String> {
        let signature = sig(module, params, returns);
        module
            .declare_function(name, Linkage::Import, &signature)
            .map_err(|error| error.to_string())
    }

    Ok(RuntimeFns {
        unit: declare(module, "fuse_unit", &[], &[pointer_type])?,
        int: declare(module, "fuse_int", &[types::I64], &[pointer_type])?,
        float: declare(module, "fuse_float", &[types::F64], &[pointer_type])?,
        bool_: declare(module, "fuse_bool", &[types::I8], &[pointer_type])?,
        string_new_utf8: declare(module, "fuse_string_new_utf8", &[pointer_type, pointer_type], &[pointer_type])?,
        to_string: declare(module, "fuse_to_string", &[pointer_type], &[pointer_type])?,
        concat: declare(module, "fuse_concat", &[pointer_type, pointer_type], &[pointer_type])?,
        add: declare(module, "fuse_add", &[pointer_type, pointer_type], &[pointer_type])?,
        sub: declare(module, "fuse_sub", &[pointer_type, pointer_type], &[pointer_type])?,
        mul: declare(module, "fuse_mul", &[pointer_type, pointer_type], &[pointer_type])?,
        div: declare(module, "fuse_div", &[pointer_type, pointer_type], &[pointer_type])?,
        mod_: declare(module, "fuse_mod", &[pointer_type, pointer_type], &[pointer_type])?,
        eq: declare(module, "fuse_eq", &[pointer_type, pointer_type], &[pointer_type])?,
        lt: declare(module, "fuse_lt", &[pointer_type, pointer_type], &[pointer_type])?,
        le: declare(module, "fuse_le", &[pointer_type, pointer_type], &[pointer_type])?,
        gt: declare(module, "fuse_gt", &[pointer_type, pointer_type], &[pointer_type])?,
        ge: declare(module, "fuse_ge", &[pointer_type, pointer_type], &[pointer_type])?,
        truthy: declare(module, "fuse_is_truthy", &[pointer_type], &[types::I8])?,
        extract_int: declare(module, "fuse_extract_int", &[pointer_type], &[types::I64])?,
        println: declare(module, "fuse_builtin_println", &[pointer_type], &[])?,
        none: declare(module, "fuse_none", &[], &[pointer_type])?,
        some: declare(module, "fuse_some", &[pointer_type], &[pointer_type])?,
        option_is_some: declare(module, "fuse_option_is_some", &[pointer_type], &[types::I8])?,
        option_unwrap: declare(module, "fuse_option_unwrap", &[pointer_type], &[pointer_type])?,
        ok: declare(module, "fuse_ok", &[pointer_type], &[pointer_type])?,
        err: declare(module, "fuse_err", &[pointer_type], &[pointer_type])?,
        result_is_ok: declare(module, "fuse_result_is_ok", &[pointer_type], &[types::I8])?,
        result_unwrap: declare(module, "fuse_result_unwrap", &[pointer_type], &[pointer_type])?,
        list_new: declare(module, "fuse_list_new", &[], &[pointer_type])?,
        list_push: declare(module, "fuse_list_push", &[pointer_type, pointer_type], &[])?,
        list_len: declare(module, "fuse_list_len", &[pointer_type], &[types::I64])?,
        list_get: declare(module, "fuse_list_get", &[pointer_type, pointer_type], &[pointer_type])?,
        list_get_handle: declare(module, "fuse_list_get_handle", &[pointer_type, pointer_type], &[pointer_type])?,
        rt_list_get: declare(module, "fuse_rt_list_get", &[pointer_type, pointer_type], &[pointer_type])?,
        chan_bounded: declare(module, "fuse_chan_runtime_bounded", &[pointer_type], &[pointer_type])?,
        chan_new: declare(module, "fuse_chan_runtime_new", &[], &[pointer_type])?,
        chan_send: declare(module, "fuse_chan_runtime_send", &[pointer_type, pointer_type], &[pointer_type])?,
        chan_recv: declare(module, "fuse_chan_runtime_recv", &[pointer_type], &[pointer_type])?,
        chan_try_recv: declare(module, "fuse_chan_runtime_try_recv", &[pointer_type], &[pointer_type])?,
        chan_close: declare(module, "fuse_chan_runtime_close", &[pointer_type], &[])?,
        chan_is_closed: declare(module, "fuse_chan_runtime_is_closed", &[pointer_type], &[types::I8])?,
        chan_len: declare(module, "fuse_chan_runtime_len", &[pointer_type], &[types::I64])?,
        chan_cap: declare(module, "fuse_chan_runtime_cap", &[pointer_type], &[pointer_type])?,
        shared_new: declare(module, "fuse_shared_runtime_new", &[pointer_type], &[pointer_type])?,
        shared_read: declare(module, "fuse_shared_runtime_read", &[pointer_type], &[pointer_type])?,
        shared_write: declare(module, "fuse_shared_runtime_write", &[pointer_type], &[pointer_type])?,
        shared_try_write: declare(module, "fuse_shared_runtime_try_write", &[pointer_type, pointer_type], &[pointer_type])?,
        shared_try_read: declare(module, "fuse_shared_runtime_try_read", &[pointer_type, pointer_type], &[pointer_type])?,
        simd_sum: declare(module, "fuse_simd_runtime_sum", &[pointer_type], &[pointer_type])?,
        simd_dot: declare(module, "fuse_simd_runtime_dot", &[pointer_type, pointer_type], &[pointer_type])?,
        simd_add: declare(module, "fuse_simd_runtime_add", &[pointer_type, pointer_type], &[pointer_type])?,
        simd_sub: declare(module, "fuse_simd_runtime_sub", &[pointer_type, pointer_type], &[pointer_type])?,
        simd_mul: declare(module, "fuse_simd_runtime_mul", &[pointer_type, pointer_type], &[pointer_type])?,
        simd_div: declare(module, "fuse_simd_runtime_div", &[pointer_type, pointer_type], &[pointer_type])?,
        simd_min: declare(module, "fuse_simd_runtime_min", &[pointer_type, pointer_type], &[pointer_type])?,
        simd_max: declare(module, "fuse_simd_runtime_max", &[pointer_type, pointer_type], &[pointer_type])?,
        simd_abs: declare(module, "fuse_simd_runtime_abs", &[pointer_type], &[pointer_type])?,
        simd_sqrt: declare(module, "fuse_simd_runtime_sqrt", &[pointer_type], &[pointer_type])?,
        simd_broadcast: declare(module, "fuse_simd_runtime_broadcast", &[pointer_type, types::I64], &[pointer_type])?,
        simd_get: declare(module, "fuse_simd_runtime_get", &[pointer_type, pointer_type], &[pointer_type])?,
        simd_len: declare(module, "fuse_simd_runtime_len", &[pointer_type], &[pointer_type])?,
        simd_extract_raw_f64: declare(module, "fuse_simd_extract_raw_f64", &[pointer_type], &[types::F64])?,
        simd_extract_raw_f32: declare(module, "fuse_simd_extract_raw_f32", &[pointer_type], &[types::F32])?,
        simd_extract_raw_i64: declare(module, "fuse_simd_extract_raw_i64", &[pointer_type], &[types::I64])?,
        simd_extract_raw_i32: declare(module, "fuse_simd_extract_raw_i32", &[pointer_type], &[types::I32])?,
        data_new: declare(module, "fuse_data_new", &[pointer_type, pointer_type, pointer_type, pointer_type], &[pointer_type])?,
        data_set_field: declare(module, "fuse_data_set_field", &[pointer_type, pointer_type, pointer_type], &[])?,
        data_get_field: declare(module, "fuse_data_get_field", &[pointer_type, pointer_type], &[pointer_type])?,
        release: declare(module, "fuse_release", &[pointer_type], &[])?,
        asap_release: declare(module, "fuse_asap_release", &[pointer_type], &[])?,
        to_upper: declare(module, "fuse_to_upper", &[pointer_type], &[pointer_type])?,
        string_is_empty: declare(module, "fuse_string_is_empty", &[pointer_type], &[pointer_type])?,
        string_char_count: declare(module, "fuse_rt_string_char_count", &[pointer_type], &[pointer_type])?,
        enum_new: declare(module, "fuse_enum_new", &[pointer_type, pointer_type, types::I64, pointer_type, pointer_type, pointer_type], &[pointer_type])?,
        enum_add_payload: declare(module, "fuse_enum_add_payload", &[pointer_type, pointer_type], &[])?,
        enum_tag: declare(module, "fuse_enum_tag", &[pointer_type], &[types::I64])?,
        enum_payload: declare(module, "fuse_enum_payload", &[pointer_type, pointer_type], &[pointer_type])?,
        map_new: declare(module, "fuse_map_new", &[], &[pointer_type])?,
        map_set: declare(module, "fuse_map_set", &[pointer_type, pointer_type, pointer_type], &[])?,
        map_get: declare(module, "fuse_map_get", &[pointer_type, pointer_type], &[pointer_type])?,
        rt_map_get: declare(module, "fuse_rt_map_get", &[pointer_type, pointer_type], &[pointer_type])?,
        map_remove: declare(module, "fuse_map_remove", &[pointer_type, pointer_type], &[pointer_type])?,
        map_len: declare(module, "fuse_map_len", &[pointer_type], &[types::I64])?,
        map_contains: declare(module, "fuse_map_contains", &[pointer_type, pointer_type], &[types::I8])?,
        map_keys: declare(module, "fuse_map_keys", &[pointer_type], &[pointer_type])?,
        map_values: declare(module, "fuse_map_values", &[pointer_type], &[pointer_type])?,
        map_entries: declare(module, "fuse_map_entries", &[pointer_type], &[pointer_type])?,
        panic: declare(module, "fuse_rt_panic", &[], &[])?,
        test_assert_eq: declare(module, "fuse_rt_test_assert_eq", &[pointer_type, pointer_type, pointer_type], &[pointer_type])?,
        test_assert_ne: declare(module, "fuse_rt_test_assert_ne", &[pointer_type, pointer_type, pointer_type], &[pointer_type])?,
        test_assert_approx: declare(module, "fuse_rt_test_assert_approx", &[pointer_type, pointer_type, pointer_type, pointer_type], &[pointer_type])?,
        test_assert_panics: declare(module, "fuse_rt_test_assert_panics", &[pointer_type], &[pointer_type])?,
        f32_new: declare(module, "fuse_rt_f32_new", &[types::F64], &[pointer_type])?,
        f32_add: declare(module, "fuse_rt_f32_add", &[pointer_type, pointer_type], &[pointer_type])?,
        f32_sub: declare(module, "fuse_rt_f32_sub", &[pointer_type, pointer_type], &[pointer_type])?,
        f32_mul: declare(module, "fuse_rt_f32_mul", &[pointer_type, pointer_type], &[pointer_type])?,
        f32_div: declare(module, "fuse_rt_f32_div", &[pointer_type, pointer_type], &[pointer_type])?,
        f32_eq: declare(module, "fuse_rt_f32_eq", &[pointer_type, pointer_type], &[pointer_type])?,
        f32_lt: declare(module, "fuse_rt_f32_lt", &[pointer_type, pointer_type], &[pointer_type])?,
        f32_le: declare(module, "fuse_rt_f32_le", &[pointer_type, pointer_type], &[pointer_type])?,
        f32_gt: declare(module, "fuse_rt_f32_gt", &[pointer_type, pointer_type], &[pointer_type])?,
        f32_ge: declare(module, "fuse_rt_f32_ge", &[pointer_type, pointer_type], &[pointer_type])?,
        f32_to_string: declare(module, "fuse_rt_f32_to_string", &[pointer_type], &[pointer_type])?,
        i8: {
            let pp = &[pointer_type, pointer_type]; let p = &[pointer_type]; let r = &[pointer_type];
            SizedIntFns { new: declare(module, "fuse_rt_i8_new", &[types::I64], r)?, add: declare(module, "fuse_rt_i8_add", pp, r)?, sub: declare(module, "fuse_rt_i8_sub", pp, r)?, mul: declare(module, "fuse_rt_i8_mul", pp, r)?, div: declare(module, "fuse_rt_i8_div", pp, r)?, mod_: declare(module, "fuse_rt_i8_mod", pp, r)?, eq: declare(module, "fuse_rt_i8_eq", pp, r)?, lt: declare(module, "fuse_rt_i8_lt", pp, r)?, le: declare(module, "fuse_rt_i8_le", pp, r)?, gt: declare(module, "fuse_rt_i8_gt", pp, r)?, ge: declare(module, "fuse_rt_i8_ge", pp, r)?, to_string: declare(module, "fuse_rt_i8_to_string", p, r)? }
        },
        u8: {
            let pp = &[pointer_type, pointer_type]; let p = &[pointer_type]; let r = &[pointer_type];
            SizedIntFns { new: declare(module, "fuse_rt_u8_new", &[types::I64], r)?, add: declare(module, "fuse_rt_u8_add", pp, r)?, sub: declare(module, "fuse_rt_u8_sub", pp, r)?, mul: declare(module, "fuse_rt_u8_mul", pp, r)?, div: declare(module, "fuse_rt_u8_div", pp, r)?, mod_: declare(module, "fuse_rt_u8_mod", pp, r)?, eq: declare(module, "fuse_rt_u8_eq", pp, r)?, lt: declare(module, "fuse_rt_u8_lt", pp, r)?, le: declare(module, "fuse_rt_u8_le", pp, r)?, gt: declare(module, "fuse_rt_u8_gt", pp, r)?, ge: declare(module, "fuse_rt_u8_ge", pp, r)?, to_string: declare(module, "fuse_rt_u8_to_string", p, r)? }
        },
        i32: {
            let pp = &[pointer_type, pointer_type]; let p = &[pointer_type]; let r = &[pointer_type];
            SizedIntFns { new: declare(module, "fuse_rt_i32_new", &[types::I64], r)?, add: declare(module, "fuse_rt_i32_add", pp, r)?, sub: declare(module, "fuse_rt_i32_sub", pp, r)?, mul: declare(module, "fuse_rt_i32_mul", pp, r)?, div: declare(module, "fuse_rt_i32_div", pp, r)?, mod_: declare(module, "fuse_rt_i32_mod", pp, r)?, eq: declare(module, "fuse_rt_i32_eq", pp, r)?, lt: declare(module, "fuse_rt_i32_lt", pp, r)?, le: declare(module, "fuse_rt_i32_le", pp, r)?, gt: declare(module, "fuse_rt_i32_gt", pp, r)?, ge: declare(module, "fuse_rt_i32_ge", pp, r)?, to_string: declare(module, "fuse_rt_i32_to_string", p, r)? }
        },
        u32: {
            let pp = &[pointer_type, pointer_type]; let p = &[pointer_type]; let r = &[pointer_type];
            SizedIntFns { new: declare(module, "fuse_rt_u32_new", &[types::I64], r)?, add: declare(module, "fuse_rt_u32_add", pp, r)?, sub: declare(module, "fuse_rt_u32_sub", pp, r)?, mul: declare(module, "fuse_rt_u32_mul", pp, r)?, div: declare(module, "fuse_rt_u32_div", pp, r)?, mod_: declare(module, "fuse_rt_u32_mod", pp, r)?, eq: declare(module, "fuse_rt_u32_eq", pp, r)?, lt: declare(module, "fuse_rt_u32_lt", pp, r)?, le: declare(module, "fuse_rt_u32_le", pp, r)?, gt: declare(module, "fuse_rt_u32_gt", pp, r)?, ge: declare(module, "fuse_rt_u32_ge", pp, r)?, to_string: declare(module, "fuse_rt_u32_to_string", p, r)? }
        },
        u64: {
            let pp = &[pointer_type, pointer_type]; let p = &[pointer_type]; let r = &[pointer_type];
            SizedIntFns { new: declare(module, "fuse_rt_u64_new", &[types::I64], r)?, add: declare(module, "fuse_rt_u64_add", pp, r)?, sub: declare(module, "fuse_rt_u64_sub", pp, r)?, mul: declare(module, "fuse_rt_u64_mul", pp, r)?, div: declare(module, "fuse_rt_u64_div", pp, r)?, mod_: declare(module, "fuse_rt_u64_mod", pp, r)?, eq: declare(module, "fuse_rt_u64_eq", pp, r)?, lt: declare(module, "fuse_rt_u64_lt", pp, r)?, le: declare(module, "fuse_rt_u64_le", pp, r)?, gt: declare(module, "fuse_rt_u64_gt", pp, r)?, ge: declare(module, "fuse_rt_u64_ge", pp, r)?, to_string: declare(module, "fuse_rt_u64_to_string", p, r)? }
        },
    })
}

fn build_wrapper(_input: &Path, output: &Path, object: &[u8]) -> Result<(), String> {
    let stage1_root = repo_root().join("stage1");
    let generated_root = stage1_root.join("target").join("generated");
    fs::create_dir_all(&generated_root)
        .map_err(|error| format!("failed to create generated directory: {error}"))?;
    let workdir = generated_root.join("wrapper");
    let src_dir = workdir.join("src");
    fs::create_dir_all(&src_dir)
        .map_err(|error| format!("failed to create wrapper directory: {error}"))?;
    // Use a unique object file name derived from the output path so that
    // consecutive compilations each reference a different file in build.rs.
    // This guarantees Cargo's rerun-if-changed check sees a content change
    // in build.rs and always re-links — preventing stale-binary bugs when
    // multiple fixtures compile in rapid succession (same-second mtime).
    let obj_ext = if cfg!(windows) { "obj" } else { "o" };
    let obj_stem = output
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("program");
    let object_name = format!("{obj_stem}.{obj_ext}");
    let object_path = workdir.join(&object_name);
    fs::write(&object_path, object)
        .map_err(|error| format!("failed to write object file: {error}"))?;
    let runtime_path = escape_path(&stage1_root.join("fuse-runtime"));
    let cranelift_ffi_path = escape_path(&stage1_root.join("cranelift-ffi"));
    fs::write(
        workdir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"fuse_generated_wrapper\"\nversion = \"0.1.0\"\nedition = \"2024\"\nbuild = \"build.rs\"\n\n[workspace]\n\n[dependencies]\nfuse-runtime = {{ path = \"{runtime_path}\" }}\ncranelift-ffi = {{ path = \"{cranelift_ffi_path}\" }}\n"
        ),
    )
    .map_err(|error| format!("failed to write wrapper Cargo.toml: {error}"))?;
    let object_literal = escape_string(&object_path);
    fs::write(
        workdir.join("build.rs"),
        format!(
            "fn main() {{\n    println!(\"cargo:rerun-if-changed={object_literal}\");\n    println!(\"cargo:rustc-link-arg={object_literal}\");\n    if cfg!(target_os = \"windows\") {{\n        println!(\"cargo:rustc-link-arg=/STACK:8388608\");\n    }}\n}}\n"
        ),
    )
    .map_err(|error| format!("failed to write wrapper build.rs: {error}"))?;
    fs::write(
        src_dir.join("main.rs"),
        "use fuse_runtime as _;\nuse cranelift_ffi as _;\n\nunsafe extern \"C\" {\n    fn fuse_user_entry() -> i32;\n}\n\nfn main() {\n    std::process::exit(unsafe { fuse_user_entry() });\n}\n",
    )
    .map_err(|error| format!("failed to write wrapper main.rs: {error}"))?;
    let target_dir = stage1_root.join("target").join("generated-target");
    fs::create_dir_all(&target_dir)
        .map_err(|error| format!("failed to create shared wrapper target dir: {error}"))?;
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--target-dir")
        .arg(&target_dir)
        .current_dir(&workdir)
        .status()
        .map_err(|error| format!("failed to launch cargo build: {error}"))?;
    if !status.success() {
        return Err("generated wrapper build failed".to_string());
    }
    let built = target_dir
        .join("release")
        .join(format!("fuse_generated_wrapper{}", std::env::consts::EXE_SUFFIX));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create output directory: {error}"))?;
    }
    fs::copy(&built, output)
        .map_err(|error| format!("failed to copy generated executable: {error}"))?;
    Ok(())
}

struct LoweringState<'a, 'b> {
    compiler: &'a mut BackendCompiler<'b>,
    module_path: &'a Path,
    _function: &'a fa::FunctionDecl,
    locals: HashMap<String, LocalBinding>,
    next_var: usize,
    loops: Vec<LoopFrame>,
}

impl<'a, 'b> LoweringState<'a, 'b> {
    fn new(
        compiler: &'a mut BackendCompiler<'b>,
        module_path: &'a Path,
        function: &'a fa::FunctionDecl,
    ) -> Self {
        Self {
            compiler,
            module_path,
            _function: function,
            locals: HashMap::new(),
            next_var: 0,
            loops: Vec::new(),
        }
    }

    fn new_var(&mut self, builder: &mut FunctionBuilder, ty: cranelift_codegen::ir::Type) -> Variable {
        let variable = builder.declare_var(ty);
        self.next_var += 1;
        variable
    }

    fn compile_statements(
        &mut self,
        builder: &mut FunctionBuilder,
        statements: &[fa::Statement],
    ) -> Result<(), String> {
        let future = compute_future_uses(statements);
        for (index, statement) in statements.iter().enumerate() {
            self.compile_statement(builder, statement)?;
            if self.current_block_is_terminated(builder) {
                break;
            }
            self.release_dead(builder, &future[index]);
        }
        Ok(())
    }

    fn compile_statement(
        &mut self,
        builder: &mut FunctionBuilder,
        statement: &fa::Statement,
    ) -> Result<(), String> {
        match statement {
            fa::Statement::VarDecl(var_decl) => {
                let value = self.compile_expr(builder, &var_decl.value)?;
                let variable = self.new_var(builder, self.compiler.pointer_type);
                builder.def_var(variable, value.value);
                self.locals.insert(
                    var_decl.name.clone(),
                    LocalBinding {
                        var: variable,
                        ty: var_decl.type_name.clone().or_else(|| value.ty.clone()),
                        destroy: true,
                    },
                );
            }
            fa::Statement::Assign(assign) => self.compile_assign(builder, assign)?,
            fa::Statement::Return(ret) => {
                let value = if let Some(expr) = &ret.value {
                    self.compile_expr(builder, expr)?.value
                } else {
                    self.runtime_nullary(builder, self.compiler.runtime.unit)
                };
                builder.ins().return_(&[value]);
            }
            fa::Statement::Break(_) => {
                let frame = self.loops.last().ok_or_else(|| "`break` outside loop".to_string())?;
                builder.ins().jump(frame.break_block, &[]);
            }
            fa::Statement::Continue(_) => {
                let frame = self.loops.last().ok_or_else(|| "`continue` outside loop".to_string())?;
                builder.ins().jump(frame.continue_block, &[]);
            }
            fa::Statement::Spawn(spawn_stmt) => {
                let snapshot = self.locals.clone();
                let mut child_locals = snapshot.clone();
                for binding in child_locals.values_mut() {
                    binding.destroy = false;
                }
                self.locals = child_locals;
                self.compile_statements(builder, &spawn_stmt.body.statements)?;
                self.locals = snapshot;
            }
            fa::Statement::While(while_stmt) => self.compile_while(builder, while_stmt)?,
            fa::Statement::For(for_stmt) => self.compile_for(builder, for_stmt)?,
            fa::Statement::Loop(loop_stmt) => self.compile_loop(builder, &loop_stmt.body.statements)?,
            fa::Statement::Defer(_) => {}
            fa::Statement::TupleDestruct(td) => {
                let value = self.compile_expr(builder, &td.value)?;
                // Extract element types from the tuple type "(T1,T2,...)"
                let elem_types = value.ty.as_deref().and_then(split_tuple_types);
                for (i, name) in td.names.iter().enumerate() {
                    let idx = builder.ins().iconst(self.compiler.pointer_type, i as i64);
                    let item = self.runtime(
                        builder,
                        self.compiler.runtime.list_get,
                        &[value.value, idx],
                        self.compiler.pointer_type,
                    );
                    let variable = self.new_var(builder, self.compiler.pointer_type);
                    builder.def_var(variable, item);
                    let elem_ty = elem_types.as_ref().and_then(|ts| ts.get(i).cloned());
                    // Disable ASAP release for reference-like types (Chan, Shared)
                    // that alias the same handle — releasing one would invalidate the other.
                    let is_ref_type = elem_ty.as_deref()
                        .map(|t| matches!(layout::canonical_type_name(t), "Chan" | "Shared"))
                        .unwrap_or(false);
                    self.locals.insert(
                        name.clone(),
                        LocalBinding {
                            var: variable,
                            ty: elem_ty,
                            destroy: !is_ref_type,
                        },
                    );
                }
            }
            fa::Statement::Expr(expr_stmt) => {
                let _ = self.compile_expr(builder, &expr_stmt.expr)?;
            }
        }
        Ok(())
    }

    fn compile_assign(
        &mut self,
        builder: &mut FunctionBuilder,
        assign: &fa::Assign,
    ) -> Result<(), String> {
        let value = self.compile_expr(builder, &assign.value)?;
        match &assign.target {
            fa::Expr::Name(name) => {
                let binding = self
                    .locals
                    .get(&name.value)
                    .cloned()
                    .ok_or_else(|| format!("unknown binding `{}`", name.value))?;
                builder.def_var(binding.var, value.value);
            }
            fa::Expr::Member(member) => {
                let object = self.compile_expr(builder, &member.object)?;
                let object_type = object
                    .ty
                    .clone()
                    .ok_or_else(|| format!("cannot infer member target `{}`", member.name))?;
                let layout = self
                    .compiler
                    .session
                    .layout
                    .data_layout(&object_type)
                    .ok_or_else(|| format!("missing layout for `{object_type}`"))?;
                let field_index = layout
                    .field_index(&member.name)
                    .ok_or_else(|| format!("unknown field `{}`", member.name))?;
                let field_index = self.usize_const(builder, field_index as i64);
                self.runtime_void(
                    builder,
                    self.compiler.runtime.data_set_field,
                    &[object.value, field_index, value.value],
                );
            }
            other => return Err(format!("unsupported assignment target `{:?}`", other)),
        }
        Ok(())
    }

    fn compile_while(
        &mut self,
        builder: &mut FunctionBuilder,
        while_stmt: &fa::WhileStmt,
    ) -> Result<(), String> {
        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let exit_block = builder.create_block();
        builder.ins().jump(cond_block, &[]);
        builder.switch_to_block(cond_block);
        let condition = self.compile_expr(builder, &while_stmt.condition)?;
        let truthy = self.truthy_value(builder, condition.value);
        builder.ins().brif(truthy, body_block, &[], exit_block, &[]);
        builder.switch_to_block(body_block);
        self.loops.push(LoopFrame {
            break_block: exit_block,
            continue_block: cond_block,
        });
        // Protect outer locals from ASAP release inside the loop body.
        let snapshot = self.locals.clone();
        for binding in self.locals.values_mut() {
            binding.destroy = false;
        }
        self.compile_statements(builder, &while_stmt.body.statements)?;
        self.locals = snapshot;
        self.loops.pop();
        if !self.current_block_is_terminated(builder) {
            builder.ins().jump(cond_block, &[]);
        }
        builder.switch_to_block(exit_block);
        Ok(())
    }

    fn compile_for(
        &mut self,
        builder: &mut FunctionBuilder,
        for_stmt: &fa::ForStmt,
    ) -> Result<(), String> {
        let iterable = self.compile_expr(builder, &for_stmt.iterable)?;
        let list_var = self.new_var(builder, self.compiler.pointer_type);
        builder.def_var(list_var, iterable.value);
        let index_var = self.new_var(builder, types::I64);
        let zero = builder.ins().iconst(types::I64, 0);
        builder.def_var(index_var, zero);
        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let increment_block = builder.create_block();
        let exit_block = builder.create_block();
        builder.ins().jump(cond_block, &[]);
        builder.switch_to_block(cond_block);
        let list = builder.use_var(list_var);
        let len = self.runtime_value(builder, self.compiler.runtime.list_len, &[list], types::I64);
        let index = builder.use_var(index_var);
        let cond = builder.ins().icmp(IntCC::UnsignedLessThan, index, len);
        builder.ins().brif(cond, body_block, &[], exit_block, &[]);
        builder.switch_to_block(body_block);
        let item = self.runtime(
            builder,
            self.compiler.runtime.list_get,
            &[list, index],
            self.compiler.pointer_type,
        );
        let item_var = self.new_var(builder, self.compiler.pointer_type);
        builder.def_var(item_var, item);
        let element_type = iterable.ty.as_deref()
            .and_then(|t| {
                if t.starts_with("List<") && t.ends_with('>') {
                    Some(t[5..t.len()-1].to_string())
                } else {
                    None
                }
            });
        self.locals.insert(
            for_stmt.name.clone(),
            LocalBinding {
                var: item_var,
                ty: element_type,
                destroy: false,
            },
        );
        self.loops.push(LoopFrame {
            break_block: exit_block,
            continue_block: increment_block,
        });
        // Protect outer locals from ASAP release inside the loop body.
        let snapshot = self.locals.clone();
        for binding in self.locals.values_mut() {
            binding.destroy = false;
        }
        self.compile_statements(builder, &for_stmt.body.statements)?;
        self.locals = snapshot;
        self.loops.pop();
        if !self.current_block_is_terminated(builder) {
            builder.ins().jump(increment_block, &[]);
        }
        // Increment block: advance index then re-check condition.
        // Both normal body fallthrough and `continue` land here.
        builder.switch_to_block(increment_block);
        let cur_index = builder.use_var(index_var);
        let next = builder.ins().iadd_imm(cur_index, 1);
        builder.def_var(index_var, next);
        builder.ins().jump(cond_block, &[]);
        builder.switch_to_block(exit_block);
        Ok(())
    }

    fn compile_loop(
        &mut self,
        builder: &mut FunctionBuilder,
        statements: &[fa::Statement],
    ) -> Result<(), String> {
        let body_block = builder.create_block();
        let exit_block = builder.create_block();
        builder.ins().jump(body_block, &[]);
        builder.switch_to_block(body_block);
        self.loops.push(LoopFrame {
            break_block: exit_block,
            continue_block: body_block,
        });
        // Protect outer locals from ASAP release inside the loop body.
        let snapshot = self.locals.clone();
        for binding in self.locals.values_mut() {
            binding.destroy = false;
        }
        self.compile_statements(builder, statements)?;
        self.locals = snapshot;
        self.loops.pop();
        if !self.current_block_is_terminated(builder) {
            builder.ins().jump(body_block, &[]);
        }
        builder.switch_to_block(exit_block);
        Ok(())
    }

    fn compile_expr(
        &mut self,
        builder: &mut FunctionBuilder,
        expr: &fa::Expr,
    ) -> Result<TypedValue, String> {
        match expr {
            fa::Expr::Literal(literal) => self.compile_literal(builder, literal),
            fa::Expr::FString(fstring) => self.compile_fstring(builder, &fstring.template),
            fa::Expr::Name(name) => self.compile_name(builder, &name.value),
            fa::Expr::List(list) => self.compile_list(builder, list),
            fa::Expr::Unary(unary) => self.compile_unary(builder, unary),
            fa::Expr::Binary(binary) => self.compile_binary(builder, binary),
            fa::Expr::Call(call) => self.compile_call(builder, call),
            fa::Expr::Member(member) => self.compile_member(builder, member),
            fa::Expr::Move(move_expr) => self.compile_move(builder, move_expr),
            fa::Expr::Ref(reference) => self.compile_expr(builder, &reference.value),
            fa::Expr::MutRef(reference) => self.compile_expr(builder, &reference.value),
            fa::Expr::Question(question) => self.compile_question(builder, question),
            fa::Expr::If(if_expr) => self.compile_if(builder, if_expr),
            fa::Expr::Match(match_expr) => self.compile_match(builder, match_expr),
            fa::Expr::When(when_expr) => self.compile_when(builder, when_expr),
            fa::Expr::Lambda(lambda) => self.compile_lambda(builder, lambda),
            fa::Expr::Tuple(tuple) => self.compile_tuple(builder, tuple),
        }
    }

    fn compile_literal(
        &mut self,
        builder: &mut FunctionBuilder,
        literal: &fa::Literal,
    ) -> Result<TypedValue, String> {
        let (value, ty) = match &literal.value {
            fa::LiteralValue::Int(value) => (
                {
                    let raw = builder.ins().iconst(types::I64, *value);
                    self.runtime(
                        builder,
                        self.compiler.runtime.int,
                        &[raw],
                        self.compiler.pointer_type,
                    )
                },
                Some("Int".to_string()),
            ),
            fa::LiteralValue::Bool(value) => (
                {
                    let raw = builder.ins().iconst(types::I8, i64::from(*value));
                    self.runtime(
                        builder,
                        self.compiler.runtime.bool_,
                        &[raw],
                        self.compiler.pointer_type,
                    )
                },
                Some("Bool".to_string()),
            ),
            fa::LiteralValue::String(value) => (self.string_value(builder, value)?, Some("String".to_string())),
            fa::LiteralValue::Float(value) => (
                {
                    let raw = builder.ins().f64const(*value);
                    self.runtime(
                        builder,
                        self.compiler.runtime.float,
                        &[raw],
                        self.compiler.pointer_type,
                    )
                },
                Some("Float".to_string()),
            )
        };
        Ok(TypedValue { value, ty })
    }

    fn compile_fstring(
        &mut self,
        builder: &mut FunctionBuilder,
        template: &str,
    ) -> Result<TypedValue, String> {
        let mut current = self.string_value(builder, "")?;
        let mut rest = template;
        while let Some(start) = rest.find('{') {
            if start > 0 {
                let piece = self.string_value(builder, &rest[..start])?;
                current = self.runtime(
                    builder,
                    self.compiler.runtime.concat,
                    &[current, piece],
                    self.compiler.pointer_type,
                );
            }
            let after = &rest[start + 1..];
            let end = fstring_brace_end(after)
                .ok_or_else(|| "unterminated f-string interpolation".to_string())?;
            let expr_text = after[..end].trim();
            let source = format!("fn __fstr__() => {expr_text}");
            let program = parse_source(&source, "<fstring>")
                .map_err(|d| format!("f-string parse error: {}", d.render()))?;
            let expr = program
                .declarations
                .first()
                .and_then(|decl| {
                    if let fa::Declaration::Function(func) = decl {
                        func.body.statements.first()
                    } else {
                        None
                    }
                })
                .and_then(|stmt| {
                    if let fa::Statement::Expr(expr_stmt) = stmt {
                        Some(&expr_stmt.expr)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| format!("failed to parse f-string expression: {expr_text}"))?;
            let value = self.compile_expr(builder, expr)?;
            let rendered = self.runtime(
                builder,
                self.compiler.runtime.to_string,
                &[value.value],
                self.compiler.pointer_type,
            );
            current = self.runtime(
                builder,
                self.compiler.runtime.concat,
                &[current, rendered],
                self.compiler.pointer_type,
            );
            rest = &after[end + 1..];
        }
        if !rest.is_empty() {
            let piece = self.string_value(builder, rest)?;
            current = self.runtime(
                builder,
                self.compiler.runtime.concat,
                &[current, piece],
                self.compiler.pointer_type,
            );
        }
        Ok(TypedValue {
            value: current,
            ty: Some("String".to_string()),
        })
    }

    fn call_zero_arg_member(
        &mut self,
        builder: &mut FunctionBuilder,
        receiver: TypedValue,
        name: &str,
    ) -> Result<TypedValue, String> {
        let receiver_type = receiver
            .ty
            .clone()
            .ok_or_else(|| format!("cannot infer receiver type for `{name}()`"))?;
        if layout::canonical_type_name(&receiver_type) == "Chan" {
            return match name {
                "recv" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.chan_recv,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: {
                        let inner = chan_inner_type(&receiver_type).unwrap_or_else(|| "Unknown".to_string());
                        Some(format!("Result<{inner}, String>"))
                    },
                }),
                "tryRecv" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.chan_try_recv,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: {
                        let inner = chan_inner_type(&receiver_type).unwrap_or_else(|| "Unknown".to_string());
                        Some(format!("Option<{inner}>"))
                    },
                }),
                "close" => {
                    self.runtime_void(
                        builder,
                        self.compiler.runtime.chan_close,
                        &[receiver.value],
                    );
                    Ok(TypedValue {
                        value: self.runtime_nullary(builder, self.compiler.runtime.unit),
                        ty: Some("Unit".to_string()),
                    })
                }
                "isClosed" => {
                    let raw = self.runtime_value(
                        builder,
                        self.compiler.runtime.chan_is_closed,
                        &[receiver.value],
                        types::I8,
                    );
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.bool_,
                            &[raw],
                            self.compiler.pointer_type,
                        ),
                        ty: Some("Bool".to_string()),
                    })
                }
                "len" => {
                    let raw_len = self.runtime_value(
                        builder,
                        self.compiler.runtime.chan_len,
                        &[receiver.value],
                        types::I64,
                    );
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.int,
                            &[raw_len],
                            self.compiler.pointer_type,
                        ),
                        ty: Some("Int".to_string()),
                    })
                }
                "cap" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.chan_cap,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("Option<Int>".to_string()),
                }),
                other => Err(format!("unsupported Chan zero-arg member `{other}()`")),
            };
        }
        if layout::canonical_type_name(&receiver_type) == "Map" {
            return match name {
                "len" => {
                    let raw_len = self.runtime_value(
                        builder,
                        self.compiler.runtime.map_len,
                        &[receiver.value],
                        types::I64,
                    );
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.int,
                            &[raw_len],
                            self.compiler.pointer_type,
                        ),
                        ty: Some("Int".to_string()),
                    })
                }
                "isEmpty" => {
                    let raw_len = self.runtime_value(
                        builder,
                        self.compiler.runtime.map_len,
                        &[receiver.value],
                        types::I64,
                    );
                    let is_zero = builder.ins().icmp_imm(IntCC::Equal, raw_len, 0);
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.bool_,
                            &[is_zero],
                            self.compiler.pointer_type,
                        ),
                        ty: Some("Bool".to_string()),
                    })
                }
                "keys" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.map_keys,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("List<String>".to_string()),
                }),
                "values" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.map_values,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("List<String>".to_string()),
                }),
                "entries" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.map_entries,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("List<(String,String)>".to_string()),
                }),
                other => Err(format!("unsupported Map zero-arg member `{other}()`")),
            };
        }
        if receiver_type.starts_with("List") {
            return match name {
                "len" => {
                    let raw_len = self.runtime_value(
                        builder,
                        self.compiler.runtime.list_len,
                        &[receiver.value],
                        types::I64,
                    );
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.int,
                            &[raw_len],
                            self.compiler.pointer_type,
                        ),
                        ty: Some("Int".to_string()),
                    })
                }
                other => Err(format!("unsupported List zero-arg member `{other}()`")),
            };
        }
        if layout::canonical_type_name(&receiver_type) == "String" {
            return match name {
                "toUpper" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.to_upper,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("String".to_string()),
                }),
                "isEmpty" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.string_is_empty,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("Bool".to_string()),
                }),
                "len" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.string_char_count,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("Int".to_string()),
                }),
                other => Err(format!("unsupported String zero-arg member `{other}()`")),
            };
        }
        if let Some((target_module, function)) = self
            .compiler
            .session
            .resolve_extension(&receiver_type, name)
        {
            let symbol = symbol_for_function(target_module, function);
            let func_id = *self
                .compiler
                .function_ids
                .get(&symbol)
                .ok_or_else(|| format!("missing function id for `{symbol}`"))?;
            let local = self.compiler.module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(local, &[receiver.value]);
            return Ok(TypedValue {
                value: builder.inst_results(call)[0],
                ty: function.return_type.clone(),
            });
        }
        Err(format!(
            "unsupported zero-arg member call `{name}()` on `{receiver_type}`"
        ))
    }

    fn compile_name(
        &mut self,
        builder: &mut FunctionBuilder,
        name: &str,
    ) -> Result<TypedValue, String> {
        if name == "None" {
            return Ok(TypedValue {
                value: self.runtime_nullary(builder, self.compiler.runtime.none),
                ty: Some("Option<Unknown>".to_string()),
            });
        }
        let binding = self
            .locals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unknown binding `{name}`"))?;
        Ok(TypedValue {
            value: builder.use_var(binding.var),
            ty: binding.ty,
        })
    }

    fn compile_list(
        &mut self,
        builder: &mut FunctionBuilder,
        list: &fa::ListExpr,
    ) -> Result<TypedValue, String> {
        let handle = self.runtime_nullary(builder, self.compiler.runtime.list_new);
        let mut item_type = None;
        for item in &list.items {
            let item_value = self.compile_expr(builder, item)?;
            if item_type.is_none() {
                item_type = item_value.ty.clone();
            }
            self.runtime_void(builder, self.compiler.runtime.list_push, &[handle, item_value.value]);
        }
        Ok(TypedValue {
            value: handle,
            ty: Some(format!(
                "List<{}>",
                item_type.unwrap_or_else(|| "Unknown".to_string())
            )),
        })
    }

    fn compile_tuple(
        &mut self,
        builder: &mut FunctionBuilder,
        tuple: &fa::TupleExpr,
    ) -> Result<TypedValue, String> {
        let handle = self.runtime_nullary(builder, self.compiler.runtime.list_new);
        let mut types = Vec::new();
        for item in &tuple.items {
            let item_value = self.compile_expr(builder, item)?;
            types.push(item_value.ty.clone().unwrap_or_else(|| "Unknown".to_string()));
            self.runtime_void(builder, self.compiler.runtime.list_push, &[handle, item_value.value]);
        }
        Ok(TypedValue {
            value: handle,
            ty: Some(format!("({})", types.join(","))),
        })
    }

    fn compile_unary(
        &mut self,
        builder: &mut FunctionBuilder,
        unary: &fa::UnaryOp,
    ) -> Result<TypedValue, String> {
        match unary.op.as_str() {
            "-" => {
                let value = self.compile_expr(builder, &unary.value)?;
                let raw_zero = builder.ins().iconst(types::I64, 0);
                let zero =
                    self.runtime(builder, self.compiler.runtime.int, &[raw_zero], self.compiler.pointer_type);
                Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.sub,
                        &[zero, value.value],
                        self.compiler.pointer_type,
                    ),
                    ty: value.ty,
                })
            }
            "not" => {
                let value = self.compile_expr(builder, &unary.value)?;
                let truthy = self.truthy_value(builder, value.value);
                let inverted = builder.ins().bxor_imm(truthy, 1);
                Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.bool_,
                        &[inverted],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("Bool".to_string()),
                })
            }
            other => Err(format!("unsupported unary operator `{other}`")),
        }
    }

    fn compile_binary(
        &mut self,
        builder: &mut FunctionBuilder,
        binary: &fa::BinaryOp,
    ) -> Result<TypedValue, String> {
        match binary.op.as_str() {
            "?:" => self.compile_elvis(builder, binary),
            "and" | "or" => self.compile_short_circuit(builder, binary),
            _ => {
                let left = self.compile_expr(builder, &binary.left)?;
                let right = self.compile_expr(builder, &binary.right)?;
                let rt = &self.compiler.runtime;
                // Dispatch arithmetic/comparison to the correct runtime functions based on operand type.
                let (add, sub, mul, div, mod_fn, eq, lt, le, gt, ge, result_ty) =
                    match left.ty.as_deref() {
                        Some("Float32") => (rt.f32_add, rt.f32_sub, rt.f32_mul, rt.f32_div, rt.f32_div, rt.f32_eq, rt.f32_lt, rt.f32_le, rt.f32_gt, rt.f32_ge, "Float32"),
                        Some("Int8") => (rt.i8.add, rt.i8.sub, rt.i8.mul, rt.i8.div, rt.i8.mod_, rt.i8.eq, rt.i8.lt, rt.i8.le, rt.i8.gt, rt.i8.ge, "Int8"),
                        Some("UInt8") => (rt.u8.add, rt.u8.sub, rt.u8.mul, rt.u8.div, rt.u8.mod_, rt.u8.eq, rt.u8.lt, rt.u8.le, rt.u8.gt, rt.u8.ge, "UInt8"),
                        Some("Int32") => (rt.i32.add, rt.i32.sub, rt.i32.mul, rt.i32.div, rt.i32.mod_, rt.i32.eq, rt.i32.lt, rt.i32.le, rt.i32.gt, rt.i32.ge, "Int32"),
                        Some("UInt32") => (rt.u32.add, rt.u32.sub, rt.u32.mul, rt.u32.div, rt.u32.mod_, rt.u32.eq, rt.u32.lt, rt.u32.le, rt.u32.gt, rt.u32.ge, "UInt32"),
                        Some("UInt64") => (rt.u64.add, rt.u64.sub, rt.u64.mul, rt.u64.div, rt.u64.mod_, rt.u64.eq, rt.u64.lt, rt.u64.le, rt.u64.gt, rt.u64.ge, "UInt64"),
                        _ => (rt.add, rt.sub, rt.mul, rt.div, rt.mod_, rt.eq, rt.lt, rt.le, rt.gt, rt.ge, "Int"),
                    };
                let (value, ty) = match binary.op.as_str() {
                    "+" => (
                        self.runtime(builder, add, &[left.value, right.value], self.compiler.pointer_type),
                        if result_ty == "Int" && (left.ty.as_deref() == Some("String") || right.ty.as_deref() == Some("String")) {
                            Some("String".to_string())
                        } else {
                            Some(result_ty.to_string())
                        },
                    ),
                    "-" => (self.runtime(builder, sub, &[left.value, right.value], self.compiler.pointer_type), Some(result_ty.to_string())),
                    "*" => (self.runtime(builder, mul, &[left.value, right.value], self.compiler.pointer_type), Some(result_ty.to_string())),
                    "/" => (self.runtime(builder, div, &[left.value, right.value], self.compiler.pointer_type), Some(result_ty.to_string())),
                    "%" => (self.runtime(builder, mod_fn, &[left.value, right.value], self.compiler.pointer_type), Some(result_ty.to_string())),
                    "==" => {
                        // Dispatch to eq() extension if available.
                        if let Some(recv_ty) = &left.ty {
                            if let Some(result) = self.try_extension_call(builder, recv_ty, "eq", &[left.value, right.value]) {
                                (result.value, Some("Bool".to_string()))
                            } else {
                                (self.runtime(builder, eq, &[left.value, right.value], self.compiler.pointer_type), Some("Bool".to_string()))
                            }
                        } else {
                            (self.runtime(builder, eq, &[left.value, right.value], self.compiler.pointer_type), Some("Bool".to_string()))
                        }
                    }
                    "!=" => {
                        // Dispatch to eq() extension + negate if available.
                        let eq_val = if let Some(recv_ty) = &left.ty {
                            if let Some(result) = self.try_extension_call(builder, recv_ty, "eq", &[left.value, right.value]) {
                                result.value
                            } else {
                                self.runtime(builder, eq, &[left.value, right.value], self.compiler.pointer_type)
                            }
                        } else {
                            self.runtime(builder, eq, &[left.value, right.value], self.compiler.pointer_type)
                        };
                        let truthy = self.truthy_value(builder, eq_val);
                        let inverted = builder.ins().bxor_imm(truthy, 1);
                        (self.runtime(builder, self.compiler.runtime.bool_, &[inverted], self.compiler.pointer_type), Some("Bool".to_string()))
                    }
                    "<" | "<=" | ">" | ">=" => {
                        // Try compareTo() extension dispatch for user types.
                        let cmp_result = left.ty.as_deref().and_then(|recv_ty| {
                            let r = self.try_extension_call(builder, recv_ty, "compareTo", &[left.value, right.value])?;
                            Some(r.value)
                        });
                        if let Some(cmp_val) = cmp_result {
                            let int_val = self.extract_int(builder, cmp_val);
                            let zero = builder.ins().iconst(types::I64, 0);
                            let cc = match binary.op.as_str() {
                                "<" => IntCC::SignedLessThan,
                                "<=" => IntCC::SignedLessThanOrEqual,
                                ">" => IntCC::SignedGreaterThan,
                                ">=" => IntCC::SignedGreaterThanOrEqual,
                                _ => unreachable!(),
                            };
                            let cmp = builder.ins().icmp(cc, int_val, zero);
                            (self.runtime(builder, self.compiler.runtime.bool_, &[cmp], self.compiler.pointer_type), Some("Bool".to_string()))
                        } else {
                            let fallback = match binary.op.as_str() {
                                "<" => lt,
                                "<=" => le,
                                ">" => gt,
                                ">=" => ge,
                                _ => unreachable!(),
                            };
                            (self.compare(builder, fallback, left.value, right.value), Some("Bool".to_string()))
                        }
                    }
                    other => return Err(format!("unsupported binary operator `{other}`")),
                };
                Ok(TypedValue { value, ty })
            }
        }
    }

    fn compare(
        &mut self,
        builder: &mut FunctionBuilder,
        func_id: FuncId,
        left: Value,
        right: Value,
    ) -> Value {
        self.runtime(builder, func_id, &[left, right], self.compiler.pointer_type)
    }

    /// Extract the raw i64 value from a FuseHandle wrapping an Int.
    fn extract_int(&mut self, builder: &mut FunctionBuilder, handle: Value) -> Value {
        self.runtime_value(builder, self.compiler.runtime.extract_int, &[handle], types::I64)
    }

    /// Try to call an extension method on a type. Returns None if the method
    /// doesn't exist as an extension.
    fn try_extension_call(
        &mut self,
        builder: &mut FunctionBuilder,
        receiver_type: &str,
        method: &str,
        args: &[Value],
    ) -> Option<TypedValue> {
        let (target_module, function) = self.compiler.session.resolve_extension(receiver_type, method)?;
        let symbol = symbol_for_function(target_module, function);
        let func_id = *self.compiler.function_ids.get(&symbol)?;
        let local = self.compiler.module.declare_func_in_func(func_id, builder.func);
        let call = builder.ins().call(local, args);
        Some(TypedValue {
            value: builder.inst_results(call)[0],
            ty: function.return_type.clone(),
        })
    }

    fn jump_value(&mut self, builder: &mut FunctionBuilder, block: cranelift_codegen::ir::Block, value: Value) {
        let args = [BlockArg::Value(value)];
        builder.ins().jump(block, &args);
    }

    fn compile_short_circuit(
        &mut self,
        builder: &mut FunctionBuilder,
        binary: &fa::BinaryOp,
    ) -> Result<TypedValue, String> {
        let left = self.compile_expr(builder, &binary.left)?;
        let left_truthy = self.truthy_value(builder, left.value);

        // Snapshot all live local variables so we can thread their SSA values
        // explicitly through the merge block.  This prevents Cranelift's
        // automatic SSA resolution from picking up stale values when
        // and/or appears inside loops with mutref writebacks.
        let live_vars: Vec<(String, Variable)> = self
            .locals
            .iter()
            .map(|(name, binding)| (name.clone(), binding.var))
            .collect();

        let rhs_block = builder.create_block();
        let fallback_block = builder.create_block();
        let done = builder.create_block();
        // Block param 0: the boolean result.
        builder.append_block_param(done, self.compiler.pointer_type);
        // Block params 1..N: each live local variable.
        for _ in &live_vars {
            builder.append_block_param(done, self.compiler.pointer_type);
        }

        match binary.op.as_str() {
            "and" => builder.ins().brif(left_truthy, rhs_block, &[], fallback_block, &[]),
            "or" => builder.ins().brif(left_truthy, fallback_block, &[], rhs_block, &[]),
            _ => unreachable!(),
        };

        // --- rhs_block: evaluate right operand ---
        builder.switch_to_block(rhs_block);
        let right = self.compile_expr(builder, &binary.right)?;
        let right_truthy = self.truthy_value(builder, right.value);
        let right_bool = self.runtime(
            builder,
            self.compiler.runtime.bool_,
            &[right_truthy],
            self.compiler.pointer_type,
        );
        let mut rhs_args: Vec<BlockArg> = vec![BlockArg::Value(right_bool)];
        for (_, var) in &live_vars {
            rhs_args.push(BlockArg::Value(builder.use_var(*var)));
        }
        builder.ins().jump(done, &rhs_args);

        // --- fallback_block: produce constant boolean ---
        builder.switch_to_block(fallback_block);
        let raw = builder
            .ins()
            .iconst(types::I8, if binary.op == "or" { 1 } else { 0 });
        let fallback = self.runtime(
            builder,
            self.compiler.runtime.bool_,
            &[raw],
            self.compiler.pointer_type,
        );
        let mut fallback_args: Vec<BlockArg> = vec![BlockArg::Value(fallback)];
        for (_, var) in &live_vars {
            fallback_args.push(BlockArg::Value(builder.use_var(*var)));
        }
        builder.ins().jump(done, &fallback_args);

        // --- done: merge block — rebind all live variables from block params ---
        builder.switch_to_block(done);
        let done_params = builder.block_params(done).to_vec();
        for (i, (name, _)) in live_vars.iter().enumerate() {
            if let Some(binding) = self.locals.get(name) {
                builder.def_var(binding.var, done_params[1 + i]);
            }
        }
        Ok(TypedValue {
            value: done_params[0],
            ty: Some("Bool".to_string()),
        })
    }

    fn compile_elvis(
        &mut self,
        builder: &mut FunctionBuilder,
        binary: &fa::BinaryOp,
    ) -> Result<TypedValue, String> {
        let left = self.compile_expr(builder, &binary.left)?;
        let some = self.runtime_value(builder, self.compiler.runtime.option_is_some, &[left.value], types::I8);

        // Snapshot live locals for explicit SSA threading (same fix as
        // compile_short_circuit — prevents stale values at merge point).
        let live_vars: Vec<(String, Variable)> = self
            .locals
            .iter()
            .map(|(name, binding)| (name.clone(), binding.var))
            .collect();

        let then_block = builder.create_block();
        let else_block = builder.create_block();
        let done = builder.create_block();
        builder.append_block_param(done, self.compiler.pointer_type);
        for _ in &live_vars {
            builder.append_block_param(done, self.compiler.pointer_type);
        }
        let cond = builder.ins().icmp_imm(IntCC::NotEqual, some, 0);
        builder.ins().brif(cond, then_block, &[], else_block, &[]);

        builder.switch_to_block(then_block);
        let inner = self.runtime(
            builder,
            self.compiler.runtime.option_unwrap,
            &[left.value],
            self.compiler.pointer_type,
        );
        let mut then_args: Vec<BlockArg> = vec![BlockArg::Value(inner)];
        for (_, var) in &live_vars {
            then_args.push(BlockArg::Value(builder.use_var(*var)));
        }
        builder.ins().jump(done, &then_args);

        builder.switch_to_block(else_block);
        let right = self.compile_expr(builder, &binary.right)?;
        let right_value = right.value;
        let right_ty = right.ty.clone();
        let mut else_args: Vec<BlockArg> = vec![BlockArg::Value(right_value)];
        for (_, var) in &live_vars {
            else_args.push(BlockArg::Value(builder.use_var(*var)));
        }
        builder.ins().jump(done, &else_args);

        builder.switch_to_block(done);
        let done_params = builder.block_params(done).to_vec();
        for (i, (name, _)) in live_vars.iter().enumerate() {
            if let Some(binding) = self.locals.get(name) {
                builder.def_var(binding.var, done_params[1 + i]);
            }
        }
        Ok(TypedValue {
            value: done_params[0],
            ty: left
                .ty
                .as_deref()
                .and_then(option_inner_type)
                .or(right_ty)
                .map(|value| value.to_string()),
        })
    }

    fn compile_call(
        &mut self,
        builder: &mut FunctionBuilder,
        call: &fa::Call,
    ) -> Result<TypedValue, String> {
        match call.callee.as_ref() {
            fa::Expr::Name(name) => {
                if self.locals.contains_key(&name.value) {
                    let binding = &self.locals[&name.value];
                    if binding.ty.as_ref().is_some_and(|t| t.starts_with("fn(")) {
                        return self.compile_indirect_call(builder, call);
                    }
                }
                self.compile_named_call(builder, &name.value, &call.args)
            }
            fa::Expr::Member(member) => self.compile_member_call(builder, member, &call.args),
            _ => self.compile_indirect_call(builder, call),
        }
    }

    fn compile_indirect_call(
        &mut self,
        builder: &mut FunctionBuilder,
        call: &fa::Call,
    ) -> Result<TypedValue, String> {
        let closure = self.compile_expr(builder, &call.callee)?;
        let zero = self.usize_const(builder, 0);
        let fn_ptr = self.runtime(
            builder,
            self.compiler.runtime.list_get,
            &[closure.value, zero],
            self.compiler.pointer_type,
        );
        // Uniform ABI: (closure_list, user_arg1, user_arg2, ...) -> result
        let mut lowered_args = vec![closure.value];
        for arg in &call.args {
            lowered_args.push(self.compile_expr(builder, arg)?.value);
        }
        let sig = self.compiler.handle_signature(lowered_args.len());
        let sig_ref = builder.import_signature(sig);
        let inst = builder.ins().call_indirect(sig_ref, fn_ptr, &lowered_args);
        Ok(TypedValue {
            value: builder.inst_results(inst)[0],
            ty: None,
        })
    }

    fn compile_named_call(
        &mut self,
        builder: &mut FunctionBuilder,
        name: &str,
        args: &[fa::Expr],
    ) -> Result<TypedValue, String> {
        match name {
            "println" => {
                let value = self.compile_expr(
                    builder,
                    args.first()
                        .ok_or_else(|| "println requires an argument".to_string())?,
                )?;
                self.runtime_void(builder, self.compiler.runtime.println, &[value.value]);
                return Ok(TypedValue {
                    value: self.runtime_nullary(builder, self.compiler.runtime.unit),
                    ty: Some("Unit".to_string()),
                });
            }
            "Some" => {
                let value = self.compile_expr(builder, &args[0])?;
                return Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.some,
                        &[value.value],
                        self.compiler.pointer_type,
                    ),
                    ty: value.ty.map(|inner| format!("Option<{inner}>")),
                });
            }
            "Ok" => {
                let value = self.compile_expr(builder, &args[0])?;
                return Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.ok,
                        &[value.value],
                        self.compiler.pointer_type,
                    ),
                    ty: value.ty.map(|inner| format!("Result<{inner}, Unknown>")),
                });
            }
            "Err" => {
                let value = self.compile_expr(builder, &args[0])?;
                return Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.err,
                        &[value.value],
                        self.compiler.pointer_type,
                    ),
                    ty: value.ty.map(|inner| format!("Result<Unknown, {inner}>")),
                });
            }
            _ => {}
        }

        if let Some((data_module, data)) = self.compiler.session.find_data(name) {
            return self.compile_data_constructor(builder, data_module, data, args);
        }

        if let Some((struct_module, s)) = self.compiler.session.find_struct(name) {
            return self.compile_struct_constructor(builder, struct_module, s, args);
        }

        if let Some(extern_fn) = self.compiler.session.find_extern_fn(name) {
            let func_id = *self
                .compiler
                .function_ids
                .get(&extern_fn.name)
                .ok_or_else(|| format!("missing extern function id for `{}`", extern_fn.name))?;
            let local = self.compiler.module.declare_func_in_func(func_id, builder.func);
            let mut lowered_args = Vec::with_capacity(args.len());
            for arg in args {
                lowered_args.push(self.compile_expr(builder, arg)?.value);
            }
            let call = builder.ins().call(local, &lowered_args);
            let results = builder.inst_results(call);
            return Ok(TypedValue {
                value: if results.is_empty() {
                    self.runtime_nullary(builder, self.compiler.runtime.unit)
                } else {
                    results[0]
                },
                ty: extern_fn.return_type.clone(),
            });
        }

        let (target_module, function) = self
            .compiler
            .session
            .resolve_function(name)
            .ok_or_else(|| format!("unknown function `{name}`"))?;
        let symbol = symbol_for_function(target_module, function);
        let func_id = *self
            .compiler
            .function_ids
            .get(&symbol)
            .ok_or_else(|| format!("missing function id for `{symbol}`"))?;
        let local = self.compiler.module.declare_func_in_func(func_id, builder.func);
        let lowered_args = self.lower_args_with_variadic(builder, &function.params, args)?;
        let call = builder.ins().call(local, &lowered_args);
        Ok(TypedValue {
            value: builder.inst_results(call)[0],
            ty: function.return_type.clone(),
        })
    }

    fn lower_args_with_variadic(
        &mut self,
        builder: &mut FunctionBuilder,
        params: &[fa::Param],
        args: &[fa::Expr],
    ) -> Result<Vec<Value>, String> {
        let has_variadic = params.last().is_some_and(|p| p.variadic);
        if !has_variadic {
            let mut lowered = Vec::with_capacity(args.len());
            for arg in args {
                lowered.push(self.compile_expr(builder, arg)?.value);
            }
            return Ok(lowered);
        }
        let fixed_count = params.len() - 1;
        let mut lowered = Vec::with_capacity(params.len());
        for arg in args.iter().take(fixed_count) {
            lowered.push(self.compile_expr(builder, arg)?.value);
        }
        let list = self.runtime_nullary(builder, self.compiler.runtime.list_new);
        for arg in args.iter().skip(fixed_count) {
            let val = self.compile_expr(builder, arg)?.value;
            self.runtime_void(builder, self.compiler.runtime.list_push, &[list, val]);
        }
        lowered.push(list);
        Ok(lowered)
    }

    fn compile_type_namespace_call(
        &mut self,
        builder: &mut FunctionBuilder,
        namespace: &str,
        member: &str,
        _args: &[fa::Expr],
    ) -> Result<TypedValue, String> {
        let base = layout::canonical_type_name(namespace);
        match (base, member) {
            ("Chan", "new") => Ok(TypedValue {
                value: self.runtime_nullary(builder, self.compiler.runtime.chan_new),
                ty: Some(namespace.replace("::", "")),
            }),
            ("Chan", "bounded") => {
                let arg = self.compile_expr(
                    builder,
                    _args.first().ok_or_else(|| "Chan::<T>.bounded requires a capacity".to_string())?,
                )?;
                let chan = self.runtime(
                    builder,
                    self.compiler.runtime.chan_bounded,
                    &[arg.value],
                    self.compiler.pointer_type,
                );
                // Return (Chan<T>, Chan<T>) tuple — both halves share the same channel
                let tuple = self.runtime_nullary(builder, self.compiler.runtime.list_new);
                self.runtime_void(builder, self.compiler.runtime.list_push, &[tuple, chan]);
                self.runtime_void(builder, self.compiler.runtime.list_push, &[tuple, chan]);
                let chan_type = namespace.replace("::", "");
                Ok(TypedValue {
                    value: tuple,
                    ty: Some(format!("({chan_type},{chan_type})")),
                })
            }
            ("Chan", "unbounded") => {
                let chan = self.runtime_nullary(builder, self.compiler.runtime.chan_new);
                // Return (Chan<T>, Chan<T>) tuple — both halves share the same channel
                let tuple = self.runtime_nullary(builder, self.compiler.runtime.list_new);
                self.runtime_void(builder, self.compiler.runtime.list_push, &[tuple, chan]);
                self.runtime_void(builder, self.compiler.runtime.list_push, &[tuple, chan]);
                let chan_type = namespace.replace("::", "");
                Ok(TypedValue {
                    value: tuple,
                    ty: Some(format!("({chan_type},{chan_type})")),
                })
            }
            ("Shared", "new") => {
                let init = if let Some(arg) = _args.first() {
                    self.compile_expr(builder, arg)?.value
                } else {
                    self.runtime_nullary(builder, self.compiler.runtime.unit)
                };
                Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.shared_new,
                        &[init],
                        self.compiler.pointer_type,
                    ),
                    ty: Some(namespace.replace("::", "")),
                })
            }
            ("SIMD", method) => {
                let (simd_type, lane_count) = parse_simd_params(namespace)?;
                validate_simd_type(&simd_type)?;
                validate_simd_lanes(lane_count)?;
                let scalar_type = if simd_type.starts_with("Float") { "Float" } else { "Int" };
                let list_type = format!("List<{scalar_type}>");
                let native = cranelift_simd_type(&simd_type, lane_count);
                match method {
                    // Constructors: one list arg → list result
                    "fromList" => {
                        let list = self.compile_expr(builder, _args.first()
                            .ok_or_else(|| "SIMD.fromList requires one argument".to_string())?)?;
                        Ok(TypedValue { value: list.value, ty: Some(list_type) })
                    }
                    // broadcast(value) → list of N copies
                    "broadcast" => {
                        let value = self.compile_expr(builder, _args.first()
                            .ok_or_else(|| "SIMD.broadcast requires one argument".to_string())?)?;
                        if let Some(vec_ty) = native {
                            let lane_ty = simd_lane_type(vec_ty);
                            let extract_fn = match lane_ty {
                                types::F64 => self.compiler.runtime.simd_extract_raw_f64,
                                types::F32 => self.compiler.runtime.simd_extract_raw_f32,
                                types::I64 => self.compiler.runtime.simd_extract_raw_i64,
                                types::I32 => self.compiler.runtime.simd_extract_raw_i32,
                                _ => unreachable!(),
                            };
                            let scalar = self.runtime_value(builder, extract_fn, &[value.value], lane_ty);
                            let vec = builder.ins().splat(vec_ty, scalar);
                            let packed = self.simd_pack_to_list(builder, vec, vec_ty);
                            Ok(TypedValue { value: packed, ty: Some(list_type) })
                        } else {
                            let lanes = builder.ins().iconst(types::I64, lane_count as i64);
                            Ok(TypedValue {
                                value: self.runtime(builder, self.compiler.runtime.simd_broadcast,
                                    &[value.value, lanes], self.compiler.pointer_type),
                                ty: Some(list_type),
                            })
                        }
                    }
                    // Reductions: one list arg → scalar result
                    "sum" => {
                        let list = self.compile_expr(builder, _args.first()
                            .ok_or_else(|| "SIMD.sum requires one argument".to_string())?)?;
                        if let Some(vec_ty) = native {
                            let (native_block, fallback_block, done) =
                                self.simd_length_guard(builder, list.value, lane_count);

                            builder.switch_to_block(native_block);
                            let v = self.simd_unpack_list(builder, list.value, vec_ty);
                            let lane_ty = simd_lane_type(vec_ty);
                            let is_float = simd_is_float(vec_ty);
                            let count = simd_lane_count(vec_ty);
                            let mut acc = builder.ins().extractlane(v, 0u8);
                            for i in 1..count {
                                let lane = builder.ins().extractlane(v, i);
                                acc = if is_float { builder.ins().fadd(acc, lane) }
                                      else { builder.ins().iadd(acc, lane) };
                            }
                            let boxed = self.simd_box_scalar(builder, acc, lane_ty);
                            builder.ins().jump(done, &[BlockArg::Value(boxed)]);

                            builder.switch_to_block(fallback_block);
                            let rt = self.runtime(builder, self.compiler.runtime.simd_sum,
                                &[list.value], self.compiler.pointer_type);
                            builder.ins().jump(done, &[BlockArg::Value(rt)]);

                            builder.switch_to_block(done);
                            Ok(TypedValue { value: builder.block_params(done)[0], ty: Some(scalar_type.to_string()) })
                        } else {
                            Ok(TypedValue {
                                value: self.runtime(builder, self.compiler.runtime.simd_sum,
                                    &[list.value], self.compiler.pointer_type),
                                ty: Some(scalar_type.to_string()),
                            })
                        }
                    }
                    // Two-list reductions: two list args → scalar result
                    "dot" => {
                        let a = self.compile_expr(builder, _args.get(0)
                            .ok_or_else(|| "SIMD.dot requires two arguments".to_string())?)?;
                        let b = self.compile_expr(builder, _args.get(1)
                            .ok_or_else(|| "SIMD.dot requires two arguments".to_string())?)?;
                        if let Some(vec_ty) = native {
                            let (native_block, fallback_block, done) =
                                self.simd_length_guard(builder, a.value, lane_count);

                            builder.switch_to_block(native_block);
                            let va = self.simd_unpack_list(builder, a.value, vec_ty);
                            let vb = self.simd_unpack_list(builder, b.value, vec_ty);
                            let is_float = simd_is_float(vec_ty);
                            let product = if is_float { builder.ins().fmul(va, vb) }
                                          else { builder.ins().imul(va, vb) };
                            let lane_ty = simd_lane_type(vec_ty);
                            let count = simd_lane_count(vec_ty);
                            let mut acc = builder.ins().extractlane(product, 0u8);
                            for i in 1..count {
                                let lane = builder.ins().extractlane(product, i);
                                acc = if is_float { builder.ins().fadd(acc, lane) }
                                      else { builder.ins().iadd(acc, lane) };
                            }
                            let boxed = self.simd_box_scalar(builder, acc, lane_ty);
                            builder.ins().jump(done, &[BlockArg::Value(boxed)]);

                            builder.switch_to_block(fallback_block);
                            let rt = self.runtime(builder, self.compiler.runtime.simd_dot,
                                &[a.value, b.value], self.compiler.pointer_type);
                            builder.ins().jump(done, &[BlockArg::Value(rt)]);

                            builder.switch_to_block(done);
                            Ok(TypedValue { value: builder.block_params(done)[0], ty: Some(scalar_type.to_string()) })
                        } else {
                            Ok(TypedValue {
                                value: self.runtime(builder, self.compiler.runtime.simd_dot,
                                    &[a.value, b.value], self.compiler.pointer_type),
                                ty: Some(scalar_type.to_string()),
                            })
                        }
                    }
                    // Elementwise binary: two list args → list result
                    "add" | "sub" | "mul" | "div" | "min" | "max" => {
                        let a = self.compile_expr(builder, _args.get(0)
                            .ok_or_else(|| format!("SIMD.{method} requires two arguments"))?)?;
                        let b = self.compile_expr(builder, _args.get(1)
                            .ok_or_else(|| format!("SIMD.{method} requires two arguments"))?)?;
                        let runtime_func = match method {
                            "add" => self.compiler.runtime.simd_add,
                            "sub" => self.compiler.runtime.simd_sub,
                            "mul" => self.compiler.runtime.simd_mul,
                            "div" => self.compiler.runtime.simd_div,
                            "min" => self.compiler.runtime.simd_min,
                            "max" => self.compiler.runtime.simd_max,
                            _ => unreachable!(),
                        };
                        if let Some(vec_ty) = native {
                            let is_float = simd_is_float(vec_ty);
                            // Integer div has no vector instruction; always use runtime.
                            if method == "div" && !is_float {
                                return Ok(TypedValue {
                                    value: self.runtime(builder, runtime_func,
                                        &[a.value, b.value], self.compiler.pointer_type),
                                    ty: Some(list_type),
                                });
                            }
                            let (native_block, fallback_block, done) =
                                self.simd_length_guard(builder, a.value, lane_count);

                            builder.switch_to_block(native_block);
                            let va = self.simd_unpack_list(builder, a.value, vec_ty);
                            let vb = self.simd_unpack_list(builder, b.value, vec_ty);
                            let result_vec = match (method, is_float) {
                                ("add", true) => builder.ins().fadd(va, vb),
                                ("add", false) => builder.ins().iadd(va, vb),
                                ("sub", true) => builder.ins().fsub(va, vb),
                                ("sub", false) => builder.ins().isub(va, vb),
                                ("mul", true) => builder.ins().fmul(va, vb),
                                ("mul", false) => builder.ins().imul(va, vb),
                                ("div", true) => builder.ins().fdiv(va, vb),
                                ("min", true) => builder.ins().fmin(va, vb),
                                ("min", false) => builder.ins().smin(va, vb),
                                ("max", true) => builder.ins().fmax(va, vb),
                                ("max", false) => builder.ins().smax(va, vb),
                                _ => unreachable!(),
                            };
                            let packed = self.simd_pack_to_list(builder, result_vec, vec_ty);
                            builder.ins().jump(done, &[BlockArg::Value(packed)]);

                            builder.switch_to_block(fallback_block);
                            let rt = self.runtime(builder, runtime_func,
                                &[a.value, b.value], self.compiler.pointer_type);
                            builder.ins().jump(done, &[BlockArg::Value(rt)]);

                            builder.switch_to_block(done);
                            Ok(TypedValue { value: builder.block_params(done)[0], ty: Some(list_type) })
                        } else {
                            Ok(TypedValue {
                                value: self.runtime(builder, runtime_func,
                                    &[a.value, b.value], self.compiler.pointer_type),
                                ty: Some(list_type),
                            })
                        }
                    }
                    // Unary list → list
                    "abs" => {
                        let list = self.compile_expr(builder, _args.first()
                            .ok_or_else(|| "SIMD.abs requires one argument".to_string())?)?;
                        if let Some(vec_ty) = native {
                            let (native_block, fallback_block, done) =
                                self.simd_length_guard(builder, list.value, lane_count);

                            builder.switch_to_block(native_block);
                            let v = self.simd_unpack_list(builder, list.value, vec_ty);
                            let result_vec = if simd_is_float(vec_ty) {
                                builder.ins().fabs(v)
                            } else {
                                builder.ins().iabs(v)
                            };
                            let packed = self.simd_pack_to_list(builder, result_vec, vec_ty);
                            builder.ins().jump(done, &[BlockArg::Value(packed)]);

                            builder.switch_to_block(fallback_block);
                            let rt = self.runtime(builder, self.compiler.runtime.simd_abs,
                                &[list.value], self.compiler.pointer_type);
                            builder.ins().jump(done, &[BlockArg::Value(rt)]);

                            builder.switch_to_block(done);
                            Ok(TypedValue { value: builder.block_params(done)[0], ty: Some(list_type) })
                        } else {
                            Ok(TypedValue {
                                value: self.runtime(builder, self.compiler.runtime.simd_abs,
                                    &[list.value], self.compiler.pointer_type),
                                ty: Some(list_type),
                            })
                        }
                    }
                    "sqrt" => {
                        let list = self.compile_expr(builder, _args.first()
                            .ok_or_else(|| "SIMD.sqrt requires one argument".to_string())?)?;
                        if let Some(vec_ty) = native {
                            let (native_block, fallback_block, done) =
                                self.simd_length_guard(builder, list.value, lane_count);

                            builder.switch_to_block(native_block);
                            // sqrt always produces float results
                            let float_vec_ty = if simd_is_float(vec_ty) { vec_ty }
                                else { match vec_ty {
                                    types::I32X4 => types::F32X4,
                                    types::I64X2 => types::F64X2,
                                    _ => unreachable!(),
                                }};
                            let v = self.simd_unpack_list(builder, list.value, float_vec_ty);
                            let result_vec = builder.ins().sqrt(v);
                            let packed = self.simd_pack_to_list(builder, result_vec, float_vec_ty);
                            builder.ins().jump(done, &[BlockArg::Value(packed)]);

                            builder.switch_to_block(fallback_block);
                            let rt = self.runtime(builder, self.compiler.runtime.simd_sqrt,
                                &[list.value], self.compiler.pointer_type);
                            builder.ins().jump(done, &[BlockArg::Value(rt)]);

                            builder.switch_to_block(done);
                            Ok(TypedValue { value: builder.block_params(done)[0], ty: Some("List<Float>".to_string()) })
                        } else {
                            Ok(TypedValue {
                                value: self.runtime(builder, self.compiler.runtime.simd_sqrt,
                                    &[list.value], self.compiler.pointer_type),
                                ty: Some(list_type),
                            })
                        }
                    }
                    // toList: identity (SIMD vectors are already lists)
                    "toList" => {
                        let list = self.compile_expr(builder, _args.first()
                            .ok_or_else(|| "SIMD.toList requires one argument".to_string())?)?;
                        Ok(TypedValue { value: list.value, ty: Some(list_type) })
                    }
                    // get(list, index) → scalar
                    "get" => {
                        let list = self.compile_expr(builder, _args.get(0)
                            .ok_or_else(|| "SIMD.get requires two arguments".to_string())?)?;
                        let index = self.compile_expr(builder, _args.get(1)
                            .ok_or_else(|| "SIMD.get requires two arguments".to_string())?)?;
                        Ok(TypedValue {
                            value: self.runtime(builder, self.compiler.runtime.simd_get,
                                &[list.value, index.value], self.compiler.pointer_type),
                            ty: Some(scalar_type.to_string()),
                        })
                    }
                    // len(list) → Int
                    "len" => {
                        if native.is_some() {
                            let val = builder.ins().iconst(types::I64, lane_count as i64);
                            let boxed = self.runtime(builder, self.compiler.runtime.int,
                                &[val], self.compiler.pointer_type);
                            Ok(TypedValue { value: boxed, ty: Some("Int".to_string()) })
                        } else {
                            let list = self.compile_expr(builder, _args.first()
                                .ok_or_else(|| "SIMD.len requires one argument".to_string())?)?;
                            Ok(TypedValue {
                                value: self.runtime(builder, self.compiler.runtime.simd_len,
                                    &[list.value], self.compiler.pointer_type),
                                ty: Some("Int".to_string()),
                            })
                        }
                    }
                    _ => Err(format!("unsupported SIMD method `{method}`")),
                }
            }
            ("Map", "new") => Ok(TypedValue {
                value: self.runtime_nullary(builder, self.compiler.runtime.map_new),
                ty: Some(namespace.replace("::", "").to_string()),
            }),
            _ => {
                if let Some(enum_decl) = self.compiler.session.find_enum(base) {
                    let variant_index = enum_decl
                        .variants
                        .iter()
                        .position(|v| v.name == member)
                        .ok_or_else(|| format!("unknown variant `{member}` on enum `{base}`"))?;
                    let type_data_id = self.compiler.string_data_id(base)?;
                    let type_local = self.compiler.module.declare_data_in_func(type_data_id, builder.func);
                    let type_ptr = builder.ins().symbol_value(self.compiler.pointer_type, type_local);
                    let type_len = self.usize_const(builder, base.len() as i64);
                    let variant_data_id = self.compiler.string_data_id(member)?;
                    let variant_local = self.compiler.module.declare_data_in_func(variant_data_id, builder.func);
                    let variant_ptr = builder.ins().symbol_value(self.compiler.pointer_type, variant_local);
                    let variant_len = self.usize_const(builder, member.len() as i64);
                    let tag = builder.ins().iconst(types::I64, variant_index as i64);
                    let first_payload = if _args.is_empty() {
                        builder.ins().iconst(self.compiler.pointer_type, 0)
                    } else {
                        self.compile_expr(builder, &_args[0])?.value
                    };
                    let result = self.runtime(
                        builder,
                        self.compiler.runtime.enum_new,
                        &[type_ptr, type_len, tag, variant_ptr, variant_len, first_payload],
                        self.compiler.pointer_type,
                    );
                    // Add remaining payloads for multi-payload variants.
                    for extra_arg in _args.iter().skip(1) {
                        let extra = self.compile_expr(builder, extra_arg)?.value;
                        self.runtime_void(builder, self.compiler.runtime.enum_add_payload, &[result, extra]);
                    }
                    return Ok(TypedValue {
                        value: result,
                        ty: Some(base.to_string()),
                    });
                }
                if let Some((target_module, function)) = self
                    .compiler
                    .session
                    .resolve_static(base, member)
                {
                    let symbol = symbol_for_function(target_module, function);
                    let func_id = *self
                        .compiler
                        .function_ids
                        .get(&symbol)
                        .ok_or_else(|| format!("missing function id for `{symbol}`"))?;
                    let local = self.compiler.module.declare_func_in_func(func_id, builder.func);
                    let lowered = self.lower_args_with_variadic(builder, &function.params, _args)?;
                    let call = builder.ins().call(local, &lowered);
                    return Ok(TypedValue {
                        value: builder.inst_results(call)[0],
                        ty: function.return_type.clone(),
                    });
                }
                // Fallback: try module-qualified function call (e.g., string.fromChar).
                if let Some((target_module, function)) = self
                    .compiler
                    .session
                    .resolve_module_function(self.module_path, base, member)
                {
                    let symbol = symbol_for_function(target_module, function);
                    let func_id = *self
                        .compiler
                        .function_ids
                        .get(&symbol)
                        .ok_or_else(|| format!("missing function id for `{symbol}`"))?;
                    let local = self.compiler.module.declare_func_in_func(func_id, builder.func);
                    let lowered = self.lower_args_with_variadic(builder, &function.params, _args)?;
                    let call = builder.ins().call(local, &lowered);
                    return Ok(TypedValue {
                        value: builder.inst_results(call)[0],
                        ty: function.return_type.clone(),
                    });
                }
                Err(format!("unsupported type namespace call `{namespace}.{member}`"))
            }
        }
    }

    fn compile_member_call(
        &mut self,
        builder: &mut FunctionBuilder,
        member: &fa::Member,
        args: &[fa::Expr],
    ) -> Result<TypedValue, String> {
        if let fa::Expr::Name(name) = member.object.as_ref() {
            if !self.locals.contains_key(&name.value) {
                return self.compile_type_namespace_call(builder, &name.value, &member.name, args);
            }
        }
        let receiver = self.compile_expr(builder, &member.object)?;
        let receiver_type = receiver
            .ty
            .clone()
            .ok_or_else(|| format!("cannot infer receiver type for `{}`", member.name))?;

        // Optional chaining: receiver?.method() — unwrap Option, call on inner, rewrap.
        if member.optional {
            if let Some(inner_type) = option_inner_type(&receiver_type) {
                let some = self.runtime_value(
                    builder, self.compiler.runtime.option_is_some,
                    &[receiver.value], types::I8,
                );
                let then_block = builder.create_block();
                let else_block = builder.create_block();
                let done = builder.create_block();
                builder.append_block_param(done, self.compiler.pointer_type);
                let cond = builder.ins().icmp_imm(IntCC::NotEqual, some, 0);
                builder.ins().brif(cond, then_block, &[], else_block, &[]);

                builder.switch_to_block(then_block);
                let inner = self.runtime(
                    builder, self.compiler.runtime.option_unwrap,
                    &[receiver.value], self.compiler.pointer_type,
                );
                // Recurse with a non-optional Member using the unwrapped receiver.
                let inner_member = fa::Member {
                    object: member.object.clone(),
                    name: member.name.clone(),
                    optional: false,
                    span: member.span,
                };
                // We need to compile the call on the unwrapped value. Build a fake
                // Name expression that resolves to a temp variable holding `inner`.
                let temp_name = format!("__opt_chain_{}", self.locals.len());
                let temp_var = self.new_var(builder, self.compiler.pointer_type);
                builder.def_var(temp_var, inner);
                self.locals.insert(temp_name.clone(), LocalBinding {
                    var: temp_var, ty: Some(inner_type.clone()), destroy: false,
                });
                // Dispatch the method call on the unwrapped type.
                let inner_member2 = fa::Member {
                    object: Box::new(fa::Expr::Name(fa::Name {
                        value: temp_name.clone(),
                        span: member.span,
                    })),
                    name: member.name.clone(),
                    optional: false,
                    span: member.span,
                };
                let inner_result = self.compile_member_call(builder, &inner_member2, args)?;
                self.locals.remove(&temp_name);
                let wrapped = self.runtime(
                    builder, self.compiler.runtime.some,
                    &[inner_result.value], self.compiler.pointer_type,
                );
                self.jump_value(builder, done, wrapped);

                builder.switch_to_block(else_block);
                let none = self.runtime_nullary(builder, self.compiler.runtime.none);
                self.jump_value(builder, done, none);

                builder.switch_to_block(done);
                return Ok(TypedValue {
                    value: builder.block_params(done)[0],
                    ty: inner_result.ty.map(|t| format!("Option<{t}>")),
                });
            }
        }
        if let Some((target_module, function)) = self
            .compiler
            .session
            .resolve_extension(&receiver_type, &member.name)
        {
            let symbol = symbol_for_function(target_module, function);
            let func_id = *self
                .compiler
                .function_ids
                .get(&symbol)
                .ok_or_else(|| format!("missing function id for `{symbol}`"))?;
            let local = self.compiler.module.declare_func_in_func(func_id, builder.func);
            let mut lowered_args = vec![receiver.value];
            for arg in args {
                lowered_args.push(self.compile_expr(builder, arg)?.value);
            }
            let call = builder.ins().call(local, &lowered_args);
            return Ok(TypedValue {
                value: builder.inst_results(call)[0],
                ty: function.return_type.clone(),
            });
        }
        if receiver_type.starts_with("List") {
            return match member.name.as_str() {
                "len" => {
                    let raw_len = self.runtime_value(
                        builder,
                        self.compiler.runtime.list_len,
                        &[receiver.value],
                        types::I64,
                    );
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.int,
                            &[raw_len],
                            self.compiler.pointer_type,
                        ),
                        ty: Some("Int".to_string()),
                    })
                }
                "get" => {
                    let index = self.compile_expr(builder, &args[0])?;
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.rt_list_get,
                            &[receiver.value, index.value],
                            self.compiler.pointer_type,
                        ),
                        ty: {
                            // List<X>.get(i) returns Option<X>
                            let inner = receiver_type.strip_prefix("List<")
                                .and_then(|s| s.strip_suffix('>'))
                                .unwrap_or("Unknown")
                                .to_string();
                            Some(format!("Option<{inner}>"))
                        },
                    })
                }
                "push" => {
                    let item = self.compile_expr(builder, &args[0])?;
                    self.runtime_void(
                        builder,
                        self.compiler.runtime.list_push,
                        &[receiver.value, item.value],
                    );
                    Ok(TypedValue {
                        value: self.runtime_nullary(builder, self.compiler.runtime.unit),
                        ty: Some("Unit".to_string()),
                    })
                }
                _ => {
                    // Fall through to extension resolution below.
                    Err(format!("unsupported List member call `{}`", member.name))
                }
            };
        }
        if layout::canonical_type_name(&receiver_type) == "Chan" {
            return match member.name.as_str() {
                "send" => {
                    let value = self.compile_expr(builder, &args[0])?;
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.chan_send,
                            &[receiver.value, value.value],
                            self.compiler.pointer_type,
                        ),
                        ty: Some("Result<Unit, String>".to_string()),
                    })
                }
                "recv" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.chan_recv,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: {
                        let inner = chan_inner_type(&receiver_type).unwrap_or_else(|| "Unknown".to_string());
                        Some(format!("Result<{inner}, String>"))
                    },
                }),
                "close" => {
                    self.runtime_void(
                        builder,
                        self.compiler.runtime.chan_close,
                        &[receiver.value],
                    );
                    Ok(TypedValue {
                        value: self.runtime_nullary(builder, self.compiler.runtime.unit),
                        ty: Some("Unit".to_string()),
                    })
                }
                "tryRecv" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.chan_try_recv,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: {
                        let inner = chan_inner_type(&receiver_type).unwrap_or_else(|| "Unknown".to_string());
                        Some(format!("Option<{inner}>"))
                    },
                }),
                "isClosed" => {
                    let raw = self.runtime_value(
                        builder,
                        self.compiler.runtime.chan_is_closed,
                        &[receiver.value],
                        types::I8,
                    );
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.bool_,
                            &[raw],
                            self.compiler.pointer_type,
                        ),
                        ty: Some("Bool".to_string()),
                    })
                }
                "len" => {
                    let raw_len = self.runtime_value(
                        builder,
                        self.compiler.runtime.chan_len,
                        &[receiver.value],
                        types::I64,
                    );
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.int,
                            &[raw_len],
                            self.compiler.pointer_type,
                        ),
                        ty: Some("Int".to_string()),
                    })
                }
                "cap" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.chan_cap,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("Option<Int>".to_string()),
                }),
                other => Err(format!("unsupported Chan member call `{other}`")),
            };
        }
        if layout::canonical_type_name(&receiver_type) == "Shared" {
            return match member.name.as_str() {
                "read" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.shared_read,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: shared_inner_type(&receiver_type).or(Some("Unit".to_string())),
                }),
                "write" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.shared_write,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: shared_inner_type(&receiver_type).or(Some("Unit".to_string())),
                }),
                "try_write" | "tryWrite" => {
                    let timeout = self.compile_expr(
                        builder,
                        args.first()
                            .ok_or_else(|| "Shared.tryWrite requires a timeout argument".to_string())?,
                    )?;
                    let inner = shared_inner_type(&receiver_type).unwrap_or_else(|| "Unit".to_string());
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.shared_try_write,
                            &[receiver.value, timeout.value],
                            self.compiler.pointer_type,
                        ),
                        ty: Some(format!("Result<{inner}, String>")),
                    })
                }
                "tryRead" => {
                    let timeout = self.compile_expr(
                        builder,
                        args.first()
                            .ok_or_else(|| "Shared.tryRead requires a timeout argument".to_string())?,
                    )?;
                    let inner = shared_inner_type(&receiver_type).unwrap_or_else(|| "Unit".to_string());
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.shared_try_read,
                            &[receiver.value, timeout.value],
                            self.compiler.pointer_type,
                        ),
                        ty: Some(format!("Result<{inner}, String>")),
                    })
                }
                other => Err(format!("unsupported Shared member call `{other}`")),
            };
        }
        if layout::canonical_type_name(&receiver_type) == "Map" {
            return match member.name.as_str() {
                "set" => {
                    let key = self.compile_expr(builder, &args[0])?;
                    let value = self.compile_expr(builder, &args[1])?;
                    self.runtime_void(
                        builder,
                        self.compiler.runtime.map_set,
                        &[receiver.value, key.value, value.value],
                    );
                    Ok(TypedValue {
                        value: self.runtime_nullary(builder, self.compiler.runtime.unit),
                        ty: Some("Unit".to_string()),
                    })
                }
                "get" => {
                    let key = self.compile_expr(builder, &args[0])?;
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.rt_map_get,
                            &[receiver.value, key.value],
                            self.compiler.pointer_type,
                        ),
                        ty: None,
                    })
                }
                "remove" => {
                    let key = self.compile_expr(builder, &args[0])?;
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.map_remove,
                            &[receiver.value, key.value],
                            self.compiler.pointer_type,
                        ),
                        ty: None,
                    })
                }
                "len" => {
                    let raw_len = self.runtime_value(
                        builder,
                        self.compiler.runtime.map_len,
                        &[receiver.value],
                        types::I64,
                    );
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.int,
                            &[raw_len],
                            self.compiler.pointer_type,
                        ),
                        ty: Some("Int".to_string()),
                    })
                }
                "isEmpty" => {
                    let raw_len = self.runtime_value(
                        builder,
                        self.compiler.runtime.map_len,
                        &[receiver.value],
                        types::I64,
                    );
                    let is_zero = builder.ins().icmp_imm(IntCC::Equal, raw_len, 0);
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.bool_,
                            &[is_zero],
                            self.compiler.pointer_type,
                        ),
                        ty: Some("Bool".to_string()),
                    })
                }
                "contains" => {
                    let key = self.compile_expr(builder, &args[0])?;
                    let raw = self.runtime_value(
                        builder,
                        self.compiler.runtime.map_contains,
                        &[receiver.value, key.value],
                        types::I8,
                    );
                    Ok(TypedValue {
                        value: self.runtime(
                            builder,
                            self.compiler.runtime.bool_,
                            &[raw],
                            self.compiler.pointer_type,
                        ),
                        ty: Some("Bool".to_string()),
                    })
                }
                "keys" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.map_keys,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("List<String>".to_string()),
                }),
                "values" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.map_values,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("List<String>".to_string()),
                }),
                "entries" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.map_entries,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("List<(String,String)>".to_string()),
                }),
                other => Err(format!("unsupported Map member call `{other}`")),
            };
        }
        if layout::canonical_type_name(&receiver_type) == "String" {
            return match member.name.as_str() {
                "toUpper" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.to_upper,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("String".to_string()),
                }),
                "isEmpty" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.string_is_empty,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("Bool".to_string()),
                }),
                "len" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.string_char_count,
                        &[receiver.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("Int".to_string()),
                }),
                other => Err(format!("unsupported String member call `{other}`")),
            };
        }

        Err(format!("unknown extension `{receiver_type}.{}`", member.name))
    }

    fn compile_data_constructor(
        &mut self,
        builder: &mut FunctionBuilder,
        module_path: &Path,
        data: &fa::DataClassDecl,
        args: &[fa::Expr],
    ) -> Result<TypedValue, String> {
        let full_name = layout::data_type_name(module_path, &data.name);
        let data_id = self.compiler.string_data_id(&full_name)?;
        let local = self.compiler.module.declare_data_in_func(data_id, builder.func);
        let type_name_ptr = builder.ins().symbol_value(self.compiler.pointer_type, local);
        let destructor_ptr = if data.methods.iter().any(|method| method.name == "__del__") {
            let symbol = layout::destructor_symbol(module_path, &data.name);
            let func_id = *self
                .compiler
                .destructor_ids
                .get(&symbol)
                .ok_or_else(|| format!("missing destructor bridge `{symbol}`"))?;
            let local = self.compiler.module.declare_func_in_func(func_id, builder.func);
            builder.ins().func_addr(self.compiler.pointer_type, local)
        } else {
            builder.ins().iconst(self.compiler.pointer_type, 0)
        };
        let name_len = self.usize_const(builder, full_name.len() as i64);
        let field_count = self.usize_const(builder, data.fields.len() as i64);
        let handle = self.runtime(
            builder,
            self.compiler.runtime.data_new,
            &[
                type_name_ptr,
                name_len,
                field_count,
                destructor_ptr,
            ],
            self.compiler.pointer_type,
        );
        for (index, arg) in args.iter().enumerate() {
            let value = self.compile_expr(builder, arg)?;
            let field_index = self.usize_const(builder, index as i64);
            self.runtime_void(
                builder,
                self.compiler.runtime.data_set_field,
                &[handle, field_index, value.value],
            );
        }
        Ok(TypedValue {
            value: handle,
            ty: Some(data.name.clone()),
        })
    }

    fn compile_struct_constructor(
        &mut self,
        builder: &mut FunctionBuilder,
        module_path: &Path,
        s: &fa::StructDecl,
        args: &[fa::Expr],
    ) -> Result<TypedValue, String> {
        let full_name = layout::data_type_name(module_path, &s.name);
        let data_id = self.compiler.string_data_id(&full_name)?;
        let local = self.compiler.module.declare_data_in_func(data_id, builder.func);
        let type_name_ptr = builder.ins().symbol_value(self.compiler.pointer_type, local);
        let destructor_ptr = if s.methods.iter().any(|method| method.name == "__del__") {
            let symbol = layout::destructor_symbol(module_path, &s.name);
            let func_id = *self
                .compiler
                .destructor_ids
                .get(&symbol)
                .ok_or_else(|| format!("missing destructor bridge `{symbol}`"))?;
            let local = self.compiler.module.declare_func_in_func(func_id, builder.func);
            builder.ins().func_addr(self.compiler.pointer_type, local)
        } else {
            builder.ins().iconst(self.compiler.pointer_type, 0)
        };
        let name_len = self.usize_const(builder, full_name.len() as i64);
        let field_count = self.usize_const(builder, s.fields.len() as i64);
        let handle = self.runtime(
            builder,
            self.compiler.runtime.data_new,
            &[
                type_name_ptr,
                name_len,
                field_count,
                destructor_ptr,
            ],
            self.compiler.pointer_type,
        );
        for (index, arg) in args.iter().enumerate() {
            let value = self.compile_expr(builder, arg)?;
            let field_index = self.usize_const(builder, index as i64);
            self.runtime_void(
                builder,
                self.compiler.runtime.data_set_field,
                &[handle, field_index, value.value],
            );
        }
        Ok(TypedValue {
            value: handle,
            ty: Some(s.name.clone()),
        })
    }

    fn compile_member(
        &mut self,
        builder: &mut FunctionBuilder,
        member: &fa::Member,
    ) -> Result<TypedValue, String> {
        if let fa::Expr::Name(name) = member.object.as_ref() {
            if !self.locals.contains_key(&name.value) {
                if let Some(const_decl) = self.compiler.session.find_const(&name.value, &member.name) {
                    let expr = const_decl.value.clone();
                    let ty = const_decl.type_name.clone();
                    let mut result = self.compile_expr(builder, &expr)?;
                    if ty.is_some() {
                        result.ty = ty;
                    }
                    return Ok(result);
                }
                if let Some(enum_decl) = self.compiler.session.find_enum(&name.value) {
                    let base = &name.value;
                    let variant_name = &member.name;
                    let variant_index = enum_decl
                        .variants
                        .iter()
                        .position(|v| v.name == *variant_name)
                        .ok_or_else(|| format!("unknown variant `{variant_name}` on enum `{base}`"))?;
                    let type_data_id = self.compiler.string_data_id(base)?;
                    let type_local = self.compiler.module.declare_data_in_func(type_data_id, builder.func);
                    let type_ptr = builder.ins().symbol_value(self.compiler.pointer_type, type_local);
                    let type_len = self.usize_const(builder, base.len() as i64);
                    let variant_data_id = self.compiler.string_data_id(variant_name)?;
                    let variant_local = self.compiler.module.declare_data_in_func(variant_data_id, builder.func);
                    let variant_ptr = builder.ins().symbol_value(self.compiler.pointer_type, variant_local);
                    let variant_len = self.usize_const(builder, variant_name.len() as i64);
                    let tag = builder.ins().iconst(types::I64, variant_index as i64);
                    let null_payload = builder.ins().iconst(self.compiler.pointer_type, 0);
                    let result = self.runtime(
                        builder,
                        self.compiler.runtime.enum_new,
                        &[type_ptr, type_len, tag, variant_ptr, variant_len, null_payload],
                        self.compiler.pointer_type,
                    );
                    return Ok(TypedValue {
                        value: result,
                        ty: Some(base.to_string()),
                    });
                }
            }
        }
        let object = self.compile_expr(builder, &member.object)?;
        self.member_access(builder, object, &member.name, member.optional)
    }

    fn member_access(
        &mut self,
        builder: &mut FunctionBuilder,
        object: TypedValue,
        name: &str,
        optional: bool,
    ) -> Result<TypedValue, String> {
        if optional {
            let object_type = object
                .ty
                .clone()
                .ok_or_else(|| format!("cannot infer optional member `{name}`"))?;
            let some = self.runtime_value(
                builder,
                self.compiler.runtime.option_is_some,
                &[object.value],
                types::I8,
            );
            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let done = builder.create_block();
            builder.append_block_param(done, self.compiler.pointer_type);
            let cond = builder.ins().icmp_imm(IntCC::NotEqual, some, 0);
            builder.ins().brif(cond, then_block, &[], else_block, &[]);

            builder.switch_to_block(then_block);
            let inner = self.runtime(
                builder,
                self.compiler.runtime.option_unwrap,
                &[object.value],
                self.compiler.pointer_type,
            );
            let field = self.member_access(
                builder,
                TypedValue {
                    value: inner,
                    ty: option_inner_type(&object_type),
                },
                name,
                false,
            )?;
            let wrapped = self.runtime(
                builder,
                self.compiler.runtime.some,
                &[field.value],
                self.compiler.pointer_type,
            );
            self.jump_value(builder, done, wrapped);

            builder.switch_to_block(else_block);
            let none = self.runtime_nullary(builder, self.compiler.runtime.none);
            self.jump_value(builder, done, none);

            builder.switch_to_block(done);
            return Ok(TypedValue {
                value: builder.block_params(done)[0],
                ty: option_inner_type(&object_type)
                    .and_then(|inner| self.compiler.session.field_type(&inner, name))
                    .map(|field| format!("Option<{field}>")),
            });
        }

        let object_type = object
            .ty
            .clone()
            .ok_or_else(|| format!("cannot infer member `{name}`"))?;
        if layout::canonical_type_name(&object_type) == "String" {
            return match name {
                "isEmpty" => Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.string_is_empty,
                        &[object.value],
                        self.compiler.pointer_type,
                    ),
                    ty: Some("Bool".to_string()),
                }),
                other => Err(format!("unsupported String member `{other}`")),
            };
        }
        if object_type.starts_with('(') {
            if let Ok(index) = name.parse::<i64>() {
                let index_val = self.usize_const(builder, index);
                return Ok(TypedValue {
                    value: self.runtime(
                        builder,
                        self.compiler.runtime.list_get,
                        &[object.value, index_val],
                        self.compiler.pointer_type,
                    ),
                    ty: None,
                });
            }
        }
        let layout = self
            .compiler
            .session
            .layout
            .data_layout(&object_type)
            .ok_or_else(|| format!("missing layout for `{object_type}`"))?;
        let field_index = layout
            .field_index(name)
            .ok_or_else(|| format!("unknown field `{name}` on `{object_type}`"))?;
        Ok(TypedValue {
            value: {
                let index = self.usize_const(builder, field_index as i64);
                self.runtime(
                    builder,
                    self.compiler.runtime.data_get_field,
                    &[object.value, index],
                    self.compiler.pointer_type,
                )
            },
            ty: self.compiler.session.field_type(&object_type, name),
        })
    }

    fn compile_move(
        &mut self,
        builder: &mut FunctionBuilder,
        move_expr: &fa::MoveExpr,
    ) -> Result<TypedValue, String> {
        if let fa::Expr::Name(name) = move_expr.value.as_ref() {
            let binding = self
                .locals
                .get_mut(&name.value)
                .ok_or_else(|| format!("unknown binding `{}`", name.value))?;
            binding.destroy = false;
            return Ok(TypedValue {
                value: builder.use_var(binding.var),
                ty: binding.ty.clone(),
            });
        }
        self.compile_expr(builder, &move_expr.value)
    }

    fn compile_question(
        &mut self,
        builder: &mut FunctionBuilder,
        question: &fa::QuestionExpr,
    ) -> Result<TypedValue, String> {
        let value = self.compile_expr(builder, &question.value)?;
        let ty = value
            .ty
            .clone()
            .ok_or_else(|| "cannot infer type for `?` operand".to_string())?;
        if layout::canonical_type_name(&ty) == "Option" {
            let some = self.runtime_value(
                builder,
                self.compiler.runtime.option_is_some,
                &[value.value],
                types::I8,
            );
            let cont = builder.create_block();
            let fail = builder.create_block();
            let cond = builder.ins().icmp_imm(IntCC::NotEqual, some, 0);
            builder.ins().brif(cond, cont, &[], fail, &[]);
            builder.switch_to_block(fail);
            let none = self.runtime_nullary(builder, self.compiler.runtime.none);
            builder.ins().return_(&[none]);
            builder.switch_to_block(cont);
            return Ok(TypedValue {
                value: self.runtime(
                    builder,
                    self.compiler.runtime.option_unwrap,
                    &[value.value],
                    self.compiler.pointer_type,
                ),
                ty: option_inner_type(&ty),
            });
        }
        let ok = self.runtime_value(
            builder,
            self.compiler.runtime.result_is_ok,
            &[value.value],
            types::I8,
        );
        let cont = builder.create_block();
        let fail = builder.create_block();
        let cond = builder.ins().icmp_imm(IntCC::NotEqual, ok, 0);
        builder.ins().brif(cond, cont, &[], fail, &[]);
        builder.switch_to_block(fail);
        let err_value = self.runtime(
            builder,
            self.compiler.runtime.result_unwrap,
            &[value.value],
            self.compiler.pointer_type,
        );
        let err_handle = self.runtime(
            builder,
            self.compiler.runtime.err,
            &[err_value],
            self.compiler.pointer_type,
        );
        builder.ins().return_(&[err_handle]);
        builder.switch_to_block(cont);
        Ok(TypedValue {
            value: self.runtime(
                builder,
                self.compiler.runtime.result_unwrap,
                &[value.value],
                self.compiler.pointer_type,
            ),
            ty: result_ok_type(&ty),
        })
    }

    fn compile_if(
        &mut self,
        builder: &mut FunctionBuilder,
        if_expr: &fa::IfExpr,
    ) -> Result<TypedValue, String> {
        let condition = self.compile_expr(builder, &if_expr.condition)?;
        let cond = self.truthy_value(builder, condition.value);
        let then_block = builder.create_block();
        let else_block = builder.create_block();
        let done = builder.create_block();
        builder.append_block_param(done, self.compiler.pointer_type);
        builder.ins().brif(cond, then_block, &[], else_block, &[]);

        let snapshot = self.locals.clone();

        // --- then branch ---
        builder.switch_to_block(then_block);
        let (then_prefix, then_final_expr) =
            split_block_final_expr(&if_expr.then_branch.statements);
        self.compile_statements(builder, then_prefix)?;
        if !self.current_block_is_terminated(builder) {
            let value = if let Some(expr) = then_final_expr {
                self.compile_expr(builder, expr)?.value
            } else {
                self.runtime_nullary(builder, self.compiler.runtime.unit)
            };
            self.jump_value(builder, done, value);
        }

        // --- else branch ---
        self.locals = snapshot.clone();
        builder.switch_to_block(else_block);
        match &if_expr.else_branch {
            Some(fa::ElseBranch::Block(block)) => {
                let (else_prefix, else_final_expr) =
                    split_block_final_expr(&block.statements);
                self.compile_statements(builder, else_prefix)?;
                if !self.current_block_is_terminated(builder) {
                    let value = if let Some(expr) = else_final_expr {
                        self.compile_expr(builder, expr)?.value
                    } else {
                        self.runtime_nullary(builder, self.compiler.runtime.unit)
                    };
                    self.jump_value(builder, done, value);
                }
            }
            Some(fa::ElseBranch::IfExpr(expr)) => {
                let value = self.compile_if(builder, expr)?;
                if !self.current_block_is_terminated(builder) {
                    self.jump_value(builder, done, value.value);
                }
            }
            None => {
                let value = self.runtime_nullary(builder, self.compiler.runtime.unit);
                self.jump_value(builder, done, value);
            }
        }

        // --- result type ---
        // When both branches exist and the then-branch has a final expression,
        // infer the type from that expression (same approach as compile_match).
        // If no else branch or no final expression, the type is Unit.
        let result_type = if if_expr.else_branch.is_some() {
            then_final_expr
                .and_then(|expr| self.infer_expr_type(expr))
                .or_else(|| Some("Unit".to_string()))
        } else {
            Some("Unit".to_string())
        };

        self.locals = snapshot;
        builder.switch_to_block(done);
        Ok(TypedValue {
            value: builder.block_params(done)[0],
            ty: result_type,
        })
    }

    fn compile_match(
        &mut self,
        builder: &mut FunctionBuilder,
        match_expr: &fa::MatchExpr,
    ) -> Result<TypedValue, String> {
        let subject = self.compile_expr(builder, &match_expr.subject)?;
        let subject_type = subject
            .ty
            .clone()
            .unwrap_or_else(|| "Result<Unknown, Unknown>".to_string());
        if matches!(layout::canonical_type_name(&subject_type), "Result" | "Option")
            && match_expr.arms.len() == 2
        {
            return self.compile_two_arm_match(builder, subject, &subject_type, match_expr);
        }
        let done = builder.create_block();
        builder.append_block_param(done, self.compiler.pointer_type);
        let mut next = builder.create_block();
        builder.ins().jump(next, &[]);
        let base_locals = self.locals.clone();

        for arm in &match_expr.arms {
            builder.switch_to_block(next);
            let body_block = builder.create_block();
            let miss_block = builder.create_block();
            let matched = self.pattern_matches(builder, subject.value, &subject_type, &arm.pattern)?;
            builder.ins().brif(matched, body_block, &[], miss_block, &[]);

            builder.switch_to_block(body_block);
            self.locals = base_locals.clone();
            self.bind_pattern(builder, subject.value, &subject_type, &arm.pattern)?;
            // Protect outer locals from ASAP release inside arm body.
            for (name, binding) in self.locals.iter_mut() {
                if base_locals.contains_key(name) {
                    binding.destroy = false;
                }
            }
            match &arm.body {
                fa::ArmBody::Expr(expr) => {
                    let value = self.compile_expr(builder, expr)?;
                    if !self.current_block_is_terminated(builder) {
                        self.jump_value(builder, done, value.value);
                    }
                }
                fa::ArmBody::Block(block) => {
                    self.compile_statements(builder, &block.statements)?;
                    if !self.current_block_is_terminated(builder) {
                        let unit = self.runtime_nullary(builder, self.compiler.runtime.unit);
                        self.jump_value(builder, done, unit);
                    }
                }
            };
            next = miss_block;
        }

        builder.switch_to_block(next);
        let value = self.runtime_nullary(builder, self.compiler.runtime.unit);
        self.jump_value(builder, done, value);
        builder.switch_to_block(done);
        self.locals = base_locals;
        Ok(TypedValue {
            value: builder.block_params(done)[0],
            ty: match_expr.arms.iter().find_map(|arm| match &arm.body {
                fa::ArmBody::Expr(expr) => self.infer_expr_type(expr),
                fa::ArmBody::Block(_) => Some("Unit".to_string()),
            }),
        })
    }

    fn compile_two_arm_match(
        &mut self,
        builder: &mut FunctionBuilder,
        subject: TypedValue,
        subject_type: &str,
        match_expr: &fa::MatchExpr,
    ) -> Result<TypedValue, String> {
        let done = builder.create_block();
        builder.append_block_param(done, self.compiler.pointer_type);
        let first_block = builder.create_block();
        let second_block = builder.create_block();
        let matched = self.pattern_matches(builder, subject.value, subject_type, &match_expr.arms[0].pattern)?;
        builder.ins().brif(matched, first_block, &[], second_block, &[]);

        let base_locals = self.locals.clone();

        builder.switch_to_block(first_block);
        self.locals = base_locals.clone();
        self.bind_pattern(builder, subject.value, subject_type, &match_expr.arms[0].pattern)?;
        // Protect outer locals from ASAP release inside arm body.
        for (name, binding) in self.locals.iter_mut() {
            if base_locals.contains_key(name) {
                binding.destroy = false;
            }
        }
        match &match_expr.arms[0].body {
            fa::ArmBody::Expr(expr) => {
                let value = self.compile_expr(builder, expr)?;
                if !self.current_block_is_terminated(builder) {
                    self.jump_value(builder, done, value.value);
                }
            }
            fa::ArmBody::Block(block) => {
                self.compile_statements(builder, &block.statements)?;
                if !self.current_block_is_terminated(builder) {
                    let unit = self.runtime_nullary(builder, self.compiler.runtime.unit);
                    self.jump_value(builder, done, unit);
                }
            }
        };

        builder.switch_to_block(second_block);
        self.locals = base_locals.clone();
        self.bind_pattern(builder, subject.value, subject_type, &match_expr.arms[1].pattern)?;
        // Protect outer locals from ASAP release inside arm body.
        for (name, binding) in self.locals.iter_mut() {
            if base_locals.contains_key(name) {
                binding.destroy = false;
            }
        }
        match &match_expr.arms[1].body {
            fa::ArmBody::Expr(expr) => {
                let value = self.compile_expr(builder, expr)?;
                if !self.current_block_is_terminated(builder) {
                    self.jump_value(builder, done, value.value);
                }
            }
            fa::ArmBody::Block(block) => {
                self.compile_statements(builder, &block.statements)?;
                if !self.current_block_is_terminated(builder) {
                    let unit = self.runtime_nullary(builder, self.compiler.runtime.unit);
                    self.jump_value(builder, done, unit);
                }
            }
        };

        builder.switch_to_block(done);
        self.locals = base_locals;
        Ok(TypedValue {
            value: builder.block_params(done)[0],
            ty: match_expr.arms.iter().find_map(|arm| match &arm.body {
                fa::ArmBody::Expr(expr) => self.infer_expr_type(expr),
                fa::ArmBody::Block(_) => Some("Unit".to_string()),
            }),
        })
    }

    fn compile_when(
        &mut self,
        builder: &mut FunctionBuilder,
        when_expr: &fa::WhenExpr,
    ) -> Result<TypedValue, String> {
        let done = builder.create_block();
        builder.append_block_param(done, self.compiler.pointer_type);
        let mut next = builder.create_block();
        builder.ins().jump(next, &[]);
        for arm in &when_expr.arms {
            builder.switch_to_block(next);
            let body_block = builder.create_block();
            let miss_block = builder.create_block();
            if let Some(condition) = &arm.condition {
                let value = self.compile_expr(builder, condition)?;
                let truthy = self.truthy_value(builder, value.value);
                builder.ins().brif(truthy, body_block, &[], miss_block, &[]);
            } else {
                builder.ins().jump(body_block, &[]);
            }
            builder.switch_to_block(body_block);
            let value = match &arm.body {
                fa::ArmBody::Expr(expr) => self.compile_expr(builder, expr)?,
                fa::ArmBody::Block(block) => {
                    self.compile_statements(builder, &block.statements)?;
                    TypedValue {
                        value: self.runtime_nullary(builder, self.compiler.runtime.unit),
                        ty: Some("Unit".to_string()),
                    }
                }
            };
            if !self.current_block_is_terminated(builder) {
                self.jump_value(builder, done, value.value);
            }
            next = miss_block;
        }
        builder.switch_to_block(next);
        let unit = self.runtime_nullary(builder, self.compiler.runtime.unit);
        self.jump_value(builder, done, unit);
        builder.switch_to_block(done);
        Ok(TypedValue {
            value: builder.block_params(done)[0],
            ty: Some("Unit".to_string()),
        })
    }

    fn compile_lambda(
        &mut self,
        builder: &mut FunctionBuilder,
        lambda: &fa::LambdaExpr,
    ) -> Result<TypedValue, String> {
        let id = self.compiler.lambda_counter;
        self.compiler.lambda_counter += 1;
        let lambda_name = format!("__lambda_{id}");
        let module_path = self.module_path.to_path_buf();

        let param_names: HashSet<String> = lambda.params.iter().map(|p| p.name.clone()).collect();
        let body_names = collect_block_names(&lambda.body);
        let captures: Vec<String> = body_names
            .iter()
            .filter(|name| !param_names.contains(*name) && self.locals.contains_key(*name))
            .cloned()
            .collect();

        // __env param (closure list) + user params only. Captures unpacked from __env in compile_function.
        let mut all_params = vec![fa::Param {
            convention: None,
            name: "__env".to_string(),
            type_name: None,
            variadic: false,
            span: lambda.span,
        }];
        all_params.extend(lambda.params.clone());

        let symbol = layout::function_symbol(&module_path, &lambda_name);
        let sig = self.compiler.handle_signature(all_params.len());
        let func_id = self
            .compiler
            .module
            .declare_function(&symbol, Linkage::Local, &sig)
            .map_err(|error| error.to_string())?;
        self.compiler.function_ids.insert(symbol.clone(), func_id);

        let decl = fa::FunctionDecl {
            name: lambda_name,
            type_params: Vec::new(),
            params: all_params,
            return_type: lambda.return_type.clone(),
            body: lambda.body.clone(),
            is_pub: false,
            annotations: Vec::new(),
            receiver_type: None,
            span: lambda.span,
        };
        let capture_info: Vec<(String, Option<String>)> = captures
            .iter()
            .map(|c| (c.clone(), self.locals.get(c).and_then(|b| b.ty.clone())))
            .collect();
        self.compiler.pending_lambdas.push(PendingLambda {
            module_path,
            decl,
            captures: capture_info,
        });

        let local = self.compiler.module.declare_func_in_func(func_id, builder.func);
        let addr = builder.ins().func_addr(self.compiler.pointer_type, local);

        let closure_list = self.runtime_nullary(builder, self.compiler.runtime.list_new);
        self.runtime_void(builder, self.compiler.runtime.list_push, &[closure_list, addr]);
        for cap in &captures {
            let binding = &self.locals[cap];
            let val = builder.use_var(binding.var);
            self.runtime_void(builder, self.compiler.runtime.list_push, &[closure_list, val]);
        }

        let param_types: Vec<String> = lambda
            .params
            .iter()
            .map(|p| p.type_name.clone().unwrap_or_else(|| "Any".to_string()))
            .collect();
        let ret = lambda
            .return_type
            .clone()
            .unwrap_or_else(|| "Any".to_string());
        Ok(TypedValue {
            value: closure_list,
            ty: Some(format!("fn({}) -> {}", param_types.join(", "), ret)),
        })
    }

    fn pattern_matches(
        &mut self,
        builder: &mut FunctionBuilder,
        subject: Value,
        subject_type: &str,
        pattern: &fa::Pattern,
    ) -> Result<Value, String> {
        match pattern {
            fa::Pattern::Wildcard(_) | fa::Pattern::Name(_) | fa::Pattern::Tuple(_) => {
                Ok(builder.ins().iconst(types::I8, 1))
            }
            fa::Pattern::Literal(literal) => {
                let expected = self.compile_literal(
                    builder,
                    &fa::Literal {
                        value: literal.value.clone(),
                        span: literal.span,
                    },
                )?;
                let eq = self.runtime(
                    builder,
                    self.compiler.runtime.eq,
                    &[subject, expected.value],
                    self.compiler.pointer_type,
                );
                Ok(self.truthy_value(builder, eq))
            }
            fa::Pattern::Variant(variant) => match (layout::canonical_type_name(subject_type), variant.name.as_str()) {
                ("Result", "Ok") => {
                    let value = self.runtime_value(
                        builder,
                        self.compiler.runtime.result_is_ok,
                        &[subject],
                        types::I8,
                    );
                    Ok(builder.ins().icmp_imm(IntCC::NotEqual, value, 0))
                }
                ("Result", "Err") => {
                    let is_ok = self.runtime_value(builder, self.compiler.runtime.result_is_ok, &[subject], types::I8);
                    let inverted = builder.ins().bxor_imm(is_ok, 1);
                    Ok(builder.ins().icmp_imm(IntCC::NotEqual, inverted, 0))
                }
                ("Option", "Some") => {
                    let value = self.runtime_value(
                        builder,
                        self.compiler.runtime.option_is_some,
                        &[subject],
                        types::I8,
                    );
                    Ok(builder.ins().icmp_imm(IntCC::NotEqual, value, 0))
                }
                ("Option", "None") => {
                    let is_some = self.runtime_value(builder, self.compiler.runtime.option_is_some, &[subject], types::I8);
                    let inverted = builder.ins().bxor_imm(is_some, 1);
                    Ok(builder.ins().icmp_imm(IntCC::NotEqual, inverted, 0))
                }
                (base, variant_name) => {
                    if let Some(enum_decl) = self.compiler.session.find_enum(base) {
                        let clean_name = variant_name.rsplit('.').next().unwrap_or(variant_name);
                        let variant_index = enum_decl
                            .variants
                            .iter()
                            .position(|v| v.name == clean_name)
                            .ok_or_else(|| format!("unknown variant `{clean_name}` on enum `{base}`"))?;
                        let tag = self.runtime_value(
                            builder,
                            self.compiler.runtime.enum_tag,
                            &[subject],
                            types::I64,
                        );
                        Ok(builder.ins().icmp_imm(IntCC::Equal, tag, variant_index as i64))
                    } else {
                        Err(format!("unsupported match pattern `{variant_name}` on `{base}`"))
                    }
                }
            },
        }
    }

    fn bind_pattern(
        &mut self,
        builder: &mut FunctionBuilder,
        subject: Value,
        subject_type: &str,
        pattern: &fa::Pattern,
    ) -> Result<(), String> {
        match pattern {
            fa::Pattern::Name(name) => {
                let variable = self.new_var(builder, self.compiler.pointer_type);
                builder.def_var(variable, subject);
                self.locals.insert(
                    name.name.clone(),
                    LocalBinding {
                        var: variable,
                        ty: Some(subject_type.to_string()),
                        destroy: false,
                    },
                );
            }
            fa::Pattern::Variant(variant) => {
                let base = layout::canonical_type_name(subject_type);
                let payload = match base {
                    "Result" => self.runtime(
                        builder,
                        self.compiler.runtime.result_unwrap,
                        &[subject],
                        self.compiler.pointer_type,
                    ),
                    "Option" => self.runtime(
                        builder,
                        self.compiler.runtime.option_unwrap,
                        &[subject],
                        self.compiler.pointer_type,
                    ),
                    _ => {
                        let index = self.usize_const(builder, 0);
                        self.runtime(
                            builder,
                            self.compiler.runtime.enum_payload,
                            &[subject, index],
                            self.compiler.pointer_type,
                        )
                    }
                };
                for (i, arg) in variant.args.iter().enumerate() {
                    if let fa::Pattern::Name(name) = arg {
                        let val = if i == 0 {
                            payload
                        } else {
                            let idx = self.usize_const(builder, i as i64);
                            self.runtime(
                                builder,
                                self.compiler.runtime.enum_payload,
                                &[subject, idx],
                                self.compiler.pointer_type,
                            )
                        };
                        let variable = self.new_var(builder, self.compiler.pointer_type);
                        builder.def_var(variable, val);
                        self.locals.insert(
                            name.name.clone(),
                            LocalBinding {
                                var: variable,
                                ty: if variant.args.len() == 1 {
                                    match variant.name.as_str() {
                                        "Ok" => result_ok_type(subject_type),
                                        "Err" => result_err_type(subject_type),
                                        "Some" => option_inner_type(subject_type),
                                        _ => None,
                                    }
                                } else {
                                    None
                                },
                                destroy: false,
                            },
                        );
                    }
                }
            }
            fa::Pattern::Tuple(tuple) => {
                for (i, elem) in tuple.elements.iter().enumerate() {
                    if let fa::Pattern::Name(name) = elem {
                        let idx = self.usize_const(builder, i as i64);
                        let val = self.runtime(
                            builder,
                            self.compiler.runtime.list_get,
                            &[subject, idx],
                            self.compiler.pointer_type,
                        );
                        let variable = self.new_var(builder, self.compiler.pointer_type);
                        builder.def_var(variable, val);
                        self.locals.insert(
                            name.name.clone(),
                            LocalBinding {
                                var: variable,
                                ty: None,
                                destroy: false,
                            },
                        );
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn release_dead(&mut self, builder: &mut FunctionBuilder, future_names: &HashSet<String>) {
        let names = self.locals.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let should_release = self
                .locals
                .get(&name)
                .map(|binding| binding.destroy && !future_names.contains(&name))
                .unwrap_or(false);
            if !should_release {
                continue;
            }
            let variable = self.locals.get(&name).expect("binding exists").var;
            let value = builder.use_var(variable);
            self.runtime_void(builder, self.compiler.runtime.asap_release, &[value]);
            self.locals.get_mut(&name).expect("binding exists").destroy = false;
        }
    }

    fn release_remaining(&mut self, builder: &mut FunctionBuilder) {
        let names = self.locals.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let should_release = self
                .locals
                .get(&name)
                .map(|binding| binding.destroy)
                .unwrap_or(false);
            if !should_release {
                continue;
            }
            let variable = self.locals.get(&name).expect("binding exists").var;
            let value = builder.use_var(variable);
            self.runtime_void(builder, self.compiler.runtime.release, &[value]);
            self.locals.get_mut(&name).expect("binding exists").destroy = false;
        }
    }

    fn runtime(
        &mut self,
        builder: &mut FunctionBuilder,
        func_id: FuncId,
        args: &[Value],
        _ret_ty: cranelift_codegen::ir::Type,
    ) -> Value {
        let local = self.compiler.module.declare_func_in_func(func_id, builder.func);
        let call = builder.ins().call(local, args);
        builder.inst_results(call)[0]
    }

    fn runtime_value(
        &mut self,
        builder: &mut FunctionBuilder,
        func_id: FuncId,
        args: &[Value],
        ret_ty: cranelift_codegen::ir::Type,
    ) -> Value {
        self.runtime(builder, func_id, args, ret_ty)
    }

    fn runtime_nullary(&mut self, builder: &mut FunctionBuilder, func_id: FuncId) -> Value {
        self.runtime(builder, func_id, &[], self.compiler.pointer_type)
    }

    fn runtime_void(
        &mut self,
        builder: &mut FunctionBuilder,
        func_id: FuncId,
        args: &[Value],
    ) {
        let local = self.compiler.module.declare_func_in_func(func_id, builder.func);
        builder.ins().call(local, args);
    }

    fn truthy_value(&mut self, builder: &mut FunctionBuilder, value: Value) -> Value {
        let val_type = builder.func.dfg.value_type(value);
        if val_type == types::I8 {
            // Value is already a native boolean (e.g. from fuse_map_contains,
            // fuse_is_truthy, icmp).  Use it directly instead of passing it
            // through fuse_is_truthy which expects a FuseHandle (pointer).
            return builder.ins().icmp_imm(IntCC::NotEqual, value, 0);
        }
        let truthy = self.runtime_value(builder, self.compiler.runtime.truthy, &[value], types::I8);
        builder.ins().icmp_imm(IntCC::NotEqual, truthy, 0)
    }

    /// Box a raw scalar lane value into a FuseHandle, widening if needed.
    fn simd_box_scalar(
        &mut self,
        builder: &mut FunctionBuilder,
        scalar: Value,
        lane_ty: cranelift_codegen::ir::Type,
    ) -> Value {
        match lane_ty {
            types::F64 => self.runtime(builder, self.compiler.runtime.float,
                &[scalar], self.compiler.pointer_type),
            types::F32 => {
                let wide = builder.ins().fpromote(types::F64, scalar);
                self.runtime(builder, self.compiler.runtime.float,
                    &[wide], self.compiler.pointer_type)
            }
            types::I64 => self.runtime(builder, self.compiler.runtime.int,
                &[scalar], self.compiler.pointer_type),
            types::I32 => {
                let wide = builder.ins().sextend(types::I64, scalar);
                self.runtime(builder, self.compiler.runtime.int,
                    &[wide], self.compiler.pointer_type)
            }
            _ => unreachable!(),
        }
    }

    /// Emit a runtime length check on a list.  Returns `(native_block,
    /// fallback_block, done_block)`.  The caller must emit instructions in
    /// `native_block`, jump to `done` with one `pointer_type` block-arg,
    /// then do the same in `fallback_block`.  After both jumps, switch to
    /// `done` and read `block_params(done)[0]` for the merged result.
    fn simd_length_guard(
        &mut self,
        builder: &mut FunctionBuilder,
        list: Value,
        lane_count: u64,
    ) -> (cranelift_codegen::ir::Block, cranelift_codegen::ir::Block, cranelift_codegen::ir::Block) {
        let len = self.runtime_value(
            builder, self.compiler.runtime.list_len, &[list], types::I64,
        );
        let expected = builder.ins().iconst(types::I64, lane_count as i64);
        let len_ok = builder.ins().icmp(IntCC::Equal, len, expected);
        let native_block = builder.create_block();
        let fallback_block = builder.create_block();
        let done = builder.create_block();
        builder.append_block_param(done, self.compiler.pointer_type);
        builder.ins().brif(len_ok, native_block, &[], fallback_block, &[]);
        (native_block, fallback_block, done)
    }

    /// Unpack a FuseHandle list into a Cranelift vector register.
    /// Emits N `fuse_list_get` + `fuse_simd_extract_raw_*` calls, then
    /// builds the vector via `scalar_to_vector` + `insertlane`.
    fn simd_unpack_list(
        &mut self,
        builder: &mut FunctionBuilder,
        list_handle: Value,
        vec_ty: cranelift_codegen::ir::Type,
    ) -> Value {
        let lane_ty = simd_lane_type(vec_ty);
        let count = simd_lane_count(vec_ty);
        let extract_fn = match lane_ty {
            types::F64 => self.compiler.runtime.simd_extract_raw_f64,
            types::F32 => self.compiler.runtime.simd_extract_raw_f32,
            types::I64 => self.compiler.runtime.simd_extract_raw_i64,
            types::I32 => self.compiler.runtime.simd_extract_raw_i32,
            _ => unreachable!(),
        };

        // Extract lane 0 and seed the vector via scalar_to_vector.
        let idx0 = builder.ins().iconst(types::I64, 0);
        let item0 = self.runtime(builder, self.compiler.runtime.list_get,
            &[list_handle, idx0], self.compiler.pointer_type);
        let scalar0 = self.runtime_value(builder, extract_fn, &[item0], lane_ty);
        let mut vec = builder.ins().scalar_to_vector(vec_ty, scalar0);

        // Insert remaining lanes.
        for i in 1..count {
            let idx = builder.ins().iconst(types::I64, i as i64);
            let item = self.runtime(builder, self.compiler.runtime.list_get,
                &[list_handle, idx], self.compiler.pointer_type);
            let scalar = self.runtime_value(builder, extract_fn, &[item], lane_ty);
            vec = builder.ins().insertlane(vec, scalar, i);
        }
        vec
    }

    /// Pack a Cranelift vector register back into a FuseHandle list.
    /// Emits N `extractlane` + boxing calls + `fuse_list_push`.
    fn simd_pack_to_list(
        &mut self,
        builder: &mut FunctionBuilder,
        vec: Value,
        vec_ty: cranelift_codegen::ir::Type,
    ) -> Value {
        let lane_ty = simd_lane_type(vec_ty);
        let count = simd_lane_count(vec_ty);
        let result = self.runtime_nullary(builder, self.compiler.runtime.list_new);
        for i in 0..count {
            let scalar = builder.ins().extractlane(vec, i);
            // Widen to the boxing type and box.
            let boxed = match lane_ty {
                types::F64 => self.runtime(builder, self.compiler.runtime.float,
                    &[scalar], self.compiler.pointer_type),
                types::F32 => {
                    let wide = builder.ins().fpromote(types::F64, scalar);
                    self.runtime(builder, self.compiler.runtime.float,
                        &[wide], self.compiler.pointer_type)
                }
                types::I64 => self.runtime(builder, self.compiler.runtime.int,
                    &[scalar], self.compiler.pointer_type),
                types::I32 => {
                    let wide = builder.ins().sextend(types::I64, scalar);
                    self.runtime(builder, self.compiler.runtime.int,
                        &[wide], self.compiler.pointer_type)
                }
                _ => unreachable!(),
            };
            self.runtime_void(builder, self.compiler.runtime.list_push, &[result, boxed]);
        }
        result
    }

    fn current_block_is_terminated(&self, builder: &FunctionBuilder) -> bool {
        let Some(block) = builder.current_block() else {
            return true;
        };
        let Some(inst) = builder.func.layout.last_inst(block) else {
            return false;
        };
        builder.func.dfg.insts[inst].opcode().is_terminator()
    }

    fn usize_const(&self, builder: &mut FunctionBuilder, value: i64) -> Value {
        builder.ins().iconst(self.compiler.pointer_type, value)
    }

    fn string_value(
        &mut self,
        builder: &mut FunctionBuilder,
        value: &str,
    ) -> Result<Value, String> {
        let data_id = self.compiler.string_data_id(value)?;
        let local = self.compiler.module.declare_data_in_func(data_id, builder.func);
        let ptr = builder.ins().symbol_value(self.compiler.pointer_type, local);
        let len = self.usize_const(builder, value.len() as i64);
        Ok(self.runtime(
            builder,
            self.compiler.runtime.string_new_utf8,
            &[ptr, len],
            self.compiler.pointer_type,
        ))
    }

    fn infer_expr_type(&self, expr: &fa::Expr) -> Option<String> {
        match expr {
            fa::Expr::Literal(literal) => match literal.value {
                fa::LiteralValue::Int(_) => Some("Int".to_string()),
                fa::LiteralValue::Float(_) => Some("Float".to_string()),
                fa::LiteralValue::String(_) => Some("String".to_string()),
                fa::LiteralValue::Bool(_) => Some("Bool".to_string()),
            },
            fa::Expr::FString(_) => Some("String".to_string()),
            fa::Expr::Name(name) => {
                if name.value == "None" {
                    Some("Option<Unknown>".to_string())
                } else {
                    self.locals.get(&name.value).and_then(|binding| binding.ty.clone())
                }
            }
            fa::Expr::List(list) => Some(format!(
                "List<{}>",
                list.items
                    .first()
                    .and_then(|item| self.infer_expr_type(item))
                    .unwrap_or_else(|| "Unknown".to_string())
            )),
            fa::Expr::Unary(unary) => self.infer_expr_type(&unary.value),
            fa::Expr::Binary(binary) => match binary.op.as_str() {
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "and" | "or" => Some("Bool".to_string()),
                "?:" => self
                    .infer_expr_type(&binary.left)
                    .as_deref()
                    .and_then(option_inner_type)
                    .or_else(|| self.infer_expr_type(&binary.right))
                    .map(|value| value.to_string()),
                "+" => {
                    let left = self.infer_expr_type(&binary.left)?;
                    let right = self.infer_expr_type(&binary.right)?;
                    if left == "String" || right == "String" {
                        Some("String".to_string())
                    } else {
                        Some("Int".to_string())
                    }
                }
                "-" | "*" | "/" | "%" => Some("Int".to_string()),
                _ => None,
            },
            fa::Expr::Call(call) => match call.callee.as_ref() {
                fa::Expr::Name(name) => match name.value.as_str() {
                    "Some" => self
                        .infer_expr_type(call.args.first()?)
                        .map(|inner| format!("Option<{inner}>")),
                    "Ok" => self
                        .infer_expr_type(call.args.first()?)
                        .map(|inner| format!("Result<{inner}, Unknown>")),
                    "Err" => self
                        .infer_expr_type(call.args.first()?)
                        .map(|inner| format!("Result<Unknown, {inner}>")),
                    data_name => {
                        if self.compiler.session.find_data(data_name).is_some()
                            || self.compiler.session.find_struct(data_name).is_some()
                        {
                            Some(data_name.to_string())
                        } else if layout::canonical_type_name(data_name) == "Chan" {
                            Some(data_name.replace("::", ""))
                        } else if layout::canonical_type_name(data_name) == "Shared" {
                            Some(data_name.replace("::", ""))
                        } else if layout::canonical_type_name(data_name) == "Map" {
                            Some(data_name.replace("::", ""))
                        } else {
                            self.compiler
                                .session
                                .resolve_function(data_name)
                                .and_then(|(_, function)| function.return_type.clone())
                        }
                    }
                },
                fa::Expr::Member(member) => {
                    let receiver_type = self.infer_expr_type(&member.object)?;
                    if let Some((_, function)) = self
                        .compiler
                        .session
                        .resolve_extension(&receiver_type, &member.name)
                    {
                        return function.return_type.clone();
                    }
                    if layout::canonical_type_name(&receiver_type) == "Chan" {
                        return match member.name.as_str() {
                            "send" => Some("Result<Unit, String>".to_string()),
                            "recv" => {
                                let inner = chan_inner_type(&receiver_type).unwrap_or_else(|| "Unknown".to_string());
                                Some(format!("Result<{inner}, String>"))
                            }
                            "tryRecv" => {
                                let inner = chan_inner_type(&receiver_type).unwrap_or_else(|| "Unknown".to_string());
                                Some(format!("Option<{inner}>"))
                            }
                            "close" => Some("Unit".to_string()),
                            "isClosed" => Some("Bool".to_string()),
                            "len" => Some("Int".to_string()),
                            "cap" => Some("Option<Int>".to_string()),
                            _ => None,
                        };
                    }
                    if layout::canonical_type_name(&receiver_type) == "Shared" {
                        return match member.name.as_str() {
                            "read" | "write" => shared_inner_type(&receiver_type).or(Some("Unit".to_string())),
                            "try_write" | "tryWrite" | "tryRead" => {
                                let inner = shared_inner_type(&receiver_type).unwrap_or_else(|| "Unit".to_string());
                                Some(format!("Result<{inner}, String>"))
                            }
                            _ => None,
                        };
                    }
                    if layout::canonical_type_name(&receiver_type) == "Map" {
                        return match member.name.as_str() {
                            "len" => Some("Int".to_string()),
                            "isEmpty" | "contains" => Some("Bool".to_string()),
                            "keys" | "values" => Some("List<String>".to_string()),
                            "entries" => Some("List<(String,String)>".to_string()),
                            "set" => Some("Unit".to_string()),
                            _ => None,
                        };
                    }
                    if layout::canonical_type_name(&receiver_type) == "String" {
                        return match member.name.as_str() {
                            "toUpper" => Some("String".to_string()),
                            "isEmpty" => Some("Bool".to_string()),
                            _ => None,
                        };
                    }
                    None
                }
                _ => None,
            },
            fa::Expr::Member(member) => {
                let object_type = self.infer_expr_type(&member.object)?;
                if member.optional {
                    let inner = option_inner_type(&object_type)?;
                    self.compiler
                        .session
                        .field_type(&inner, &member.name)
                        .map(|field| format!("Option<{field}>"))
                } else {
                    self.compiler.session.field_type(&object_type, &member.name)
                }
            }
            fa::Expr::Move(move_expr) => self.infer_expr_type(&move_expr.value),
            fa::Expr::Ref(reference) => self.infer_expr_type(&reference.value),
            fa::Expr::MutRef(reference) => self.infer_expr_type(&reference.value),
            fa::Expr::Question(question) => self
                .infer_expr_type(&question.value)
                .as_deref()
                .and_then(result_ok_type)
                .map(|value| value.to_string()),
            fa::Expr::If(_) | fa::Expr::When(_) => Some("Unit".to_string()),
            fa::Expr::Match(match_expr) => match_expr.arms.iter().find_map(|arm| match &arm.body {
                fa::ArmBody::Expr(expr) => self.infer_expr_type(expr),
                fa::ArmBody::Block(_) => Some("Unit".to_string()),
            }),
            fa::Expr::Lambda(lambda) => {
                let param_types: Vec<String> = lambda
                    .params
                    .iter()
                    .map(|p| p.type_name.clone().unwrap_or_else(|| "Any".to_string()))
                    .collect();
                let ret = lambda.return_type.clone().unwrap_or_else(|| "Any".to_string());
                Some(format!("fn({}) -> {}", param_types.join(", "), ret))
            }
            fa::Expr::Tuple(tuple) => {
                let types: Vec<String> = tuple.items.iter().filter_map(|item| self.infer_expr_type(item)).collect();
                Some(format!("({})", types.join(",")))
            }
        }
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|part| part.to_str())
        .unwrap_or("<input>")
        .to_string()
}

fn escape_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace("\\\\?\\", "")
        .replace('\\', "/")
}

fn escape_string(path: &Path) -> String {
    path.to_string_lossy()
        .replace("\\\\?\\", "")
        .replace('\\', "\\\\")
}

fn compute_future_uses(statements: &[fa::Statement]) -> Vec<HashSet<String>> {
    let mut future = vec![HashSet::new(); statements.len()];
    let mut seen = HashSet::new();
    for index in (0..statements.len()).rev() {
        future[index] = seen.clone();
        seen.extend(collect_stmt_names(&statements[index]));
    }
    future
}

fn collect_stmt_names(statement: &fa::Statement) -> HashSet<String> {
    match statement {
        fa::Statement::VarDecl(var_decl) => collect_expr_names(&var_decl.value),
        fa::Statement::Assign(assign) => {
            let mut names = collect_expr_names(&assign.target);
            names.extend(collect_expr_names(&assign.value));
            names
        }
        fa::Statement::Return(ret) => ret
            .value
            .as_ref()
            .map(collect_expr_names)
            .unwrap_or_default(),
        fa::Statement::While(while_stmt) => {
            let mut names = collect_expr_names(&while_stmt.condition);
            for statement in &while_stmt.body.statements {
                names.extend(collect_stmt_names(statement));
            }
            names
        }
        fa::Statement::For(for_stmt) => {
            let mut names = collect_expr_names(&for_stmt.iterable);
            for statement in &for_stmt.body.statements {
                names.extend(collect_stmt_names(statement));
            }
            names
        }
        fa::Statement::Loop(loop_stmt) => loop_stmt
            .body
            .statements
            .iter()
            .flat_map(collect_stmt_names)
            .collect(),
        fa::Statement::Spawn(spawn_stmt) => spawn_stmt
            .body
            .statements
            .iter()
            .flat_map(collect_stmt_names)
            .collect(),
        fa::Statement::Defer(defer_stmt) => collect_expr_names(&defer_stmt.expr),
        fa::Statement::Expr(expr_stmt) => collect_expr_names(&expr_stmt.expr),
        fa::Statement::TupleDestruct(td) => collect_expr_names(&td.value),
        fa::Statement::Break(_) | fa::Statement::Continue(_) => HashSet::new(),
    }
}

fn collect_block_names(block: &fa::Block) -> HashSet<String> {
    let mut names = HashSet::new();
    for stmt in &block.statements {
        names.extend(collect_stmt_names(stmt));
    }
    names
}

fn collect_expr_names(expr: &fa::Expr) -> HashSet<String> {
    match expr {
        fa::Expr::Literal(_) => HashSet::new(),
        fa::Expr::FString(fstring) => {
            let mut names = HashSet::new();
            let mut rest = fstring.template.as_str();
            while let Some(start) = rest.find('{') {
                let after = &rest[start + 1..];
                if let Some(end) = fstring_brace_end(after) {
                    let expr_text = after[..end].trim();
                    let source = format!("fn __names__() => {expr_text}");
                    if let Ok(program) = parse_source(&source, "<fstring-names>") {
                        for decl in &program.declarations {
                            if let fa::Declaration::Function(func) = decl {
                                if let Some(fa::Statement::Expr(expr_stmt)) = func.body.statements.first() {
                                    names.extend(collect_expr_names(&expr_stmt.expr));
                                }
                            }
                        }
                    }
                    rest = &after[end + 1..];
                } else {
                    break;
                }
            }
            names
        }
        fa::Expr::Name(name) => HashSet::from([name.value.clone()]),
        fa::Expr::List(list) => list.items.iter().flat_map(collect_expr_names).collect(),
        fa::Expr::Unary(unary) => collect_expr_names(&unary.value),
        fa::Expr::Binary(binary) => {
            let mut names = collect_expr_names(&binary.left);
            names.extend(collect_expr_names(&binary.right));
            names
        }
        fa::Expr::Call(call) => {
            let mut names = collect_expr_names(&call.callee);
            for arg in &call.args {
                names.extend(collect_expr_names(arg));
            }
            names
        }
        fa::Expr::Member(member) => collect_expr_names(&member.object),
        fa::Expr::Move(value) => collect_expr_names(&value.value),
        fa::Expr::Ref(value) => collect_expr_names(&value.value),
        fa::Expr::MutRef(value) => collect_expr_names(&value.value),
        fa::Expr::Question(value) => collect_expr_names(&value.value),
        fa::Expr::If(if_expr) => {
            let mut names = collect_expr_names(&if_expr.condition);
            for statement in &if_expr.then_branch.statements {
                names.extend(collect_stmt_names(statement));
            }
            if let Some(else_branch) = &if_expr.else_branch {
                match else_branch {
                    fa::ElseBranch::Block(block) => {
                        for statement in &block.statements {
                            names.extend(collect_stmt_names(statement));
                        }
                    }
                    fa::ElseBranch::IfExpr(expr) => {
                        names.extend(collect_expr_names(&fa::Expr::If(*expr.clone())))
                    }
                }
            }
            names
        }
        fa::Expr::Match(match_expr) => {
            let mut names = collect_expr_names(&match_expr.subject);
            for arm in &match_expr.arms {
                match &arm.body {
                    fa::ArmBody::Block(block) => {
                        for statement in &block.statements {
                            names.extend(collect_stmt_names(statement));
                        }
                    }
                    fa::ArmBody::Expr(expr) => names.extend(collect_expr_names(expr)),
                }
            }
            names
        }
        fa::Expr::When(when_expr) => {
            let mut names = HashSet::new();
            for arm in &when_expr.arms {
                if let Some(condition) = &arm.condition {
                    names.extend(collect_expr_names(condition));
                }
                match &arm.body {
                    fa::ArmBody::Block(block) => {
                        for statement in &block.statements {
                            names.extend(collect_stmt_names(statement));
                        }
                    }
                    fa::ArmBody::Expr(expr) => names.extend(collect_expr_names(expr)),
                }
            }
            names
        }
        fa::Expr::Tuple(tuple) => {
            let mut names = HashSet::new();
            for item in &tuple.items {
                names.extend(collect_expr_names(item));
            }
            names
        }
        fa::Expr::Lambda(lambda) => {
            let mut names = HashSet::new();
            for statement in &lambda.body.statements {
                names.extend(collect_stmt_names(statement));
            }
            names
        }
    }
}

/// Parse `"SIMD::<T,N>"` into `(T, N)`.
/// Accepts both `"SIMD::<Int>"` (lane count defaults to 4) and `"SIMD::<Float32,8>"`.
fn parse_simd_params(namespace: &str) -> Result<(String, u64), String> {
    let Some(start) = namespace.find('<') else {
        return Err("SIMD requires type parameters: SIMD::<T, N>".to_string());
    };
    let Some(end) = namespace.rfind('>') else {
        return Err("SIMD requires type parameters: SIMD::<T, N>".to_string());
    };
    let inner = &namespace[start + 1..end];
    let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
    match parts.len() {
        1 => Ok((parts[0].to_string(), 4)), // default lane count
        2 => {
            let n = parts[1].parse::<u64>().map_err(|_| {
                format!("SIMD lane count must be an integer, got `{}`", parts[1])
            })?;
            Ok((parts[0].to_string(), n))
        }
        _ => Err("SIMD expects SIMD::<T, N> with one type and one lane count".to_string()),
    }
}

fn validate_simd_type(t: &str) -> Result<(), String> {
    match t {
        "Float32" | "Float64" | "Int32" | "Int64" | "Int" | "Float" => Ok(()),
        _ => Err(format!(
            "unsupported SIMD element type `{t}` — must be Float32, Float64, Int32, or Int64"
        )),
    }
}

fn validate_simd_lanes(n: u64) -> Result<(), String> {
    if matches!(n, 2 | 4 | 8 | 16) {
        Ok(())
    } else {
        Err(format!(
            "unsupported SIMD lane count `{n}` — must be a power of 2 in {{2, 4, 8, 16}}"
        ))
    }
}

/// Map a Fuse SIMD element type + lane count to a Cranelift vector type.
/// Returns `None` for combinations that have no native Cranelift vector type
/// (e.g. 8 or 16 lanes on 128-bit ISAs), in which case the runtime fallback is used.
fn cranelift_simd_type(element_type: &str, lanes: u64) -> Option<cranelift_codegen::ir::Type> {
    match (element_type, lanes) {
        ("Float32", 4) => Some(types::F32X4),
        ("Float64" | "Float", 2) => Some(types::F64X2),
        ("Int32", 4) => Some(types::I32X4),
        ("Int64" | "Int", 2) => Some(types::I64X2),
        _ => None,
    }
}

/// Return the scalar lane type for a Cranelift vector type.
fn simd_lane_type(vec_ty: cranelift_codegen::ir::Type) -> cranelift_codegen::ir::Type {
    match vec_ty {
        types::F32X4 => types::F32,
        types::F64X2 => types::F64,
        types::I32X4 => types::I32,
        types::I64X2 => types::I64,
        _ => unreachable!("unsupported vector type"),
    }
}

/// Return the number of lanes in a Cranelift vector type.
fn simd_lane_count(vec_ty: cranelift_codegen::ir::Type) -> u8 {
    match vec_ty {
        types::F32X4 | types::I32X4 => 4,
        types::F64X2 | types::I64X2 => 2,
        _ => unreachable!("unsupported vector type"),
    }
}

/// Return true if the vector type has floating-point lanes.
fn simd_is_float(vec_ty: cranelift_codegen::ir::Type) -> bool {
    matches!(vec_ty, types::F32X4 | types::F64X2)
}

/// Find the closing `}` for an f-string interpolation, accounting for nested
/// brace pairs (e.g. in map literals or nested f-strings).
fn fstring_brace_end(s: &str) -> Option<usize> {
    let mut depth: usize = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' if depth == 0 => return Some(i),
            '}' => depth -= 1,
            _ => {}
        }
    }
    None
}
