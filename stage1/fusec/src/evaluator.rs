use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::ast::nodes as fa;
use crate::codegen;
use crate::common::resolve_import_path;
use crate::parser::parse_source;

thread_local! {
    static EMBEDDED_PROGRAM: RefCell<Option<(PathBuf, String)>> = const { RefCell::new(None) };
}

#[derive(Clone)]
struct Binding {
    value: Value,
    mutable: bool,
    moved: bool,
    destroy: bool,
}

#[derive(Clone)]
struct Environment(Rc<RefCell<EnvFrame>>);

struct EnvFrame {
    parent: Option<Environment>,
    values: HashMap<String, Rc<RefCell<Binding>>>,
}

impl Environment {
    fn new(parent: Option<Environment>) -> Self {
        Self(Rc::new(RefCell::new(EnvFrame {
            parent,
            values: HashMap::new(),
        })))
    }

    fn define(&self, name: impl Into<String>, value: Value, mutable: bool, destroy: bool) {
        self.0.borrow_mut().values.insert(
            name.into(),
            Rc::new(RefCell::new(Binding {
                value,
                mutable,
                moved: false,
                destroy,
            })),
        );
    }

    fn resolve(&self, name: &str) -> Option<Rc<RefCell<Binding>>> {
        if let Some(binding) = self.0.borrow().values.get(name) {
            return Some(binding.clone());
        }
        self.0
            .borrow()
            .parent
            .as_ref()
            .and_then(|parent| parent.resolve(name))
    }

    fn get(&self, name: &str) -> Result<Value, RuntimeError> {
        let binding = self
            .resolve(name)
            .ok_or_else(|| RuntimeError::plain(format!("unknown name `{name}`")))?;
        let binding = binding.borrow();
        if binding.moved {
            return Err(RuntimeError::plain(format!("cannot use `{name}` after `move`")));
        }
        Ok(binding.value.clone())
    }

    fn set(&self, name: &str, value: Value) -> Result<(), RuntimeError> {
        let binding = self
            .resolve(name)
            .ok_or_else(|| RuntimeError::plain(format!("unknown name `{name}`")))?;
        let mut binding = binding.borrow_mut();
        if !binding.mutable {
            return Err(RuntimeError::plain(format!(
                "cannot assign to immutable binding `{name}`"
            )));
        }
        binding.value = value;
        Ok(())
    }

    fn mark_moved(&self, name: &str) -> Result<(), RuntimeError> {
        let binding = self
            .resolve(name)
            .ok_or_else(|| RuntimeError::plain(format!("unknown name `{name}`")))?;
        binding.borrow_mut().moved = true;
        Ok(())
    }

    fn local_bindings(&self) -> Vec<(String, Rc<RefCell<Binding>>)> {
        self.0
            .borrow()
            .values
            .iter()
            .map(|(name, binding)| (name.clone(), binding.clone()))
            .collect()
    }
}

#[derive(Clone)]
enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Value>),
    Data(Rc<RefCell<DataInstance>>),
    Option(Option<Box<Value>>),
    Result { is_ok: bool, value: Box<Value> },
    NativeFunction(Rc<NativeFunction>),
    UserFunction(UserFunction),
    BoundMethod(Rc<BoundMethod>),
    ModuleValue(Rc<ModuleValue>),
    Moved(Box<Value>),
    Unit,
}

#[derive(Clone)]
enum NativeFunction {
    Println,
    Some,
    Ok,
    Err,
    DataConstructor { module_path: PathBuf, name: String },
    StringToUpper(Box<Value>),
    StringIsEmpty(Box<Value>),
}

#[derive(Clone)]
struct UserFunction {
    module_path: PathBuf,
    decl: fa::FunctionDecl,
}

#[derive(Clone)]
struct BoundMethod {
    receiver: Value,
    function: UserFunction,
}

#[derive(Clone)]
struct ModuleValue {
    exports: HashMap<String, Value>,
}

#[derive(Clone)]
struct DataDef {
    decl: fa::DataClassDecl,
    methods: HashMap<String, fa::FunctionDecl>,
}

#[derive(Clone)]
struct DataInstance {
    module_path: PathBuf,
    type_name: String,
    fields: HashMap<String, Value>,
    field_order: Vec<String>,
    methods: HashMap<String, fa::FunctionDecl>,
    destroyed: bool,
}

#[derive(Clone)]
enum ExportSymbol {
    Function(fa::FunctionDecl),
    Data(fa::DataClassDecl),
}

#[derive(Clone)]
struct ModuleRuntime {
    path: PathBuf,
    functions: HashMap<String, fa::FunctionDecl>,
    extensions: HashMap<(String, String), fa::FunctionDecl>,
    data_defs: HashMap<String, DataDef>,
    exports: HashMap<String, ExportSymbol>,
    imports: Vec<fa::ImportDecl>,
}

#[derive(Clone)]
struct DeferredExpr {
    expr: fa::Expr,
    env: Environment,
}

#[derive(Debug)]
struct RuntimeError {
    message: String,
    filename: String,
    line: usize,
    column: usize,
}

impl RuntimeError {
    fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            filename: "<runtime>".to_string(),
            line: 1,
            column: 1,
        }
    }

    fn render(&self) -> String {
        format!(
            "error: {}\n  --> {}:{}:{}",
            self.message, self.filename, self.line, self.column
        )
    }
}

enum ControlFlow {
    Return(Value),
    Break,
    Continue,
    Abort(RuntimeError),
}

impl From<RuntimeError> for ControlFlow {
    fn from(error: RuntimeError) -> Self {
        Self::Abort(error)
    }
}

impl From<ControlFlow> for RuntimeError {
    fn from(flow: ControlFlow) -> Self {
        control_to_runtime(flow)
    }
}

pub fn run_embedded_source(source: &str, source_path: &str) -> i32 {
    let path = PathBuf::from(source_path);
    EMBEDDED_PROGRAM.with(|slot| {
        *slot.borrow_mut() = Some((path.canonicalize().unwrap_or(path), source.to_string()));
    });
    let result = codegen::cranelift::run_host_entry(run_current_embedded);
    EMBEDDED_PROGRAM.with(|slot| {
        slot.borrow_mut().take();
    });
    result.unwrap_or_else(|error| {
        eprint!("error: {error}");
        1
    })
}

extern "C" fn run_current_embedded() -> i32 {
    EMBEDDED_PROGRAM.with(|slot| {
        let borrowed = slot.borrow();
        let Some((path, source)) = borrowed.as_ref() else {
            eprint!("error: missing embedded program context");
            return 1;
        };
        match Evaluator::new(path.clone(), source.clone()).eval_root() {
            Ok(output) => {
                if !output.is_empty() {
                    print!("{output}");
                }
                0
            }
            Err(error) => {
                eprint!("{}", error.render());
                1
            }
        }
    })
}

pub fn run_repl() -> i32 {
    use std::io::{self, BufRead, Write};

    let repl_path = PathBuf::from("<repl>");
    let mut evaluator = Evaluator::new(repl_path.clone(), String::new());
    let env = evaluator.base_env();
    let mut deferred = Vec::new();

    let stdin = io::stdin();
    let mut reader = stdin.lock();

    loop {
        print!("fuse> ");
        if io::stdout().flush().is_err() {
            break;
        }

        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,   // Ctrl-D / EOF
            Ok(_) => {}
            Err(_) => break,
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if matches!(trimmed, "exit" | "quit") {
            break;
        }

        // Wrap the line in a dummy function so the parser can handle it.
        let wrapped = format!("fn __repl__() {{\n  {trimmed}\n}}");
        let program = match parse_source(&wrapped, "<repl>") {
            Ok(p) => p,
            Err(error) => {
                eprintln!("{}", error.render());
                continue;
            }
        };

        // Extract the body statements from the wrapper function.
        let statements: Vec<fa::Statement> = program
            .declarations
            .into_iter()
            .filter_map(|decl| match decl {
                fa::Declaration::Function(f) if f.name == "__repl__" => {
                    Some(f.body.statements)
                }
                _ => None,
            })
            .flatten()
            .collect();

        let stdout_before = evaluator.stdout.len();
        let mut last_value = None;
        let mut last_was_expr = false;

        for statement in &statements {
            last_was_expr = matches!(statement, fa::Statement::Expr(_));
            match evaluator.eval_statement(&repl_path, statement, &env, &mut deferred) {
                Ok(value) => {
                    last_value = value;
                }
                Err(ControlFlow::Abort(error)) => {
                    eprintln!("{}", error.render());
                    last_value = None;
                    break;
                }
                Err(ControlFlow::Return(value)) => {
                    last_value = Some(value);
                    last_was_expr = true;
                    break;
                }
                Err(_) => break,
            }
        }

        // Flush any new println output.
        for line in &evaluator.stdout[stdout_before..] {
            println!("{line}");
        }

        // Print non-Unit result values for expression statements only.
        if last_was_expr {
            if let Some(value) = last_value {
                if !matches!(value, Value::Unit) {
                    println!("{}", evaluator.stringify(&value));
                }
            }
        }
    }

    0
}

struct Evaluator {
    root_path: PathBuf,
    root_source: String,
    modules: HashMap<PathBuf, ModuleRuntime>,
    stdout: Vec<String>,
}

impl Evaluator {
    fn new(root_path: PathBuf, root_source: String) -> Self {
        Self {
            root_path: root_path.canonicalize().unwrap_or(root_path),
            root_source,
            modules: HashMap::new(),
            stdout: Vec::new(),
        }
    }

    fn eval_root(mut self) -> Result<String, RuntimeError> {
        let root = self.root_path.clone();
        self.eval_file(&root)?;
        Ok(self.stdout.join("\n"))
    }

    fn eval_file(&mut self, path: &Path) -> Result<(), RuntimeError> {
        let module = self.load_module(path)?;
        let env = self.module_env(&module)?;
        let entry = module
            .functions
            .values()
            .find(|function| function.decorators.iter().any(|decorator| decorator == "entrypoint"))
            .cloned()
            .ok_or_else(|| runtime_error("missing @entrypoint function", &display_name(path), 1, 1))?;
        let _ = self.call_user_function(&module.path, &entry, Vec::new(), Some(env))?;
        Ok(())
    }

    fn load_module(&mut self, path: &Path) -> Result<ModuleRuntime, RuntimeError> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(module) = self.modules.get(&canonical) {
            return Ok(module.clone());
        }
        let source = if canonical == self.root_path {
            self.root_source.clone()
        } else {
            std::fs::read_to_string(&canonical).map_err(|error| {
                runtime_error(
                    format!("cannot read `{}`: {error}", canonical.display()),
                    &display_name(&canonical),
                    1,
                    1,
                )
            })?
        };
        let program = parse_source(&source, &display_name(&canonical))
            .map_err(|diag| RuntimeError::plain(diag.render()))?;
        let mut module = ModuleRuntime {
            path: canonical.clone(),
            functions: HashMap::new(),
            extensions: HashMap::new(),
            data_defs: HashMap::new(),
            exports: HashMap::new(),
            imports: Vec::new(),
        };
        for declaration in program.declarations {
            match declaration {
                fa::Declaration::Import(import_decl) => module.imports.push(import_decl),
                fa::Declaration::Function(function) => {
                    if let Some(receiver_type) = &function.receiver_type {
                        module
                            .extensions
                            .insert((receiver_type.clone(), function.name.clone()), function.clone());
                    } else {
                        if function.is_pub {
                            module
                                .exports
                                .insert(function.name.clone(), ExportSymbol::Function(function.clone()));
                        }
                        module.functions.insert(function.name.clone(), function);
                    }
                }
                fa::Declaration::DataClass(data_class) => {
                    let methods = data_class
                        .methods
                        .iter()
                        .map(|method| (method.name.clone(), method.clone()))
                        .collect::<HashMap<_, _>>();
                    if data_class.is_pub {
                        module
                            .exports
                            .insert(data_class.name.clone(), ExportSymbol::Data(data_class.clone()));
                    }
                    module.data_defs.insert(
                        data_class.name.clone(),
                        DataDef {
                            decl: data_class,
                            methods,
                        },
                    );
                }
                fa::Declaration::Enum(_) => {}
                fa::Declaration::ExternFn(_) => {}
            }
        }
        self.modules.insert(canonical, module.clone());
        Ok(module)
    }

    fn base_env(&self) -> Environment {
        let env = Environment::new(None);
        env.define("println", Value::NativeFunction(Rc::new(NativeFunction::Println)), false, false);
        env.define("Some", Value::NativeFunction(Rc::new(NativeFunction::Some)), false, false);
        env.define("Ok", Value::NativeFunction(Rc::new(NativeFunction::Ok)), false, false);
        env.define("Err", Value::NativeFunction(Rc::new(NativeFunction::Err)), false, false);
        env.define("None", Value::Option(None), false, false);
        env
    }

    fn export_value(&self, module_path: &Path, export: &ExportSymbol) -> Value {
        match export {
            ExportSymbol::Function(function) => Value::UserFunction(UserFunction {
                module_path: module_path.to_path_buf(),
                decl: function.clone(),
            }),
            ExportSymbol::Data(data_class) => Value::NativeFunction(Rc::new(NativeFunction::DataConstructor {
                module_path: module_path.to_path_buf(),
                name: data_class.name.clone(),
            })),
        }
    }

    fn module_env(&mut self, module: &ModuleRuntime) -> Result<Environment, RuntimeError> {
        let env = self.base_env();
        for (name, function) in &module.functions {
            env.define(
                name,
                Value::UserFunction(UserFunction {
                    module_path: module.path.clone(),
                    decl: function.clone(),
                }),
                false,
                false,
            );
        }
        for data_def in module.data_defs.values() {
            env.define(
                data_def.decl.name.clone(),
                Value::NativeFunction(Rc::new(NativeFunction::DataConstructor {
                    module_path: module.path.clone(),
                    name: data_def.decl.name.clone(),
                })),
                false,
                false,
            );
        }
        for import_decl in &module.imports {
            let target_path = resolve_import_path(&module.path, &import_decl.module_path).ok_or_else(|| {
                runtime_error(
                    format!("cannot resolve import `{}`", import_decl.module_path),
                    &display_name(&module.path),
                    import_decl.span.line,
                    import_decl.span.column,
                )
            })?;
            let imported = self.load_module(&target_path)?;
            if let Some(items) = &import_decl.items {
                for item in items {
                    if let Some(export) = imported.exports.get(item) {
                        env.define(item, self.export_value(&imported.path, export), false, false);
                    }
                }
            } else {
                let exports = imported
                    .exports
                    .iter()
                    .map(|(name, export)| (name.clone(), self.export_value(&imported.path, export)))
                    .collect::<HashMap<_, _>>();
                let alias = import_decl
                    .module_path
                    .split('.')
                    .next_back()
                    .unwrap_or(import_decl.module_path.as_str())
                    .to_string();
                env.define(alias, Value::ModuleValue(Rc::new(ModuleValue { exports })), false, false);
            }
        }
        Ok(env)
    }

    fn construct(
        &mut self,
        module_path: &Path,
        name: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        let module = self.load_module(module_path)?;
        let data_def = module
            .data_defs
            .get(name)
            .ok_or_else(|| runtime_error(format!("unknown data class `{name}`"), &display_name(module_path), 1, 1))?;
        let mut fields = HashMap::new();
        let mut field_order = Vec::new();
        for (field, arg) in data_def.decl.fields.iter().zip(args.into_iter()) {
            field_order.push(field.name.clone());
            fields.insert(field.name.clone(), arg);
        }
        Ok(Value::Data(Rc::new(RefCell::new(DataInstance {
            module_path: module.path.clone(),
            type_name: data_def.decl.name.clone(),
            fields,
            field_order,
            methods: data_def.methods.clone(),
            destroyed: false,
        }))))
    }

    fn stringify(&self, value: &Value) -> String {
        match value {
            Value::Int(value) => value.to_string(),
            Value::Float(value) => value.to_string(),
            Value::Bool(value) => {
                if *value { "true".to_string() } else { "false".to_string() }
            }
            Value::String(value) => value.clone(),
            Value::List(items) => format!(
                "[{}]",
                items
                    .iter()
                    .map(|item| self.stringify(item))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Value::Data(instance) => {
                let instance = instance.borrow();
                format!(
                    "{}({})",
                    instance.type_name,
                    instance
                        .field_order
                        .iter()
                        .filter_map(|field| instance.fields.get(field))
                        .map(|field| self.stringify(field))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Value::Option(Some(value)) => format!("Some({})", self.stringify(value)),
            Value::Option(None) => "None".to_string(),
            Value::Result { is_ok, value } => {
                let tag = if *is_ok { "Ok" } else { "Err" };
                format!("{tag}({})", self.stringify(value))
            }
            Value::Moved(value) => self.stringify(value),
            Value::Unit => "Unit".to_string(),
            Value::NativeFunction(_) | Value::UserFunction(_) | Value::BoundMethod(_) | Value::ModuleValue(_) => {
                "<function>".to_string()
            }
        }
    }

    fn deep_clone(value: &Value) -> Value {
        match value {
            Value::Int(value) => Value::Int(*value),
            Value::Float(value) => Value::Float(*value),
            Value::Bool(value) => Value::Bool(*value),
            Value::String(value) => Value::String(value.clone()),
            Value::List(items) => Value::List(items.iter().map(Self::deep_clone).collect()),
            Value::Option(Some(value)) => Value::Option(Some(Box::new(Self::deep_clone(value)))),
            Value::Option(None) => Value::Option(None),
            Value::Result { is_ok, value } => Value::Result {
                is_ok: *is_ok,
                value: Box::new(Self::deep_clone(value)),
            },
            Value::Data(instance) => {
                let instance = instance.borrow();
                let fields = instance
                    .fields
                    .iter()
                    .map(|(name, value)| (name.clone(), Self::deep_clone(value)))
                    .collect::<HashMap<_, _>>();
                Value::Data(Rc::new(RefCell::new(DataInstance {
                    module_path: instance.module_path.clone(),
                    type_name: instance.type_name.clone(),
                    fields,
                    field_order: instance.field_order.clone(),
                    methods: instance.methods.clone(),
                    destroyed: false,
                })))
            }
            Value::NativeFunction(function) => Value::NativeFunction(function.clone()),
            Value::UserFunction(function) => Value::UserFunction(function.clone()),
            Value::BoundMethod(method) => Value::BoundMethod(method.clone()),
            Value::ModuleValue(module) => Value::ModuleValue(module.clone()),
            Value::Moved(value) => Value::Moved(Box::new(Self::deep_clone(value))),
            Value::Unit => Value::Unit,
        }
    }

    fn call_user_function(
        &mut self,
        module_path: &Path,
        function: &fa::FunctionDecl,
        args: Vec<Value>,
        caller_env: Option<Environment>,
    ) -> Result<Value, RuntimeError> {
        let module = self.load_module(module_path)?;
        let base = caller_env.unwrap_or(self.module_env(&module)?);
        let env = Environment::new(Some(base));
        let mut deferred = Vec::new();
        for (param, arg) in function.params.iter().zip(args.into_iter()) {
            let (value, destroy) = match arg {
                Value::Moved(value) => (*value, true),
                value if param.convention.as_deref() == Some("owned") => (Self::deep_clone(&value), true),
                value => (value, false),
            };
            env.define(param.name.clone(), value, true, destroy);
        }
        match self.eval_block(module_path, &function.body, &env, &mut deferred) {
            Ok(Some(value)) => {
                self.destroy_remaining(&env)?;
                self.run_defers(&mut deferred)?;
                Ok(value)
            }
            Ok(None) => {
                self.destroy_remaining(&env)?;
                self.run_defers(&mut deferred)?;
                Ok(Value::Unit)
            }
            Err(ControlFlow::Return(value)) => {
                self.destroy_remaining(&env)?;
                self.run_defers(&mut deferred)?;
                Ok(value)
            }
            Err(ControlFlow::Abort(error)) => Err(error),
            Err(ControlFlow::Break) | Err(ControlFlow::Continue) => Err(runtime_error(
                "loop control escaped function body",
                &display_name(module_path),
                function.span.line,
                function.span.column,
            )),
        }
    }

    fn eval_block(
        &mut self,
        module_path: &Path,
        block: &fa::Block,
        env: &Environment,
        deferred: &mut Vec<DeferredExpr>,
    ) -> Result<Option<Value>, ControlFlow> {
        let future = compute_future_uses(&block.statements);
        let mut last = None;
        for (index, statement) in block.statements.iter().enumerate() {
            last = self.eval_statement(module_path, statement, env, deferred)?;
            if let Err(error) = self.destroy_unused(env, &future[index], deferred) {
                return Err(ControlFlow::Abort(error));
            }
        }
        Ok(last)
    }

    fn eval_statement(
        &mut self,
        module_path: &Path,
        statement: &fa::Statement,
        env: &Environment,
        deferred: &mut Vec<DeferredExpr>,
    ) -> Result<Option<Value>, ControlFlow> {
        match statement {
            fa::Statement::VarDecl(var_decl) => {
                let value = self.eval_expr(module_path, &var_decl.value, env)?;
                env.define(var_decl.name.clone(), value.clone(), var_decl.mutable, true);
                Ok(Some(value))
            }
            fa::Statement::Assign(assign) => {
                let value = self.eval_expr(module_path, &assign.value, env)?;
                self.assign_target(module_path, &assign.target, env, value.clone())
                    .map_err(ControlFlow::Abort)?;
                Ok(Some(value))
            }
            fa::Statement::Return(return_stmt) => Err(ControlFlow::Return(
                return_stmt
                    .value
                    .as_ref()
                    .map(|expr| self.eval_expr(module_path, expr, env))
                    .transpose()
                    ?
                    .unwrap_or(Value::Unit),
            )),
            fa::Statement::Break(_) => Err(ControlFlow::Break),
            fa::Statement::Continue(_) => Err(ControlFlow::Continue),
            fa::Statement::Spawn(_) => Err(ControlFlow::Abort(runtime_error(
                "`spawn` is not implemented yet",
                &display_name(module_path),
                statement_span(statement).line,
                statement_span(statement).column,
            ))),
            fa::Statement::While(while_stmt) => {
                while truthy(
                    &self.eval_expr(module_path, &while_stmt.condition, env)?,
                ) {
                    let child = Environment::new(Some(env.clone()));
                    match self.eval_block(module_path, &while_stmt.body, &child, deferred) {
                        Ok(_) => self.destroy_remaining(&child).map_err(ControlFlow::Abort)?,
                        Err(ControlFlow::Break) => break,
                        Err(ControlFlow::Continue) => continue,
                        Err(other) => return Err(other),
                    }
                }
                Ok(None)
            }
            fa::Statement::For(for_stmt) => {
                let iterable = self.eval_expr(module_path, &for_stmt.iterable, env)?;
                let items = match iterable {
                    Value::List(items) => items,
                    _ => Vec::new(),
                };
                for item in items {
                    let child = Environment::new(Some(env.clone()));
                    child.define(for_stmt.name.clone(), item, true, false);
                    match self.eval_block(module_path, &for_stmt.body, &child, deferred) {
                        Ok(_) => self.destroy_remaining(&child).map_err(ControlFlow::Abort)?,
                        Err(ControlFlow::Break) => break,
                        Err(ControlFlow::Continue) => continue,
                        Err(other) => return Err(other),
                    }
                }
                Ok(None)
            }
            fa::Statement::Loop(loop_stmt) => {
                loop {
                    let child = Environment::new(Some(env.clone()));
                    match self.eval_block(module_path, &loop_stmt.body, &child, deferred) {
                        Ok(_) => self.destroy_remaining(&child).map_err(ControlFlow::Abort)?,
                        Err(ControlFlow::Break) => break,
                        Err(ControlFlow::Continue) => continue,
                        Err(other) => return Err(other),
                    }
                }
                Ok(None)
            }
            fa::Statement::Defer(defer_stmt) => {
                deferred.push(DeferredExpr {
                    expr: defer_stmt.expr.clone(),
                    env: Environment::new(Some(env.clone())),
                });
                Ok(None)
            }
            fa::Statement::TupleDestruct(td) => {
                let value = self.eval_expr(module_path, &td.value, env)?;
                if let Value::List(items) = value {
                    for (i, name) in td.names.iter().enumerate() {
                        let item = items.get(i).cloned().unwrap_or(Value::Unit);
                        env.define(name, item, false, true);
                    }
                }
                Ok(None)
            }
            fa::Statement::Expr(expr_stmt) => {
                let value = self.eval_expr(module_path, &expr_stmt.expr, env)?;
                Ok(Some(value))
            }
        }
    }

    fn assign_target(
        &mut self,
        module_path: &Path,
        target: &fa::Expr,
        env: &Environment,
        value: Value,
    ) -> Result<(), RuntimeError> {
        match target {
            fa::Expr::Name(name) => env.set(&name.value, value),
            fa::Expr::Member(member) => {
                let object = self.eval_expr(module_path, &member.object, env)?;
                if let Value::Data(instance) = object {
                    instance.borrow_mut().fields.insert(member.name.clone(), value);
                    Ok(())
                } else {
                    Err(runtime_error(
                        format!("cannot assign member `{}`", member.name),
                        &display_name(module_path),
                        member.span.line,
                        member.span.column,
                    ))
                }
            }
            _ => Err(runtime_error(
                "unsupported assignment target",
                &display_name(module_path),
                target.span().line,
                target.span().column,
            )),
        }
    }

    fn eval_expr(
        &mut self,
        module_path: &Path,
        expr: &fa::Expr,
        env: &Environment,
    ) -> Result<Value, ControlFlow> {
        match expr {
            fa::Expr::Literal(literal) => Ok(match &literal.value {
                fa::LiteralValue::Int(value) => Value::Int(*value),
                fa::LiteralValue::Float(value) => Value::Float(*value),
                fa::LiteralValue::String(value) => Value::String(value.clone()),
                fa::LiteralValue::Bool(value) => Value::Bool(*value),
            }),
            fa::Expr::FString(fstring) => Ok(Value::String(self.render_fstring(&fstring.template, env))),
            fa::Expr::Name(name) => {
                if name.value == "None" {
                    Ok(Value::Option(None))
                } else {
                    env.get(&name.value).map_err(|mut error| {
                        error.filename = display_name(module_path);
                        error.line = name.span.line;
                        error.column = name.span.column;
                        ControlFlow::Abort(error)
                    })
                }
            }
            fa::Expr::List(list) => {
                let mut items = Vec::new();
                for item in &list.items {
                    items.push(self.eval_expr(module_path, item, env)?);
                }
                Ok(Value::List(items))
            }
            fa::Expr::Unary(unary) => {
                let value = self.eval_expr(module_path, &unary.value, env)?;
                match unary.op.as_str() {
                    "-" => match value {
                        Value::Int(value) => Ok(Value::Int(-value)),
                        Value::Float(value) => Ok(Value::Float(-value)),
                        _ => Ok(Value::Unit),
                    },
                    "not" => Ok(Value::Bool(!truthy(&value))),
                    _ => Err(ControlFlow::Abort(runtime_error(
                        format!("unsupported operator `{}`", unary.op),
                        &display_name(module_path),
                        unary.span.line,
                        unary.span.column,
                    ))),
                }
            }
            fa::Expr::Binary(binary) => {
                if binary.op == "?:" {
                    let left = self.eval_expr(module_path, &binary.left, env)?;
                    return Ok(match left {
                        Value::Option(Some(value)) => *value,
                        Value::Option(None) => self.eval_expr(module_path, &binary.right, env)?,
                        value => value,
                    });
                }
                let left = self.eval_expr(module_path, &binary.left, env)?;
                let right = self.eval_expr(module_path, &binary.right, env)?;
                eval_binary(binary, left, right).map_err(ControlFlow::Abort)
            }
            fa::Expr::Member(member) => {
                let object = self.eval_expr(module_path, &member.object, env)?;
                if member.optional {
                    return Ok(match object {
                        Value::Option(Some(value)) => Value::Option(Some(Box::new(
                            self.resolve_member(module_path, *value, &member.name, member.span)?,
                        ))),
                        _ => Value::Option(None),
                    });
                }
                self.resolve_member(module_path, object, &member.name, member.span)
                    .map_err(ControlFlow::Abort)
            }
            fa::Expr::Move(move_expr) => {
                if let fa::Expr::Name(name) = move_expr.value.as_ref() {
                    let value = env.get(&name.value)?;
                    env.mark_moved(&name.value)?;
                    Ok(Value::Moved(Box::new(value)))
                } else {
                    Ok(Value::Moved(Box::new(self.eval_expr(module_path, &move_expr.value, env)?)))
                }
            }
            fa::Expr::Ref(reference) => self.eval_expr(module_path, &reference.value, env),
            fa::Expr::MutRef(reference) => self.eval_expr(module_path, &reference.value, env),
            fa::Expr::Await(await_expr) => self.eval_expr(module_path, &await_expr.value, env),
            fa::Expr::Question(question) => {
                let value = self.eval_expr(module_path, &question.value, env)?;
                match value {
                    Value::Result { is_ok: true, value } => Ok(*value),
                    Value::Result { is_ok: false, value } => Err(ControlFlow::Return(Value::Result {
                        is_ok: false,
                        value,
                    })),
                    Value::Option(Some(value)) => Ok(*value),
                    Value::Option(None) => Err(ControlFlow::Return(Value::Option(None))),
                    value => Ok(value),
                }
            }
            fa::Expr::Call(call) => {
                let callee = self.eval_expr(module_path, &call.callee, env)?;
                let mut args = Vec::new();
                for arg in &call.args {
                    args.push(self.eval_expr(module_path, arg, env)?);
                }
                self.call_value(module_path, callee, args, call.span)
                    .map_err(ControlFlow::Abort)
            }
            fa::Expr::If(if_expr) => {
                if truthy(&self.eval_expr(module_path, &if_expr.condition, env)?) {
                    let child = Environment::new(Some(env.clone()));
                    Ok(self
                        .eval_block(module_path, &if_expr.then_branch, &child, &mut Vec::new())?
                        .unwrap_or(Value::Unit))
                } else if let Some(else_branch) = &if_expr.else_branch {
                    match else_branch {
                        fa::ElseBranch::Block(block) => {
                            let child = Environment::new(Some(env.clone()));
                            Ok(self
                                .eval_block(module_path, block, &child, &mut Vec::new())?
                                .unwrap_or(Value::Unit))
                        }
                        fa::ElseBranch::IfExpr(expr) => self.eval_expr(module_path, &fa::Expr::If(*expr.clone()), env),
                    }
                } else {
                    Ok(Value::Unit)
                }
            }
            fa::Expr::Match(match_expr) => {
                let subject = self.eval_expr(module_path, &match_expr.subject, env)?;
                for arm in &match_expr.arms {
                    if let Some(bindings) = match_pattern(&arm.pattern, &subject) {
                        let child = Environment::new(Some(env.clone()));
                        for (name, value) in bindings {
                            child.define(name, value, true, false);
                        }
                        return match &arm.body {
                            fa::ArmBody::Block(block) => Ok(
                                self.eval_block(module_path, block, &child, &mut Vec::new())?
                                    .unwrap_or(Value::Unit),
                            ),
                            fa::ArmBody::Expr(expr) => self.eval_expr(module_path, expr, &child),
                        };
                    }
                }
                Err(ControlFlow::Abort(runtime_error(
                    "non-exhaustive match",
                    &display_name(module_path),
                    match_expr.span.line,
                    match_expr.span.column,
                )))
            }
            fa::Expr::When(when_expr) => {
                for arm in &when_expr.arms {
                    let matches = if let Some(condition) = &arm.condition {
                        truthy(&self.eval_expr(module_path, condition, env)?)
                    } else {
                        true
                    };
                    if matches {
                        return match &arm.body {
                            fa::ArmBody::Block(block) => {
                                let child = Environment::new(Some(env.clone()));
                                Ok(self
                                    .eval_block(module_path, block, &child, &mut Vec::new())?
                                    .unwrap_or(Value::Unit))
                            }
                            fa::ArmBody::Expr(expr) => self.eval_expr(module_path, expr, env),
                        };
                    }
                }
                Ok(Value::Unit)
            }
            fa::Expr::Tuple(tuple) => {
                let mut items = Vec::new();
                for item in &tuple.items {
                    items.push(self.eval_expr(module_path, item, env)?);
                }
                Ok(Value::List(items))
            }
            fa::Expr::Lambda(lambda) => {
                let decl = fa::FunctionDecl {
                    name: format!("__lambda_{}", lambda.span.line),
                    params: lambda.params.clone(),
                    return_type: lambda.return_type.clone(),
                    body: lambda.body.clone(),
                    is_pub: false,
                    decorators: Vec::new(),
                    is_async: false,
                    is_suspend: false,
                    receiver_type: None,
                    span: lambda.span,
                };
                Ok(Value::UserFunction(UserFunction {
                    module_path: module_path.to_path_buf(),
                    decl,
                }))
            }
        }
    }

    fn resolve_member(
        &mut self,
        module_path: &Path,
        object: Value,
        name: &str,
        span: crate::error::Span,
    ) -> Result<Value, RuntimeError> {
        match object.clone() {
            Value::Data(instance) => {
                let instance_ref = instance.borrow();
                if let Some(field) = instance_ref.fields.get(name) {
                    return Ok(field.clone());
                }
                if let Some(method) = instance_ref.methods.get(name) {
                    return Ok(Value::BoundMethod(Rc::new(BoundMethod {
                        receiver: object,
                        function: UserFunction {
                            module_path: instance_ref.module_path.clone(),
                            decl: method.clone(),
                        },
                    })));
                }
            }
            Value::ModuleValue(module) => {
                if let Some(value) = module.exports.get(name) {
                    return Ok(value.clone());
                }
            }
            Value::String(_) => match name {
                "toUpper" => {
                    return Ok(Value::NativeFunction(Rc::new(NativeFunction::StringToUpper(
                        Box::new(object),
                    ))))
                }
                "isEmpty" => {
                    return Ok(Value::NativeFunction(Rc::new(NativeFunction::StringIsEmpty(
                        Box::new(object),
                    ))))
                }
                _ => {}
            },
            Value::List(items) => {
                if let Ok(index) = name.parse::<usize>() {
                    if let Some(item) = items.get(index) {
                        return Ok(item.clone());
                    }
                }
            }
            _ => {}
        }
        let receiver_type = runtime_type(&object);
        if let Some(extension) = self.find_extension(&receiver_type, name) {
            return Ok(Value::BoundMethod(Rc::new(BoundMethod {
                receiver: object,
                function: extension,
            })));
        }
        Err(runtime_error(
            format!("unknown member `{name}`"),
            &display_name(module_path),
            span.line,
            span.column,
        ))
    }

    fn find_extension(&self, receiver_type: &str, name: &str) -> Option<UserFunction> {
        for module in self.modules.values() {
            let key = (
                receiver_type.split('<').next().unwrap_or(receiver_type).to_string(),
                name.to_string(),
            );
            if let Some(function) = module.extensions.get(&key) {
                return Some(UserFunction {
                    module_path: module.path.clone(),
                    decl: function.clone(),
                });
            }
        }
        None
    }

    fn call_value(
        &mut self,
        module_path: &Path,
        callee: Value,
        args: Vec<Value>,
        span: crate::error::Span,
    ) -> Result<Value, RuntimeError> {
        match callee {
            Value::NativeFunction(function) => match function.as_ref() {
                NativeFunction::Println => {
                    if let Some(value) = args.first() {
                        self.stdout.push(self.stringify(value));
                    }
                    Ok(Value::Unit)
                }
                NativeFunction::Some => Ok(Value::Option(args.into_iter().next().map(Box::new))),
                NativeFunction::Ok => Ok(Value::Result {
                    is_ok: true,
                    value: Box::new(strip_moved(args.into_iter().next().unwrap_or(Value::Unit))),
                }),
                NativeFunction::Err => Ok(Value::Result {
                    is_ok: false,
                    value: Box::new(strip_moved(args.into_iter().next().unwrap_or(Value::Unit))),
                }),
                NativeFunction::DataConstructor { module_path, name } => {
                    self.construct(&module_path, &name, args.into_iter().map(strip_moved).collect())
                }
                NativeFunction::StringToUpper(receiver) => {
                    Ok(Value::String(self.stringify(receiver).to_uppercase()))
                }
                NativeFunction::StringIsEmpty(receiver) => {
                    Ok(Value::Bool(self.stringify(receiver).is_empty()))
                }
            },
            Value::UserFunction(function) => self.call_user_function(&function.module_path, &function.decl, args, None),
            Value::BoundMethod(method) => {
                let mut bound_args = vec![method.receiver.clone()];
                bound_args.extend(args);
                self.call_user_function(&method.function.module_path, &method.function.decl, bound_args, None)
            }
            other => Err(runtime_error(
                format!("cannot call value `{}`", self.stringify(&other)),
                &display_name(module_path),
                span.line,
                span.column,
            )),
        }
    }

    fn destroy_unused(
        &mut self,
        env: &Environment,
        future_names: &HashSet<String>,
        deferred: &[DeferredExpr],
    ) -> Result<(), RuntimeError> {
        let mut defer_names = HashSet::new();
        for item in deferred {
            defer_names.extend(collect_expr_names(&item.expr));
        }
        for (name, binding) in env.local_bindings() {
            if future_names.contains(&name) || defer_names.contains(&name) {
                continue;
            }
            let mut binding_ref = binding.borrow_mut();
            if binding_ref.moved || !binding_ref.destroy {
                continue;
            }
            self.destroy_value(&binding_ref.value)?;
            binding_ref.moved = true;
        }
        Ok(())
    }

    fn destroy_remaining(&mut self, env: &Environment) -> Result<(), RuntimeError> {
        for (_, binding) in env.local_bindings() {
            let mut binding_ref = binding.borrow_mut();
            if binding_ref.moved || !binding_ref.destroy {
                continue;
            }
            self.destroy_value(&binding_ref.value)?;
            binding_ref.moved = true;
        }
        Ok(())
    }

    fn destroy_value(&mut self, value: &Value) -> Result<(), RuntimeError> {
        if let Value::Data(instance) = value {
            let mut instance_ref = instance.borrow_mut();
            if instance_ref.destroyed {
                return Ok(());
            }
            instance_ref.destroyed = true;
            if let Some(method) = instance_ref.methods.get("__del__").cloned() {
                let module_path = instance_ref.module_path.clone();
                drop(instance_ref);
                let _ = self.call_user_function(&module_path, &method, vec![Value::Moved(Box::new(value.clone()))], None)?;
            }
        }
        Ok(())
    }

    fn run_defers(&mut self, deferred: &mut Vec<DeferredExpr>) -> Result<(), RuntimeError> {
        while let Some(item) = deferred.pop() {
            match self.eval_expr(&self.root_path.clone(), &item.expr, &item.env) {
                Ok(_) => {}
                Err(ControlFlow::Abort(error)) => return Err(error),
                Err(_) => return Err(RuntimeError::plain("unexpected control flow inside defer")),
            }
        }
        Ok(())
    }

    fn render_fstring(&self, template: &str, env: &Environment) -> String {
        let mut output = String::new();
        let mut rest = template;
        while let Some(start) = rest.find('{') {
            output.push_str(&rest[..start]);
            if let Some(end) = rest[start + 1..].find('}') {
                let expr = &rest[start + 1..start + 1 + end];
                output.push_str(&self.interpolate(expr.trim(), env));
                rest = &rest[start + 2 + end..];
            } else {
                output.push_str(&rest[start..]);
                rest = "";
            }
        }
        output.push_str(rest);
        output
    }

    fn interpolate(&self, expr: &str, env: &Environment) -> String {
        let mut parts = expr.split('.');
        let Some(first) = parts.next() else {
            return String::new();
        };
        let mut value = match env.get(first) {
            Ok(value) => value,
            Err(_) => return String::new(),
        };
        for part in parts {
            value = match value {
                Value::Data(instance) => instance.borrow().fields.get(part).cloned().unwrap_or(Value::Unit),
                Value::Option(Some(value)) => match *value {
                    Value::Data(instance) => instance.borrow().fields.get(part).cloned().unwrap_or(Value::Unit),
                    other => other,
                },
                other => other,
            };
        }
        self.stringify(&value)
    }
}

fn strip_moved(value: Value) -> Value {
    match value {
        Value::Moved(value) => *value,
        other => other,
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Bool(value) => *value,
        Value::Option(value) => value.is_some(),
        Value::Result { is_ok, .. } => *is_ok,
        Value::Int(value) => *value != 0,
        Value::Float(value) => *value != 0.0,
        Value::String(value) => !value.is_empty(),
        Value::List(items) => !items.is_empty(),
        Value::Unit => false,
        Value::Moved(value) => truthy(value),
        _ => true,
    }
}

fn runtime_type(value: &Value) -> String {
    match value {
        Value::Bool(_) => "Bool".to_string(),
        Value::Int(_) => "Int".to_string(),
        Value::Float(_) => "Float".to_string(),
        Value::String(_) => "String".to_string(),
        Value::List(_) => "List".to_string(),
        Value::Data(instance) => instance.borrow().type_name.clone(),
        Value::Option(_) => "Option".to_string(),
        Value::Result { .. } => "Result".to_string(),
        Value::Moved(value) => runtime_type(value),
        _ => "Unit".to_string(),
    }
}

fn runtime_error(message: impl Into<String>, filename: &str, line: usize, column: usize) -> RuntimeError {
    RuntimeError {
        message: message.into(),
        filename: filename.to_string(),
        line,
        column,
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|part| part.to_str())
        .unwrap_or("<input>")
        .to_string()
}

fn control_to_runtime(control: ControlFlow) -> RuntimeError {
    match control {
        ControlFlow::Return(_) => RuntimeError::plain("unexpected return"),
        ControlFlow::Break => RuntimeError::plain("unexpected break"),
        ControlFlow::Continue => RuntimeError::plain("unexpected continue"),
        ControlFlow::Abort(error) => error,
    }
}

fn eval_binary(binary: &fa::BinaryOp, left: Value, right: Value) -> Result<Value, RuntimeError> {
    match binary.op.as_str() {
        "+" => match (left, right) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Int(left + right)),
            (Value::String(left), Value::String(right)) => Ok(Value::String(left + &right)),
            (left, right) => Ok(Value::String(render_value(&left) + &render_value(&right))),
        },
        "-" => match (left, right) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Int(left - right)),
            (Value::Float(left), Value::Float(right)) => Ok(Value::Float(left - right)),
            _ => Ok(Value::Unit),
        },
        "*" => match (left, right) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Int(left * right)),
            (Value::Float(left), Value::Float(right)) => Ok(Value::Float(left * right)),
            _ => Ok(Value::Unit),
        },
        "/" => match (left, right) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Int(left / right)),
            (Value::Float(left), Value::Float(right)) => Ok(Value::Float(left / right)),
            _ => Ok(Value::Unit),
        },
        "%" => match (left, right) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Int(left % right)),
            _ => Ok(Value::Unit),
        },
        "==" => Ok(Value::Bool(value_eq(&left, &right))),
        "!=" => Ok(Value::Bool(!value_eq(&left, &right))),
        "<" => compare_binary(left, right, |left, right| left < right),
        ">" => compare_binary(left, right, |left, right| left > right),
        "<=" => compare_binary(left, right, |left, right| left <= right),
        ">=" => compare_binary(left, right, |left, right| left >= right),
        "and" => Ok(Value::Bool(truthy(&left) && truthy(&right))),
        "or" => Ok(Value::Bool(truthy(&left) || truthy(&right))),
        other => Err(runtime_error(
            format!("unsupported operator `{other}`"),
            "<runtime>",
            binary.span.line,
            binary.span.column,
        )),
    }
}

fn compare_binary(left: Value, right: Value, cmp: impl Fn(i64, i64) -> bool) -> Result<Value, RuntimeError> {
    match (left, right) {
        (Value::Int(left), Value::Int(right)) => Ok(Value::Bool(cmp(left, right))),
        _ => Ok(Value::Bool(false)),
    }
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Int(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::Bool(value) => {
            if *value { "true".to_string() } else { "false".to_string() }
        }
        Value::String(value) => value.clone(),
        Value::Option(None) => "None".to_string(),
        Value::Option(Some(value)) => format!("Some({})", render_value(value)),
        Value::Result { is_ok, value } => {
            let tag = if *is_ok { "Ok" } else { "Err" };
            format!("{tag}({})", render_value(value))
        }
        Value::Unit => "Unit".to_string(),
        Value::Moved(value) => render_value(value),
        _ => "<value>".to_string(),
    }
}

fn value_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Int(left), Value::Int(right)) => left == right,
        (Value::Float(left), Value::Float(right)) => left == right,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Unit, Value::Unit) => true,
        (Value::Option(None), Value::Option(None)) => true,
        (Value::Option(Some(left)), Value::Option(Some(right))) => value_eq(left, right),
        (
            Value::Result {
                is_ok: left_ok,
                value: left_value,
            },
            Value::Result {
                is_ok: right_ok,
                value: right_value,
            },
        ) => left_ok == right_ok && value_eq(left_value, right_value),
        (Value::Data(left), Value::Data(right)) => {
            let left = left.borrow();
            let right = right.borrow();
            left.type_name == right.type_name
                && left.field_order == right.field_order
                && left.field_order.iter().all(|field| match (left.fields.get(field), right.fields.get(field)) {
                    (Some(left_value), Some(right_value)) => value_eq(left_value, right_value),
                    (None, None) => true,
                    _ => false,
                })
        }
        _ => false,
    }
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
        fa::Statement::Return(return_stmt) => return_stmt
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
        fa::Statement::Loop(loop_stmt) => {
            let mut names = HashSet::new();
            for statement in &loop_stmt.body.statements {
                names.extend(collect_stmt_names(statement));
            }
            names
        }
        fa::Statement::Spawn(spawn_stmt) => {
            let mut names = HashSet::new();
            for statement in &spawn_stmt.body.statements {
                names.extend(collect_stmt_names(statement));
            }
            names
        }
        fa::Statement::Defer(defer_stmt) => collect_expr_names(&defer_stmt.expr),
        fa::Statement::Expr(expr_stmt) => collect_expr_names(&expr_stmt.expr),
        fa::Statement::TupleDestruct(td) => collect_expr_names(&td.value),
        fa::Statement::Break(_) | fa::Statement::Continue(_) => HashSet::new(),
    }
}

fn statement_span(statement: &fa::Statement) -> crate::error::Span {
    match statement {
        fa::Statement::VarDecl(node) => node.span,
        fa::Statement::Assign(node) => node.span,
        fa::Statement::Return(node) => node.span,
        fa::Statement::Break(span) => *span,
        fa::Statement::Continue(span) => *span,
        fa::Statement::Spawn(node) => node.span,
        fa::Statement::While(node) => node.span,
        fa::Statement::For(node) => node.span,
        fa::Statement::Loop(node) => node.span,
        fa::Statement::Defer(node) => node.span,
        fa::Statement::Expr(node) => node.span,
        fa::Statement::TupleDestruct(node) => node.span,
    }
}

fn collect_expr_names(expr: &fa::Expr) -> HashSet<String> {
    match expr {
        fa::Expr::Name(name) => HashSet::from([name.value.clone()]),
        fa::Expr::Literal(_) => HashSet::new(),
        fa::Expr::FString(fstring) => fstring
            .template
            .split('{')
            .skip(1)
            .filter_map(|part| part.split('}').next())
            .map(|part| part.trim().split('.').next().unwrap_or_default().to_string())
            .filter(|part| !part.is_empty())
            .collect(),
        fa::Expr::Member(member) => collect_expr_names(&member.object),
        fa::Expr::Move(move_expr) => collect_expr_names(&move_expr.value),
        fa::Expr::Ref(reference) => collect_expr_names(&reference.value),
        fa::Expr::MutRef(reference) => collect_expr_names(&reference.value),
        fa::Expr::Await(await_expr) => collect_expr_names(&await_expr.value),
        fa::Expr::Question(question) => collect_expr_names(&question.value),
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
        fa::Expr::List(list) => {
            let mut names = HashSet::new();
            for item in &list.items {
                names.extend(collect_expr_names(item));
            }
            names
        }
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
                    fa::ElseBranch::IfExpr(expr) => names.extend(collect_expr_names(&fa::Expr::If(*expr.clone()))),
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

fn match_pattern(pattern: &fa::Pattern, value: &Value) -> Option<HashMap<String, Value>> {
    match pattern {
        fa::Pattern::Wildcard(_) => Some(HashMap::new()),
        fa::Pattern::Name(pattern) => Some(HashMap::from([(pattern.name.clone(), value.clone())])),
        fa::Pattern::Literal(pattern) => {
            let matches = match (&pattern.value, value) {
                (fa::LiteralValue::Int(left), Value::Int(right)) => left == right,
                (fa::LiteralValue::Float(left), Value::Float(right)) => left == right,
                (fa::LiteralValue::String(left), Value::String(right)) => left == right,
                (fa::LiteralValue::Bool(left), Value::Bool(right)) => left == right,
                _ => false,
            };
            matches.then(HashMap::new)
        }
        fa::Pattern::Variant(pattern) => {
            let name = pattern.name.split('.').next_back().unwrap_or(pattern.name.as_str());
            match (name, value) {
                ("Some", Value::Option(Some(inner))) => pattern
                    .args
                    .first()
                    .map(|pattern| match_pattern(pattern, inner))
                    .unwrap_or_else(|| Some(HashMap::new())),
                ("None", Value::Option(None)) => Some(HashMap::new()),
                ("Ok", Value::Result { is_ok: true, value }) => pattern
                    .args
                    .first()
                    .map(|pattern| match_pattern(pattern, value))
                    .unwrap_or_else(|| Some(HashMap::new())),
                ("Err", Value::Result { is_ok: false, value }) => pattern
                    .args
                    .first()
                    .map(|pattern| match_pattern(pattern, value))
                    .unwrap_or_else(|| Some(HashMap::new())),
                (variant, Value::Data(instance)) if instance.borrow().type_name == variant => Some(HashMap::new()),
                _ => None,
            }
        }
    }
}
