use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::ast::nodes as fa;
use crate::codegen;
use crate::common::resolve_import_path;
use crate::fstring::{parse_fstring_template, FStringPart};
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
    Enum { type_name: String, variant: String, payloads: Vec<Value> },
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
    enums: HashMap<String, fa::EnumDecl>,
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
    recursion_depth: usize,
}

impl Evaluator {
    fn new(root_path: PathBuf, root_source: String) -> Self {
        Self {
            root_path: root_path.canonicalize().unwrap_or(root_path),
            root_source,
            modules: HashMap::new(),
            module_envs: HashMap::new(),
            stdout: Vec::new(),
            recursion_depth: 0,
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
            .find(|function| function.annotations.iter().any(|a| a.is("entrypoint")))
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
            enums: HashMap::new(),
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
                fa::Declaration::Enum(enum_decl) => {
                    module.enums.insert(enum_decl.name.clone(), enum_decl);
                }
                fa::Declaration::ExternFn(ext) => {
                    if ext.is_pub || true {
                        module.functions.insert(ext.name.clone(), fa::FunctionDecl {
                            name: ext.name.clone(),
                            type_params: Vec::new(),
                            params: ext.params.clone(),
                            return_type: ext.return_type.clone(),
                            body: fa::Block { statements: Vec::new(), span: ext.span },
                            is_pub: ext.is_pub,
                            annotations: Vec::new(),
                            receiver_type: None,
                            span: ext.span,
                        });
                    }
                }
                fa::Declaration::Struct(struct_decl) => {
                    // Treat structs like data classes in the evaluator — same constructor
                    // and method resolution. Opaqueness is enforced by the checker, not runtime.
                    let as_data = fa::DataClassDecl {
                        name: struct_decl.name.clone(),
                        type_params: struct_decl.type_params.clone(),
                        fields: struct_decl.fields.clone(),
                        methods: struct_decl.methods.clone(),
                        is_pub: struct_decl.is_pub,
                        annotations: struct_decl.annotations.clone(),
                        implements: struct_decl.implements.clone(),
                        span: struct_decl.span,
                    };
                    let methods = as_data
                        .methods
                        .iter()
                        .map(|method| (method.name.clone(), method.clone()))
                        .collect::<HashMap<_, _>>();
                    if as_data.is_pub {
                        module
                            .exports
                            .insert(as_data.name.clone(), ExportSymbol::Data(as_data.clone()));
                    }
                    module.data_defs.insert(
                        as_data.name.clone(),
                        DataDef {
                            decl: as_data,
                            methods,
                        },
                    );
                }
                fa::Declaration::Const(_) => {}
                fa::Declaration::Interface(_) => {}
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
                // Bare import: make all public symbols available directly.
                let mut exports = HashMap::new();
                for (name, export) in &imported.exports {
                    let value = self.export_value(&imported.path, export);
                    env.define(name, value.clone(), false, false);
                    exports.insert(name.clone(), value);
                }
                // Also define module namespace for qualified access (e.g., crypto.sha256).
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
            Value::Enum { type_name, variant, payloads } => {
                if payloads.is_empty() {
                    variant.clone()
                } else {
                    format!("{}({})", variant, payloads.iter().map(|p| self.stringify(p)).collect::<Vec<_>>().join(", "))
                }
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
            Value::Enum { type_name, variant, payloads } => Value::Enum {
                type_name: type_name.clone(),
                variant: variant.clone(),
                payloads: payloads.iter().map(Self::deep_clone).collect(),
            },
            Value::Moved(value) => Value::Moved(Box::new(Self::deep_clone(value))),
            Value::Unit => Value::Unit,
        }
    }


    /// FFI dispatch extracted from call_user_function to reduce stack frame
    /// size (Bug #11 proper fix). Each branch handles one extern fn stub.
    #[inline(never)]
    fn dispatch_ffi(&self, name: &str, args: &[Value]) -> Option<Result<Value, RuntimeError>> {
        // Use a closure so that `return Ok(...)` inside match arms
        // exits the closure, not the outer function.
        let mut handled = true;
        let result = (|| {
            match name {
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
                        return Ok(Value::Int(s.len() as i64));
                    }
                    return Ok(Value::Int(0));
                }
                "fuse_rt_string_char_count" => {
                    if let Some(Value::String(s)) = args.first() {
                        return Ok(Value::Int(s.chars().count() as i64));
                    }
                    return Ok(Value::Int(0));
                }
                "fuse_rt_string_byte_at" => {
                    if let Some(Value::String(s)) = args.first() {
                        if let Some(Value::Int(i)) = args.get(1) {
                            let idx = *i as usize;
                            if idx < s.len() {
                                return Ok(Value::Int(s.as_bytes()[idx] as i64));
                            }
                        }
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
                "fuse_rt_os_exists" => {
                    if let Some(Value::String(p)) = args.first() {
                        return Ok(Value::Bool(std::path::Path::new(p.as_str()).exists()));
                    }
                    return Ok(Value::Bool(false));
                }
                "fuse_rt_os_is_file" => {
                    if let Some(Value::String(p)) = args.first() {
                        return Ok(Value::Bool(std::path::Path::new(p.as_str()).is_file()));
                    }
                    return Ok(Value::Bool(false));
                }
                "fuse_rt_os_is_dir" => {
                    if let Some(Value::String(p)) = args.first() {
                        return Ok(Value::Bool(std::path::Path::new(p.as_str()).is_dir()));
                    }
                    return Ok(Value::Bool(false));
                }
                "fuse_rt_os_stat" => {
                    if let Some(Value::String(p)) = args.first() {
                        return match std::fs::metadata(p.as_str()) {
                            Ok(meta) => {
                                use std::time::UNIX_EPOCH;
                                let kind = if meta.is_file() { 0i64 } else if meta.is_dir() { 1 } else { 3 };
                                let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
                                let created = meta.created().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
                                Ok(Value::Result { is_ok: true, value: Box::new(Value::Data(Rc::new(RefCell::new(DataInstance {
                                    module_path: PathBuf::new(), type_name: "FileInfo".to_string(),
                                    fields: [("path", Value::String(p.clone())), ("kind", Value::Int(kind)), ("size", Value::Int(meta.len() as i64)),
                                             ("modifiedAt", Value::Int(modified)), ("createdAt", Value::Int(created)), ("isReadOnly", Value::Bool(meta.permissions().readonly()))]
                                        .into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                                    field_order: vec!["path","kind","size","modifiedAt","createdAt","isReadOnly"].into_iter().map(String::from).collect(),
                                    methods: HashMap::new(), destroyed: false,
                                })))) })
                            }
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("os: expected path".to_string())) });
                }
                "fuse_rt_os_read_dir" => {
                    if let Some(Value::String(p)) = args.first() {
                        return match std::fs::read_dir(p.as_str()) {
                            Ok(entries) => {
                                use std::time::UNIX_EPOCH;
                                let mut items = Vec::new();
                                for entry in entries.flatten() {
                                    let name = entry.file_name().to_string_lossy().to_string();
                                    let path = entry.path().to_string_lossy().to_string();
                                    let meta = entry.metadata().ok();
                                    let kind = meta.as_ref().map(|m| if m.is_file() { 0i64 } else if m.is_dir() { 1 } else { 3 }).unwrap_or(3);
                                    let size = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
                                    let modified = meta.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
                                    items.push(Value::Data(Rc::new(RefCell::new(DataInstance {
                                        module_path: PathBuf::new(), type_name: "DirEntry".to_string(),
                                        fields: [("name", Value::String(name)), ("path", Value::String(path)), ("kind", Value::Int(kind)),
                                                 ("size", Value::Int(size)), ("modifiedAt", Value::Int(modified))]
                                            .into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                                        field_order: vec!["name","path","kind","size","modifiedAt"].into_iter().map(String::from).collect(),
                                        methods: HashMap::new(), destroyed: false,
                                    }))));
                                }
                                Ok(Value::Result { is_ok: true, value: Box::new(Value::List(items)) })
                            }
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("os: expected path".to_string())) });
                }
                "fuse_rt_os_mkdir" => {
                    if let Some(Value::String(p)) = args.first() {
                        return match std::fs::create_dir(p.as_str()) {
                            Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("os: expected path".to_string())) });
                }
                "fuse_rt_os_mkdir_all" => {
                    if let Some(Value::String(p)) = args.first() {
                        return match std::fs::create_dir_all(p.as_str()) {
                            Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("os: expected path".to_string())) });
                }
                "fuse_rt_os_create_file" => {
                    if let Some(Value::String(p)) = args.first() {
                        return match std::fs::OpenOptions::new().write(true).create_new(true).open(p.as_str()) {
                            Ok(_) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("os: expected path".to_string())) });
                }
                "fuse_rt_os_copy_file" => {
                    if let (Some(Value::String(s)), Some(Value::String(d))) = (args.first(), args.get(1)) {
                        return match std::fs::copy(s.as_str(), d.as_str()) {
                            Ok(_) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("os: expected src and dst".to_string())) });
                }
                "fuse_rt_os_copy_dir" | "fuse_rt_os_rename" => {
                    if let (Some(Value::String(s)), Some(Value::String(d))) = (args.first(), args.get(1)) {
                        let result = if name == "fuse_rt_os_rename" {
                            std::fs::rename(s.as_str(), d.as_str())
                        } else {
                            fn copy_rec(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
                                std::fs::create_dir_all(dst)?;
                                for e in std::fs::read_dir(src)? { let e = e?; let t = dst.join(e.file_name());
                                    if e.file_type()?.is_dir() { copy_rec(&e.path(), &t)?; } else { std::fs::copy(e.path(), t)?; } } Ok(()) }
                            copy_rec(std::path::Path::new(s.as_str()), std::path::Path::new(d.as_str()))
                        };
                        return match result {
                            Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("os: expected src and dst".to_string())) });
                }
                "fuse_rt_os_remove_file" => {
                    if let Some(Value::String(p)) = args.first() {
                        return match std::fs::remove_file(p.as_str()) {
                            Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("os: expected path".to_string())) });
                }
                "fuse_rt_os_remove_dir" => {
                    if let Some(Value::String(p)) = args.first() {
                        return match std::fs::remove_dir(p.as_str()) {
                            Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("os: expected path".to_string())) });
                }
                "fuse_rt_os_remove_dir_all" => {
                    if let Some(Value::String(p)) = args.first() {
                        return match std::fs::remove_dir_all(p.as_str()) {
                            Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("os: expected path".to_string())) });
                }
                "fuse_rt_os_read_dir_recursive" => {
                    if let Some(Value::String(p)) = args.first() {
                        use std::time::UNIX_EPOCH;
                        fn walk(dir: &std::path::Path, items: &mut Vec<Value>) -> std::io::Result<()> {
                            for entry in std::fs::read_dir(dir)? {
                                let entry = entry?;
                                let name = entry.file_name().to_string_lossy().to_string();
                                let path = entry.path().to_string_lossy().to_string();
                                let meta = entry.metadata().ok();
                                let kind = meta.as_ref().map(|m| if m.is_file() { 0i64 } else if m.is_dir() { 1 } else { 3 }).unwrap_or(3);
                                let size = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
                                let modified = meta.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
                                items.push(Value::Data(Rc::new(RefCell::new(DataInstance {
                                    module_path: PathBuf::new(), type_name: "DirEntry".to_string(),
                                    fields: [("name", Value::String(name)), ("path", Value::String(path)), ("kind", Value::Int(kind)),
                                             ("size", Value::Int(size)), ("modifiedAt", Value::Int(modified))]
                                        .into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                                    field_order: vec!["name","path","kind","size","modifiedAt"].into_iter().map(String::from).collect(),
                                    methods: HashMap::new(), destroyed: false,
                                }))));
                                if entry.file_type()?.is_dir() { walk(&entry.path(), items)?; }
                            }
                            Ok(())
                        }
                        let mut items = Vec::new();
                        return match walk(std::path::Path::new(p.as_str()), &mut items) {
                            Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::List(items)) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("os: expected path".to_string())) });
                }
                "fuse_rt_os_move" => {
                    if let (Some(Value::String(s)), Some(Value::String(d))) = (args.first(), args.get(1)) {
                        // Try rename first
                        if std::fs::rename(s.as_str(), d.as_str()).is_ok() {
                            return Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) });
                        }
                        // Fallback: copy + remove
                        let src_path = std::path::Path::new(s.as_str());
                        let result = if src_path.is_dir() {
                            fn copy_rec(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
                                std::fs::create_dir_all(dst)?;
                                for e in std::fs::read_dir(src)? { let e = e?; let t = dst.join(e.file_name());
                                    if e.file_type()?.is_dir() { copy_rec(&e.path(), &t)?; } else { std::fs::copy(e.path(), t)?; } } Ok(()) }
                            copy_rec(src_path, std::path::Path::new(d.as_str())).and_then(|()| std::fs::remove_dir_all(src_path))
                        } else {
                            std::fs::copy(s.as_str(), d.as_str()).map(|_| ()).and_then(|()| std::fs::remove_file(s.as_str()))
                        };
                        return match result {
                            Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("os: expected src and dst".to_string())) });
                }
                "fuse_rt_env_get" => {
                    if let Some(Value::String(key)) = args.first() {
                        return match std::env::var(key.as_str()) {
                            Ok(val) => Ok(Value::Option(Some(Box::new(Value::String(val))))),
                            Err(_) => Ok(Value::Option(None)),
                        };
                    }
                    return Ok(Value::Option(None));
                }
                "fuse_rt_env_set" => {
                    if let (Some(Value::String(k)), Some(Value::String(v))) = (args.first(), args.get(1)) {
                        unsafe { std::env::set_var(k.as_str(), v.as_str()); }
                        return Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) });
                    }
                    return Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) });
                }
                "fuse_rt_env_remove" => {
                    if let Some(Value::String(k)) = args.first() {
                        unsafe { std::env::remove_var(k.as_str()); }
                    }
                    return Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) });
                }
                "fuse_rt_env_all" => {
                    let entries: Vec<(Value, Value)> = std::env::vars()
                        .map(|(k, v)| (Value::String(k), Value::String(v))).collect();
                    return Ok(Value::Map(entries));
                }
                "fuse_rt_env_has" => {
                    if let Some(Value::String(key)) = args.first() {
                        return Ok(Value::Bool(std::env::var(key.as_str()).is_ok()));
                    }
                    return Ok(Value::Bool(false));
                }
                "fuse_rt_http_get" | "fuse_rt_http_post" | "fuse_rt_http_post_json"
                | "fuse_rt_http_put" | "fuse_rt_http_delete" | "fuse_rt_http_request" => {
                    // HTTP in evaluator: real requests via ureq
                    let url = if let Some(Value::String(u)) = args.first() { u.clone() } else { return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("http: expected url".to_string())) }); };
                    let body = args.get(1).and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None });
                    let method = match name {
                        "fuse_rt_http_get" => "GET",
                        "fuse_rt_http_post" | "fuse_rt_http_post_json" => "POST",
                        "fuse_rt_http_put" => "PUT",
                        "fuse_rt_http_delete" => "DELETE",
                        "fuse_rt_http_request" => if let Some(Value::String(m)) = args.first() { return Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }); } else { "GET" },
                        _ => "GET",
                    };
                    let mut request = match method {
                        "POST" => ureq::post(&url),
                        "PUT" => ureq::put(&url),
                        "DELETE" => ureq::delete(&url),
                        _ => ureq::get(&url),
                    };
                    if name == "fuse_rt_http_post_json" {
                        request = request.set("Content-Type", "application/json");
                    }
                    let result = if let Some(b) = &body { request.send_string(b) } else { request.call() };
                    return match result {
                        Ok(response) => {
                            let status = response.status() as i64;
                            let body_str = response.into_string().unwrap_or_default();
                            Ok(Value::Result { is_ok: true, value: Box::new(Value::Data(Rc::new(RefCell::new(DataInstance {
                                module_path: PathBuf::new(), type_name: "Response".to_string(),
                                fields: [("status", Value::Int(status)), ("headers", Value::Map(Vec::new())), ("body", Value::String(body_str))]
                                    .into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                                field_order: vec!["status","headers","body"].into_iter().map(String::from).collect(),
                                methods: HashMap::new(), destroyed: false,
                            })))) })
                        }
                        Err(ureq::Error::Status(code, response)) => {
                            let body_str = response.into_string().unwrap_or_default();
                            Ok(Value::Result { is_ok: true, value: Box::new(Value::Data(Rc::new(RefCell::new(DataInstance {
                                module_path: PathBuf::new(), type_name: "Response".to_string(),
                                fields: [("status", Value::Int(code as i64)), ("headers", Value::Map(Vec::new())), ("body", Value::String(body_str))]
                                    .into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                                field_order: vec!["status","headers","body"].into_iter().map(String::from).collect(),
                                methods: HashMap::new(), destroyed: false,
                            })))) })
                        }
                        Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("http: {e}"))) }),
                    };
                }
                "fuse_rt_json_parse" => {
                    if let Some(Value::String(s)) = args.first() {
                        fn parse_value(s: &str, pos: &mut usize) -> Result<Value, String> {
                            skip_ws(s, pos);
                            if *pos >= s.len() { return Err("unexpected end".to_string()); }
                            match s.as_bytes()[*pos] {
                                b'"' => { let st = parse_str(s, pos)?; Ok(make_jv(3, Value::String(st))) }
                                b't' => { if s[*pos..].starts_with("true") { *pos+=4; Ok(make_jv(1, Value::Bool(true))) } else { Err(format!("unexpected at {}", *pos)) } }
                                b'f' => { if s[*pos..].starts_with("false") { *pos+=5; Ok(make_jv(1, Value::Bool(false))) } else { Err(format!("unexpected at {}", *pos)) } }
                                b'n' => { if s[*pos..].starts_with("null") { *pos+=4; Ok(make_jv(0, Value::Unit)) } else { Err(format!("unexpected at {}", *pos)) } }
                                b'-' | b'0'..=b'9' => {
                                    let start = *pos;
                                    if s.as_bytes()[*pos]==b'-' { *pos+=1; }
                                    while *pos<s.len() && s.as_bytes()[*pos].is_ascii_digit() { *pos+=1; }
                                    if *pos<s.len() && s.as_bytes()[*pos]==b'.' { *pos+=1; while *pos<s.len() && s.as_bytes()[*pos].is_ascii_digit() { *pos+=1; } }
                                    if *pos<s.len() && matches!(s.as_bytes()[*pos], b'e'|b'E') { *pos+=1; if *pos<s.len() && matches!(s.as_bytes()[*pos], b'+'|b'-') { *pos+=1; } while *pos<s.len() && s.as_bytes()[*pos].is_ascii_digit() { *pos+=1; } }
                                    let n: f64 = s[start..*pos].parse().map_err(|e| format!("{e}"))?;
                                    Ok(make_jv(2, Value::Float(n)))
                                }
                                b'[' => {
                                    *pos+=1; let mut items = Vec::new();
                                    skip_ws(s, pos);
                                    if *pos<s.len() && s.as_bytes()[*pos]==b']' { *pos+=1; return Ok(make_jv(4, Value::List(items))); }
                                    loop {
                                        items.push(parse_value(s, pos)?);
                                        skip_ws(s, pos);
                                        if *pos>=s.len() { return Err("unterminated array".to_string()); }
                                        if s.as_bytes()[*pos]==b']' { *pos+=1; break; }
                                        if s.as_bytes()[*pos]!=b',' { return Err(format!("expected ',' at {}", *pos)); }
                                        *pos+=1;
                                    }
                                    Ok(make_jv(4, Value::List(items)))
                                }
                                b'{' => {
                                    *pos+=1; let mut entries = Vec::new();
                                    skip_ws(s, pos);
                                    if *pos<s.len() && s.as_bytes()[*pos]==b'}' { *pos+=1; return Ok(make_jv(5, Value::Map(entries))); }
                                    loop {
                                        skip_ws(s, pos);
                                        let key = parse_str(s, pos)?;
                                        skip_ws(s, pos);
                                        if *pos>=s.len() || s.as_bytes()[*pos]!=b':' { return Err(format!("expected ':' at {}", *pos)); }
                                        *pos+=1;
                                        let val = parse_value(s, pos)?;
                                        entries.push((Value::String(key), val));
                                        skip_ws(s, pos);
                                        if *pos>=s.len() { return Err("unterminated object".to_string()); }
                                        if s.as_bytes()[*pos]==b'}' { *pos+=1; break; }
                                        if s.as_bytes()[*pos]!=b',' { return Err(format!("expected ',' at {}", *pos)); }
                                        *pos+=1;
                                    }
                                    Ok(make_jv(5, Value::Map(entries)))
                                }
                                c => Err(format!("unexpected '{}' at {}", c as char, *pos))
                            }
                        }
                        fn skip_ws(s: &str, pos: &mut usize) { while *pos<s.len() && matches!(s.as_bytes()[*pos], b' '|b'\t'|b'\n'|b'\r') { *pos+=1; } }
                        fn parse_str(s: &str, pos: &mut usize) -> Result<String, String> {
                            if *pos>=s.len() || s.as_bytes()[*pos]!=b'"' { return Err(format!("expected '\"' at {}", *pos)); }
                            *pos+=1; let mut r = String::new();
                            while *pos<s.len() { let c=s.as_bytes()[*pos]; if c==b'"' { *pos+=1; return Ok(r); }
                                if c==b'\\' { *pos+=1; if *pos>=s.len() { return Err("escape".to_string()); }
                                    match s.as_bytes()[*pos] { b'"'=>r.push('"'), b'\\'=>r.push('\\'), b'n'=>r.push('\n'), b'r'=>r.push('\r'), b't'=>r.push('\t'), b'/'=>r.push('/'), c=>r.push(c as char) } }
                                else { r.push(c as char); } *pos+=1; } Err("unterminated string".to_string())
                        }
                        fn make_jv(tag: i64, val: Value) -> Value {
                            use std::cell::RefCell; use std::rc::Rc; use std::collections::HashMap; use std::path::PathBuf;
                            Value::Data(Rc::new(RefCell::new(DataInstance {
                                module_path: PathBuf::new(), type_name: "JsonValue".to_string(),
                                fields: [("tag", Value::Int(tag)), ("value", val)].into_iter().map(|(k,v)|(k.to_string(),v)).collect(),
                                field_order: vec!["tag".to_string(),"value".to_string()],
                                methods: HashMap::new(), destroyed: false,
                            })))
                        }
                        let mut pos = 0usize;
                        return match parse_value(s.as_str(), &mut pos) {
                            Ok(v) => Ok(Value::Result { is_ok: true, value: Box::new(v) }),
                            Err(msg) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(msg)) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("json: expected string".to_string())) });
                }
                "fuse_rt_json_stringify" | "fuse_rt_json_stringify_pretty" => {
                    fn stringify_jv(v: &Value, pretty: bool, indent: usize, depth: usize) -> String {
                        if let Value::Data(rc) = v {
                            let data = rc.borrow();
                            let tag = match data.fields.get("tag") { Some(Value::Int(n)) => *n, _ => -1 };
                            let val = data.fields.get("value").cloned().unwrap_or(Value::Unit);
                            return match tag {
                                0 => "null".to_string(),
                                1 => match val { Value::Bool(b) => b.to_string(), _ => "false".to_string() },
                                2 => match val { Value::Float(f) => { if f==f.floor() && f.is_finite() { format!("{f:.1}") } else { f.to_string() } }, Value::Int(n) => format!("{n}.0"), _ => "0".to_string() },
                                3 => match val { Value::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")), _ => "\"\"".to_string() },
                                4 => {
                                    if let Value::List(items) = val { if items.is_empty() { return "[]".to_string(); }
                                        let parts: Vec<String> = items.iter().map(|i| stringify_jv(i, pretty, indent, depth+1)).collect();
                                        if pretty { let pad=" ".repeat(indent*(depth+1)); let pc=" ".repeat(indent*depth); format!("[\n{pad}{}\n{pc}]", parts.join(&format!(",\n{pad}"))) }
                                        else { format!("[{}]", parts.join(",")) }
                                    } else { "[]".to_string() }
                                }
                                5 => {
                                    if let Value::Map(entries) = val { if entries.is_empty() { return "{}".to_string(); }
                                        let parts: Vec<String> = entries.iter().map(|(k,v)| { let ks = match k { Value::String(s)=>s.clone(), _=>String::new() };
                                            let vs = stringify_jv(v, pretty, indent, depth+1);
                                            if pretty { format!("\"{ks}\": {vs}") } else { format!("\"{ks}\":{vs}") }
                                        }).collect();
                                        if pretty { let pad=" ".repeat(indent*(depth+1)); let pc=" ".repeat(indent*depth); format!("{{\n{pad}{}\n{pc}}}", parts.join(&format!(",\n{pad}"))) }
                                        else { format!("{{{}}}", parts.join(",")) }
                                    } else { "{}".to_string() }
                                }
                                _ => "null".to_string(),
                            };
                        }
                        "null".to_string()
                    }
                    let pretty = name == "fuse_rt_json_stringify_pretty";
                    let indent = if pretty { if let Some(Value::Int(n)) = args.get(1) { *n as usize } else { 2 } } else { 0 };
                    if let Some(v) = args.first() {
                        return Ok(Value::String(stringify_jv(v, pretty, indent, 0)));
                    }
                    return Ok(Value::String("null".to_string()));
                }
                "fuse_rt_net_tcp_connect" | "fuse_rt_net_tcp_connect_timeout" => {
                    // Evaluator: attempt real connection for testing
                    if let (Some(Value::String(addr)), Some(Value::Int(port))) = (args.first(), args.get(1)) {
                        return match std::net::TcpStream::connect((addr.as_str(), *port as u16)) {
                            Ok(_stream) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("net: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("net: expected addr and port".to_string())) });
                }
                "fuse_rt_net_tcp_bind" => {
                    if let (Some(Value::String(addr)), Some(Value::Int(port))) = (args.first(), args.get(1)) {
                        return match std::net::TcpListener::bind((addr.as_str(), *port as u16)) {
                            Ok(_listener) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("net: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("net: expected addr and port".to_string())) });
                }
                "fuse_rt_net_udp_bind" => {
                    if let (Some(Value::String(addr)), Some(Value::Int(port))) = (args.first(), args.get(1)) {
                        return match std::net::UdpSocket::bind((addr.as_str(), *port as u16)) {
                            Ok(_socket) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("net: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("net: expected addr and port".to_string())) });
                }
                "fuse_rt_net_tcp_read" | "fuse_rt_net_tcp_read_all" | "fuse_rt_net_tcp_write"
                | "fuse_rt_net_tcp_write_bytes" | "fuse_rt_net_tcp_flush"
                | "fuse_rt_net_tcp_set_read_timeout" | "fuse_rt_net_tcp_set_write_timeout"
                | "fuse_rt_net_tcp_local_addr" | "fuse_rt_net_tcp_peer_addr"
                | "fuse_rt_net_tcp_close" | "fuse_rt_net_tcp_accept"
                | "fuse_rt_net_tcp_listener_local_addr" | "fuse_rt_net_tcp_listener_close"
                | "fuse_rt_net_udp_send_to" | "fuse_rt_net_udp_recv_from"
                | "fuse_rt_net_udp_set_broadcast" | "fuse_rt_net_udp_close" => {
                    // Opaque handle operations — evaluator returns stub results
                    return Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) });
                }
                "fuse_rt_process_run" => {
                    if let (Some(Value::String(prog)), Some(Value::List(args_list))) = (args.first(), args.get(1)) {
                        let mut cmd = std::process::Command::new(prog.as_str());
                        for arg in args_list { if let Value::String(s) = arg { cmd.arg(s.as_str()); } }
                        return match cmd.output() {
                            Ok(output) => {
                                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                                let code = output.status.code().unwrap_or(-1) as i64;
                                Ok(Value::Result { is_ok: true, value: Box::new(Value::Data(Rc::new(RefCell::new(DataInstance {
                                    module_path: PathBuf::new(), type_name: "Output".to_string(),
                                    fields: [("stdout", Value::String(stdout)), ("stderr", Value::String(stderr)),
                                             ("exitCode", Value::Int(code)), ("success", Value::Bool(output.status.success()))]
                                        .into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                                    field_order: vec!["stdout","stderr","exitCode","success"].into_iter().map(String::from).collect(),
                                    methods: HashMap::new(), destroyed: false,
                                })))) })
                            }
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("process: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("process: expected program and args".to_string())) });
                }
                "fuse_rt_process_shell" => {
                    if let Some(Value::String(cmd_str)) = args.first() {
                        let result = if cfg!(windows) {
                            std::process::Command::new("cmd.exe").args(["/C", cmd_str.as_str()]).output()
                        } else {
                            std::process::Command::new("sh").args(["-c", cmd_str.as_str()]).output()
                        };
                        return match result {
                            Ok(output) => {
                                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                                let code = output.status.code().unwrap_or(-1) as i64;
                                Ok(Value::Result { is_ok: true, value: Box::new(Value::Data(Rc::new(RefCell::new(DataInstance {
                                    module_path: PathBuf::new(), type_name: "Output".to_string(),
                                    fields: [("stdout", Value::String(stdout)), ("stderr", Value::String(stderr)),
                                             ("exitCode", Value::Int(code)), ("success", Value::Bool(output.status.success()))]
                                        .into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                                    field_order: vec!["stdout","stderr","exitCode","success"].into_iter().map(String::from).collect(),
                                    methods: HashMap::new(), destroyed: false,
                                })))) })
                            }
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("process: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("process: expected command".to_string())) });
                }
                "fuse_rt_process_run_with_stdin" => {
                    // Simplified evaluator: just delegate to fuse_rt_process_run logic
                    if let (Some(Value::String(prog)), Some(Value::List(args_list))) = (args.first(), args.get(1)) {
                        let mut cmd = std::process::Command::new(prog.as_str());
                        for arg in args_list { if let Value::String(s) = arg { cmd.arg(s.as_str()); } }
                        // cwd
                        if let Some(Value::String(cwd)) = args.get(3) {
                            if !cwd.is_empty() { cmd.current_dir(cwd.as_str()); }
                        }
                        // stdin
                        let stdin_str = if let Some(Value::String(s)) = args.get(2) { s.clone() } else { String::new() };
                        if !stdin_str.is_empty() { cmd.stdin(std::process::Stdio::piped()); }
                        return match cmd.spawn() {
                            Ok(mut child) => {
                                if !stdin_str.is_empty() {
                                    use std::io::Write;
                                    if let Some(ref mut stdin) = child.stdin { let _ = stdin.write_all(stdin_str.as_bytes()); }
                                    child.stdin.take();
                                }
                                match child.wait_with_output() {
                                    Ok(output) => {
                                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                                        let code = output.status.code().unwrap_or(-1) as i64;
                                        Ok(Value::Result { is_ok: true, value: Box::new(Value::Data(Rc::new(RefCell::new(DataInstance {
                                            module_path: PathBuf::new(), type_name: "Output".to_string(),
                                            fields: [("stdout", Value::String(stdout)), ("stderr", Value::String(stderr)),
                                                     ("exitCode", Value::Int(code)), ("success", Value::Bool(output.status.success()))]
                                                .into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                                            field_order: vec!["stdout","stderr","exitCode","success"].into_iter().map(String::from).collect(),
                                            methods: HashMap::new(), destroyed: false,
                                        })))) })
                                    }
                                    Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("process: {e}"))) }),
                                }
                            }
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("process: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("process: expected args".to_string())) });
                }
                "fuse_rt_random_new" => {
                    let seed = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as i64;
                    return Ok(Value::Int(seed));
                }
                "fuse_rt_random_seeded" => {
                    if let Some(Value::Int(s)) = args.first() { return Ok(Value::Int(*s)); }
                    return Ok(Value::Int(0));
                }
                "fuse_rt_random_next_int" => {
                    fn splitmix64(state: i64) -> (i64, i64) {
                        let s = state.wrapping_add(0x9e3779b97f4a7c15_u64 as i64);
                        let mut z = s;
                        z = (z ^ (z as u64 >> 30) as i64).wrapping_mul(0xbf58476d1ce4e5b9_u64 as i64);
                        z = (z ^ (z as u64 >> 27) as i64).wrapping_mul(0x94d049bb133111eb_u64 as i64);
                        z = z ^ (z as u64 >> 31) as i64;
                        (s, z)
                    }
                    let s = if let Some(Value::Int(n)) = args.first() { *n } else { 0 };
                    let (ns, val) = splitmix64(s);
                    return Ok(Value::List(vec![Value::Int(ns), Value::Int(val)]));
                }
                "fuse_rt_random_next_float" => {
                    fn splitmix64(state: i64) -> (i64, i64) {
                        let s = state.wrapping_add(0x9e3779b97f4a7c15_u64 as i64);
                        let mut z = s;
                        z = (z ^ (z as u64 >> 30) as i64).wrapping_mul(0xbf58476d1ce4e5b9_u64 as i64);
                        z = (z ^ (z as u64 >> 27) as i64).wrapping_mul(0x94d049bb133111eb_u64 as i64);
                        z = z ^ (z as u64 >> 31) as i64;
                        (s, z)
                    }
                    let s = if let Some(Value::Int(n)) = args.first() { *n } else { 0 };
                    let (ns, val) = splitmix64(s);
                    let f = ((val as u64) >> 11) as f64 / (1u64 << 53) as f64;
                    return Ok(Value::List(vec![Value::Int(ns), Value::Float(f)]));
                }
                "fuse_rt_time_instant_now" => {
                    thread_local! { static BASE: std::time::Instant = std::time::Instant::now(); }
                    let n = BASE.with(|base| base.elapsed().as_nanos() as i64);
                    return Ok(Value::Int(n));
                }
                "fuse_rt_time_system_now" => {
                    let secs = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
                    return Ok(Value::Int(secs));
                }
                "fuse_rt_time_elapsed_nanos" => {
                    thread_local! { static BASE: std::time::Instant = std::time::Instant::now(); }
                    let now = BASE.with(|base| base.elapsed().as_nanos() as i64);
                    let start = if let Some(Value::Int(n)) = args.first() { *n } else { 0 };
                    return Ok(Value::Int(now - start));
                }
                "fuse_rt_sys_args" => {
                    let items: Vec<Value> = std::env::args().map(Value::String).collect();
                    return Ok(Value::List(items));
                }
                "fuse_rt_sys_exit" => {
                    let code = if let Some(Value::Int(n)) = args.first() { *n as i32 } else { 1 };
                    std::process::exit(code);
                }
                "fuse_rt_sys_cwd" => {
                    return match std::env::current_dir() {
                        Ok(p) => Ok(Value::Result { is_ok: true, value: Box::new(Value::String(p.to_string_lossy().to_string())) }),
                        Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("sys: {e}"))) }),
                    };
                }
                "fuse_rt_sys_set_cwd" => {
                    if let Some(Value::String(p)) = args.first() {
                        return match std::env::set_current_dir(p.as_str()) {
                            Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) }),
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("sys: {e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("sys: expected path".to_string())) });
                }
                "fuse_rt_sys_pid" => { return Ok(Value::Int(std::process::id() as i64)); }
                "fuse_rt_sys_platform" => {
                    let p = if cfg!(target_os = "windows") { "windows" } else if cfg!(target_os = "macos") { "macos" } else if cfg!(target_os = "linux") { "linux" } else { "unknown" };
                    return Ok(Value::String(p.to_string()));
                }
                "fuse_rt_sys_arch" => {
                    let a = if cfg!(target_arch = "x86_64") { "x86_64" } else if cfg!(target_arch = "aarch64") { "aarch64" } else { "unknown" };
                    return Ok(Value::String(a.to_string()));
                }
                "fuse_rt_sys_num_cpus" => {
                    return Ok(Value::Int(std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1)));
                }
                "fuse_rt_sys_mem_total" => { return Ok(Value::Int(0)); }
                "fuse_rt_os_create_symlink" | "fuse_rt_os_read_symlink" | "fuse_rt_os_set_read_only" => {
                    // Symlinks and permissions: minimal evaluator support
                    return Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) });
                }
                "fuse_rt_os_temp_dir" => {
                    return Ok(Value::String(std::env::temp_dir().to_string_lossy().to_string()));
                }
                "fuse_rt_os_temp_file" | "fuse_rt_os_temp_dir_create" => {
                    let pfx = if let Some(Value::String(s)) = args.first() { s.as_str() } else { "fuse" };
                    let dir = std::env::temp_dir();
                    let temp_name = format!("{pfx}{}", std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
                    let path = dir.join(&temp_name);
                    let result = if name == "fuse_rt_os_temp_dir_create" {
                        std::fs::create_dir(&path).map(|_| ())
                    } else {
                        std::fs::File::create(&path).map(|_| ())
                    };
                    return match result {
                        Ok(()) => Ok(Value::Result { is_ok: true, value: Box::new(Value::String(path.to_string_lossy().to_string())) }),
                        Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("os: {e}"))) }),
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
                // --- test assertion FFI ---
                "fuse_rt_test_assert_eq" | "fuse_rt_test_assert_ne" => {
                    // handled below (needs &self for stringify)
                }
                "fuse_rt_test_assert_approx" => {
                    let av = Self::extract_float(args.first());
                    let bv = Self::extract_float(args.get(1));
                    let ev = Self::extract_float(args.get(2));
                    let msg = if let Some(Value::String(m)) = args.get(3) { m.clone() } else { String::new() };
                    if (av - bv).abs() > ev {
                        eprintln!("[FAIL] assertApprox: {msg}");
                        eprintln!("  expected: {bv} ± {ev}");
                        eprintln!("  actual:   {av}");
                        std::process::exit(1);
                    }
                    return Ok(Value::Unit);
                }
                "fuse_rt_test_assert_panics" => {
                    eprintln!("[SKIP] assertPanics: not supported in --run mode");
                    return Ok(Value::Unit);
                }
                "fuse_rt_panic" => {
                    std::process::exit(101);
                }
                // --- log FFI ---
                "fuse_rt_log_eprintln" => {
                    if let Some(Value::String(s)) = args.first() {
                        eprintln!("{s}");
                    }
                    return Ok(Value::Unit);
                }
                "fuse_rt_log_timestamp" => {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                    let days = (secs / 86400) as i64;
                    let day_secs = (secs % 86400) as i64;
                    let h = day_secs / 3600; let m = (day_secs % 3600) / 60; let s = day_secs % 60;
                    let z = days + 719468;
                    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
                    let doe = z - era * 146097;
                    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
                    let y = yoe + era * 400;
                    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                    let mp = (5 * doy + 2) / 153;
                    let d = doy - (153 * mp + 2) / 5 + 1;
                    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
                    let y = if mo <= 2 { y + 1 } else { y };
                    return Ok(Value::String(format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")));
                }
                "fuse_rt_log_global_level" => {
                    thread_local! { static LVL: std::cell::Cell<i64> = const { std::cell::Cell::new(2) }; }
                    return Ok(Value::Int(LVL.with(|c| c.get())));
                }
                "fuse_rt_log_set_global_level" => {
                    if let Some(Value::Int(n)) = args.first() {
                        thread_local! { static LVL: std::cell::Cell<i64> = const { std::cell::Cell::new(2) }; }
                        LVL.with(|c| c.set(*n));
                    }
                    return Ok(Value::Unit);
                }
                "fuse_rt_log_append_file" => {
                    if let (Some(Value::String(p)), Some(Value::String(m))) = (args.first(), args.get(1)) {
                        use std::io::Write;
                        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p.as_str()) {
                            let _ = writeln!(f, "{m}");
                        }
                    }
                    return Ok(Value::Unit);
                }
                // --- regex FFI ---
                "fuse_rt_regex_compile" => {
                    if let Some(Value::String(pat)) = args.first() {
                        return match regex::Regex::new(pat) {
                            Ok(re) => {
                                thread_local! { static STORE: std::cell::RefCell<std::collections::HashMap<i64, regex::Regex>> = std::cell::RefCell::new(std::collections::HashMap::new()); static NXT: std::cell::Cell<i64> = const { std::cell::Cell::new(1) }; }
                                let id = NXT.with(|c| { let id = c.get(); c.set(id + 1); id });
                                STORE.with(|s| s.borrow_mut().insert(id, re));
                                Ok(Value::Result { is_ok: true, value: Box::new(Value::Int(id)) })
                            }
                            Err(e) => Ok(Value::Result { is_ok: false, value: Box::new(Value::String(format!("{e}"))) }),
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("regex: expected pattern".into())) });
                }
                // --- toml FFI ---
                "fuse_rt_toml_parse" => {
                    if let Some(Value::String(s)) = args.first() {
                        return match s.parse::<toml::Table>() {
                            Ok(table) => {
                                let tv = Self::toml_to_value(&toml::Value::Table(table));
                                Ok(Value::Result { is_ok: true, value: Box::new(tv) })
                            }
                            Err(e) => {
                                let err = Value::Data(std::rc::Rc::new(std::cell::RefCell::new(
                                    DataInstance {
                                        module_path: std::path::PathBuf::new(),
                                        type_name: "TomlError".to_string(),
                                        fields: {
                                            let mut f = std::collections::HashMap::new();
                                            f.insert("message".to_string(), Value::String(format!("{e}")));
                                            f.insert("line".to_string(), Value::Int(0));
                                            f.insert("col".to_string(), Value::Int(0));
                                            f
                                        },
                                        field_order: vec!["message".to_string(), "line".to_string(), "col".to_string()],
                                        methods: std::collections::HashMap::new(),
                                        destroyed: false,
                                    },
                                )));
                                Ok(Value::Result { is_ok: false, value: Box::new(err) })
                            }
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("toml: expected string".into())) });
                }
                // --- http_server FFI ---
                "fuse_rt_http_server_route" => {
                    // Stub: routes registered but no server runs in evaluator.
                    return Ok(Value::Unit);
                }
                "fuse_rt_http_server_listen" => {
                    // Stub: cannot start a server in evaluator mode.
                    return Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) });
                }
                // --- crypto FFI ---
                "fuse_rt_crypto_sha256" => {
                    use sha2::Digest;
                    if let Some(Value::String(s)) = args.first() {
                        let hash = sha2::Sha256::digest(s.as_bytes());
                        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
                        return Ok(Value::String(hex));
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_crypto_sha256_bytes" => {
                    use sha2::Digest;
                    let mut bytes = Vec::new();
                    if let Some(Value::List(items)) = args.first() {
                        for item in items { if let Value::Int(n) = item { bytes.push(*n as u8); } }
                    }
                    let hash = sha2::Sha256::digest(&bytes);
                    return Ok(Value::List(hash.iter().map(|b| Value::Int(*b as i64)).collect()));
                }
                "fuse_rt_crypto_sha512" => {
                    use sha2::Digest;
                    if let Some(Value::String(s)) = args.first() {
                        let hash = sha2::Sha512::digest(s.as_bytes());
                        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
                        return Ok(Value::String(hex));
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_crypto_md5" => {
                    use md5::Digest;
                    if let Some(Value::String(s)) = args.first() {
                        let hash = md5::Md5::digest(s.as_bytes());
                        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
                        return Ok(Value::String(hex));
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_crypto_blake3" => {
                    if let Some(Value::String(s)) = args.first() {
                        let hash = blake3::hash(s.as_bytes());
                        return Ok(Value::String(hash.to_hex().to_string()));
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_crypto_hmac_sha256" => {
                    use hmac::{Hmac, Mac};
                    if let (Some(Value::String(k)), Some(Value::String(d))) = (args.first(), args.get(1)) {
                        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(k.as_bytes()).unwrap();
                        mac.update(d.as_bytes());
                        let result = mac.finalize().into_bytes();
                        let hex: String = result.iter().map(|b| format!("{b:02x}")).collect();
                        return Ok(Value::String(hex));
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_crypto_constant_time_eq" => {
                    if let (Some(Value::String(a)), Some(Value::String(b))) = (args.first(), args.get(1)) {
                        let eq = a.len() == b.len() && a.as_bytes().iter().zip(b.as_bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0;
                        return Ok(Value::Bool(eq));
                    }
                    return Ok(Value::Bool(false));
                }
                "fuse_rt_crypto_random_bytes" => {
                    let n = if let Some(Value::Int(n)) = args.first() { *n as usize } else { 0 };
                    let mut buf = vec![0u8; n];
                    getrandom::getrandom(&mut buf).unwrap_or(());
                    return Ok(Value::List(buf.iter().map(|b| Value::Int(*b as i64)).collect()));
                }
                "fuse_rt_crypto_random_hex" => {
                    let n = if let Some(Value::Int(n)) = args.first() { *n as usize } else { 0 };
                    let mut buf = vec![0u8; n];
                    getrandom::getrandom(&mut buf).unwrap_or(());
                    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
                    return Ok(Value::String(hex));
                }
                // --- json_schema FFI ---
                "fuse_rt_json_schema_compile" => {
                    // Stub: store schema as-is, return handle.
                    thread_local! { static SCHEMAS: std::cell::RefCell<std::collections::HashMap<i64, Value>> = std::cell::RefCell::new(std::collections::HashMap::new()); static NXT: std::cell::Cell<i64> = const { std::cell::Cell::new(1) }; }
                    if let Some(v) = args.first() {
                        let id = NXT.with(|c| { let id = c.get(); c.set(id + 1); id });
                        SCHEMAS.with(|s| s.borrow_mut().insert(id, v.clone()));
                        return Ok(Value::Result { is_ok: true, value: Box::new(Value::Int(id)) });
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("schema: expected value".into())) });
                }
                "fuse_rt_json_schema_compile_str" => {
                    // Compile from JSON string — same stub as compile.
                    thread_local! { static SCHEMAS_STR: std::cell::RefCell<std::collections::HashMap<i64, Value>> = std::cell::RefCell::new(std::collections::HashMap::new()); static NXT_STR: std::cell::Cell<i64> = const { std::cell::Cell::new(1000) }; }
                    if let Some(Value::String(s)) = args.first() {
                        let id = NXT_STR.with(|c| { let id = c.get(); c.set(id + 1); id });
                        SCHEMAS_STR.with(|ss| ss.borrow_mut().insert(id, Value::String(s.clone())));
                        return Ok(Value::Result { is_ok: true, value: Box::new(Value::Int(id)) });
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("schema: expected string".into())) });
                }
                "fuse_rt_json_schema_validate" => {
                    // Stub: always valid in evaluator mode.
                    return Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) });
                }
                "fuse_rt_json_schema_validate_str" => {
                    // Stub: always valid in evaluator mode.
                    return Ok(Value::Result { is_ok: true, value: Box::new(Value::Unit) });
                }
                // --- yaml FFI ---
                "fuse_rt_yaml_parse" => {
                    if let Some(Value::String(s)) = args.first() {
                        return match serde_yaml::from_str::<serde_yaml::Value>(s) {
                            Ok(val) => Ok(Value::Result { is_ok: true, value: Box::new(Self::yaml_to_value(&val)) }),
                            Err(e) => {
                                let err = Value::Data(std::rc::Rc::new(std::cell::RefCell::new(
                                    DataInstance {
                                        module_path: std::path::PathBuf::new(),
                                        type_name: "YamlError".to_string(),
                                        fields: {
                                            let mut f = std::collections::HashMap::new();
                                            f.insert("message".to_string(), Value::String(format!("{e}")));
                                            f.insert("line".to_string(), Value::Int(0));
                                            f.insert("col".to_string(), Value::Int(0));
                                            f
                                        },
                                        field_order: vec!["message".to_string(), "line".to_string(), "col".to_string()],
                                        methods: std::collections::HashMap::new(),
                                        destroyed: false,
                                    },
                                )));
                                Ok(Value::Result { is_ok: false, value: Box::new(err) })
                            }
                        };
                    }
                    return Ok(Value::Result { is_ok: false, value: Box::new(Value::String("yaml: expected string".into())) });
                }
                "fuse_rt_yaml_stringify" | "fuse_rt_yaml_stringify_pretty" => {
                    if let Some(v) = args.first() {
                        return Ok(Value::String(self.stringify(v)));
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_toml_stringify" => {
                    // Stub: convert enum to string representation.
                    if let Some(v) = args.first() {
                        return Ok(Value::String(self.stringify(v)));
                    }
                    return Ok(Value::String(String::new()));
                }
                "fuse_rt_regex_is_match" | "fuse_rt_regex_find" | "fuse_rt_regex_find_all"
                | "fuse_rt_regex_replace" | "fuse_rt_regex_replace_all"
                | "fuse_rt_regex_split" | "fuse_rt_regex_captures" => {
                    // Regex operations in evaluator: stub implementations.
                    // Full support requires the compiled path.
                    return match name {
                        "fuse_rt_regex_is_match" => Ok(Value::Bool(false)),
                        "fuse_rt_regex_find" | "fuse_rt_regex_captures" => Ok(Value::Option(None)),
                        "fuse_rt_regex_find_all" | "fuse_rt_regex_split" => Ok(Value::List(vec![])),
                        "fuse_rt_regex_replace" | "fuse_rt_regex_replace_all" => {
                            if let Some(Value::String(s)) = args.get(1) { Ok(Value::String(s.clone())) }
                            else { Ok(Value::String(String::new())) }
                        }
                        _ => Ok(Value::Unit),
                    };
                }
                _ => { handled = false; return Ok(Value::Unit); }
            }
            // Post-match handlers that need &self (for stringify).
            match name {
                "fuse_rt_test_assert_eq" => {
                    let a_str = self.stringify(args.first().unwrap_or(&Value::Unit));
                    let b_str = self.stringify(args.get(1).unwrap_or(&Value::Unit));
                    let msg = if let Some(Value::String(m)) = args.get(2) { m.clone() } else { String::new() };
                    if a_str != b_str {
                        eprintln!("[FAIL] assertEq: {msg}");
                        eprintln!("  expected: {b_str}");
                        eprintln!("  actual:   {a_str}");
                        std::process::exit(1);
                    }
                    return Ok(Value::Unit);
                }
                "fuse_rt_test_assert_ne" => {
                    let a_str = self.stringify(args.first().unwrap_or(&Value::Unit));
                    let b_str = self.stringify(args.get(1).unwrap_or(&Value::Unit));
                    let msg = if let Some(Value::String(m)) = args.get(2) { m.clone() } else { String::new() };
                    if a_str == b_str {
                        eprintln!("[FAIL] assertNe: {msg}");
                        eprintln!("  both values: {a_str}");
                        std::process::exit(1);
                    }
                    return Ok(Value::Unit);
                }
                _ => { handled = false; return Ok(Value::Unit); }

            }
        })();
        if handled { Some(result) } else { None }
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
            if let Some(result) = self.dispatch_ffi(&function.name, &args) {
                return result;
            }
        }
        self.recursion_depth += 1;
        if self.recursion_depth > 256 {
            self.recursion_depth -= 1;
            return Err(runtime_error(
                "stack overflow: recursion depth exceeded 256",
                &display_name(module_path),
                function.span.line,
                function.span.column,
            ));
        }
        let result = self.call_user_function_body(module_path, function, args, caller_env);
        self.recursion_depth -= 1;
        result
    }

    fn call_user_function_body(
        &mut self,
        module_path: &Path,
        function: &fa::FunctionDecl,
        args: Vec<Value>,
        caller_env: Option<Environment>,
    ) -> Result<Value, RuntimeError> {
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
                // Check for zero-arity enum variant: Type.Variant
                if let fa::Expr::Name(name) = member.object.as_ref() {
                    if env.resolve(&name.value).is_none() {
                        if let Some(enum_decl) = self.find_enum(&name.value) {
                            if let Some(variant) = enum_decl.variants.iter().find(|v| v.name == member.name && v.payload_types.is_empty()) {
                                return Ok(Value::Enum { type_name: name.value.clone(), variant: variant.name.clone(), payloads: Vec::new() });
                            }
                        }
                    }
                }
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
                        // Try static dispatch (Type.method, Enum.Variant, Map.new) when the
                        // name is unresolved OR resolves to a type constructor (not a value).
                        let is_type_name = match env.resolve(&name.value) {
                            None => true,
                            Some(binding) => matches!(binding.borrow().value, Value::NativeFunction(ref f) if matches!(f.as_ref(), NativeFunction::DataConstructor { .. })),
                        };
                        if is_type_name {
                            let base = name.value.split("::").next().unwrap_or(&name.value);
                            if base == "Map" && member.name == "new" {
                                return Ok(Value::Map(Vec::new()));
                            }
                            // Check for user enum variant construction: Type.Variant(args...)
                            if let Some(enum_decl) = self.find_enum(&name.value) {
                                if let Some(variant) = enum_decl.variants.iter().find(|v| v.name == member.name) {
                                    let mut payloads = Vec::new();
                                    for arg in &call.args {
                                        payloads.push(self.eval_expr(module_path, arg, env)?);
                                    }
                                    if payloads.len() != variant.payload_types.len() {
                                        return Err(ControlFlow::Abort(runtime_error(
                                            format!("enum variant `{}.{}` expects {} argument(s), got {}", name.value, member.name, variant.payload_types.len(), payloads.len()),
                                            &display_name(module_path), call.span.line, call.span.column,
                                        )));
                                    }
                                    return Ok(Value::Enum { type_name: name.value.clone(), variant: member.name.clone(), payloads });
                                }
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
                    type_params: Vec::new(),
                    params: lambda.params.clone(),
                    return_type: lambda.return_type.clone(),
                    body: lambda.body.clone(),
                    is_pub: false,
                    annotations: Vec::new(),
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

    fn yaml_to_value(yv: &serde_yaml::Value) -> Value {
        match yv {
            serde_yaml::Value::Null => Value::Enum { type_name: "YamlValue".into(), variant: "Null".into(), payloads: vec![] },
            serde_yaml::Value::Bool(b) => Value::Enum { type_name: "YamlValue".into(), variant: "Bool".into(), payloads: vec![Value::Bool(*b)] },
            serde_yaml::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Enum { type_name: "YamlValue".into(), variant: "Int".into(), payloads: vec![Value::Int(i)] }
                } else {
                    Value::Enum { type_name: "YamlValue".into(), variant: "Float".into(), payloads: vec![Value::Float(n.as_f64().unwrap_or(0.0))] }
                }
            }
            serde_yaml::Value::String(s) => Value::Enum { type_name: "YamlValue".into(), variant: "Str".into(), payloads: vec![Value::String(s.clone())] },
            serde_yaml::Value::Sequence(seq) => Value::Enum { type_name: "YamlValue".into(), variant: "Seq".into(), payloads: vec![Value::List(seq.iter().map(Self::yaml_to_value).collect())] },
            serde_yaml::Value::Mapping(map) => {
                let entries: Vec<(Value, Value)> = map.iter().map(|(k, v)| {
                    let key = match k { serde_yaml::Value::String(s) => s.clone(), other => format!("{other:?}") };
                    (Value::String(key), Self::yaml_to_value(v))
                }).collect();
                Value::Enum { type_name: "YamlValue".into(), variant: "Map".into(), payloads: vec![Value::Map(entries)] }
            }
            serde_yaml::Value::Tagged(tagged) => Self::yaml_to_value(&tagged.value),
        }
    }

    fn toml_to_value(tv: &toml::Value) -> Value {
        match tv {
            toml::Value::Boolean(b) => Value::Enum { type_name: "TomlValue".into(), variant: "Bool".into(), payloads: vec![Value::Bool(*b)] },
            toml::Value::Integer(n) => Value::Enum { type_name: "TomlValue".into(), variant: "Int".into(), payloads: vec![Value::Int(*n)] },
            toml::Value::Float(f) => Value::Enum { type_name: "TomlValue".into(), variant: "Float".into(), payloads: vec![Value::Float(*f)] },
            toml::Value::String(s) => Value::Enum { type_name: "TomlValue".into(), variant: "Str".into(), payloads: vec![Value::String(s.clone())] },
            toml::Value::Datetime(dt) => Value::Enum { type_name: "TomlValue".into(), variant: "DateTime".into(), payloads: vec![Value::String(dt.to_string())] },
            toml::Value::Array(arr) => Value::Enum { type_name: "TomlValue".into(), variant: "Array".into(), payloads: vec![Value::List(arr.iter().map(Self::toml_to_value).collect())] },
            toml::Value::Table(tbl) => Value::Enum { type_name: "TomlValue".into(), variant: "Table".into(), payloads: vec![Value::Map(tbl.iter().map(|(k, v)| (Value::String(k.clone()), Self::toml_to_value(v))).collect())] },
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

    fn find_enum(&self, type_name: &str) -> Option<fa::EnumDecl> {
        for module in self.modules.values() {
            if let Some(decl) = module.enums.get(type_name) {
                return Some(decl.clone());
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
        let parts = match parse_fstring_template(template) {
            Ok(parts) => parts,
            Err(_) => return template.to_string(),
        };
        for part in parts {
            match part {
                FStringPart::Literal(text) => output.push_str(&text),
                FStringPart::Interp(expr_text) => {
                    let trimmed = expr_text.trim();
                    output.push_str(&self.interpolate(trimmed, module_path, env));
                }
            }
        }
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
        Value::Enum { type_name, .. } => type_name.clone(),
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
        (Value::Enum { type_name: lt, variant: lv, payloads: lp },
         Value::Enum { type_name: rt, variant: rv, payloads: rp }) => {
            lt == rt && lv == rv && lp.len() == rp.len() && lp.iter().zip(rp.iter()).all(|(l, r)| value_eq(l, r))
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
            let parts = match parse_fstring_template(&fstring.template) {
                Ok(parts) => parts,
                Err(_) => return names,
            };
            for part in parts {
                if let FStringPart::Interp(expr_text) = part {
                    let trimmed = expr_text.trim();
                    let source = format!("fn __names__() => {trimmed}");
                    if let Ok(program) = parse_source(&source, "<fstring-names>") {
                        for decl in &program.declarations {
                            if let fa::Declaration::Function(func) = decl {
                                if let Some(fa::Statement::Expr(expr_stmt)) =
                                    func.body.statements.first()
                                {
                                    names.extend(collect_expr_names(&expr_stmt.expr));
                                }
                            }
                        }
                    }
                }
            }
            names
        }
        fa::Expr::Member(member) => collect_expr_names(&member.object),
        fa::Expr::Move(move_expr) => collect_expr_names(&move_expr.value),
        fa::Expr::Ref(reference) => collect_expr_names(&reference.value),
        fa::Expr::MutRef(reference) => collect_expr_names(&reference.value),
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
                (variant, Value::Enum { variant: v, payloads, .. }) if v == variant => {
                    let mut bindings = HashMap::new();
                    for (i, arg_pat) in pattern.args.iter().enumerate() {
                        if let Some(payload) = payloads.get(i) {
                            if let Some(inner) = match_pattern(arg_pat, payload) {
                                bindings.extend(inner);
                            } else {
                                return None;
                            }
                        }
                    }
                    Some(bindings)
                }
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