mod ownership;
mod types;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::nodes as hir;
use crate::common::resolve_import_path;
use crate::error::{Diagnostic, Span};
use crate::hir::{lower_program, Module};
use crate::parser::parse_source;

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
struct ModuleInfo {
    path: PathBuf,
    module: Module,
    symbols: HashMap<String, Symbol>,
    extension_functions: HashMap<(String, String), hir::FunctionDecl>,
    static_functions: HashMap<(String, String), hir::FunctionDecl>,
}

pub struct Checker {
    module_cache: HashMap<PathBuf, ModuleInfo>,
    diagnostics: Vec<Diagnostic>,
    current_file: PathBuf,
    warn_unused: bool,
}

impl Checker {
    pub fn new() -> Self {
        Self {
            module_cache: HashMap::new(),
            diagnostics: Vec::new(),
            current_file: PathBuf::from("<unknown>"),
            warn_unused: false,
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
        for data in &info.module.data_classes {
            info.symbols.insert(
                data.name.clone(),
                Symbol::Data {
                    node: data.clone(),
                    is_pub: data.is_pub,
                },
            );
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
        }
        for extern_fn in &info.module.extern_fns {
            let synthetic = hir::FunctionDecl {
                name: extern_fn.name.clone(),
                params: extern_fn.params.clone(),
                return_type: extern_fn.return_type.clone(),
                body: hir::Block {
                    statements: Vec::new(),
                    span: extern_fn.span,
                },
                is_pub: extern_fn.is_pub,
                decorators: Vec::new(),
                is_async: false,
                is_suspend: false,
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

        for function in module.module.functions.clone() {
            self.check_function(module, &function, None);
        }
        for data in module.module.data_classes.clone() {
            for method in data.methods.clone() {
                self.check_function(module, &method, Some(&data));
            }
        }
    }

    fn check_import(&mut self, module: &ModuleInfo, import: &hir::ImportDecl) {
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
            _ => Some("Unit".to_string()),
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
                let ty = var_decl
                    .type_name
                    .clone()
                    .or_else(|| self.infer_expr_type(module, &var_decl.value, scope, owner_name));
                self.check_expr(module, &var_decl.value, scope, owner_name, loop_depth);
                let held = self.held_rank_from_expr(scope, &var_decl.value);
                let held_rank = held.map(|(r, _)| r);
                let held_rank_is_write = held.map_or(false, |(_, w)| w);
                if let Some(type_name) = ty.as_deref() {
                    if type_name.starts_with("Shared") && var_decl.rank.is_none() {
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
                        rank: var_decl.rank,
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
            }
            hir::Statement::Loop(loop_stmt) => {
                let mut child = scope.clone();
                for inner in &loop_stmt.body.statements {
                    self.check_statement(module, inner, &mut child, loop_depth + 1, owner_name);
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
            hir::Expr::Await(await_expr) => {
                self.check_spawn_expr(module, &await_expr.value, outer_names, local_names);
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
            hir::Expr::Await(expr) => {
                self.check_expr(module, &expr.value, scope, owner_name, loop_depth);
                if scope.values().any(|binding| binding.held_rank.is_some() && binding.held_rank_is_write) {
                    self.diagnostics.push(Diagnostic::warning(
                        "write guard held across await",
                        display_name(&module.path),
                        expr.span,
                        None,
                    ));
                }
            }
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
                        }
                        hir::ElseBranch::IfExpr(expr) => {
                            self.check_expr(module, &hir::Expr::If((**expr).clone()), scope, owner_name, loop_depth)
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
                for arm in &match_expr.arms {
                    let mut local = scope.clone();
                    self.check_arm_body(module, &arm.body, &mut local, owner_name, loop_depth);
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
                if resolved.is_none() {
                    if let Some(receiver_type) = self.infer_expr_type(module, &member.object, scope, owner_name) {
                        resolved = self.resolve_extension(&receiver_type, &member.name);
                    }
                }
            }
            _ => {}
        }

        if let Some(function) = resolved {
            for (param, arg) in function.params.iter().zip(call.args.iter()) {
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
        } else if let Some(name) = callee_name {
            if types::builtin_return_type(&name).is_none() && self.find_data_decl(&name).is_none() {
                let _ = owner_name;
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
        let receiver_key = receiver_type.split('<').next().unwrap_or(receiver_type);
        self.module_cache.values().find_map(|module| {
            module
                .extension_functions
                .get(&(receiver_key.to_string(), name.to_string()))
                .cloned()
        })
    }

    fn resolve_static_function(&self, type_name: &str, name: &str) -> Option<hir::FunctionDecl> {
        let type_key = type_name.split('<').next().unwrap_or(type_name);
        self.module_cache.values().find_map(|module| {
            module
                .static_functions
                .get(&(type_key.to_string(), name.to_string()))
                .cloned()
        })
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

    fn find_enum_decl(&self, type_name: &str) -> Option<hir::EnumDecl> {
        let base = type_name.split('<').next().unwrap_or(type_name);
        self.module_cache.values().find_map(|module| {
            module.symbols.get(base).and_then(|symbol| match symbol {
                Symbol::Enum { node, .. } => Some(node.clone()),
                _ => None,
            })
        })
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
                if self.find_data_decl(&name.value).is_some() {
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
                if inner.starts_with("Result<") && inner.ends_with('>') {
                    return inner[7..inner.len() - 1].split(',').next().map(|part| part.trim().to_string());
                }
                if inner.starts_with("Option<") && inner.ends_with('>') {
                    return Some(inner[7..inner.len() - 1].trim().to_string());
                }
                None
            }
            hir::Expr::Call(call) => match call.callee.as_ref() {
                hir::Expr::Name(name) => match name.value.as_str() {
                    "Some" => self.infer_expr_type(module, call.args.first()?, scope, owner_name).map(|inner| format!("Option<{inner}>")),
                    "Ok" => self.infer_expr_type(module, call.args.first()?, scope, owner_name).map(|inner| format!("Result<{inner}, Any>")),
                    "Err" => self.infer_expr_type(module, call.args.first()?, scope, owner_name).map(|inner| format!("Result<Any, {inner}>")),
                    other => {
                        if self.find_data_decl(other).is_some() {
                            return Some(other.to_string());
                        }
                        self.resolve_function(other).and_then(|function| function.return_type.clone())
                    }
                },
                hir::Expr::Member(member) => {
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
            hir::Expr::Match(match_expr) => match_expr.arms.first().and_then(|arm| match &arm.body {
                hir::ArmBody::Expr(expr) => self.infer_expr_type(module, expr, scope, owner_name),
                hir::ArmBody::Block(block) => self.infer_block_type(module, block, scope, owner_name),
            }),
            hir::Expr::Move(move_expr) => self.infer_expr_type(module, &move_expr.value, scope, owner_name),
            hir::Expr::Ref(expr) => self.infer_expr_type(module, &expr.value, scope, owner_name),
            hir::Expr::MutRef(expr) => self.infer_expr_type(module, &expr.value, scope, owner_name),
            hir::Expr::Await(expr) => self.infer_expr_type(module, &expr.value, scope, owner_name),
            hir::Expr::When(_) => None,
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
        if let (Some(receiver_start), Some(decl_receiver)) = (receiver_type.find('<'), function.receiver_type.as_ref().and_then(|r| r.find('<'))) {
            let receiver_args_str = &receiver_type[receiver_start + 1..receiver_type.len().saturating_sub(1)];
            let decl_receiver_type = function.receiver_type.as_ref().unwrap();
            let decl_args_str = &decl_receiver_type[decl_receiver + 1..decl_receiver_type.len().saturating_sub(1)];
            let receiver_args: Vec<&str> = receiver_args_str.split(',').map(|s| s.trim()).collect();
            let decl_args: Vec<&str> = decl_args_str.split(',').map(|s| s.trim()).collect();
            for (decl_arg, actual_arg) in decl_args.iter().zip(receiver_args.iter()) {
                if decl_arg.len() <= 2 && decl_arg.chars().all(|c| c.is_uppercase()) {
                    result = result.replace(decl_arg, actual_arg);
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
    check_file_with_options(path, false)
}

pub fn check_file_with_options(path: &Path, warn_unused: bool) -> Vec<Diagnostic> {
    let mut checker = Checker::new();
    checker.warn_unused = warn_unused;
    checker.check_path(path)
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}


