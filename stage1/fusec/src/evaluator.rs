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
    Map(Vec<(Value, Value)>),
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
    MapMethod { receiver: Box<Value>, method: String },
    ListMethod { receiver: Box<Value>, method: String },
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
    module_envs: HashMap<PathBuf, Environment>,
    stdout: Vec<String>,
}

impl Evaluator {
    fn new(root_path: PathBuf, root_source: String) -> Self {
        Self {
            root_path: root_path.canonicalize().unwrap_or(root_path),
            root_source,
            modules: HashMap::new(),
            module_envs: HashMap::new(),
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
                fa::Declaration::ExternFn(ext) => {
                    if ext.is_pub || true {
                        module.functions.insert(ext.name.clone(), fa::FunctionDecl {
                            name: ext.name.clone(),
                            params: ext.params.clone(),
                            return_type: ext.return_type.clone(),
                            body: fa::Block { statements: Vec::new(), span: ext.span },
                            is_pub: ext.is_pub,
                            decorators: Vec::new(),
                            is_async: false,
                            is_suspend: false,
                            receiver_type: None,
                            span: ext.span,
                        });
                    }
                }
                fa::Declaration::Struct(_) => {}
                fa::Declaration::Const(_) => {}
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
            Value::Float(value) => {
                let s = value.to_string();
                if s.contains('.') || s.contains("NaN") || s.contains("inf") { s } else { format!("{s}.0") }
            }
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
            Value::Map(entries) => {
                let pairs: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", self.stringify(k), self.stringify(v)))
                    .collect();
                format!("{{{}}}", pairs.join(", "))
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
            Value::Map(entries) => Value::Map(
                entries
                    .iter()
                    .map(|(k, v)| (Self::deep_clone(k), Self::deep_clone(v)))
                    .collect(),
            ),
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
        // Handle known FFI functions in the evaluator (before args are moved).
        if function.body.statements.is_empty() {
            match function.name.as_str() {
                "fuse_rt_int_to_float" => {
                    if let Some(Value::Int(n)) = args.first() {
                        return Ok(Value::Float(*n as f64));
                    }
                    return Ok(Value::Float(0.0));
                }
                "fuse_rt_int_parse" => {
                    if let Some(Value::String(s)) = args.first() {
                        return match s.parse::<i64>() {
                            Ok(n) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Int(n)) }),
                            Err(_) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("int: invalid number: {s}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("int: expected string".to_string())) });
                }
                "fuse_rt_string_len" => {
                    if let Some(Value::String(s)) = args.first() {
                        return Ok(Value::Int(s.chars().count() as i64));
                    }
                    return Ok(Value::Int(0));
                }
                "fuse_rt_string_char_at" => {
                    if let (Some(Value::String(s)), Some(Value::Int(i))) = (args.first(), args.get(1)) {
                        if let Some(ch) = s.chars().nth(*i as usize) {
                            return Ok(Value::String(ch.to_string()));
                        }
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_float_abs" => return Ok(Value::Float(Self::extract_float(args.first()).abs())),
                "fuse_rt_float_floor" => return Ok(Value::Float(Self::extract_float(args.first()).floor())),
                "fuse_rt_float_ceil" => return Ok(Value::Float(Self::extract_float(args.first()).ceil())),
                "fuse_rt_float_round" => return Ok(Value::Float(Self::extract_float(args.first()).round())),
                "fuse_rt_float_trunc" => return Ok(Value::Float(Self::extract_float(args.first()).trunc())),
                "fuse_rt_float_fract" => return Ok(Value::Float(Self::extract_float(args.first()).fract())),
                "fuse_rt_float_sqrt" => return Ok(Value::Float(Self::extract_float(args.first()).sqrt())),
                "fuse_rt_float_pow" => return Ok(Value::Float(Self::extract_float(args.first()).powf(Self::extract_float(args.get(1))))),
                "fuse_rt_float_is_nan" => return Ok(Value::Bool(Self::extract_float(args.first()).is_nan())),
                "fuse_rt_float_is_infinite" => return Ok(Value::Bool(Self::extract_float(args.first()).is_infinite())),
                "fuse_rt_float_is_finite" => return Ok(Value::Bool(Self::extract_float(args.first()).is_finite())),
                "fuse_rt_float_to_int" => return Ok(Value::Int(Self::extract_float(args.first()) as i64)),
                "fuse_rt_float_parse" => {
                    if let Some(Value::String(s)) = args.first() {
                        return match s.parse::<f64>() {
                            Ok(v) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Float(v)) }),
                            Err(_) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("float: invalid number: {s}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("float: expected string".to_string())) });
                }
                "fuse_rt_float_to_string_fixed" => {
                    let v = Self::extract_float(args.first());
                    let d = match args.get(1) { Some(Value::Int(n)) => *n as usize, _ => 2 };
                    return Ok(Value::String(format!("{v:.prec$}", prec = d)));
                }
                "fuse_list_new" => { return Ok(Value::List(Vec::new())); }
                "fuse_list_push" => {
                    // In the evaluator, List is cloned so mutation doesn't propagate.
                    // For HOF methods (map, filter, etc.), the list is built locally
                    // within the same function scope, so we need a mutable approach.
                    // Return Unit — the list module's HOF methods use a pattern that
                    // works with the evaluator's FFI handler for fuse_rt_list_push.
                    return Ok(Value::Unit);
                }
                "fuse_list_len" => {
                    if let Some(Value::List(items)) = args.first() { return Ok(Value::Int(items.len() as i64)); }
                    return Ok(Value::Int(0));
                }
                "fuse_list_get" => {
                    if let (Some(Value::List(items)), Some(Value::Int(i))) = (args.first(), args.get(1)) {
                        return Ok(items.get(*i as usize).cloned().unwrap_or(Value::Unit));
                    }
                    return Ok(Value::Unit);
                }
                "fuse_rt_io_read_file" => {
                    if let Some(Value::String(p)) = args.first() {
                        return match std::fs::read_to_string(p) {
                            Ok(s) => Ok(Value::Result { is_ok: true, value: Box::new(Value::String(s)) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("io: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("io: expected path".to_string())) });
                }
                "fuse_rt_io_read_file_bytes" => {
                    if let Some(Value::String(p)) = args.first() {
                        return match std::fs::read(p) {
                            Ok(bytes) => Ok(Value::Result { is_ok: true, value: Box::new(Value::List(bytes.into_iter().map(|b| Value::Int(b as i64)).collect())) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("io: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("io: expected path".to_string())) });
                }
                "fuse_rt_io_write_file" => {
                    if let (Some(Value::String(p)), Some(Value::String(c))) = (args.first(), args.get(1)) {
                        return match std::fs::write(p, c) {
                            Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("io: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("io: expected path and content".to_string())) });
                }
                "fuse_rt_io_write_file_bytes" => {
                    if let (Some(Value::String(p)), Some(Value::List(items))) = (args.first(), args.get(1)) {
                        let bytes: Vec<u8> = items.iter().filter_map(|v| match v { Value::Int(n) => Some(*n as u8), _ => None }).collect();
                        return match std::fs::write(p, &bytes) {
                            Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("io: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("io: expected path and bytes".to_string())) });
                }
                "fuse_rt_io_append_file" => {
                    if let (Some(Value::String(p)), Some(Value::String(c))) = (args.first(), args.get(1)) {
                        use std::io::Write;
                        return match std::fs::OpenOptions::new().append(true).create(true).open(p) {
                            Ok(mut f) => match f.write_all(c.as_bytes()) {
                                Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                                Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("io: {e}"))) }),
                            },
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("io: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("io: expected path and content".to_string())) });
                }
                "fuse_rt_io_read_line" => {
                    let mut line = String::new();
                    return match std::io::stdin().read_line(&mut line) {
                        Ok(_) => Ok(Value::Result { is_ok: true, value: Box::new(Value::String(line.trim_end_matches('\n').trim_end_matches('\r').to_string())) }),
                        Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("io: {e}"))) }),
                    };
                }
                "fuse_rt_io_read_all" => {
                    use std::io::Read;
                    let mut s = String::new();
                    return match std::io::stdin().read_to_string(&mut s) {
                        Ok(_) => Ok(Value::Result { is_ok: true, value: Box::new(Value::String(s)) }),
                        Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("io: {e}"))) }),
                    };
                }
                "fuse_rt_file_open" | "fuse_rt_file_close" => { return Ok(Value::Unit); }
                "fuse_rt_path_separator" => {
                    let sep = if cfg!(windows) { "\\" } else { "/" };
                    return Ok(Value::String(sep.to_string()));
                }
                "fuse_rt_path_cwd" => {
                    return match std::env::current_dir() {
                        Ok(p) => Ok(Value::Result { is_ok: true, value: Box::new(Value::String(p.to_string_lossy().to_string())) }),
                        Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("path: {e}"))) }),
                    };
                }
                "fuse_map_new" => { return Ok(Value::Map(Vec::new())); }
                "fuse_map_set" => {
                    // Mutation limited in evaluator (value semantics)
                    return Ok(Value::Unit);
                }
                "fuse_map_get" => {
                    if let (Some(Value::Map(entries)), Some(key)) = (args.first(), args.get(1)) {
                        let key_str = self.stringify(key);
                        for (k, v) in entries {
                            if self.stringify(k) == key_str { return Ok(v.clone()); }
                        }
                    }
                    return Ok(Value::Unit);
                }
                "fuse_map_len" => {
                    if let Some(Value::Map(entries)) = args.first() { return Ok(Value::Int(entries.len() as i64)); }
                    return Ok(Value::Int(0));
                }
                "fuse_map_contains" => {
                    if let (Some(Value::Map(entries)), Some(key)) = (args.first(), args.get(1)) {
                        let key_str = self.stringify(key);
                        return Ok(Value::Bool(entries.iter().any(|(k, _)| self.stringify(k) == key_str)));
                    }
                    return Ok(Value::Bool(false));
                }
                "fuse_map_keys" => {
                    if let Some(Value::Map(entries)) = args.first() {
                        return Ok(Value::List(entries.iter().map(|(k, _)| k.clone()).collect()));
                    }
                    return Ok(Value::List(Vec::new()));
                }
                "fuse_map_values" => {
                    if let Some(Value::Map(entries)) = args.first() {
                        return Ok(Value::List(entries.iter().map(|(_, v)| v.clone()).collect()));
                    }
                    return Ok(Value::List(Vec::new()));
                }
                "fuse_map_entries" => {
                    if let Some(Value::Map(entries)) = args.first() {
                        return Ok(Value::List(entries.iter().map(|(k, v)| Value::List(vec![k.clone(), v.clone()])).collect()));
                    }
                    return Ok(Value::List(Vec::new()));
                }
                "fuse_map_remove" => { return Ok(Value::Unit); }
                "fuse_rt_list_len" => {
                    if let Some(Value::List(items)) = args.first() { return Ok(Value::Int(items.len() as i64)); }
                    return Ok(Value::Int(0));
                }
                "fuse_rt_list_get" => {
                    if let (Some(Value::List(items)), Some(Value::Int(i))) = (args.first(), args.get(1)) {
                        return Ok(items.get(*i as usize).cloned().map(|v| Value::Option(Some(Box::new(v)))).unwrap_or(Value::Option(None)));
                    }
                    return Ok(Value::Option(None));
                }
                "fuse_rt_list_push" => {
                    // Evaluator limitation: can't mutate cloned list
                    return Ok(Value::Unit);
                }
                "fuse_rt_list_pop" => { return Ok(Value::Option(None)); }
                "fuse_rt_list_set" | "fuse_rt_list_insert" | "fuse_rt_list_remove_at" | "fuse_rt_list_clear" | "fuse_rt_list_reverse_in_place" => {
                    return Ok(Value::Unit);
                }
                "fuse_rt_list_slice" => {
                    if let (Some(Value::List(items)), Some(Value::Int(s)), Some(Value::Int(e))) = (args.first(), args.get(1), args.get(2)) {
                        let start = (*s as usize).min(items.len());
                        let end = (*e as usize).min(items.len());
                        return Ok(Value::List(items[start..end].to_vec()));
                    }
                    return Ok(Value::List(Vec::new()));
                }
                "fuse_rt_list_concat" => {
                    let mut result = Vec::new();
                    if let Some(Value::List(a)) = args.first() { result.extend(a.iter().cloned()); }
                    if let Some(Value::List(b)) = args.get(1) { result.extend(b.iter().cloned()); }
                    return Ok(Value::List(result));
                }
                "fuse_rt_list_reverse" => {
                    if let Some(Value::List(items)) = args.first() {
                        return Ok(Value::List(items.iter().rev().cloned().collect()));
                    }
                    return Ok(Value::List(Vec::new()));
                }
                "fuse_rt_list_join" => {
                    if let (Some(Value::List(items)), Some(Value::String(sep))) = (args.first(), args.get(1)) {
                        let parts: Vec<String> = items.iter().map(|v| self.stringify(v)).collect();
                        return Ok(Value::String(parts.join(sep.as_str())));
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_string_to_lower" => {
                    if let Some(Value::String(s)) = args.first() { return Ok(Value::String(s.to_lowercase())); }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_string_contains" => {
                    if let (Some(Value::String(s)), Some(Value::String(sub))) = (args.first(), args.get(1)) {
                        return Ok(Value::Bool(s.contains(sub.as_str())));
                    }
                    return Ok(Value::Bool(false));
                }
                "fuse_rt_string_starts_with" => {
                    if let (Some(Value::String(s)), Some(Value::String(p))) = (args.first(), args.get(1)) {
                        return Ok(Value::Bool(s.starts_with(p.as_str())));
                    }
                    return Ok(Value::Bool(false));
                }
                "fuse_rt_string_ends_with" => {
                    if let (Some(Value::String(s)), Some(Value::String(p))) = (args.first(), args.get(1)) {
                        return Ok(Value::Bool(s.ends_with(p.as_str())));
                    }
                    return Ok(Value::Bool(false));
                }
                "fuse_rt_string_index_of" => {
                    if let (Some(Value::String(s)), Some(Value::String(sub))) = (args.first(), args.get(1)) {
                        let idx = s.find(sub.as_str()).map(|byte_idx| s[..byte_idx].chars().count() as i64).unwrap_or(-1);
                        return Ok(Value::Int(idx));
                    }
                    return Ok(Value::Int(-1));
                }
                "fuse_rt_string_last_index_of" => {
                    if let (Some(Value::String(s)), Some(Value::String(sub))) = (args.first(), args.get(1)) {
                        let idx = s.rfind(sub.as_str()).map(|byte_idx| s[..byte_idx].chars().count() as i64).unwrap_or(-1);
                        return Ok(Value::Int(idx));
                    }
                    return Ok(Value::Int(-1));
                }
                "fuse_rt_string_trim" => {
                    if let Some(Value::String(s)) = args.first() { return Ok(Value::String(s.trim().to_string())); }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_string_trim_start" => {
                    if let Some(Value::String(s)) = args.first() { return Ok(Value::String(s.trim_start().to_string())); }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_string_trim_end" => {
                    if let Some(Value::String(s)) = args.first() { return Ok(Value::String(s.trim_end().to_string())); }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_string_replace" => {
                    if let (Some(Value::String(s)), Some(Value::String(from)), Some(Value::String(to))) = (args.first(), args.get(1), args.get(2)) {
                        return Ok(Value::String(s.replace(from.as_str(), to.as_str())));
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_string_replace_first" => {
                    if let (Some(Value::String(s)), Some(Value::String(from)), Some(Value::String(to))) = (args.first(), args.get(1), args.get(2)) {
                        return Ok(Value::String(s.replacen(from.as_str(), to.as_str(), 1)));
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_string_split" => {
                    if let (Some(Value::String(s)), Some(Value::String(sep))) = (args.first(), args.get(1)) {
                        let parts: Vec<Value> = s.split(sep.as_str()).map(|p| Value::String(p.to_string())).collect();
                        return Ok(Value::List(parts));
                    }
                    return Ok(Value::List(Vec::new()));
                }
                "fuse_rt_string_to_bytes" => {
                    if let Some(Value::String(s)) = args.first() {
                        let bytes: Vec<Value> = s.as_bytes().iter().map(|b| Value::Int(*b as i64)).collect();
                        return Ok(Value::List(bytes));
                    }
                    return Ok(Value::List(Vec::new()));
                }
                "fuse_rt_string_from_bytes" => {
                    if let Some(Value::List(items)) = args.first() {
                        let bytes: Vec<u8> = items.iter().filter_map(|v| match v { Value::Int(n) => Some(*n as u8), _ => None }).collect();
                        return match String::from_utf8(bytes) {
                            Ok(s) => Ok(Value::Result { is_ok: true, value: Box::new(Value::String(s)) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("string: invalid UTF-8: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("string: expected byte list".to_string())) });
                }
                "fuse_rt_string_from_char_code" => {
                    if let Some(Value::Int(n)) = args.first() {
                        if let Some(ch) = char::from_u32(*n as u32) { return Ok(Value::String(ch.to_string())); }
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_string_chars_list" => {
                    if let Some(Value::String(s)) = args.first() {
                        let chars: Vec<Value> = s.chars().map(|ch| Value::String(ch.to_string())).collect();
                        return Ok(Value::List(chars));
                    }
                    return Ok(Value::List(Vec::new()));
                }
                "fuse_rt_string_reverse" => {
                    if let Some(Value::String(s)) = args.first() { return Ok(Value::String(s.chars().rev().collect())); }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_string_compare" => {
                    if let (Some(Value::String(a)), Some(Value::String(b))) = (args.first(), args.get(1)) {
                        return Ok(Value::Int(match a.cmp(b) { std::cmp::Ordering::Less => -1, std::cmp::Ordering::Equal => 0, std::cmp::Ordering::Greater => 1 }));
                    }
                    return Ok(Value::Int(0));
                }
                "fuse_rt_string_byte_len" => {
                    if let Some(Value::String(s)) = args.first() { return Ok(Value::Int(s.len() as i64)); }
                    return Ok(Value::Int(0));
                }
                "fuse_rt_string_capitalize" => {
                    if let Some(Value::String(s)) = args.first() {
                        let mut chars = s.chars();
                        let result = match chars.next() {
                            Some(first) => format!("{}{}", first.to_uppercase().collect::<String>(), chars.collect::<String>().to_lowercase()),
                            None => String::new(),
                        };
                        return Ok(Value::String(result));
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_float_to_string_scientific" => {
                    let v = Self::extract_float(args.first());
                    let d = match args.get(1) { Some(Value::Int(n)) => *n as usize, _ => 2 };
                    return Ok(Value::String(format!("{v:.prec$e}", prec = d)));
                }
                "fuse_rt_string_slice" => {
                    if let Some(Value::String(s)) = args.first() {
                        let start = match args.get(1) { Some(Value::Int(n)) => *n as usize, _ => 0 };
                        let end = match args.get(2) { Some(Value::Int(n)) => *n as usize, _ => s.len() };
                        let result: String = s.chars().skip(start).take(end.saturating_sub(start)).collect();
                        return Ok(Value::String(result));
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_math_sin" => return Ok(Value::Float(Self::extract_float(args.first()).sin())),
                "fuse_rt_math_cos" => return Ok(Value::Float(Self::extract_float(args.first()).cos())),
                "fuse_rt_math_tan" => return Ok(Value::Float(Self::extract_float(args.first()).tan())),
                "fuse_rt_math_asin" => return Ok(Value::Float(Self::extract_float(args.first()).asin())),
                "fuse_rt_math_acos" => return Ok(Value::Float(Self::extract_float(args.first()).acos())),
                "fuse_rt_math_atan" => return Ok(Value::Float(Self::extract_float(args.first()).atan())),
                "fuse_rt_math_atan2" => return Ok(Value::Float(Self::extract_float(args.first()).atan2(Self::extract_float(args.get(1))))),
                "fuse_rt_math_exp" => return Ok(Value::Float(Self::extract_float(args.first()).exp())),
                "fuse_rt_math_exp2" => return Ok(Value::Float(Self::extract_float(args.first()).exp2())),
                "fuse_rt_math_ln" => return Ok(Value::Float(Self::extract_float(args.first()).ln())),
                "fuse_rt_math_log2" => return Ok(Value::Float(Self::extract_float(args.first()).log2())),
                "fuse_rt_math_log10" => return Ok(Value::Float(Self::extract_float(args.first()).log10())),
                "fuse_rt_math_cbrt" => return Ok(Value::Float(Self::extract_float(args.first()).cbrt())),
                "fuse_rt_math_hypot" => return Ok(Value::Float(Self::extract_float(args.first()).hypot(Self::extract_float(args.get(1))))),
                _ => {}
            }
        }
        let module = self.load_module(module_path)?;
        let base = if let Some(ce) = caller_env {
            ce
        } else {
            let canonical = module.path.clone();
            if let Some(cached) = self.module_envs.get(&canonical) {
                cached.clone()
            } else {
                let env = self.module_env(&module)?;
                self.module_envs.insert(canonical, env.clone());
                env
            }
        };
        let env = Environment::new(Some(base));
        let mut deferred = Vec::new();
        let has_variadic = function.params.last().is_some_and(|p| p.variadic);
        let fixed_count = if has_variadic { function.params.len() - 1 } else { function.params.len() };
        let mut args_iter = args.into_iter();
        for param in function.params.iter().take(fixed_count) {
            if let Some(arg) = args_iter.next() {
                let (value, destroy) = match arg {
                    Value::Moved(value) => (*value, true),
                    value if param.convention.as_deref() == Some("owned") => (Self::deep_clone(&value), true),
                    value => (value, false),
                };
                env.define(param.name.clone(), value, true, destroy);
            }
        }
        if has_variadic {
            if let Some(param) = function.params.last() {
                let rest: Vec<Value> = args_iter.collect();
                env.define(param.name.clone(), Value::List(rest), true, false);
            }
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
            fa::Expr::FString(fstring) => Ok(Value::String(self.render_fstring(&fstring.template, module_path, env))),
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
                if let fa::Expr::Member(member) = call.callee.as_ref() {
                    if let fa::Expr::Name(name) = member.object.as_ref() {
                        if env.resolve(&name.value).is_none() {
                            let base = name.value.split("::").next().unwrap_or(&name.value);
                            if base == "Map" && member.name == "new" {
                                return Ok(Value::Map(Vec::new()));
                            }
                            if let Some(ext) = self.find_extension(&name.value, &member.name) {
                                let mut args = Vec::new();
                                for arg in &call.args {
                                    args.push(self.eval_expr(module_path, arg, env)?);
                                }
                                return self.call_user_function(&ext.module_path, &ext.decl, args, None)
                                    .map_err(ControlFlow::Abort);
                            }
                        }
                    }
                }
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
            Value::List(ref items) => {
                if let Ok(index) = name.parse::<usize>() {
                    if let Some(item) = items.get(index) {
                        return Ok(item.clone());
                    }
                }
                // Use ListMethod for all known list methods to avoid the
                // evaluator's value-semantics limitation with fuse_list_push.
                return Ok(Value::NativeFunction(Rc::new(NativeFunction::ListMethod {
                    receiver: Box::new(object),
                    method: name.to_string(),
                })));
            }
            Value::Map(_) => {
                return Ok(Value::NativeFunction(Rc::new(NativeFunction::MapMethod {
                    receiver: Box::new(object),
                    method: name.to_string(),
                })));
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

    fn extract_float(arg: Option<&Value>) -> f64 {
        match arg {
            Some(Value::Float(v)) => *v,
            Some(Value::Int(v)) => *v as f64,
            _ => 0.0,
        }
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
                NativeFunction::MapMethod { receiver, method } => {
                    match method.as_str() {
                        "set" => {
                            // We can't mutate through Box, but for evaluator simplicity
                            // we return Unit and note this is a known limitation
                            Ok(Value::Unit)
                        }
                        "get" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                let key = self.stringify(args.first().unwrap_or(&Value::Unit));
                                for (k, v) in entries {
                                    if self.stringify(k) == key {
                                        return Ok(v.clone());
                                    }
                                }
                            }
                            Ok(Value::Unit)
                        }
                        "len" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                Ok(Value::Int(entries.len() as i64))
                            } else {
                                Ok(Value::Int(0))
                            }
                        }
                        "isEmpty" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                Ok(Value::Bool(entries.is_empty()))
                            } else {
                                Ok(Value::Bool(true))
                            }
                        }
                        "contains" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                let key = self.stringify(args.first().unwrap_or(&Value::Unit));
                                Ok(Value::Bool(entries.iter().any(|(k, _)| self.stringify(k) == key)))
                            } else {
                                Ok(Value::Bool(false))
                            }
                        }
                        "keys" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                Ok(Value::List(entries.iter().map(|(k, _)| k.clone()).collect()))
                            } else {
                                Ok(Value::List(Vec::new()))
                            }
                        }
                        "values" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                Ok(Value::List(entries.iter().map(|(_, v)| v.clone()).collect()))
                            } else {
                                Ok(Value::List(Vec::new()))
                            }
                        }
                        "entries" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                Ok(Value::List(entries.iter().map(|(k, v)| Value::List(vec![k.clone(), v.clone()])).collect()))
                            } else {
                                Ok(Value::List(Vec::new()))
                            }
                        }
                        "remove" => Ok(Value::Unit),
                        "getOrDefault" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                let key = self.stringify(args.first().unwrap_or(&Value::Unit));
                                for (k, v) in entries {
                                    if self.stringify(k) == key { return Ok(v.clone()); }
                                }
                            }
                            Ok(args.get(1).cloned().unwrap_or(Value::Unit))
                        }
                        "getOrInsert" => {
                            // mutation limited in evaluator — behaves like getOrDefault
                            if let Value::Map(entries) = receiver.as_ref() {
                                let key = self.stringify(args.first().unwrap_or(&Value::Unit));
                                for (k, v) in entries {
                                    if self.stringify(k) == key { return Ok(v.clone()); }
                                }
                            }
                            Ok(args.get(1).cloned().unwrap_or(Value::Unit))
                        }
                        "mapValues" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                let cb = args.into_iter().next().unwrap_or(Value::Unit);
                                let mut result = Vec::new();
                                for (k, v) in entries {
                                    let new_v = self.call_value(module_path, cb.clone(), vec![v.clone()], span)?;
                                    result.push((k.clone(), new_v));
                                }
                                return Ok(Value::Map(result));
                            }
                            Ok(Value::Map(Vec::new()))
                        }
                        "filter" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                let cb = args.into_iter().next().unwrap_or(Value::Unit);
                                let mut result = Vec::new();
                                for (k, v) in entries {
                                    if truthy(&self.call_value(module_path, cb.clone(), vec![k.clone(), v.clone()], span)?) {
                                        result.push((k.clone(), v.clone()));
                                    }
                                }
                                return Ok(Value::Map(result));
                            }
                            Ok(Value::Map(Vec::new()))
                        }
                        "merge" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                let mut result = entries.clone();
                                if let Some(Value::Map(other)) = args.first() {
                                    for (k, v) in other {
                                        let key_str = self.stringify(k);
                                        if let Some(pos) = result.iter().position(|(rk, _)| self.stringify(rk) == key_str) {
                                            result[pos] = (k.clone(), v.clone());
                                        } else {
                                            result.push((k.clone(), v.clone()));
                                        }
                                    }
                                }
                                return Ok(Value::Map(result));
                            }
                            Ok(Value::Map(Vec::new()))
                        }
                        "forEach" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                let cb = args.into_iter().next().unwrap_or(Value::Unit);
                                for (k, v) in entries {
                                    self.call_value(module_path, cb.clone(), vec![k.clone(), v.clone()], span)?;
                                }
                            }
                            Ok(Value::Unit)
                        }
                        "toList" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                Ok(Value::List(entries.iter().map(|(k, v)| Value::List(vec![k.clone(), v.clone()])).collect()))
                            } else { Ok(Value::List(Vec::new())) }
                        }
                        "invert" => {
                            if let Value::Map(entries) = receiver.as_ref() {
                                Ok(Value::Map(entries.iter().map(|(k, v)| (v.clone(), k.clone())).collect()))
                            } else { Ok(Value::Map(Vec::new())) }
                        }
                        other => Err(runtime_error(
                            format!("unsupported Map method `{other}`"),
                            "<eval>", 0, 0,
                        )),
                    }
                }
                NativeFunction::ListMethod { receiver, method } => {
                    if let Value::List(items) = receiver.as_ref() {
                        match method.as_str() {
                            "len" => Ok(Value::Int(items.len() as i64)),
                            "get" => {
                                if let Some(Value::Int(i)) = args.first() {
                                    Ok(items.get(*i as usize).cloned().map(|v| Value::Option(Some(Box::new(v)))).unwrap_or(Value::Option(None)))
                                } else { Ok(Value::Option(None)) }
                            }
                            "isEmpty" => Ok(Value::Bool(items.is_empty())),
                            "first" => Ok(items.first().cloned().map(|v| Value::Option(Some(Box::new(v)))).unwrap_or(Value::Option(None))),
                            "last" => Ok(items.last().cloned().map(|v| Value::Option(Some(Box::new(v)))).unwrap_or(Value::Option(None))),
                            "contains" => {
                                let needle = args.first().cloned().unwrap_or(Value::Unit);
                                Ok(Value::Bool(items.iter().any(|v| value_eq(v, &needle))))
                            }
                            "indexOf" => {
                                let needle = args.first().cloned().unwrap_or(Value::Unit);
                                Ok(items.iter().position(|v| value_eq(v, &needle)).map(|i| Value::Option(Some(Box::new(Value::Int(i as i64))))).unwrap_or(Value::Option(None)))
                            }
                            "count" => {
                                let cb = args.into_iter().next().unwrap_or(Value::Unit);
                                let mut count = 0i64;
                                for item in items { if truthy(&self.call_value(module_path, cb.clone(), vec![item.clone()], span)?) { count += 1; } }
                                Ok(Value::Int(count))
                            }
                            "any" => {
                                let cb = args.into_iter().next().unwrap_or(Value::Unit);
                                for item in items { if truthy(&self.call_value(module_path, cb.clone(), vec![item.clone()], span)?) { return Ok(Value::Bool(true)); } }
                                Ok(Value::Bool(false))
                            }
                            "all" => {
                                let cb = args.into_iter().next().unwrap_or(Value::Unit);
                                for item in items { if !truthy(&self.call_value(module_path, cb.clone(), vec![item.clone()], span)?) { return Ok(Value::Bool(false)); } }
                                Ok(Value::Bool(true))
                            }
                            "map" => {
                                let cb = args.into_iter().next().unwrap_or(Value::Unit);
                                let mut result = Vec::new();
                                for item in items { result.push(self.call_value(module_path, cb.clone(), vec![item.clone()], span)?); }
                                Ok(Value::List(result))
                            }
                            "filter" => {
                                let cb = args.into_iter().next().unwrap_or(Value::Unit);
                                let mut result = Vec::new();
                                for item in items { if truthy(&self.call_value(module_path, cb.clone(), vec![item.clone()], span)?) { result.push(item.clone()); } }
                                Ok(Value::List(result))
                            }
                            "flatMap" => {
                                let cb = args.into_iter().next().unwrap_or(Value::Unit);
                                let mut result = Vec::new();
                                for item in items {
                                    if let Value::List(inner) = self.call_value(module_path, cb.clone(), vec![item.clone()], span)? {
                                        result.extend(inner);
                                    }
                                }
                                Ok(Value::List(result))
                            }
                            "reduce" => {
                                let mut ai = args.into_iter();
                                let mut acc = ai.next().unwrap_or(Value::Unit);
                                let cb = ai.next().unwrap_or(Value::Unit);
                                for item in items { acc = self.call_value(module_path, cb.clone(), vec![acc, item.clone()], span)?; }
                                Ok(acc)
                            }
                            "sorted" => {
                                let mut result = items.clone();
                                result.sort_by(|a, b| {
                                    let a_str = self.stringify(a); let b_str = self.stringify(b);
                                    a_str.cmp(&b_str)
                                });
                                Ok(Value::List(result))
                            }
                            "sortedBy" => {
                                let cb = args.into_iter().next().unwrap_or(Value::Unit);
                                let mut result = items.clone();
                                // Simple insertion sort using the comparator
                                for i in 1..result.len() {
                                    let key = result[i].clone();
                                    let mut j = i;
                                    while j > 0 {
                                        let cmp = self.call_value(module_path, cb.clone(), vec![result[j-1].clone(), key.clone()], span)?;
                                        if matches!(cmp, Value::Int(n) if n > 0) { result[j] = result[j-1].clone(); j -= 1; } else { break; }
                                    }
                                    result[j] = key;
                                }
                                Ok(Value::List(result))
                            }
                            "unique" => {
                                let mut result = Vec::new();
                                for item in items { if !result.iter().any(|v| value_eq(v, item)) { result.push(item.clone()); } }
                                Ok(Value::List(result))
                            }
                            "reversed" => Ok(Value::List(items.iter().rev().cloned().collect())),
                            "slice" => {
                                let s = match args.first() { Some(Value::Int(n)) => *n as usize, _ => 0 };
                                let e = match args.get(1) { Some(Value::Int(n)) => *n as usize, _ => items.len() };
                                Ok(Value::List(items[s.min(items.len())..e.min(items.len())].to_vec()))
                            }
                            "take" => {
                                let n = match args.first() { Some(Value::Int(n)) => *n as usize, _ => 0 };
                                Ok(Value::List(items[..n.min(items.len())].to_vec()))
                            }
                            "drop" => {
                                let n = match args.first() { Some(Value::Int(n)) => *n as usize, _ => 0 };
                                Ok(Value::List(items[n.min(items.len())..].to_vec()))
                            }
                            "concat" => {
                                let mut result = items.clone();
                                if let Some(Value::List(other)) = args.first() { result.extend(other.iter().cloned()); }
                                Ok(Value::List(result))
                            }
                            "join" => {
                                let sep = match args.first() { Some(Value::String(s)) => s.as_str(), _ => "" };
                                let parts: Vec<String> = items.iter().map(|v| self.stringify(v)).collect();
                                Ok(Value::String(parts.join(sep)))
                            }
                            "zip" => {
                                let other = match args.first() { Some(Value::List(o)) => o.clone(), _ => Vec::new() };
                                let result: Vec<Value> = items.iter().zip(other.iter()).map(|(a, b)| Value::List(vec![a.clone(), b.clone()])).collect();
                                Ok(Value::List(result))
                            }
                            "flatten" => {
                                let mut result = Vec::new();
                                for item in items { if let Value::List(inner) = item { result.extend(inner.iter().cloned()); } }
                                Ok(Value::List(result))
                            }
                            "repeat" => {
                                let n = match args.first() { Some(Value::Int(n)) => *n as usize, _ => 0 };
                                let s = self.stringify(receiver);
                                Ok(Value::String(s.repeat(n)))
                            }
                            "push" | "pop" | "insert" | "removeAt" | "removeWhere" | "clear" | "sortInPlace" | "reverseInPlace" => Ok(Value::Unit),
                            other => Err(runtime_error(
                                format!("unsupported List method `{other}`"),
                                "<eval>", 0, 0,
                            )),
                        }
                    } else {
                        Ok(Value::Unit)
                    }
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

    fn render_fstring(&mut self, template: &str, module_path: &Path, env: &Environment) -> String {
        let mut output = String::new();
        let mut rest = template;
        while let Some(start) = rest.find('{') {
            output.push_str(&rest[..start]);
            if let Some(end) = rest[start + 1..].find('}') {
                let expr_str = rest[start + 1..start + 1 + end].trim();
                output.push_str(&self.interpolate(expr_str, module_path, env));
                rest = &rest[start + 2 + end..];
            } else {
                output.push_str(&rest[start..]);
                rest = "";
            }
        }
        output.push_str(rest);
        output
    }

    fn interpolate(&mut self, expr_str: &str, module_path: &Path, env: &Environment) -> String {
        // Parse the interpolated expression as real Fuse code via a wrapper function,
        // then evaluate it through eval_expr so method calls, operators, etc. all work.
        let source = format!("fn __fstr__() => {expr_str}");
        if let Ok(program) = parse_source(&source, "<fstring>") {
            for decl in &program.declarations {
                if let fa::Declaration::Function(func) = decl {
                    if let Some(fa::Statement::Expr(expr_stmt)) = func.body.statements.first() {
                        if let Ok(value) = self.eval_expr(module_path, &expr_stmt.expr, env) {
                            return self.stringify(&value);
                        }
                    }
                }
            }
        }
        String::new()
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
            (Value::Float(left), Value::Float(right)) => Ok(Value::Float(left + right)),
            (Value::Int(left), Value::Float(right)) => Ok(Value::Float(left as f64 + right)),
            (Value::Float(left), Value::Int(right)) => Ok(Value::Float(left + right as f64)),
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
        (Value::Float(left), Value::Float(right)) => {
            // Re-derive the comparison from the i64 comparator by testing (0,1) and (1,0).
            let lt = cmp(0, 1); // true for <, <=
            let gt = cmp(1, 0); // true for >, >=
            let eq = cmp(0, 0); // true for <=, >=
            let result = if left < right { lt } else if left > right { gt } else { eq };
            Ok(Value::Bool(result))
        }
        (Value::Int(left), Value::Float(right)) => {
            let left = left as f64;
            let lt = cmp(0, 1); let gt = cmp(1, 0); let eq = cmp(0, 0);
            Ok(Value::Bool(if left < right { lt } else if left > right { gt } else { eq }))
        }
        (Value::Float(left), Value::Int(right)) => {
            let right = right as f64;
            let lt = cmp(0, 1); let gt = cmp(1, 0); let eq = cmp(0, 0);
            Ok(Value::Bool(if left < right { lt } else if left > right { gt } else { eq }))
        }
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
        fa::Expr::FString(fstring) => {
            // Parse each interpolated expression to extract all referenced names,
            // not just the first dot-separated segment. This ensures ASAP destruction
            // doesn't move variables that are still needed inside f-string expressions.
            let mut names = HashSet::new();
            let mut rest = fstring.template.as_str();
            while let Some(start) = rest.find('{') {
                if let Some(end) = rest[start + 1..].find('}') {
                    let expr_str = rest[start + 1..start + 1 + end].trim();
                    let source = format!("fn __names__() => {expr_str}");
                    if let Ok(program) = parse_source(&source, "<fstring-names>") {
                        for decl in &program.declarations {
                            if let fa::Declaration::Function(func) = decl {
                                if let Some(fa::Statement::Expr(expr_stmt)) = func.body.statements.first() {
                                    names.extend(collect_expr_names(&expr_stmt.expr));
                                }
                            }
                        }
                    }
                    rest = &rest[start + 2 + end..];
                } else {
                    break;
                }
            }
            names
        }
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
        fa::Pattern::Tuple(tuple) => {
            if let Value::List(items) = value {
                let mut bindings = HashMap::new();
                for (i, elem) in tuple.elements.iter().enumerate() {
                    let item = items.get(i).cloned().unwrap_or(Value::Unit);
                    if let Some(inner) = match_pattern(elem, &item) {
                        bindings.extend(inner);
                    } else {
                        return None;
                    }
                }
                Some(bindings)
            } else {
                None
            }
        }
    }
}
