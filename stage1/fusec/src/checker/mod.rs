mod builtins;
mod ownership;
mod types;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::nodes as hir;
use crate::autogen;
use crate::codegen::type_names::{split_generic_args, split_tuple_types, unify_match_arm_types};
use crate::common::resolve_import_path;
use crate::error::{Diagnostic, Span};
use crate::hir::{lower_program, Module};
use crate::parser::parse_source;
use crate::Target;

#[derive(Clone, Debug)]
struct BindingInfo {
    mutable: bool,
    param_convention: Option<String>,
    type_name: Option<String>,
    rank: Option<i64>,
    held_rank: Option<i64>,
    held_rank_is_write: bool,
    moved: bool,
    used: bool,
}

#[derive(Clone, Debug)]
enum Symbol {
    Function { node: hir::FunctionDecl, is_pub: bool },
    Data { node: hir::DataClassDecl, is_pub: bool },
    Enum { node: hir::EnumDecl, is_pub: bool },
    Struct { node: hir::StructDecl, is_pub: bool },
}

#[derive(Clone, Debug)]
struct InterfaceInfo {
    name: String,
    type_params: Vec<String>,
    parents: Vec<String>,
    methods: Vec<hir::InterfaceMethod>,
    default_methods: Vec<hir::FunctionDecl>,
    span: Span,
}

#[derive(Clone, Debug)]
struct ModuleInfo {
    path: PathBuf,
    module: Module,
    symbols: HashMap<String, Symbol>,
    extension_functions: HashMap<(String, String), hir::FunctionDecl>,
    static_functions: HashMap<(String, String), hir::FunctionDecl>,
    interfaces: HashMap<String, InterfaceInfo>,
    /// type name → list of interface names it declares `implements`
    implements: HashMap<String, Vec<String>>,
}

pub struct Checker {
    module_cache: HashMap<PathBuf, ModuleInfo>,
    diagnostics: Vec<Diagnostic>,
    current_file: PathBuf,
    warn_unused: bool,
    target: Target,
}

/// Modules unavailable when compiling to `--target wasi`.
static WASI_UNAVAILABLE_MODULES: &[&str] = &[
    "io", "net", "http", "http_server", "process", "os", "env", "sys", "time", "timer", "path",
];

impl Checker {
    pub fn new() -> Self {
        Self {
            module_cache: HashMap::new(),
            diagnostics: Vec::new(),
            current_file: PathBuf::from("<unknown>"),
            warn_unused: false,
            target: Target::Native,
        }
    }

    pub fn check_path(&mut self, path: &Path) -> Vec<Diagnostic> {
        self.diagnostics.clear();
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        match self.load_module(&path) {
            Ok(module) => self.check_module(&module),
            Err(diag) => self.diagnostics.push(diag),
        }
        std::mem::take(&mut self.diagnostics)
    }

    fn load_module(&mut self, path: &Path) -> Result<ModuleInfo, Diagnostic> {
        if let Some(module) = self.module_cache.get(path) {
            return Ok(module.clone());
        }

        let source = fs::read_to_string(path).map_err(|error| {
            Diagnostic::error(
                &format!("cannot read `{}`: {error}", path.display()),
                &display_name(path),
                Span::new(1, 1),
                None,
            )
        })?;
        let filename = display_name(path);
        let program = parse_source(&source, &filename)?;
        let module = lower_program(&program, path.to_path_buf());
        let mut info = ModuleInfo {
            path: path.to_path_buf(),
            module,
            symbols: HashMap::new(),
            extension_functions: HashMap::new(),
            static_functions: HashMap::new(),
            interfaces: HashMap::new(),
            implements: HashMap::new(),
        };

        for function in &info.module.functions {
            if let Some(receiver_type) = &function.receiver_type {
                let is_static = function.params.first().map_or(true, |p| p.name != "self");
                if is_static {
                    info.static_functions
                        .insert((receiver_type.clone(), function.name.clone()), function.clone());
                } else {
                    info.extension_functions
                        .insert((receiver_type.clone(), function.name.clone()), function.clone());
                }
            } else {
                info.symbols.insert(
                    function.name.clone(),
                    Symbol::Function {
                        node: function.clone(),
                        is_pub: function.is_pub,
                    },
                );
            }
        }
        // Load extension functions from HIR (Type.method definitions).
        // Substitute `Self` -> concrete receiver type in return types
        // and parameter types so that downstream type inference sees
        // concrete types and not the generic placeholder. Mirror of
        // load_module_recursive in codegen/object_backend.rs:297-313.
        // Without this, methods like `fn Request.withPath(mutref self,
        // p: String) -> mutref Self` produce a chain receiver type
        // that the lookup cannot canonicalize.
        for ((receiver_type, name), function) in &info.module.extension_functions {
            let is_static = function.params.first().map_or(true, |p| p.name != "self");
            let mut function = function.clone();
            if let Some(ref mut rt) = function.return_type {
                if rt.contains("Self") {
                    *rt = rt.replace("Self", receiver_type);
                }
            }
            for param in function.params.iter_mut() {
                if let Some(ref mut tn) = param.type_name {
                    if tn.contains("Self") {
                        *tn = tn.replace("Self", receiver_type);
                    }
                }
            }
            if is_static {
                info.static_functions
                    .insert((receiver_type.clone(), name.clone()), function);
            } else {
                info.extension_functions
                    .insert((receiver_type.clone(), name.clone()), function);
            }
        }
        for data in &info.module.data_classes {
            info.symbols.insert(
                data.name.clone(),
                Symbol::Data {
                    node: data.clone(),
                    is_pub: data.is_pub,
                },
            );
            // Register data class instance/static methods as extensions
            // so member calls resolve in infer_expr_type and check_call.
            // Mirror of load_module_recursive in object_backend.rs:369-385.
            for method in &data.methods {
                if method.name == "__del__" {
                    continue; // destructor — called via runtime bridge
                }
                let mut function = method.clone();
                function.receiver_type = Some(data.name.clone());
                if let Some(ref mut rt) = function.return_type {
                    if rt.contains("Self") {
                        *rt = rt.replace("Self", &data.name);
                    }
                }
                for param in function.params.iter_mut() {
                    if let Some(ref mut tn) = param.type_name {
                        if tn.contains("Self") {
                            *tn = tn.replace("Self", &data.name);
                        }
                    }
                }
                let is_static = function.params.first().map_or(true, |p| p.name != "self");
                let key = (data.name.clone(), method.name.clone());
                if is_static {
                    info.static_functions.insert(key, function);
                } else {
                    info.extension_functions.insert(key, function);
                }
            }
        }
        for enum_decl in &info.module.enums {
            info.symbols.insert(
                enum_decl.name.clone(),
                Symbol::Enum {
                    node: enum_decl.clone(),
                    is_pub: enum_decl.is_pub,
                },
            );
        }
        for struct_decl in &info.module.structs {
            info.symbols.insert(
                struct_decl.name.clone(),
                Symbol::Struct {
                    node: struct_decl.clone(),
                    is_pub: struct_decl.is_pub,
                },
            );
            // Register struct instance/static methods as extensions
            // so member calls resolve. Mirror of object_backend.rs:343-368.
            for method in &struct_decl.methods {
                let mut function = method.clone();
                function.receiver_type = Some(struct_decl.name.clone());
                if let Some(ref mut rt) = function.return_type {
                    if rt.contains("Self") {
                        *rt = rt.replace("Self", &struct_decl.name);
                    }
                }
                for param in function.params.iter_mut() {
                    if let Some(ref mut tn) = param.type_name {
                        if tn.contains("Self") {
                            *tn = tn.replace("Self", &struct_decl.name);
                        }
                    }
                }
                let is_static = function.params.first().map_or(true, |p| p.name != "self");
                let key = (struct_decl.name.clone(), method.name.clone());
                if is_static {
                    info.static_functions.insert(key, function);
                } else {
                    info.extension_functions.insert(key, function);
                }
            }
        }
        for iface in &info.module.interfaces {
            info.interfaces.insert(
                iface.name.clone(),
                InterfaceInfo {
                    name: iface.name.clone(),
                    type_params: iface.type_params.clone(),
                    parents: iface.parents.clone(),
                    methods: iface.methods.clone(),
                    default_methods: Vec::new(),
                    span: iface.span,
                },
            );
        }
        // Detect default methods: extension methods on interface names.
        for ((receiver_type, _name), function) in &info.extension_functions {
            if let Some(iface_info) = info.interfaces.get_mut(receiver_type) {
                iface_info.default_methods.push(function.clone());
            }
        }
        for data in &info.module.data_classes {
            if !data.implements.is_empty() {
                info.implements.insert(data.name.clone(), data.implements.clone());
            }
        }
        for enum_decl in &info.module.enums {
            if !enum_decl.implements.is_empty() {
                info.implements.insert(enum_decl.name.clone(), enum_decl.implements.clone());
            }
        }
        for struct_decl in &info.module.structs {
            if !struct_decl.implements.is_empty() {
                info.implements.insert(struct_decl.name.clone(), struct_decl.implements.clone());
            }
        }
        for extern_fn in &info.module.extern_fns {
            let synthetic = hir::FunctionDecl {
                name: extern_fn.name.clone(),
                type_params: Vec::new(),
                params: extern_fn.params.clone(),
                return_type: extern_fn.return_type.clone(),
                body: hir::Block {
                    statements: Vec::new(),
                    span: extern_fn.span,
                },
                is_pub: extern_fn.is_pub,
                annotations: Vec::new(),
                receiver_type: None,
                span: extern_fn.span,
            };
            info.symbols.insert(
                extern_fn.name.clone(),
                Symbol::Function {
                    node: synthetic,
                    is_pub: extern_fn.is_pub,
                },
            );
        }

        self.module_cache.insert(path.to_path_buf(), info.clone());
        Ok(info)
    }

    fn check_module(&mut self, module: &ModuleInfo) {
        self.current_file = module.path.clone();
        for import in module.module.imports.clone() {
            self.check_import(module, &import);
        }

        let filename = display_name(&module.path);
        for function in module.module.functions.clone() {
            self.validate_annotations(&function.annotations, types::AnnotationPosition::Function, &filename);
            self.check_function(module, &function, None);
        }
        for data in module.module.data_classes.clone() {
            self.validate_annotations(&data.annotations, types::AnnotationPosition::Type, &filename);
            for method in data.methods.clone() {
                self.validate_annotations(&method.annotations, types::AnnotationPosition::Function, &filename);
                self.check_function(module, &method, Some(&data));
            }
        }
        for struct_decl in module.module.structs.clone() {
            self.validate_annotations(&struct_decl.annotations, types::AnnotationPosition::Type, &filename);
            for method in struct_decl.methods.clone() {
                self.validate_annotations(&method.annotations, types::AnnotationPosition::Function, &filename);
                let as_data = hir::DataClassDecl {
                    name: struct_decl.name.clone(),
                    type_params: struct_decl.type_params.clone(),
                    fields: struct_decl.fields.clone(),
                    methods: struct_decl.methods.clone(),
                    is_pub: struct_decl.is_pub,
                    annotations: struct_decl.annotations.clone(),
                    implements: struct_decl.implements.clone(),
                    span: struct_decl.span,
                };
                self.check_function(module, &method, Some(&as_data));
            }
        }
        // Check interface conformance for all types declaring `implements`.
        self.check_interface_conformance(module);
        // Check extension and static functions (Type.method defined outside type body).
        for function in module.extension_functions.values() {
            self.validate_annotations(&function.annotations, types::AnnotationPosition::Function, &filename);
            self.check_function(module, function, None);
        }
        for function in module.static_functions.values() {
            self.validate_annotations(&function.annotations, types::AnnotationPosition::Function, &filename);
            self.check_function(module, function, None);
        }
    }

    fn check_interface_conformance(&mut self, module: &ModuleInfo) {
        let filename = display_name(&module.path);
        let module_path = module.path.clone();
        let implements = module.implements.clone();
        // Auto-load stdlib interface modules before checking conformance.
        for iface_names in implements.values() {
            for iface_name in iface_names {
                self.ensure_stdlib_interface_loaded(iface_name, &module_path);
            }
        }
        for (type_name, iface_names) in &implements {
            // Find the span for the type declaration (for error reporting).
            let type_span = self.module_cache.get(&module_path).and_then(|m| {
                m.module.data_classes.iter().find(|d| d.name == *type_name).map(|d| d.span)
                    .or_else(|| m.module.enums.iter().find(|e| e.name == *type_name).map(|e| e.span))
                    .or_else(|| m.module.structs.iter().find(|s| s.name == *type_name).map(|s| s.span))
            }).unwrap_or(Span::new(1, 1));
            for iface_name in iface_names {
                let Some(iface) = self.resolve_interface(iface_name) else {
                    self.add_error(&filename, type_span, format!("unknown interface `{iface_name}`"), None);
                    continue;
                };
                // Auto-load and verify parents exist.
                for parent_name in &iface.parents {
                    self.ensure_stdlib_interface_loaded(parent_name, &module_path);
                    if self.resolve_interface(parent_name).is_none() {
                        self.add_error(&filename, iface.span, format!("unknown parent interface `{parent_name}`"), None);
                    }
                }
                // Collect all required methods (own + inherited).
                let Some(required_methods) = self.collect_interface_methods(&iface) else {
                    continue; // parent resolution failed; error already emitted
                };
                // Marker interface (no methods) — always satisfied.
                if required_methods.is_empty() {
                    continue;
                }
                // Collect default methods from the interface (own + inherited).
                let defaults = self.collect_interface_defaults(&iface);
                // Determine type kind for auto-generation eligibility.
                let type_kind = self.module_cache.get(&module_path).and_then(|m| {
                    if m.module.data_classes.iter().any(|d| d.name == *type_name) {
                        Some(autogen::TypeKind::DataClass)
                    } else if let Some(s) = m.module.structs.iter().find(|s| s.name == *type_name) {
                        Some(autogen::classify_type(&s.annotations, true))
                    } else if m.module.enums.iter().any(|e| e.name == *type_name) {
                        Some(autogen::TypeKind::Enum)
                    } else {
                        None
                    }
                });
                for method in &required_methods {
                    // W5.4.4: type's own extension method takes priority over default.
                    let ext = self.resolve_extension(type_name, &method.name);
                    let Some(ext_fn) = ext else {
                        // W5.4.3: if method missing but default exists, mark as satisfied.
                        let has_default = defaults.iter().any(|d| d.name == method.name);
                        if has_default {
                            continue;
                        }
                        // Check if auto-generation can satisfy this method.
                        // Only suppress for stdlib interfaces (not local definitions
                        // that happen to share the same name).
                        let is_stdlib_iface = Self::stdlib_interface_module(iface_name).is_some()
                            && !self.module_cache.get(&module_path)
                                .map(|m| m.interfaces.contains_key(iface_name))
                                .unwrap_or(false);
                        let can_autogen = is_stdlib_iface
                            && type_kind
                                .map(|kind| autogen::can_auto_generate(kind, iface_name))
                                .unwrap_or(false);
                        if !can_autogen {
                            self.add_error(
                                &filename,
                                type_span,
                                format!(
                                    "type `{type_name}` declares `implements {iface_name}` but does not implement method `{}`",
                                    method.name
                                ),
                                None,
                            );
                        }
                        continue;
                    };
                    // Check param count (interface params include self, extension params include self).
                    if ext_fn.params.len() != method.params.len() {
                        self.add_error(
                            &filename,
                            type_span,
                            format!(
                                "method `{}` on `{type_name}` has {} parameter(s), but interface `{iface_name}` requires {}",
                                method.name, ext_fn.params.len(), method.params.len()
                            ),
                            None,
                        );
                        continue;
                    }
                    // Check return type match (resolve `Self` → concrete type,
                    // and generic type params from the implements clause).
                    if let Some(ref iface_rt) = method.return_type {
                        let mut resolved_rt = iface_rt.replace("Self", type_name);
                        // Substitute generic type params: e.g., for
                        // `implements Convertible<String>`, map T → String.
                        if let Some(args_str) = iface_name.split_once('<')
                            .map(|(_, rest)| rest.strip_suffix('>').unwrap_or(rest))
                        {
                            let args: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
                            for (param, arg) in iface.type_params.iter().zip(args.iter()) {
                                resolved_rt = resolved_rt.replace(param.as_str(), arg);
                            }
                        }
                        if ext_fn.return_type.as_deref() != Some(resolved_rt.as_str()) {
                            self.add_error(
                                &filename,
                                type_span,
                                format!(
                                    "method `{}` on `{type_name}` returns `{}`, but interface `{iface_name}` requires `{}`",
                                    method.name,
                                    ext_fn.return_type.as_deref().unwrap_or("Unit"),
                                    resolved_rt,
                                ),
                                None,
                            );
                        }
                    }
                    // Check ownership convention on self parameter (W5.3.10-11).
                    if let (Some(iface_self), Some(ext_self)) = (method.params.first(), ext_fn.params.first()) {
                        if iface_self.name == "self" && ext_self.name == "self" {
                            let iface_conv = iface_self.convention.as_deref().unwrap_or("owned");
                            let ext_conv = ext_self.convention.as_deref().unwrap_or("owned");
                            if iface_conv != ext_conv {
                                self.add_error(
                                    &filename,
                                    type_span,
                                    format!(
                                        "method `{}` on `{type_name}` has `{ext_conv} self` but interface `{iface_name}` requires `{iface_conv} self`",
                                        method.name
                                    ),
                                    None,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn check_import(&mut self, module: &ModuleInfo, import: &hir::ImportDecl) {
        // Validate module availability for the current target.
        if self.target == Target::Wasi {
            let mod_name = import.module_path.rsplit('.').next().unwrap_or(&import.module_path);
            if WASI_UNAVAILABLE_MODULES.contains(&mod_name) {
                self.add_error(
                    &display_name(&module.path),
                    import.span,
                    format!("module `{}` is not available on target `wasi`", import.module_path),
                    Some(format!("the `{mod_name}` module requires OS features not supported by WASI")),
                );
                return;
            }
        }

        let Some(target_path) = resolve_import_path(&module.path, &import.module_path) else {
            self.add_error(
                &display_name(&module.path),
                import.span,
                format!("cannot resolve import `{}`", import.module_path),
                None,
            );
            return;
        };

        let target = match self.load_module(&target_path) {
            Ok(target) => target,
            Err(diag) => {
                self.diagnostics.push(diag);
                return;
            }
        };

        if let Some(items) = &import.items {
            for item in items {
                let visible = target.symbols.get(item).and_then(|symbol| match symbol {
                    Symbol::Function { is_pub, .. }
                    | Symbol::Data { is_pub, .. }
                    | Symbol::Enum { is_pub, .. }
                    | Symbol::Struct { is_pub, .. } => Some(*is_pub),
                });
                if visible != Some(true) {
                    self.add_error(
                        &display_name(&module.path),
                        import.span,
                        format!("cannot import non-pub item `{item}`"),
                        None,
                    );
                }
            }
        }
    }

    fn validate_annotations(
        &mut self,
        annotations: &[hir::Annotation],
        position: types::AnnotationPosition,
        filename: &str,
    ) {
        for annotation in annotations {
            let Some(spec) = types::annotation_spec(&annotation.name) else {
                self.add_error(filename, annotation.span, format!("unknown annotation `@{}`", annotation.name), None);
                continue;
            };
            if !spec.positions.contains(&position) {
                let allowed = spec.positions.iter().map(|p| match p {
                    types::AnnotationPosition::Function => "function",
                    types::AnnotationPosition::Type => "type",
                    types::AnnotationPosition::Statement => "statement",
                }).collect::<Vec<_>>().join(", ");
                self.add_error(
                    filename,
                    annotation.span,
                    format!("`@{}` cannot be used here — allowed on: {}", annotation.name, allowed),
                    None,
                );
            }
            match spec.args {
                types::AnnotationArgs::None => {
                    if !annotation.args.is_empty() {
                        self.add_error(filename, annotation.span, format!("`@{}` takes no arguments", annotation.name), None);
                    }
                }
                types::AnnotationArgs::OneInt => {
                    if annotation.args.len() != 1 || !matches!(annotation.args.first(), Some(hir::AnnotationArg::Int(_))) {
                        self.add_error(filename, annotation.span, format!("`@{}` requires one integer argument", annotation.name), None);
                    }
                }
                types::AnnotationArgs::OneString => {
                    if annotation.args.len() != 1 || !matches!(annotation.args.first(), Some(hir::AnnotationArg::String(_))) {
                        self.add_error(filename, annotation.span, format!("`@{}` requires one string argument", annotation.name), None);
                    }
                }
            }
        }
    }

    fn check_function(
        &mut self,
        module: &ModuleInfo,
        function: &hir::FunctionDecl,
        owner: Option<&hir::DataClassDecl>,
    ) {
        let mut scope = HashMap::new();
        let owner_name = owner.map(|item| item.name.clone());
        for param in &function.params {
            scope.insert(
                param.name.clone(),
                BindingInfo {
                    mutable: true,
                    param_convention: param.convention.clone(),
                    type_name: param.type_name.clone(),
                    rank: None,
                    held_rank: None,
                    held_rank_is_write: false,
                    moved: false,
                    used: false,
                },
            );
        }
        for stmt in &function.body.statements {
            self.check_statement(module, stmt, &mut scope, 0, owner_name.as_deref());
        }
        if self.warn_unused {
            for (name, binding) in &scope {
                if !binding.used && !name.starts_with('_') && binding.param_convention.is_none() {
                    self.diagnostics.push(
                        Diagnostic::warning(
                            format!("unused binding `{name}`"),
                            display_name(&module.path),
                            function.span,
                            None,
                        )
                        .with_code("W0001")
                        .with_help(format!("prefix with `_` to suppress: `_{name}`")),
                    );
                }
            }
        }
        if let Some(expected) = &function.return_type {
            if let Some(actual) = self.infer_block_type(module, &function.body, &scope, owner_name.as_deref()) {
                if !types::type_matches(expected, &actual) {
                    self.add_error(
                        &display_name(&module.path),
                        function.span,
                        format!("type mismatch: expected `{expected}`, found `{actual}`"),
                        None,
                    );
                }
            }
        }
    }

    fn infer_block_type(
        &self,
        module: &ModuleInfo,
        block: &hir::Block,
        scope: &HashMap<String, BindingInfo>,
        owner_name: Option<&str>,
    ) -> Option<String> {
        let last = block.statements.last()?;
        match last {
            hir::Statement::Expr(expr) => self.infer_expr_type(module, &expr.expr, scope, owner_name),
            hir::Statement::Return(ret) => ret
                .value
                .as_ref()
                .and_then(|expr| self.infer_expr_type(module, expr, scope, owner_name)),
            // `loop { }` without break is infinite — must exit via return,
            // so the type is Never (!) which matches any expected type.
            hir::Statement::Loop(_) => Some("!".to_string()),
            _ => Some("Unit".to_string()),
        }
    }

    /// Compute the type a `match` / `when` arm body contributes to
    /// arm unification. This is NOT the same as `infer_block_type`:
    /// `infer_block_type` is used for function-body return
    /// verification, where `{ return foo }` means "the function
    /// returns `foo`'s type" and should contribute that type. For
    /// arm unification, `{ return foo }` means "this arm diverges
    /// before producing a value" — it contributes `!` (Never),
    /// which the unifier filters and allows siblings to dominate.
    /// Similarly for `{ break }`, `{ continue }`, and any block
    /// whose last statement is `Loop(_)` (infinite loops).
    ///
    /// Added in B12 triage because `val x = match y { Ok(v) => v,
    /// Err(_) => { sys.exit(1); return } }` in stage2/src/main.fuse
    /// produced arm types `[List<CachedLoad>, Unit]`, U5-failed the
    /// unifier, and left `x` typeless. The codegen mirror lives in
    /// `compile_match`'s Block-arm branch.
    fn arm_body_type_for_unify(
        &self,
        module: &ModuleInfo,
        body: &hir::ArmBody,
        scope: &HashMap<String, BindingInfo>,
        owner_name: Option<&str>,
    ) -> Option<String> {
        match body {
            hir::ArmBody::Expr(expr) => self.infer_expr_type(module, expr, scope, owner_name),
            hir::ArmBody::Block(block) => {
                if let Some(last) = block.statements.last() {
                    match last {
                        hir::Statement::Return(_)
                        | hir::Statement::Break(_)
                        | hir::Statement::Continue(_)
                        | hir::Statement::Loop(_) => return Some("!".to_string()),
                        _ => {}
                    }
                }
                self.infer_block_type(module, block, scope, owner_name)
            }
        }
    }

    fn check_statement(
        &mut self,
        module: &ModuleInfo,
        stmt: &hir::Statement,
        scope: &mut HashMap<String, BindingInfo>,
        loop_depth: usize,
        owner_name: Option<&str>,
    ) {
        match stmt {
            hir::Statement::VarDecl(var_decl) => {
                self.validate_annotations(&var_decl.annotations, types::AnnotationPosition::Statement, &display_name(&module.path));
                let inferred = self.infer_expr_type(module, &var_decl.value, scope, owner_name);
                let ty = var_decl.type_name.clone().or(inferred.clone());
                if let (Some(declared), Some(actual)) = (&var_decl.type_name, &inferred) {
                    if !types::type_matches(declared, actual) {
                        self.add_error(
                            &display_name(&module.path),
                            var_decl.span,
                            format!("type mismatch: `{}` declared as `{}`, initializer has type `{}`", var_decl.name, declared, actual),
                            None,
                        );
                    }
                }
                self.check_expr(module, &var_decl.value, scope, owner_name, loop_depth);
                let held = self.held_rank_from_expr(scope, &var_decl.value);
                let held_rank = held.map(|(r, _)| r);
                let held_rank_is_write = held.map_or(false, |(_, w)| w);
                if let Some(type_name) = ty.as_deref() {
                    if type_name.starts_with("Shared") && var_decl.rank().is_none() {
                        self.add_error(
                            &display_name(&module.path),
                            var_decl.span,
                            "Shared<T> requires @rank annotation",
                            None,
                        );
                    }
                }
                if let Some(acquired_rank) = held_rank {
                    if let Some(current_max) = scope.values().filter_map(|binding| binding.held_rank).max() {
                        if acquired_rank < current_max {
                            self.add_error(
                                &display_name(&module.path),
                                var_decl.span,
                                format!(
                                    "cannot acquire @rank({acquired_rank}) while holding @rank({current_max})"
                                ),
                                None,
                            );
                        }
                    }
                }
                scope.insert(
                    var_decl.name.clone(),
                    BindingInfo {
                        mutable: var_decl.mutable,
                        param_convention: None,
                        type_name: ty,
                        rank: var_decl.rank(),
                        held_rank,
                        held_rank_is_write,
                        moved: false,
                        used: false,
                    },
                );
            }
            hir::Statement::Assign(assign) => {
                self.check_expr(module, &assign.value, scope, owner_name, loop_depth);
                self.check_assignment_target(module, &assign.target, scope, owner_name, loop_depth, assign.span);
            }
            hir::Statement::Return(ret) => {
                if let Some(value) = &ret.value {
                    self.check_expr(module, value, scope, owner_name, loop_depth);
                }
            }
            hir::Statement::While(while_stmt) => {
                self.check_expr(module, &while_stmt.condition, scope, owner_name, loop_depth);
                let mut child = scope.clone();
                for inner in &while_stmt.body.statements {
                    self.check_statement(module, inner, &mut child, loop_depth + 1, owner_name);
                }
                // Second pass: re-check with moved state to catch
                // use-after-move on the next iteration.
                let any_moved = scope.iter().any(|(n,b)| !b.moved && child.get(n).map_or(false, |cb| cb.moved));
                if any_moved {
                    for inner in &while_stmt.body.statements {
                        self.check_statement(module, inner, &mut child, loop_depth + 1, owner_name);
                    }
                }
                // Merge moved state to parent.
                for (name, binding) in scope.iter_mut() {
                    if child.get(name).map_or(false, |b| b.moved) {
                        binding.moved = true;
                    }
                }
            }
            hir::Statement::For(for_stmt) => {
                self.check_expr(module, &for_stmt.iterable, scope, owner_name, loop_depth);
                let mut child = scope.clone();
                child.insert(
                    for_stmt.name.clone(),
                    BindingInfo {
                        mutable: true,
                        param_convention: None,
                        type_name: None,
                        rank: None,
                        held_rank: None,
                        held_rank_is_write: false,
                        moved: false,
                        used: false,
                    },
                );
                for inner in &for_stmt.body.statements {
                    self.check_statement(module, inner, &mut child, loop_depth + 1, owner_name);
                }
                // Second pass to catch use-after-move on next iteration.
                let any_moved = scope.iter().any(|(n,b)| !b.moved && child.get(n).map_or(false, |cb| cb.moved));
                if any_moved {
                    for inner in &for_stmt.body.statements {
                        self.check_statement(module, inner, &mut child, loop_depth + 1, owner_name);
                    }
                }
                for (name, binding) in scope.iter_mut() {
                    if child.get(name).map_or(false, |b| b.moved) {
                        binding.moved = true;
                    }
                }
            }
            hir::Statement::Loop(loop_stmt) => {
                let mut child = scope.clone();
                for inner in &loop_stmt.body.statements {
                    self.check_statement(module, inner, &mut child, loop_depth + 1, owner_name);
                }
                // Second pass to catch use-after-move on next iteration.
                let any_moved = scope.iter().any(|(n,b)| !b.moved && child.get(n).map_or(false, |cb| cb.moved));
                if any_moved {
                    for inner in &loop_stmt.body.statements {
                        self.check_statement(module, inner, &mut child, loop_depth + 1, owner_name);
                    }
                }
                for (name, binding) in scope.iter_mut() {
                    if child.get(name).map_or(false, |b| b.moved) {
                        binding.moved = true;
                    }
                }
            }
            hir::Statement::Spawn(spawn_stmt) => {
                self.check_spawn_mutref_capture(module, &spawn_stmt.body.statements, scope);
                let mut child = scope.clone();
                for inner in &spawn_stmt.body.statements {
                    self.check_statement(module, inner, &mut child, loop_depth, owner_name);
                }
            }
            hir::Statement::Break(span) => {
                if loop_depth == 0 {
                    self.add_error(&display_name(&module.path), *span, "`break` outside loop", None);
                }
            }
            hir::Statement::Continue(span) => {
                if loop_depth == 0 {
                    self.add_error(&display_name(&module.path), *span, "`continue` outside loop", None);
                }
            }
            hir::Statement::Defer(stmt) => self.check_expr(module, &stmt.expr, scope, owner_name, loop_depth),
            hir::Statement::Expr(stmt) => self.check_expr(module, &stmt.expr, scope, owner_name, loop_depth),
            hir::Statement::TupleDestruct(td) => {
                self.check_expr(module, &td.value, scope, owner_name, loop_depth);
                for name in &td.names {
                    scope.insert(name.clone(), BindingInfo {
                        mutable: false,
                        param_convention: None,
                        type_name: None,
                        rank: None,
                        held_rank: None,
                        held_rank_is_write: false,
                        moved: false,
                        used: false,
                    });
                }
            }
        }
    }

    fn check_assignment_target(
        &mut self,
        module: &ModuleInfo,
        target: &hir::Expr,
        scope: &mut HashMap<String, BindingInfo>,
        owner_name: Option<&str>,
        loop_depth: usize,
        span: Span,
    ) {
        match target {
            hir::Expr::Name(name) => {
                if let Some(binding) = scope.get(&name.value) {
                    if !binding.mutable {
                        self.add_error(
                            &display_name(&module.path),
                            span,
                            format!("cannot assign to immutable binding `{}`", name.value),
                            None,
                        );
                    }
                }
            }
            hir::Expr::Member(member) => {
                if let Some(root) = ownership::root_name(target) {
                    if let Some(binding) = scope.get(root) {
                        if binding.param_convention.as_deref() == Some("ref") {
                            self.add_error(
                                &display_name(&module.path),
                                span,
                                format!("cannot assign through `ref` parameter `{}`", root),
                                None,
                            );
                            return;
                        }
                        if let Some(root_type) = binding.type_name.as_deref() {
                            if let Some(data) = self.find_data_decl(root_type) {
                                if let Some(field) = data.fields.iter().find(|field| field.name == member.name) {
                                    if !field.mutable {
                                        self.add_error(
                                            &display_name(&module.path),
                                            span,
                                            format!("cannot assign to immutable field `{}`", member.name),
                                            None,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                self.check_expr(module, &member.object, scope, owner_name, loop_depth);
            }
            _ => self.check_expr(module, target, scope, owner_name, loop_depth),
        }
    }

    fn check_spawn_mutref_capture(
        &mut self,
        module: &ModuleInfo,
        statements: &[hir::Statement],
        outer_scope: &HashMap<String, BindingInfo>,
    ) {
        let outer_names = outer_scope.keys().cloned().collect::<HashSet<_>>();
        let mut local_names = HashSet::new();
        for statement in statements {
            self.check_spawn_statement(module, statement, &outer_names, &mut local_names);
        }
    }

    fn check_spawn_statement(
        &mut self,
        module: &ModuleInfo,
        statement: &hir::Statement,
        outer_names: &HashSet<String>,
        local_names: &mut HashSet<String>,
    ) {
        match statement {
            hir::Statement::VarDecl(var_decl) => {
                self.check_spawn_expr(module, &var_decl.value, outer_names, local_names);
                local_names.insert(var_decl.name.clone());
            }
            hir::Statement::Assign(assign) => {
                // Check if the assignment target is an outer-scope variable —
                // mutating an outer var inside spawn is a data race.
                let root_name = Self::assign_target_root(&assign.target);
                if let Some((name, span)) = root_name {
                    if outer_names.contains(name) && !local_names.contains(name) {
                        self.add_error(
                            &display_name(&module.path),
                            span,
                            format!("cannot mutate `{}` across spawn boundary — use Shared<T>", name),
                            None,
                        );
                    }
                }
                self.check_spawn_expr(module, &assign.target, outer_names, local_names);
                self.check_spawn_expr(module, &assign.value, outer_names, local_names);
            }
            hir::Statement::Return(ret) => {
                if let Some(value) = &ret.value {
                    self.check_spawn_expr(module, value, outer_names, local_names);
                }
            }
            hir::Statement::Spawn(spawn_stmt) => {
                for inner in &spawn_stmt.body.statements {
                    self.check_spawn_statement(module, inner, outer_names, local_names);
                }
            }
            hir::Statement::While(while_stmt) => {
                self.check_spawn_expr(module, &while_stmt.condition, outer_names, local_names);
                for inner in &while_stmt.body.statements {
                    self.check_spawn_statement(module, inner, outer_names, local_names);
                }
            }
            hir::Statement::For(for_stmt) => {
                self.check_spawn_expr(module, &for_stmt.iterable, outer_names, local_names);
                let inserted = local_names.insert(for_stmt.name.clone());
                for inner in &for_stmt.body.statements {
                    self.check_spawn_statement(module, inner, outer_names, local_names);
                }
                if inserted {
                    local_names.remove(&for_stmt.name);
                }
            }
            hir::Statement::Loop(loop_stmt) => {
                for inner in &loop_stmt.body.statements {
                    self.check_spawn_statement(module, inner, outer_names, local_names);
                }
            }
            hir::Statement::Defer(stmt) => {
                self.check_spawn_expr(module, &stmt.expr, outer_names, local_names);
            }
            hir::Statement::TupleDestruct(td) => {
                self.check_spawn_expr(module, &td.value, outer_names, local_names);
            }
            hir::Statement::Expr(stmt) => {
                self.check_spawn_expr(module, &stmt.expr, outer_names, local_names);
            }
            hir::Statement::Break(_) | hir::Statement::Continue(_) => {}
        }
    }

    fn check_spawn_expr(
        &mut self,
        module: &ModuleInfo,
        expr: &hir::Expr,
        outer_names: &HashSet<String>,
        local_names: &HashSet<String>,
    ) {
        match expr {
            hir::Expr::MutRef(reference) => {
                if let hir::Expr::Name(name) = reference.value.as_ref() {
                    if outer_names.contains(&name.value) && !local_names.contains(&name.value) {
                        self.add_error(
                            &display_name(&module.path),
                            reference.span,
                            "cannot capture mutref across spawn boundary",
                            None,
                        );
                    }
                }
                self.check_spawn_expr(module, &reference.value, outer_names, local_names);
            }
            hir::Expr::Unary(unary) => {
                self.check_spawn_expr(module, &unary.value, outer_names, local_names);
            }
            hir::Expr::Binary(binary) => {
                self.check_spawn_expr(module, &binary.left, outer_names, local_names);
                self.check_spawn_expr(module, &binary.right, outer_names, local_names);
            }
            hir::Expr::Call(call) => {
                self.check_spawn_expr(module, &call.callee, outer_names, local_names);
                for arg in &call.args {
                    self.check_spawn_expr(module, arg, outer_names, local_names);
                }
            }
            hir::Expr::Member(member) => {
                self.check_spawn_expr(module, &member.object, outer_names, local_names);
            }
            hir::Expr::Move(move_expr) => {
                self.check_spawn_expr(module, &move_expr.value, outer_names, local_names);
            }
            hir::Expr::Ref(reference) => {
                self.check_spawn_expr(module, &reference.value, outer_names, local_names);
            }
            hir::Expr::Question(question) => {
                self.check_spawn_expr(module, &question.value, outer_names, local_names);
            }
            hir::Expr::If(if_expr) => {
                self.check_spawn_expr(module, &if_expr.condition, outer_names, local_names);
                for statement in &if_expr.then_branch.statements {
                    self.check_spawn_statement(module, statement, outer_names, &mut local_names.clone());
                }
                if let Some(else_branch) = &if_expr.else_branch {
                    match else_branch {
                        hir::ElseBranch::Block(block) => {
                            for statement in &block.statements {
                                self.check_spawn_statement(module, statement, outer_names, &mut local_names.clone());
                            }
                        }
                        hir::ElseBranch::IfExpr(expr) => {
                            self.check_spawn_expr(module, &hir::Expr::If(*expr.clone()), outer_names, local_names);
                        }
                    }
                }
            }
            hir::Expr::Match(match_expr) => {
                self.check_spawn_expr(module, &match_expr.subject, outer_names, local_names);
                for arm in &match_expr.arms {
                    match &arm.body {
                        hir::ArmBody::Block(block) => {
                            for statement in &block.statements {
                                self.check_spawn_statement(module, statement, outer_names, &mut local_names.clone());
                            }
                        }
                        hir::ArmBody::Expr(expr) => {
                            self.check_spawn_expr(module, expr, outer_names, local_names);
                        }
                    }
                }
            }
            hir::Expr::When(when_expr) => {
                for arm in &when_expr.arms {
                    if let Some(condition) = &arm.condition {
                        self.check_spawn_expr(module, condition, outer_names, local_names);
                    }
                    match &arm.body {
                        hir::ArmBody::Block(block) => {
                            for statement in &block.statements {
                                self.check_spawn_statement(module, statement, outer_names, &mut local_names.clone());
                            }
                        }
                        hir::ArmBody::Expr(expr) => {
                            self.check_spawn_expr(module, expr, outer_names, local_names);
                        }
                    }
                }
            }
            hir::Expr::List(list) => {
                for item in &list.items {
                    self.check_spawn_expr(module, item, outer_names, local_names);
                }
            }
            hir::Expr::Literal(_) | hir::Expr::FString(_) | hir::Expr::Name(_) => {}
            hir::Expr::Tuple(tuple) => {
                for item in &tuple.items {
                    self.check_spawn_expr(module, item, outer_names, local_names);
                }
            }
            hir::Expr::Lambda(lambda) => {
                for statement in &lambda.body.statements {
                    self.check_spawn_statement(module, statement, outer_names, &mut local_names.clone());
                }
            }
        }
    }

    /// Walk an assignment target to find the root variable name.
    /// `x` → Some("x"), `x.field` → Some("x"), `x.a.b` → Some("x"), anything else → None.
    fn assign_target_root(expr: &hir::Expr) -> Option<(&str, Span)> {
        match expr {
            hir::Expr::Name(name) => Some((&name.value, name.span)),
            hir::Expr::Member(member) => Self::assign_target_root(&member.object),
            _ => None,
        }
    }

    fn check_expr(
        &mut self,
        module: &ModuleInfo,
        expr: &hir::Expr,
        scope: &mut HashMap<String, BindingInfo>,
        owner_name: Option<&str>,
        loop_depth: usize,
    ) {
        match expr {
            hir::Expr::Literal(_) | hir::Expr::FString(_) => {}
            hir::Expr::Name(name) => {
                if name.value == "None" {
                    return;
                }
                if let Some(binding) = scope.get_mut(&name.value) {
                    binding.used = true;
                    if binding.moved {
                        self.add_error(
                            &display_name(&module.path),
                            name.span,
                            format!("cannot use `{}` after `move`", name.value),
                            None,
                        );
                    }
                }
            }
            hir::Expr::List(list) => {
                for item in &list.items {
                    self.check_expr(module, item, scope, owner_name, loop_depth);
                }
            }
            hir::Expr::Unary(unary) => self.check_expr(module, &unary.value, scope, owner_name, loop_depth),
            hir::Expr::Binary(binary) => {
                self.check_expr(module, &binary.left, scope, owner_name, loop_depth);
                self.check_expr(module, &binary.right, scope, owner_name, loop_depth);
                if matches!(binary.op.as_str(), "+" | "-" | "*" | "/" | "%" | "<" | ">" | "<=" | ">=") {
                    let left_ty = self.infer_expr_type(module, &binary.left, scope, owner_name);
                    let right_ty = self.infer_expr_type(module, &binary.right, scope, owner_name);
                    if let (Some(lt), Some(rt)) = (&left_ty, &right_ty) {
                        if types::is_numeric_type(lt) && types::is_numeric_type(rt) && lt != rt {
                            self.add_error(
                                &display_name(&module.path),
                                binary.span,
                                format!("mismatched numeric types in `{}`: `{}` and `{}`", binary.op, lt, rt),
                                Some(format!("convert explicitly — no implicit coercion between numeric types")),
                            );
                        }
                    }
                }
            }
            hir::Expr::Member(member) => {
                self.check_expr(module, &member.object, scope, owner_name, loop_depth);
                if !matches!(member.object.as_ref(), hir::Expr::Name(_)) {
                    if let Some(root) = ownership::root_name(expr) {
                        if scope.get(root).map(|binding| binding.moved).unwrap_or(false) {
                            self.add_error(
                                &display_name(&module.path),
                                member.span,
                                format!("cannot use `{}` after `move`", root),
                                None,
                            );
                        }
                    }
                }
                // Struct field opaqueness: fields only accessible from methods of the same struct.
                // B9.3: Validate tuple field access. `.0`, `.1`, etc.
                // on a tuple must refer to an in-range element. The
                // codegen's `member_access` accepts any non-negative
                // integer and falls through to `rt_list_get`, which
                // returns None at runtime for out-of-bounds indices —
                // a clear error is better.
                if let Some(object_ty) = self.infer_expr_type(module, &member.object, scope, owner_name) {
                    if let Some(struct_decl) = self.find_struct_decl(&object_ty) {
                        let is_field = struct_decl.fields.iter().any(|f| f.name == member.name);
                        let inside_own_method = owner_name == Some(object_ty.as_str());
                        if is_field && !inside_own_method {
                            self.add_error(
                                &display_name(&module.path),
                                member.span,
                                format!("cannot access field `{}` on struct `{}` — struct fields are private", member.name, object_ty),
                                Some("use a method instead".to_string()),
                            );
                        }
                    }
                    if object_ty.starts_with('(') && object_ty.ends_with(')') {
                        if let Ok(index) = member.name.parse::<usize>() {
                            if let Some(elements) = split_tuple_types(&object_ty) {
                                let arity = elements.len();
                                if index >= arity {
                                    self.add_error(
                                        &display_name(&module.path),
                                        member.span,
                                        format!(
                                            "tuple index `{}` out of range for `{}` (arity {})",
                                            index, object_ty, arity
                                        ),
                                        Some(format!(
                                            "valid indices are 0..{}",
                                            arity.saturating_sub(1)
                                        )),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            hir::Expr::Move(move_expr) => {
                if let Some(root) = ownership::root_name(&move_expr.value) {
                    self.check_expr(module, &move_expr.value, scope, owner_name, loop_depth);
                    if let Some(binding) = scope.get_mut(root) {
                        if matches!(binding.param_convention.as_deref(), Some("ref") | Some("mutref")) {
                            let convention = binding.param_convention.clone().unwrap_or_default();
                            self.add_error(
                                &display_name(&module.path),
                                move_expr.span,
                                format!("cannot move from `{}` parameter `{}`", convention, root),
                                None,
                            );
                        }
                        binding.moved = true;
                    }
                }
            }
            hir::Expr::Ref(expr) => self.check_expr(module, &expr.value, scope, owner_name, loop_depth),
            hir::Expr::MutRef(expr) => self.check_expr(module, &expr.value, scope, owner_name, loop_depth),
            hir::Expr::Question(expr) => self.check_expr(module, &expr.value, scope, owner_name, loop_depth),
            hir::Expr::If(if_expr) => {
                self.check_expr(module, &if_expr.condition, scope, owner_name, loop_depth);
                let mut then_scope = scope.clone();
                for statement in &if_expr.then_branch.statements {
                    self.check_statement(module, statement, &mut then_scope, loop_depth, owner_name);
                }
                if let Some(else_branch) = &if_expr.else_branch {
                    match else_branch {
                        hir::ElseBranch::Block(block) => {
                            let mut else_scope = scope.clone();
                            for statement in &block.statements {
                                self.check_statement(module, statement, &mut else_scope, loop_depth, owner_name);
                            }
                            // Merge: if moved in either branch, mark as moved.
                            for (name, binding) in scope.iter_mut() {
                                let moved_in_then = then_scope.get(name).map_or(false, |b| b.moved);
                                let moved_in_else = else_scope.get(name).map_or(false, |b| b.moved);
                                if moved_in_then || moved_in_else {
                                    binding.moved = true;
                                }
                            }
                        }
                        hir::ElseBranch::IfExpr(expr) => {
                            let mut else_scope = scope.clone();
                            self.check_expr(module, &hir::Expr::If((**expr).clone()), &mut else_scope, owner_name, loop_depth);
                            // Merge moved state from both branches.
                            for (name, binding) in scope.iter_mut() {
                                let moved_in_then = then_scope.get(name).map_or(false, |b| b.moved);
                                let moved_in_else = else_scope.get(name).map_or(false, |b| b.moved);
                                if moved_in_then || moved_in_else {
                                    binding.moved = true;
                                }
                            }
                        }
                    }
                } else {
                    // No else branch: if moved in then, it may have been moved.
                    for (name, binding) in scope.iter_mut() {
                        if then_scope.get(name).map_or(false, |b| b.moved) {
                            binding.moved = true;
                        }
                    }
                }
            }
            hir::Expr::When(when_expr) => {
                let mut has_else = false;
                for arm in &when_expr.arms {
                    if let Some(condition) = &arm.condition {
                        self.check_expr(module, condition, scope, owner_name, loop_depth);
                    } else {
                        has_else = true;
                    }
                    self.check_arm_body(module, &arm.body, scope, owner_name, loop_depth);
                }
                if !has_else {
                    self.add_error(&display_name(&module.path), when_expr.span, "`when` requires an `else` arm", None);
                }
            }
            hir::Expr::Match(match_expr) => {
                self.check_expr(module, &match_expr.subject, scope, owner_name, loop_depth);
                let mut arm_scopes = Vec::new();
                // B7.4 — Capture each arm expression's inferred type
                // and its span so we can validate arm compatibility
                // per rules U1-U6 from the parity plan.
                let mut arm_types: Vec<(Option<String>, Span)> = Vec::with_capacity(match_expr.arms.len());
                for arm in &match_expr.arms {
                    let mut local = scope.clone();
                    self.check_arm_body(module, &arm.body, &mut local, owner_name, loop_depth);
                    // B7.4 / B12 — Use the divergence-aware
                    // `arm_body_type_for_unify` helper so that a
                    // block ending in `return` / `break` / `continue`
                    // contributes `!` (Never) to unification rather
                    // than its trailing-expression type or a hardcoded
                    // Unit. Both false assignments previously led to
                    // U5 failures on valid statement-position matches
                    // like the one in stage2/src/main.fuse:265.
                    let ty = self.arm_body_type_for_unify(module, &arm.body, &local, owner_name);
                    arm_types.push((ty, arm.span));
                    arm_scopes.push(local);
                }
                self.check_match_arm_compatibility(module, &arm_types);
                // Merge: if moved in any arm, mark as moved in parent.
                for (name, binding) in scope.iter_mut() {
                    if arm_scopes.iter().any(|s| s.get(name).map_or(false, |b| b.moved)) {
                        binding.moved = true;
                    }
                }
                self.check_match_exhaustiveness(module, match_expr, scope, owner_name);
            }
            hir::Expr::Call(call) => {
                self.check_expr(module, &call.callee, scope, owner_name, loop_depth);
                for arg in &call.args {
                    self.check_expr(module, arg, scope, owner_name, loop_depth);
                }
                self.check_call(module, call, scope, owner_name);
            }
            hir::Expr::Lambda(lambda) => {
                let mut lambda_scope = scope.clone();
                for param in &lambda.params {
                    lambda_scope.insert(param.name.clone(), BindingInfo {
                        mutable: false,
                        param_convention: param.convention.clone(),
                        type_name: param.type_name.clone(),
                        rank: None,
                        held_rank: None,
                        held_rank_is_write: false,
                        moved: false,
                        used: true,
                    });
                }
                for statement in &lambda.body.statements {
                    self.check_statement(module, statement, &mut lambda_scope, loop_depth, owner_name);
                }
            }
            hir::Expr::Tuple(tuple) => {
                for item in &tuple.items {
                    self.check_expr(module, item, scope, owner_name, loop_depth);
                }
            }
        }
    }

    fn check_arm_body(
        &mut self,
        module: &ModuleInfo,
        body: &hir::ArmBody,
        scope: &mut HashMap<String, BindingInfo>,
        owner_name: Option<&str>,
        loop_depth: usize,
    ) {
        match body {
            hir::ArmBody::Block(block) => {
                for statement in &block.statements {
                    self.check_statement(module, statement, scope, loop_depth, owner_name);
                }
            }
            hir::ArmBody::Expr(expr) => self.check_expr(module, expr, scope, owner_name, loop_depth),
        }
    }

    /// B7.4 — Validate that every known arm type unifies per rules
    /// U1-U6 (see docs/fuse-stage2-parity-plan.md). On Rule U5
    /// (incompatible arms), emit a diagnostic pointing at the two
    /// arms whose types could not unify, naming both types.
    ///
    /// The helper is conservative: it only flags arms where both
    /// types are *known*. Arms whose expression type inference
    /// returned `None` are skipped — they neither block nor trigger
    /// the diagnostic. This matches the codegen's behavior in
    /// `unify_match_arm_types` (the None case is Rule U6, not U5).
    ///
    /// The diagnostic is only emitted once per match expression,
    /// naming the first known arm and the first subsequent arm that
    /// fails to unify with it pairwise. Reporting every clashing
    /// pair would produce noise when three or more arms disagree;
    /// the first clash is enough for the user to locate the bug.
    fn check_match_arm_compatibility(
        &mut self,
        module: &ModuleInfo,
        arm_types: &[(Option<String>, Span)],
    ) {
        // Collect only arms whose type is known AND is not `Unit` or
        // `!`. A `Unit`-typed arm is almost always a block used for
        // side effects (`{ s = foo(...) }`, `{ println(...) }`) whose
        // value is not meant to be observed; treating it the same as
        // an unknown arm (Rule U6) is how statement-position matches
        // coexist with value-position matches in a checker that
        // cannot yet distinguish them. `!` (Never) arms similarly
        // contribute no concrete type — a Never arm diverges before
        // producing a value. Pre-B11, this filter was missing, so any
        // `match x { A => list_value, B => { s = foo() } }` used as a
        // statement (result discarded) was rejected — see
        // stage2/src/checker.fuse checkExpr's Expr.If arm for the
        // canonical case.
        let knowns: Vec<&(Option<String>, Span)> = arm_types
            .iter()
            .filter(|(ty, _)| {
                ty.as_deref()
                    .map(|t| t != "Unit" && t != "!")
                    .unwrap_or(false)
            })
            .collect();
        if knowns.len() < 2 {
            return;
        }

        // Feed every known arm type into the full unifier. If it
        // returns Some, every arm unifies under rules U1-U4 and
        // there is nothing to report.
        let types_only: Vec<Option<String>> =
            knowns.iter().map(|(ty, _)| ty.clone()).collect();
        if unify_match_arm_types(&types_only).is_some() {
            return;
        }

        // Unification failed — Rule U5 (incompatible arms). Find
        // the first pair of known-type arms that can't unify
        // pairwise, and point the diagnostic at the second arm of
        // that pair (the one that introduces the conflict). This
        // localizes the error to the "new" arm from the user's
        // perspective, which is usually where they made the
        // mistake. Only one diagnostic per match expression is
        // emitted to avoid cascades when 3+ arms disagree.
        for i in 0..knowns.len() {
            for j in (i + 1)..knowns.len() {
                let a = knowns[i].0.as_deref().unwrap();
                let b = knowns[j].0.as_deref().unwrap();
                let pair = [Some(a.to_string()), Some(b.to_string())];
                if unify_match_arm_types(&pair).is_none() {
                    let message = format!(
                        "match arms have incompatible types `{a}` and `{b}`"
                    );
                    let hint = Some(
                        "every arm of a match used as an expression must produce a unifiable type"
                            .to_string(),
                    );
                    self.add_error(
                        &display_name(&module.path),
                        knowns[j].1,
                        message,
                        hint,
                    );
                    return;
                }
            }
        }
    }

    fn check_call(
        &mut self,
        module: &ModuleInfo,
        call: &hir::Call,
        scope: &mut HashMap<String, BindingInfo>,
        owner_name: Option<&str>,
    ) {
        let mut resolved = None;
        let mut callee_name = None;
        match call.callee.as_ref() {
            hir::Expr::Name(name) => {
                callee_name = Some(name.value.clone());
                resolved = self.resolve_function(&name.value);
            }
            hir::Expr::Member(member) => {
                // Try static function first when the object looks like a type name.
                if let hir::Expr::Name(name) = member.object.as_ref() {
                    if !scope.contains_key(&name.value) {
                        resolved = self.resolve_static_function(&name.value, &member.name);
                    }
                }
                // B8.2/B8.3: Module-qualified calls —
                //   `alias.fn(args)`            (2-segment function)
                //   `alias.Type(args)`          (2-segment constructor)
                //   `alias.Type.method(args)`   (3-segment static method)
                // The codegen handles all three, but the checker must
                // validate the alias/type/method chain so typos fail
                // loudly (B8.3) instead of surfacing as obscure codegen
                // errors (or silently passing at --check time).
                if resolved.is_none() {
                    if let hir::Expr::Name(alias) = member.object.as_ref() {
                        if !scope.contains_key(&alias.value) {
                            if let Some(mod_path) =
                                self.alias_resolves_to_module(module, &alias.value)
                            {
                                if self.module_defines_type(&mod_path, &member.name) {
                                    // success: module-qualified constructor.
                                    // Nothing more to validate here — the
                                    // codegen will check argument arity
                                    // against the data class fields.
                                } else if let Some(function) =
                                    self.lookup_module_function(&mod_path, &member.name)
                                {
                                    resolved = Some(function);
                                } else {
                                    self.add_error(
                                        &display_name(&module.path),
                                        member.span,
                                        format!(
                                            "no type or function `{}` in module `{}`",
                                            member.name, alias.value
                                        ),
                                        None,
                                    );
                                }
                            }
                        }
                    } else if let hir::Expr::Member(inner) = member.object.as_ref() {
                        if let hir::Expr::Name(alias) = inner.object.as_ref() {
                            if !scope.contains_key(&alias.value) {
                                if let Some(mod_path) =
                                    self.alias_resolves_to_module(module, &alias.value)
                                {
                                    if !self.module_defines_type(&mod_path, &inner.name) {
                                        self.add_error(
                                            &display_name(&module.path),
                                            inner.span,
                                            format!(
                                                "no type `{}` in module `{}`",
                                                inner.name, alias.value
                                            ),
                                            None,
                                        );
                                    } else if let Some(function) = self.lookup_module_static(
                                        &mod_path,
                                        &inner.name,
                                        &member.name,
                                    ) {
                                        resolved = Some(function);
                                    } else {
                                        self.add_error(
                                            &display_name(&module.path),
                                            member.span,
                                            format!(
                                                "no static method `{}` on type `{}` in module `{}`",
                                                member.name, inner.name, alias.value
                                            ),
                                            None,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                if resolved.is_none() {
                    if let Some(receiver_type) = self.infer_expr_type(module, &member.object, scope, owner_name) {
                        resolved = self.resolve_extension(&receiver_type, &member.name);
                    }
                }
            }
            _ => {}
        }

        if let Some(function) = resolved {
            // For method calls, skip the implicit `self` parameter.
            let is_method_call = matches!(call.callee.as_ref(), hir::Expr::Member(_))
                && function.params.first().map_or(false, |p| p.name == "self");
            let params = if is_method_call { &function.params[1..] } else { &function.params[..] };
            for (param, arg) in params.iter().zip(call.args.iter()) {
                if param.convention.as_deref() == Some("mutref") && !matches!(arg, hir::Expr::MutRef(_)) {
                    self.add_error(
                        &display_name(&module.path),
                        arg.span(),
                        format!("`mutref` must be explicit at the call site for `{}`", param.name),
                        Some("did you mean `mutref`?".to_string()),
                    );
                }
                if param.convention.as_deref() == Some("ref") && matches!(arg, hir::Expr::MutRef(_)) {
                    self.add_error(
                        &display_name(&module.path),
                        arg.span(),
                        format!("`ref` parameter `{}` cannot receive `mutref`", param.name),
                        None,
                    );
                }
            }
            // Warn on use of @deprecated functions.
            if let Some(dep) = function.annotations.iter().find(|a| a.is("deprecated")) {
                let msg = dep.args.first().and_then(|a| match a {
                    hir::AnnotationArg::String(s) => Some(s.as_str()),
                    _ => None,
                }).unwrap_or("this function is deprecated");
                self.diagnostics.push(
                    Diagnostic::warning(
                        format!("`{}` is deprecated: {}", function.name, msg),
                        display_name(&module.path),
                        call.span,
                        None,
                    )
                );
            }
            // Check generic bounds: <T: Interface> at call site.
            self.check_generic_bounds(module, &function, call, scope, owner_name);
        } else if let Some(name) = callee_name {
            if types::builtin_return_type(&name).is_none() && self.find_data_decl(&name).is_none() {
                let _ = owner_name;
            }
        } else if let hir::Expr::Member(member) = call.callee.as_ref() {
            // B2.2: Method call where extension resolution failed.
            // Before B2 the checker silently dropped this case and the
            // codegen later either fell into a hardcoded specialization
            // (List.len, Map.set, etc.) or crashed with `unsupported X
            // member call Y`. Now we look up the canonical receiver in
            // the builtin mirror; if it's not a builtin, emit a hard
            // error with a stdlib-import hint when we can offer one.
            //
            // Static method calls (`Type.method(...)` where the
            // receiver looks like a type name) are handled by the
            // resolve_static_function branch above and never reach
            // here, so we only need to deal with instance calls.
            //
            // Optional-chain calls (`obj?.method()`) are skipped here
            // because infer_expr_type for an optional chain returns
            // bare "Option" without an inner type, so we cannot tell
            // whether the method exists on the unwrapped type. The
            // codegen handles optional chains via recursive dispatch
            // on the unwrapped value (object_backend.rs:3394-3454);
            // any genuinely missing method on an optional chain will
            // surface there. A future phase can improve infer_expr_type
            // to track optional inner types and let the checker catch
            // these too.
            //
            // We only emit the error if we can infer the receiver type.
            // An unknown receiver type means a prior error already
            // exists; doubling up on diagnostics there hurts more than
            // it helps.
            if member.optional {
                // optional chain — defer to codegen recursive dispatch
            } else if let Some(receiver_type) =
                self.infer_expr_type(module, &member.object, scope, owner_name)
            {
                let canonical = builtins::canonical_receiver(&receiver_type);
                let resolves_via_builtin =
                    builtins::is_builtin_method(canonical, &member.name);
                let resolves_via_interface =
                    self.type_has_method_via_interface(canonical, &member.name);
                if !resolves_via_builtin && !resolves_via_interface {
                    let hint = builtins::suggest_stdlib_import_for(canonical, &member.name)
                        .map(|module_path| format!("did you forget `import {module_path}`?"));
                    self.add_error(
                        &display_name(&module.path),
                        member.span,
                        format!(
                            "no method `{}` on type `{}`",
                            member.name, receiver_type
                        ),
                        hint,
                    );
                }
            }
        }
    }

    fn check_generic_bounds(
        &mut self,
        module: &ModuleInfo,
        function: &hir::FunctionDecl,
        call: &hir::Call,
        scope: &HashMap<String, BindingInfo>,
        owner_name: Option<&str>,
    ) {
        // Build a map of type_param_name → bound_interface for bounded params.
        let mut bounds: HashMap<&str, &str> = HashMap::new();
        for tp in &function.type_params {
            if let Some((name, bound)) = tp.split_once(':') {
                bounds.insert(name.trim(), bound.trim());
            }
        }
        if bounds.is_empty() {
            return;
        }
        // For method calls, skip the implicit `self` parameter.
        let is_method_call = matches!(call.callee.as_ref(), hir::Expr::Member(_))
            && function.params.first().map_or(false, |p| p.name == "self");
        let params = if is_method_call { &function.params[1..] } else { &function.params[..] };
        for (param, arg) in params.iter().zip(call.args.iter()) {
            let Some(param_type) = &param.type_name else { continue };
            // Check if param type is a bounded type variable.
            let Some(required_iface) = bounds.get(param_type.as_str()) else { continue };
            let Some(actual_type) = self.infer_expr_type(module, arg, scope, owner_name) else { continue };
            let type_ifaces = self.type_implements(&actual_type);
            if !type_ifaces.iter().any(|i| i == required_iface) {
                self.add_error(
                    &display_name(&module.path),
                    call.span,
                    format!(
                        "type `{actual_type}` does not implement interface `{required_iface}`",
                    ),
                    Some(format!("required by bound `{param_type}: {required_iface}` on `{}`", function.name)),
                );
            }
        }
    }

    /// Returns `(rank, is_write)` for a guard acquisition expression.
    fn held_rank_from_expr(
        &self,
        scope: &HashMap<String, BindingInfo>,
        expr: &hir::Expr,
    ) -> Option<(i64, bool)> {
        let hir::Expr::Call(call) = expr else {
            return None;
        };
        let hir::Expr::Member(member) = call.callee.as_ref() else {
            return None;
        };
        let is_write = match member.name.as_str() {
            "read" | "tryRead" => false,
            "write" | "try_write" | "tryWrite" => true,
            _ => return None,
        };
        let hir::Expr::Name(name) = member.object.as_ref() else {
            return None;
        };
        Some((scope.get(&name.value)?.rank?, is_write))
    }

    fn resolve_function(&self, name: &str) -> Option<hir::FunctionDecl> {
        self.module_cache.values().find_map(|module| {
            module.symbols.get(name).and_then(|symbol| match symbol {
                Symbol::Function { node, is_pub } => {
                    if module.path == self.current_file || *is_pub {
                        Some(node.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
        })
    }

    fn resolve_extension(&self, receiver_type: &str, name: &str) -> Option<hir::FunctionDecl> {
        // Use the same canonicalization as the codegen — strip ownership
        // prefixes and generic args. Without this, types like
        // `mutref Request` (which the parser produces as "mutrefRequest")
        // never resolve.
        let receiver_key = builtins::canonical_receiver(receiver_type);
        self.module_cache.values().find_map(|module| {
            module
                .extension_functions
                .get(&(receiver_key.to_string(), name.to_string()))
                .cloned()
        })
    }

    fn resolve_static_function(&self, type_name: &str, name: &str) -> Option<hir::FunctionDecl> {
        let type_key = builtins::canonical_receiver(type_name);
        self.module_cache.values().find_map(|module| {
            module
                .static_functions
                .get(&(type_key.to_string(), name.to_string()))
                .cloned()
        })
    }

    /// B8.3: Resolve an import alias (last segment of `import a.b.c`)
    /// relative to `info`'s imports. Returns the path of the target
    /// module if the alias matches one, None otherwise. The path is
    /// NOT canonicalized because the checker's `module_cache` is keyed
    /// by whatever `check_import` / `load_module` passed in (which is
    /// the non-canonicalized return of `resolve_import_path`), so
    /// canonicalizing here would miss the cache entry.
    fn alias_resolves_to_module(&self, info: &ModuleInfo, alias: &str) -> Option<PathBuf> {
        for import in &info.module.imports {
            let import_alias = import.module_path.split('.').next_back()?;
            if import_alias == alias {
                return resolve_import_path(&info.path, &import.module_path);
            }
        }
        None
    }

    /// B8.3: Is `type_name` defined (as a data/struct/enum) in the
    /// given module? Used by module-qualified call validation to tell
    /// "no such type in module" apart from "unknown method on type".
    fn module_defines_type(&self, module_path: &Path, type_name: &str) -> bool {
        let Some(info) = self.module_cache.get(module_path) else {
            return false;
        };
        matches!(
            info.symbols.get(type_name),
            Some(Symbol::Data { .. } | Symbol::Struct { .. } | Symbol::Enum { .. })
        )
    }

    /// B8.3: Look up a free function by name in a specific module.
    /// Unlike `resolve_function`, this is scoped — it does not iterate
    /// the whole module cache — so it reports exactly whether the
    /// function lives in the given module.
    fn lookup_module_function(
        &self,
        module_path: &Path,
        name: &str,
    ) -> Option<hir::FunctionDecl> {
        let info = self.module_cache.get(module_path)?;
        match info.symbols.get(name)? {
            Symbol::Function { node, is_pub } => {
                if *is_pub || info.path == self.current_file {
                    Some(node.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// B8.3: Look up a static method `(type_name, method_name)` in a
    /// specific module. Module-scoped variant of
    /// `resolve_static_function` for the 3-segment
    /// `alias.Type.method()` validator.
    fn lookup_module_static(
        &self,
        module_path: &Path,
        type_name: &str,
        method_name: &str,
    ) -> Option<hir::FunctionDecl> {
        let info = self.module_cache.get(module_path)?;
        let type_key = builtins::canonical_receiver(type_name);
        info.static_functions
            .get(&(type_key.to_string(), method_name.to_string()))
            .cloned()
    }

    fn find_data_decl(&self, type_name: &str) -> Option<hir::DataClassDecl> {
        if matches!(type_name, "Result" | "Option") {
            return None;
        }
        let base = type_name.split('<').next().unwrap_or(type_name);
        self.module_cache.values().find_map(|module| {
            module.symbols.get(base).and_then(|symbol| match symbol {
                Symbol::Data { node, .. } => Some(node.clone()),
                _ => None,
            })
        })
    }

    fn find_struct_decl(&self, type_name: &str) -> Option<hir::StructDecl> {
        let base = type_name.split('<').next().unwrap_or(type_name);
        self.module_cache.values().find_map(|module| {
            module.symbols.get(base).and_then(|symbol| match symbol {
                Symbol::Struct { node, .. } => Some(node.clone()),
                _ => None,
            })
        })
    }

    fn find_enum_decl(&self, type_name: &str) -> Option<hir::EnumDecl> {
        let base = type_name.split('<').next().unwrap_or(type_name);
        self.module_cache.values().find_map(|module| {
            module.symbols.get(base).and_then(|symbol| match symbol {
                Symbol::Enum { node, .. } => Some(node.clone()),
                _ => None,
            })
        })
    }

    fn resolve_interface(&self, name: &str) -> Option<InterfaceInfo> {
        // Strip generic args: "Convertible<String>" → "Convertible"
        let base = name.split('<').next().unwrap_or(name);
        self.module_cache.values().find_map(|module| {
            module.interfaces.get(name).or_else(|| module.interfaces.get(base)).cloned()
        })
    }

    /// Map well-known stdlib interface names to their module paths.
    fn stdlib_interface_module(name: &str) -> Option<&'static str> {
        let base = name.split('<').next().unwrap_or(name);
        match base {
            "Equatable" => Some("core.equatable"),
            "Hashable" => Some("core.hashable"),
            "Comparable" => Some("core.comparable"),
            "Printable" => Some("core.printable"),
            "Debuggable" => Some("core.debuggable"),
            _ => None,
        }
    }

    /// Auto-load a stdlib interface module if the interface is not already in
    /// the module cache. Called during conformance checking so that user code
    /// does not need an explicit `import core.equatable`.
    fn ensure_stdlib_interface_loaded(&mut self, iface_name: &str, referrer: &Path) {
        if self.resolve_interface(iface_name).is_some() {
            return;
        }
        if let Some(module_path) = Self::stdlib_interface_module(iface_name) {
            if let Some(target) = resolve_import_path(referrer, module_path) {
                let _ = self.load_module(&target);
            }
        }
    }

    /// Collect all required methods for an interface, including those inherited
    /// from parent interfaces (transitively). Returns None if any parent cannot
    /// be resolved.
    fn collect_interface_methods(&self, iface: &InterfaceInfo) -> Option<Vec<hir::InterfaceMethod>> {
        let mut methods = iface.methods.clone();
        for parent_name in &iface.parents {
            let parent = self.resolve_interface(parent_name)?;
            methods.extend(self.collect_interface_methods(&parent)?);
        }
        Some(methods)
    }

    /// Collect all default method implementations from an interface and its
    /// parents (transitively).
    fn collect_interface_defaults(&self, iface: &InterfaceInfo) -> Vec<hir::FunctionDecl> {
        let mut defaults = iface.default_methods.clone();
        for parent_name in &iface.parents {
            if let Some(parent) = self.resolve_interface(parent_name) {
                defaults.extend(self.collect_interface_defaults(&parent));
            }
        }
        defaults
    }

    /// True if `type_name` provides `method_name` via any interface it
    /// declares `implements` — counting abstract methods, default
    /// methods, and codegen autogen targets. Used by `check_call` to
    /// avoid false-positive "no method" errors on types that satisfy
    /// the method through interface conformance.
    ///
    /// Mirrors the codegen's three-way method discovery in
    /// `compile_member_call` (extension resolution + interface defaults
    /// from `defaults` map + autogen-from-fields). When this function
    /// returns true, the codegen will find a callable for the method.
    ///
    /// See `docs/fuse-stage2-parity-plan.md` Phase B2.2.
    fn type_has_method_via_interface(&self, type_name: &str, method_name: &str) -> bool {
        let ifaces = self.type_implements(type_name);
        if ifaces.is_empty() {
            return false;
        }

        let type_kind = if let Some(data) = self.find_data_decl(type_name) {
            Some(autogen::classify_type(&data.annotations, false))
        } else if let Some(s) = self.find_struct_decl(type_name) {
            Some(autogen::classify_type(&s.annotations, true))
        } else if self.find_enum_decl(type_name).is_some() {
            Some(autogen::TypeKind::Enum)
        } else {
            None
        };

        for iface_name in &ifaces {
            // 1. Abstract methods on the interface (transitive parents).
            if let Some(iface) = self.resolve_interface(iface_name) {
                if let Some(methods) = self.collect_interface_methods(&iface) {
                    if methods.iter().any(|m| m.name == method_name) {
                        return true;
                    }
                }
                // 2. Default methods on the interface (transitive).
                if self
                    .collect_interface_defaults(&iface)
                    .iter()
                    .any(|f| f.name == method_name)
                {
                    return true;
                }
            }

            // 3. Codegen autogen targets for built-in stdlib interfaces.
            // These methods exist even when the interface module is not
            // loaded into the checker, because the codegen synthesizes
            // them from field metadata. Mirror of the autogen dispatch
            // in load_module_recursive (object_backend.rs:444-450).
            if let Some(kind) = type_kind {
                if autogen::can_auto_generate(kind, iface_name) {
                    let target = match iface_name.as_str() {
                        "Equatable" => "eq",
                        "Hashable" => "hash",
                        "Comparable" => "compareTo",
                        "Printable" => "toString",
                        "Debuggable" => "debugString",
                        _ => "",
                    };
                    if !target.is_empty() && method_name == target {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Look up which interfaces a type declares `implements`.
    fn type_implements(&self, type_name: &str) -> Vec<String> {
        let mut ifaces = self.module_cache
            .values()
            .find_map(|module| module.implements.get(type_name).cloned())
            .unwrap_or_default();
        // Built-in types implicitly implement core interfaces.
        let base = type_name.split('<').next().unwrap_or(type_name);
        match base {
            "Int" | "Float" | "Float32" | "Bool" | "String" | "Unit"
            | "Int8" | "UInt8" | "Int32" | "UInt32" | "UInt64" => {
                for iface in ["Printable", "Debuggable", "Equatable"] {
                    if !ifaces.iter().any(|i| i == iface) {
                        ifaces.push(iface.to_string());
                    }
                }
            }
            _ => {}
        }
        ifaces
    }

    fn check_match_exhaustiveness(
        &mut self,
        module: &ModuleInfo,
        match_expr: &hir::MatchExpr,
        scope: &HashMap<String, BindingInfo>,
        owner_name: Option<&str>,
    ) {
        let Some(ty) = self.infer_expr_type(module, &match_expr.subject, scope, owner_name) else {
            return;
        };
        let mut covered = HashSet::new();
        let mut wildcard = false;
        for arm in &match_expr.arms {
            match &arm.pattern {
                hir::Pattern::Wildcard(_) => wildcard = true,
                hir::Pattern::Variant(pattern) => {
                    covered.insert(pattern.name.rsplit('.').next().unwrap_or(&pattern.name).to_string());
                }
                hir::Pattern::Literal(pattern) => {
                    covered.insert(self.literal_repr(&pattern.value));
                }
                hir::Pattern::Name(_) => {}
                hir::Pattern::Tuple(_) => { wildcard = true; }
            }
        }
        if wildcard {
            return;
        }
        if ty.starts_with("Result") && !(covered.contains("Ok") && covered.contains("Err")) {
            self.add_error(&display_name(&module.path), match_expr.span, "non-exhaustive match for `Result`", None);
            return;
        }
        if ty.starts_with("Option") && !(covered.contains("Some") && covered.contains("None")) {
            self.add_error(&display_name(&module.path), match_expr.span, "non-exhaustive match for `Option`", None);
            return;
        }
        if ty == "Bool" && !(covered.contains("True") && covered.contains("False")) {
            self.add_error(&display_name(&module.path), match_expr.span, "non-exhaustive match for `Bool`", None);
            return;
        }
        if let Some(enum_decl) = self.find_enum_decl(&ty) {
            let required: HashSet<String> = enum_decl.variants.iter().map(|variant| variant.name.clone()).collect();
            if !required.is_subset(&covered) {
                self.add_error(
                    &display_name(&module.path),
                    match_expr.span,
                    format!("non-exhaustive match for `{}`", enum_decl.name),
                    None,
                );
            }
        }
    }

    fn infer_expr_type(
        &self,
        module: &ModuleInfo,
        expr: &hir::Expr,
        scope: &HashMap<String, BindingInfo>,
        owner_name: Option<&str>,
    ) -> Option<String> {
        match expr {
            hir::Expr::Literal(literal) => match &literal.value {
                hir::LiteralValue::Int(_) => Some("Int".to_string()),
                hir::LiteralValue::Float(_) => Some("Float".to_string()),
                hir::LiteralValue::String(_) => Some("String".to_string()),
                hir::LiteralValue::Bool(_) => Some("Bool".to_string()),
            },
            hir::Expr::FString(_) => Some("String".to_string()),
            hir::Expr::Name(name) => {
                if name.value == "None" {
                    return Some("Option".to_string());
                }
                if let Some(binding) = scope.get(&name.value) {
                    return binding.type_name.clone();
                }
                if matches!(name.value.as_str(), "Some" | "Ok" | "Err") {
                    return None;
                }
                if self.find_data_decl(&name.value).is_some() || self.find_struct_decl(&name.value).is_some() {
                    return Some(name.value.clone());
                }
                types::builtin_return_type(&name.value).map(str::to_string)
            }
            hir::Expr::List(_) => Some("List".to_string()),
            hir::Expr::Member(member) => {
                let object_ty = self.infer_expr_type(module, &member.object, scope, owner_name)?;
                if member.optional {
                    return Some("Option".to_string());
                }
                if let Some(data) = self.find_data_decl(&object_ty) {
                    if let Some(field) = data.fields.iter().find(|field| field.name == member.name) {
                        return field.type_name.clone();
                    }
                }
                if object_ty == "String" && member.name == "isEmpty" {
                    return Some("Bool".to_string());
                }
                // B9.2 — Tuple field access `.0`, `.1`, ... returns
                // the element type at the given index when known.
                // Without this, downstream inference (e.g. a struct
                // field whose type depends on a tuple field) loses
                // the type and falls back to `None`.
                if object_ty.starts_with('(') && object_ty.ends_with(')') {
                    if let Ok(index) = member.name.parse::<usize>() {
                        if let Some(elements) = split_tuple_types(&object_ty) {
                            return elements.get(index).cloned();
                        }
                    }
                }
                None
            }
            hir::Expr::Binary(binary) => {
                if matches!(binary.op.as_str(), "==" | "!=" | "<" | ">" | "<=" | ">=" | "and" | "or") {
                    return Some("Bool".to_string());
                }
                if binary.op == "?:" {
                    let left_ty = self.infer_expr_type(module, &binary.left, scope, owner_name)?;
                    if left_ty == "Option" {
                        return self.infer_expr_type(module, &binary.right, scope, owner_name);
                    }
                    return Some(left_ty);
                }
                self.infer_expr_type(module, &binary.left, scope, owner_name)
            }
            hir::Expr::Unary(unary) => self.infer_expr_type(module, &unary.value, scope, owner_name),
            hir::Expr::If(if_expr) => self.infer_block_type(module, &if_expr.then_branch, scope, owner_name),
            hir::Expr::Question(question) => {
                let inner = self.infer_expr_type(module, &question.value, scope, owner_name)?;
                // B9.2 — paren-aware splitter. The old path called
                // `.split(',').next()` on the substring inside the
                // outermost `<...>`, which corrupted the Ok type when
                // it was itself a tuple (`Result<(Int,String),String>?`
                // would produce `(Int` instead of `(Int,String)`).
                if inner.starts_with("Result<") && inner.ends_with('>') {
                    if let Some(args) = split_generic_args(&inner) {
                        return args.into_iter().next();
                    }
                }
                if inner.starts_with("Option<") && inner.ends_with('>') {
                    if let Some(args) = split_generic_args(&inner) {
                        return args.into_iter().next();
                    }
                }
                None
            }
            hir::Expr::Call(call) => match call.callee.as_ref() {
                hir::Expr::Name(name) => match name.value.as_str() {
                    "Some" => self.infer_expr_type(module, call.args.first()?, scope, owner_name).map(|inner| format!("Option<{inner}>")),
                    "Ok" => self.infer_expr_type(module, call.args.first()?, scope, owner_name).map(|inner| format!("Result<{inner}, Any>")),
                    "Err" => self.infer_expr_type(module, call.args.first()?, scope, owner_name).map(|inner| format!("Result<Any, {inner}>")),
                    other => {
                        if self.find_data_decl(other).is_some() || self.find_struct_decl(other).is_some() {
                            return Some(other.to_string());
                        }
                        self.resolve_function(other).and_then(|function| function.return_type.clone())
                    }
                },
                hir::Expr::Member(member) => {
                    // B8.2: Module-qualified constructor / function /
                    // static method calls. Handled before the generic
                    // receiver-type inference because `alias` is not a
                    // value and infer_expr_type(Name(alias)) would
                    // short-circuit to None, losing the call's type.
                    if let hir::Expr::Name(alias) = member.object.as_ref() {
                        if !scope.contains_key(&alias.value) {
                            if let Some(mod_path) =
                                self.alias_resolves_to_module(module, &alias.value)
                            {
                                if self.module_defines_type(&mod_path, &member.name) {
                                    return Some(member.name.clone());
                                }
                                if let Some(function) =
                                    self.lookup_module_function(&mod_path, &member.name)
                                {
                                    return function.return_type.clone();
                                }
                                return None;
                            }
                        }
                    } else if let hir::Expr::Member(inner) = member.object.as_ref() {
                        if let hir::Expr::Name(alias) = inner.object.as_ref() {
                            if !scope.contains_key(&alias.value) {
                                if let Some(mod_path) =
                                    self.alias_resolves_to_module(module, &alias.value)
                                {
                                    if let Some(function) = self.lookup_module_static(
                                        &mod_path,
                                        &inner.name,
                                        &member.name,
                                    ) {
                                        return function.return_type.clone();
                                    }
                                    return None;
                                }
                            }
                        }
                    }
                    let receiver_type = self.infer_expr_type(module, &member.object, scope, owner_name)?;
                    if receiver_type == "String" && member.name == "toUpper" {
                        return Some("String".to_string());
                    }
                    if receiver_type == "String" && member.name == "isEmpty" {
                        return Some("Bool".to_string());
                    }
                    self.resolve_extension(&receiver_type, &member.name)
                        .and_then(|function| {
                            let ret = function.return_type.clone()?;
                            Some(self.substitute_type_vars(&ret, &receiver_type, &function, call, module, scope, owner_name))
                        })
                }
                _ => None,
            },
            // B7.5 — Match expression type = unification of every
            // arm's output type (rules U1-U6). Block arms contribute
            // their trailing expression's type via `infer_block_type`;
            // expression arms contribute their inferred type. Nested
            // Match/When arms are bounded to depth 16 in the codegen
            // mirror; the checker's recursion is naturally bounded
            // by the AST depth and the `infer_expr_type` chain, so
            // an explicit depth limit is unnecessary here.
            //
            // B12 — A block arm that ends in `return`, `break`, or
            // `continue` diverges before producing a value; it
            // contributes `!` (Never) to unification. Without this,
            // `val x = match y { Ok(v) => v, Err(_) => { return } }`
            // produced arm types `[T, Unit]`, U5-failed the unifier,
            // and left `x` typeless — `x.len()` downstream surfaced
            // as `cannot infer receiver type`. The codegen mirror
            // lives in `compile_match`'s block-arm handler, which
            // pushes `!` whenever `current_block_is_terminated`
            // reports true after compiling the block's statements.
            hir::Expr::Match(match_expr) => {
                let arm_types: Vec<Option<String>> = match_expr
                    .arms
                    .iter()
                    .map(|arm| self.arm_body_type_for_unify(module, &arm.body, scope, owner_name))
                    .collect();
                unify_match_arm_types(&arm_types)
            }
            hir::Expr::Move(move_expr) => self.infer_expr_type(module, &move_expr.value, scope, owner_name),
            hir::Expr::Ref(expr) => self.infer_expr_type(module, &expr.value, scope, owner_name),
            hir::Expr::MutRef(expr) => self.infer_expr_type(module, &expr.value, scope, owner_name),
            // B7.5 — Same unification treatment for `when` (which
            // is isomorphic to `match` for typing purposes).
            hir::Expr::When(when_expr) => {
                let arm_types: Vec<Option<String>> = when_expr
                    .arms
                    .iter()
                    .map(|arm| self.arm_body_type_for_unify(module, &arm.body, scope, owner_name))
                    .collect();
                unify_match_arm_types(&arm_types)
            }
            hir::Expr::Tuple(tuple) => {
                let types: Vec<String> = tuple.items.iter().filter_map(|item| self.infer_expr_type(module, item, scope, owner_name)).collect();
                Some(format!("({})", types.join(",")))
            }
            hir::Expr::Lambda(lambda) => {
                let param_types: Vec<String> = lambda.params.iter()
                    .map(|p| p.type_name.clone().unwrap_or_else(|| "Any".to_string()))
                    .collect();
                let ret = lambda.return_type.clone().unwrap_or_else(|| "Any".to_string());
                Some(format!("fn({}) -> {}", param_types.join(", "), ret))
            }
        }
    }

    fn substitute_type_vars(
        &self,
        return_type: &str,
        receiver_type: &str,
        function: &hir::FunctionDecl,
        call: &hir::Call,
        module: &ModuleInfo,
        scope: &HashMap<String, BindingInfo>,
        owner_name: Option<&str>,
    ) -> String {
        let mut result = return_type.to_string();

        // Step 1: Substitute type vars from receiver generics.
        // If receiver is Option<Int> and function is on Option<T>, then T=Int.
        // B9.2 — use the paren-aware `split_generic_args` so that a
        // receiver like `List<(Int,String)>` binds `T` to
        // `(Int,String)` instead of `(Int`.
        if function.receiver_type.is_some() {
            let receiver_args = split_generic_args(receiver_type);
            let decl_receiver_type = function.receiver_type.as_ref().unwrap();
            let decl_args = split_generic_args(decl_receiver_type);
            if let (Some(receiver_args), Some(decl_args)) = (receiver_args, decl_args) {
                for (decl_arg, actual_arg) in decl_args.iter().zip(receiver_args.iter()) {
                    if decl_arg.len() <= 2 && decl_arg.chars().all(|c| c.is_uppercase()) {
                        result = result.replace(decl_arg.as_str(), actual_arg.as_str());
                    }
                }
            }
        }

        // Step 2: Substitute type vars from callback return types.
        // If a param has type fn(X) -> U and the arg is a lambda with -> String, then U=String.
        for (param, arg) in function.params.iter().skip(1).zip(call.args.iter()) {
            if let Some(ref param_ty) = param.type_name {
                if param_ty.starts_with("fn(") {
                    if let Some(arrow_pos) = param_ty.rfind("->") {
                        let param_ret = param_ty[arrow_pos + 2..].trim();
                        if param_ret.len() <= 2 && param_ret.chars().all(|c| c.is_uppercase()) {
                            if let Some(actual_ret) = self.infer_expr_type(module, arg, scope, owner_name) {
                                result = result.replace(param_ret, &actual_ret);
                            }
                        }
                    }
                }
            }
        }

        result
    }

    fn literal_repr(&self, value: &hir::LiteralValue) -> String {
        match value {
            hir::LiteralValue::Bool(true) => "True".to_string(),
            hir::LiteralValue::Bool(false) => "False".to_string(),
            hir::LiteralValue::Int(value) => value.to_string(),
            hir::LiteralValue::Float(value) => value.to_string(),
            hir::LiteralValue::String(value) => value.clone(),
        }
    }

    fn add_error(
        &mut self,
        filename: &str,
        span: Span,
        message: impl Into<String>,
        hint: Option<String>,
    ) {
        let message = message.into();
        let mut diagnostic = Diagnostic::error(&message, filename, span, None);
        if let Some(hint) = hint {
            diagnostic = diagnostic.with_hint(&hint);
        }
        self.diagnostics.push(diagnostic);
    }
}

pub fn check_file(path: &Path) -> Vec<Diagnostic> {
    check_file_with_options(path, false, Target::Native)
}

pub fn check_file_with_options(path: &Path, warn_unused: bool, target: Target) -> Vec<Diagnostic> {
    let mut checker = Checker::new();
    checker.warn_unused = warn_unused;
    checker.target = target;
    checker.check_path(path)
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}


