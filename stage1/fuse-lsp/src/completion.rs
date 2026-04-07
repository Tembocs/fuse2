use fusec::ast::nodes as ast;
use fusec::Span;

// ---------------------------------------------------------------------------
// Completion item
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
}

#[derive(Debug)]
pub enum CompletionKind {
    Function,   // 3
    Variable,   // 6
    Field,      // 5
    Keyword,    // 14
    Module,     // 9
    Struct,     // 22
    Enum,       // 13
    Method,     // 2
    Constant,   // 21
    Interface,  // 8
}

impl CompletionKind {
    pub fn lsp_kind(&self) -> i32 {
        match self {
            Self::Method => 2,
            Self::Function => 3,
            Self::Field => 5,
            Self::Variable => 6,
            Self::Interface => 8,
            Self::Module => 9,
            Self::Enum => 13,
            Self::Keyword => 14,
            Self::Constant => 21,
            Self::Struct => 22,
        }
    }
}

// ---------------------------------------------------------------------------
// General completions (no dot)
// ---------------------------------------------------------------------------

static KEYWORDS: &[&str] = &[
    "fn", "val", "var", "if", "else", "while", "for", "in", "loop",
    "break", "continue", "return", "match", "when", "import", "pub",
    "data", "class", "struct", "enum", "interface", "extern",
    "spawn", "defer", "move", "ref", "mutref", "owned",
    "true", "false", "and", "or", "not",
];

pub fn general_completions(program: &ast::Program, line: usize, _col: usize) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Keywords
    for kw in KEYWORDS {
        items.push(CompletionItem {
            label: kw.to_string(),
            kind: CompletionKind::Keyword,
            detail: None,
        });
    }

    // Collect from declarations
    for decl in &program.declarations {
        match decl {
            ast::Declaration::Function(f) => {
                items.push(CompletionItem {
                    label: f.name.clone(),
                    kind: CompletionKind::Function,
                    detail: Some(format_sig(f)),
                });
                // If cursor is inside this function, add params and locals
                if is_inside_function(f, line) {
                    for p in &f.params {
                        items.push(CompletionItem {
                            label: p.name.clone(),
                            kind: CompletionKind::Variable,
                            detail: p.type_name.clone(),
                        });
                    }
                    collect_block_vars(&mut items, &f.body, line);
                }
            }
            ast::Declaration::DataClass(d) => {
                items.push(CompletionItem {
                    label: d.name.clone(),
                    kind: CompletionKind::Struct,
                    detail: Some("data class".into()),
                });
                for m in &d.methods {
                    if is_inside_function(m, line) {
                        for p in &m.params { items.push(CompletionItem { label: p.name.clone(), kind: CompletionKind::Variable, detail: p.type_name.clone() }); }
                        collect_block_vars(&mut items, &m.body, line);
                    }
                }
            }
            ast::Declaration::Struct(s) => {
                items.push(CompletionItem {
                    label: s.name.clone(),
                    kind: CompletionKind::Struct,
                    detail: Some("struct".into()),
                });
                for m in &s.methods {
                    if is_inside_function(m, line) {
                        for p in &m.params { items.push(CompletionItem { label: p.name.clone(), kind: CompletionKind::Variable, detail: p.type_name.clone() }); }
                        collect_block_vars(&mut items, &m.body, line);
                    }
                }
            }
            ast::Declaration::Enum(e) => {
                items.push(CompletionItem {
                    label: e.name.clone(),
                    kind: CompletionKind::Enum,
                    detail: None,
                });
                for v in &e.variants {
                    items.push(CompletionItem {
                        label: v.name.clone(),
                        kind: CompletionKind::Constant,
                        detail: Some(format!("{}.{}", e.name, v.name)),
                    });
                }
            }
            ast::Declaration::Import(imp) => {
                if let Some(ref items_list) = imp.items {
                    for item in items_list {
                        items.push(CompletionItem {
                            label: item.clone(),
                            kind: CompletionKind::Module,
                            detail: Some(format!("from {}", imp.module_path)),
                        });
                    }
                }
            }
            ast::Declaration::Const(c) => {
                items.push(CompletionItem {
                    label: c.name.clone(),
                    kind: CompletionKind::Constant,
                    detail: c.type_name.clone(),
                });
            }
            ast::Declaration::Interface(iface) => {
                items.push(CompletionItem {
                    label: iface.name.clone(),
                    kind: CompletionKind::Interface,
                    detail: Some("interface".into()),
                });
            }
            ast::Declaration::ExternFn(ef) => {
                items.push(CompletionItem {
                    label: ef.name.clone(),
                    kind: CompletionKind::Function,
                    detail: ef.return_type.clone(),
                });
            }
        }
    }

    // Built-in functions
    for name in &["println", "Some", "None", "Ok", "Err"] {
        items.push(CompletionItem {
            label: name.to_string(),
            kind: CompletionKind::Function,
            detail: None,
        });
    }

    // Built-in types
    for name in &["Int", "Float", "Bool", "String", "List", "Map", "Option", "Result", "Chan", "Shared"] {
        items.push(CompletionItem {
            label: name.to_string(),
            kind: CompletionKind::Struct,
            detail: Some("built-in type".into()),
        });
    }

    items
}

fn is_inside_function(f: &ast::FunctionDecl, line: usize) -> bool {
    line >= f.span.line
}

fn collect_block_vars(items: &mut Vec<CompletionItem>, block: &ast::Block, cursor_line: usize) {
    for stmt in &block.statements {
        if let ast::Statement::VarDecl(v) = stmt {
            // Only include vars declared before cursor
            if v.span.line <= cursor_line {
                items.push(CompletionItem {
                    label: v.name.clone(),
                    kind: CompletionKind::Variable,
                    detail: v.type_name.clone(),
                });
            }
        }
        if let ast::Statement::For(f) = stmt {
            if f.span.line <= cursor_line {
                items.push(CompletionItem {
                    label: f.name.clone(),
                    kind: CompletionKind::Variable,
                    detail: None,
                });
                collect_block_vars(items, &f.body, cursor_line);
            }
        }
        if let ast::Statement::While(w) = stmt {
            collect_block_vars(items, &w.body, cursor_line);
        }
        if let ast::Statement::Loop(l) = stmt {
            collect_block_vars(items, &l.body, cursor_line);
        }
    }
}

// ---------------------------------------------------------------------------
// Dot-completion: extension methods for a type
// ---------------------------------------------------------------------------

/// Given the text before the cursor and parsed program, infer the receiver
/// type and return extension method completions.
pub fn dot_completions(
    program: &ast::Program,
    line: usize,
    line_text: &str,
    col: usize,
) -> Vec<CompletionItem> {
    let before_dot = &line_text[..col.saturating_sub(1)];
    let receiver_name = before_dot.split(|c: char| !c.is_alphanumeric() && c != '_').last().unwrap_or("");
    if receiver_name.is_empty() {
        return Vec::new();
    }

    // Try to infer the type of the receiver from the AST
    let receiver_type = infer_name_type(program, receiver_name, line);

    let ty = receiver_type.as_deref().unwrap_or(receiver_name);

    // Strip generic parameters for lookup (e.g., "List<Int>" → "List")
    let base_ty = ty.split('<').next().unwrap_or(ty);

    let mut items = Vec::new();

    // Stdlib extension methods
    for (t, method, detail) in STDLIB_METHODS {
        if *t == base_ty {
            items.push(CompletionItem {
                label: method.to_string(),
                kind: CompletionKind::Method,
                detail: Some(detail.to_string()),
            });
        }
    }

    // Extension methods defined in the current file
    for decl in &program.declarations {
        if let ast::Declaration::Function(f) = decl {
            if let Some(ref rt) = f.receiver_type {
                let base_rt = rt.split('<').next().unwrap_or(rt);
                if base_rt == base_ty {
                    items.push(CompletionItem {
                        label: f.name.clone(),
                        kind: CompletionKind::Method,
                        detail: Some(format_sig(f)),
                    });
                }
            }
        }
        // Data class / struct fields
        if let ast::Declaration::DataClass(d) = decl {
            if d.name == base_ty {
                for field in &d.fields {
                    items.push(CompletionItem {
                        label: field.name.clone(),
                        kind: CompletionKind::Field,
                        detail: field.type_name.clone(),
                    });
                }
            }
        }
        if let ast::Declaration::Struct(s) = decl {
            if s.name == base_ty {
                for field in &s.fields {
                    items.push(CompletionItem {
                        label: field.name.clone(),
                        kind: CompletionKind::Field,
                        detail: field.type_name.clone(),
                    });
                }
            }
        }
    }

    items
}

/// Try to infer the type of a name from variable declarations and params.
fn infer_name_type(program: &ast::Program, name: &str, cursor_line: usize) -> Option<String> {
    for decl in &program.declarations {
        if let ast::Declaration::Function(f) = decl {
            if cursor_line >= f.span.line {
                for p in &f.params {
                    if p.name == name { return p.type_name.clone(); }
                }
                if let Some(ty) = find_var_type(&f.body, name, cursor_line) {
                    return Some(ty);
                }
            }
        }
        if let ast::Declaration::DataClass(d) = decl {
            for m in &d.methods {
                if cursor_line >= m.span.line {
                    for p in &m.params { if p.name == name { return p.type_name.clone(); } }
                    if let Some(ty) = find_var_type(&m.body, name, cursor_line) { return Some(ty); }
                }
            }
        }
        if let ast::Declaration::Struct(s) = decl {
            for m in &s.methods {
                if cursor_line >= m.span.line {
                    for p in &m.params { if p.name == name { return p.type_name.clone(); } }
                    if let Some(ty) = find_var_type(&m.body, name, cursor_line) { return Some(ty); }
                }
            }
        }
    }
    None
}

fn find_var_type(block: &ast::Block, name: &str, cursor_line: usize) -> Option<String> {
    for stmt in &block.statements {
        if let ast::Statement::VarDecl(v) = stmt {
            if v.name == name && v.span.line <= cursor_line {
                return v.type_name.clone().or_else(|| infer_expr_type(&v.value));
            }
        }
    }
    None
}

/// Simple expression type inference for common patterns.
fn infer_expr_type(expr: &ast::Expr) -> Option<String> {
    match expr {
        ast::Expr::Literal(lit) => match &lit.value {
            ast::LiteralValue::Int(_) => Some("Int".into()),
            ast::LiteralValue::Float(_) => Some("Float".into()),
            ast::LiteralValue::String(_) => Some("String".into()),
            ast::LiteralValue::Bool(_) => Some("Bool".into()),
        },
        ast::Expr::List(_) => Some("List".into()),
        ast::Expr::Call(c) => {
            // Constructor calls: Point(...) → Point
            if let ast::Expr::Name(n) = c.callee.as_ref() {
                if n.value.chars().next().is_some_and(|c| c.is_uppercase()) {
                    return Some(n.value.clone());
                }
            }
            None
        }
        ast::Expr::FString(_) => Some("String".into()),
        _ => None,
    }
}

fn format_sig(f: &ast::FunctionDecl) -> String {
    let params: Vec<String> = f.params.iter().map(|p| {
        let conv = p.convention.as_ref().map(|c| format!("{c} ")).unwrap_or_default();
        let ty = p.type_name.as_deref().unwrap_or("_");
        format!("{conv}{}: {ty}", p.name)
    }).collect();
    let ret = f.return_type.as_ref().map(|t| format!(" -> {t}")).unwrap_or_default();
    format!("({}){ret}", params.join(", "))
}

// ---------------------------------------------------------------------------
// Stdlib method table: (Type, method, detail)
// ---------------------------------------------------------------------------

static STDLIB_METHODS: &[(&str, &str, &str)] = &[
    // Bool
    ("Bool", "not", "() -> Bool"),
    ("Bool", "toInt", "() -> Int"),
    ("Bool", "toString", "() -> String"),
    // Int
    ("Int", "abs", "() -> Int"),
    ("Int", "clamp", "(low: Int, high: Int) -> Int"),
    ("Int", "gcd", "(other: Int) -> Int"),
    ("Int", "isEven", "() -> Bool"),
    ("Int", "isNegative", "() -> Bool"),
    ("Int", "isOdd", "() -> Bool"),
    ("Int", "isPositive", "() -> Bool"),
    ("Int", "isZero", "() -> Bool"),
    ("Int", "lcm", "(other: Int) -> Int"),
    ("Int", "max", "(other: Int) -> Int"),
    ("Int", "min", "(other: Int) -> Int"),
    ("Int", "pow", "(exp: Int) -> Int"),
    ("Int", "toBits", "() -> String"),
    ("Int", "toFloat", "() -> Float"),
    ("Int", "toHex", "() -> String"),
    ("Int", "toOctal", "() -> String"),
    ("Int", "toString", "() -> String"),
    // Float
    ("Float", "abs", "() -> Float"),
    ("Float", "approxEq", "(other: Float, epsilon: Float) -> Bool"),
    ("Float", "ceil", "() -> Float"),
    ("Float", "clamp", "(low: Float, high: Float) -> Float"),
    ("Float", "floor", "() -> Float"),
    ("Float", "fract", "() -> Float"),
    ("Float", "isFinite", "() -> Bool"),
    ("Float", "isInfinite", "() -> Bool"),
    ("Float", "isNaN", "() -> Bool"),
    ("Float", "isNegative", "() -> Bool"),
    ("Float", "isPositive", "() -> Bool"),
    ("Float", "max", "(other: Float) -> Float"),
    ("Float", "min", "(other: Float) -> Float"),
    ("Float", "pow", "(exp: Float) -> Float"),
    ("Float", "round", "() -> Float"),
    ("Float", "sqrt", "() -> Float"),
    ("Float", "toInt", "() -> Int"),
    ("Float", "toString", "() -> String"),
    ("Float", "trunc", "() -> Float"),
    // String
    ("String", "byteAt", "(index: Int) -> Int"),
    ("String", "capitalize", "() -> String"),
    ("String", "charCount", "() -> Int"),
    ("String", "chars", "() -> List<String>"),
    ("String", "compareTo", "(other: String) -> Int"),
    ("String", "contains", "(sub: String) -> Bool"),
    ("String", "endsWith", "(suffix: String) -> Bool"),
    ("String", "indexOf", "(sub: String) -> Option<Int>"),
    ("String", "lastIndexOf", "(sub: String) -> Option<Int>"),
    ("String", "len", "() -> Int"),
    ("String", "padEnd", "(width: Int, pad: String) -> String"),
    ("String", "padStart", "(width: Int, pad: String) -> String"),
    ("String", "repeat", "(n: Int) -> String"),
    ("String", "replace", "(old: String, new: String) -> String"),
    ("String", "reverse", "() -> String"),
    ("String", "split", "(sep: String) -> List<String>"),
    ("String", "splitLines", "() -> List<String>"),
    ("String", "startsWith", "(prefix: String) -> Bool"),
    ("String", "toBool", "() -> Result<Bool, String>"),
    ("String", "toBytes", "() -> List<Int>"),
    ("String", "toFloat", "() -> Result<Float, String>"),
    ("String", "toInt", "() -> Result<Int, String>"),
    ("String", "toLower", "() -> String"),
    ("String", "trim", "() -> String"),
    ("String", "trimEnd", "() -> String"),
    ("String", "trimStart", "() -> String"),
    // List
    ("List", "all", "(f: fn(T) -> Bool) -> Bool"),
    ("List", "any", "(f: fn(T) -> Bool) -> Bool"),
    ("List", "clear", "()"),
    ("List", "concat", "(other: List<T>) -> List<T>"),
    ("List", "contains", "(item: T) -> Bool"),
    ("List", "count", "(f: fn(T) -> Bool) -> Int"),
    ("List", "drop", "(n: Int) -> List<T>"),
    ("List", "filter", "(f: fn(T) -> Bool) -> List<T>"),
    ("List", "first", "() -> Option<T>"),
    ("List", "flatMap", "(f: fn(T) -> List<U>) -> List<U>"),
    ("List", "flatten", "() -> List<T>"),
    ("List", "get", "(index: Int) -> Option<T>"),
    ("List", "indexOf", "(item: T) -> Option<Int>"),
    ("List", "insert", "(index: Int, item: T)"),
    ("List", "isEmpty", "() -> Bool"),
    ("List", "join", "(sep: String) -> String"),
    ("List", "last", "() -> Option<T>"),
    ("List", "map", "(f: fn(T) -> U) -> List<U>"),
    ("List", "pop", "() -> Option<T>"),
    ("List", "push", "(item: T)"),
    ("List", "reduce", "(init: U, f: fn(U, T) -> U) -> U"),
    ("List", "removeAt", "(index: Int) -> T"),
    ("List", "removeWhere", "(f: fn(T) -> Bool)"),
    ("List", "reversed", "() -> List<T>"),
    ("List", "slice", "(start: Int, end: Int) -> List<T>"),
    ("List", "sorted", "() -> List<T>"),
    ("List", "sortedBy", "(f: fn(T) -> U) -> List<T>"),
    ("List", "take", "(n: Int) -> List<T>"),
    ("List", "unique", "() -> List<T>"),
    ("List", "zip", "(other: List<U>) -> List<(T, U)>"),
    // Map
    ("Map", "filter", "(f: fn(K, V) -> Bool) -> Map<K, V>"),
    ("Map", "forEach", "(f: fn(K, V))"),
    ("Map", "getOrDefault", "(key: K, default: V) -> V"),
    ("Map", "getOrInsert", "(key: K, default: V) -> V"),
    ("Map", "invert", "() -> Map<V, K>"),
    ("Map", "mapValues", "(f: fn(V) -> U) -> Map<K, U>"),
    ("Map", "merge", "(other: Map<K, V>) -> Map<K, V>"),
    ("Map", "toList", "() -> List<(K, V)>"),
    // Option
    ("Option", "filter", "(f: fn(T) -> Bool) -> Option<T>"),
    ("Option", "flatten", "() -> Option<T>"),
    ("Option", "isNone", "() -> Bool"),
    ("Option", "isSome", "() -> Bool"),
    ("Option", "map", "(f: fn(T) -> U) -> Option<U>"),
    ("Option", "okOr", "(err: E) -> Result<T, E>"),
    ("Option", "orElse", "(f: fn() -> Option<T>) -> Option<T>"),
    ("Option", "unwrap", "() -> T"),
    ("Option", "unwrapOr", "(default: T) -> T"),
    ("Option", "unwrapOrElse", "(f: fn() -> T) -> T"),
    // Result
    ("Result", "err", "() -> Option<E>"),
    ("Result", "flatten", "() -> Result<T, E>"),
    ("Result", "isErr", "() -> Bool"),
    ("Result", "isOk", "() -> Bool"),
    ("Result", "map", "(f: fn(T) -> U) -> Result<U, E>"),
    ("Result", "mapErr", "(f: fn(E) -> F) -> Result<T, F>"),
    ("Result", "ok", "() -> Option<T>"),
    ("Result", "unwrap", "() -> T"),
    ("Result", "unwrapOr", "(default: T) -> T"),
    ("Result", "unwrapOrElse", "(f: fn(E) -> T) -> T"),
    // Set
    ("Set", "add", "(item: T)"),
    ("Set", "clear", "()"),
    ("Set", "contains", "(item: T) -> Bool"),
    ("Set", "difference", "(other: Set<T>) -> Set<T>"),
    ("Set", "filter", "(f: fn(T) -> Bool) -> Set<T>"),
    ("Set", "forEach", "(f: fn(T))"),
    ("Set", "intersect", "(other: Set<T>) -> Set<T>"),
    ("Set", "isEmpty", "() -> Bool"),
    ("Set", "len", "() -> Int"),
    ("Set", "map", "(f: fn(T) -> U) -> Set<U>"),
    ("Set", "remove", "(item: T) -> Bool"),
    ("Set", "toList", "() -> List<T>"),
    ("Set", "union", "(other: Set<T>) -> Set<T>"),
];
