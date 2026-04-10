use fusec::ast::nodes as ast;
use fusec::Span;

// ---------------------------------------------------------------------------
// Symbol table built from a parsed Program
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: SymbolKind,
    pub def_span: Span,        // where the symbol is defined
    pub type_info: String,     // hover text (signature / type annotation)
    pub filename: String,
}

#[derive(Clone, Debug)]
pub enum SymbolKind {
    Function,
    Variable,
    Parameter,
    DataClass,
    Struct,
    Enum,
    EnumVariant,
    Field,
    Import,
    Const,
    Interface,
}

/// Collect all symbols from a parsed Program.
pub fn collect_symbols(program: &ast::Program) -> Vec<SymbolInfo> {
    let file = &program.filename;
    let mut syms = Vec::new();

    for decl in &program.declarations {
        match decl {
            ast::Declaration::Function(f) => {
                collect_function(&mut syms, f, file);
            }
            ast::Declaration::DataClass(d) => {
                syms.push(SymbolInfo {
                    name: d.name.clone(),
                    kind: SymbolKind::DataClass,
                    def_span: d.span,
                    type_info: format_data_class(d),
                    filename: file.clone(),
                });
                for field in &d.fields {
                    syms.push(SymbolInfo {
                        name: field.name.clone(),
                        kind: SymbolKind::Field,
                        def_span: field.span,
                        type_info: format!(
                            "{} {}: {}",
                            if field.mutable { "var" } else { "val" },
                            field.name,
                            field.type_name.as_deref().unwrap_or("_")
                        ),
                        filename: file.clone(),
                    });
                }
                for m in &d.methods {
                    collect_function(&mut syms, m, file);
                }
            }
            ast::Declaration::Struct(s) => {
                syms.push(SymbolInfo {
                    name: s.name.clone(),
                    kind: SymbolKind::Struct,
                    def_span: s.span,
                    type_info: format!("struct {}", s.name),
                    filename: file.clone(),
                });
                for field in &s.fields {
                    syms.push(SymbolInfo {
                        name: field.name.clone(),
                        kind: SymbolKind::Field,
                        def_span: field.span,
                        type_info: format!(
                            "{} {}: {}",
                            if field.mutable { "var" } else { "val" },
                            field.name,
                            field.type_name.as_deref().unwrap_or("_")
                        ),
                        filename: file.clone(),
                    });
                }
                for m in &s.methods {
                    collect_function(&mut syms, m, file);
                }
            }
            ast::Declaration::Enum(e) => {
                syms.push(SymbolInfo {
                    name: e.name.clone(),
                    kind: SymbolKind::Enum,
                    def_span: e.span,
                    type_info: format_enum(e),
                    filename: file.clone(),
                });
                for v in &e.variants {
                    syms.push(SymbolInfo {
                        name: v.name.clone(),
                        kind: SymbolKind::EnumVariant,
                        def_span: v.span,
                        type_info: if !v.payload_types.is_empty() {
                            format!(
                                "{}({})",
                                v.name,
                                v.payload_types.join(", ")
                            )
                        } else {
                            v.name.clone()
                        },
                        filename: file.clone(),
                    });
                }
            }
            ast::Declaration::Import(imp) => {
                // Each imported item is a symbol.
                if let Some(items) = &imp.items {
                    for item in items {
                        syms.push(SymbolInfo {
                            name: item.clone(),
                            kind: SymbolKind::Import,
                            def_span: imp.span,
                            type_info: format!("import {}.{}", imp.module_path, item),
                            filename: file.clone(),
                        });
                    }
                } else {
                    // Bare import — the module path itself.
                    let short = imp.module_path.rsplit('.').next().unwrap_or(&imp.module_path);
                    syms.push(SymbolInfo {
                        name: short.to_string(),
                        kind: SymbolKind::Import,
                        def_span: imp.span,
                        type_info: format!("import {}", imp.module_path),
                        filename: file.clone(),
                    });
                }
            }
            ast::Declaration::Const(c) => {
                syms.push(SymbolInfo {
                    name: c.name.clone(),
                    kind: SymbolKind::Const,
                    def_span: c.span,
                    type_info: format!(
                        "const {}.{}{}",
                        c.owner,
                        c.name,
                        c.type_name.as_ref().map(|t| format!(": {t}")).unwrap_or_default()
                    ),
                    filename: file.clone(),
                });
            }
            ast::Declaration::Interface(iface) => {
                syms.push(SymbolInfo {
                    name: iface.name.clone(),
                    kind: SymbolKind::Interface,
                    def_span: iface.span,
                    type_info: format!("interface {}", iface.name),
                    filename: file.clone(),
                });
            }
            ast::Declaration::ExternFn(ef) => {
                syms.push(SymbolInfo {
                    name: ef.name.clone(),
                    kind: SymbolKind::Function,
                    def_span: ef.span,
                    type_info: format_extern_fn(ef),
                    filename: file.clone(),
                });
            }
        }
    }
    syms
}

fn collect_function(syms: &mut Vec<SymbolInfo>, f: &ast::FunctionDecl, file: &str) {
    syms.push(SymbolInfo {
        name: f.name.clone(),
        kind: SymbolKind::Function,
        def_span: f.span,
        type_info: format_function_sig(f),
        filename: file.to_string(),
    });
    // Parameters
    for p in &f.params {
        syms.push(SymbolInfo {
            name: p.name.clone(),
            kind: SymbolKind::Parameter,
            def_span: p.span,
            type_info: format!(
                "{}{}",
                p.convention.as_ref().map(|c| format!("{c} ")).unwrap_or_default(),
                p.type_name.as_deref().unwrap_or("_")
            ),
            filename: file.to_string(),
        });
    }
    // Local variables in the body
    collect_block_locals(syms, &f.body, file);
}

fn collect_block_locals(syms: &mut Vec<SymbolInfo>, block: &ast::Block, file: &str) {
    for stmt in &block.statements {
        match stmt {
            ast::Statement::VarDecl(v) => {
                syms.push(SymbolInfo {
                    name: v.name.clone(),
                    kind: SymbolKind::Variable,
                    def_span: v.span,
                    type_info: format!(
                        "{} {}{}",
                        if v.mutable { "var" } else { "val" },
                        v.name,
                        v.type_name.as_ref().map(|t| format!(": {t}")).unwrap_or_default()
                    ),
                    filename: file.to_string(),
                });
                collect_expr_locals(syms, &v.value, file);
            }
            ast::Statement::For(f) => {
                syms.push(SymbolInfo {
                    name: f.name.clone(),
                    kind: SymbolKind::Variable,
                    def_span: f.span,
                    type_info: format!("val {}", f.name),
                    filename: file.to_string(),
                });
                collect_block_locals(syms, &f.body, file);
            }
            ast::Statement::While(w) => collect_block_locals(syms, &w.body, file),
            ast::Statement::Loop(l) => collect_block_locals(syms, &l.body, file),
            ast::Statement::Spawn(s) => collect_block_locals(syms, &s.body, file),
            ast::Statement::Expr(e) => collect_expr_locals(syms, &e.expr, file),
            _ => {}
        }
    }
}

fn collect_expr_locals(syms: &mut Vec<SymbolInfo>, expr: &ast::Expr, file: &str) {
    match expr {
        ast::Expr::If(if_expr) => {
            collect_block_locals(syms, &if_expr.then_branch, file);
            if let Some(ast::ElseBranch::Block(else_block)) = &if_expr.else_branch {
                collect_block_locals(syms, else_block, file);
            }
        }
        ast::Expr::Match(m) => {
            for arm in &m.arms {
                if let ast::ArmBody::Block(b) = &arm.body {
                    collect_block_locals(syms, b, file);
                }
            }
        }
        ast::Expr::Lambda(l) => {
            for p in &l.params {
                syms.push(SymbolInfo {
                    name: p.name.clone(),
                    kind: SymbolKind::Parameter,
                    def_span: p.span,
                    type_info: p.type_name.as_deref().unwrap_or("_").to_string(),
                    filename: file.to_string(),
                });
            }
            collect_block_locals(syms, &l.body, file);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Find the identifier at a given cursor position
// ---------------------------------------------------------------------------

/// Find the name at cursor (1-based line, 1-based column) by walking the AST.
pub fn find_name_at(program: &ast::Program, line: usize, col: usize) -> Option<String> {
    for decl in &program.declarations {
        if let Some(name) = find_name_in_decl(decl, line, col) {
            return Some(name);
        }
    }
    None
}

fn span_contains(span: &Span, line: usize, col: usize, name_len: usize) -> bool {
    span.line == line && col >= span.column && col < span.column + name_len
}

fn find_name_in_decl(decl: &ast::Declaration, line: usize, col: usize) -> Option<String> {
    match decl {
        ast::Declaration::Function(f) => find_name_in_function(f, line, col),
        ast::Declaration::DataClass(d) => {
            // The span points to `data`, but the name is later on the same line.
            if d.span.line == line {
                return Some(d.name.clone());
            }
            for field in &d.fields {
                if span_contains(&field.span, line, col, field.name.len()) {
                    return Some(field.name.clone());
                }
            }
            for m in &d.methods {
                if let Some(n) = find_name_in_function(m, line, col) { return Some(n); }
            }
            None
        }
        ast::Declaration::Struct(s) => {
            if s.span.line == line {
                return Some(s.name.clone());
            }
            for field in &s.fields {
                if span_contains(&field.span, line, col, field.name.len()) {
                    return Some(field.name.clone());
                }
            }
            for m in &s.methods {
                if let Some(n) = find_name_in_function(m, line, col) { return Some(n); }
            }
            None
        }
        ast::Declaration::Enum(e) => {
            if e.span.line == line {
                return Some(e.name.clone());
            }
            for v in &e.variants {
                if v.span.line == line {
                    return Some(v.name.clone());
                }
            }
            None
        }
        ast::Declaration::Import(imp) => {
            if imp.span.line == line {
                return Some(imp.module_path.rsplit('.').next().unwrap_or(&imp.module_path).to_string());
            }
            None
        }
        _ => None,
    }
}

fn find_name_in_function(f: &ast::FunctionDecl, line: usize, col: usize) -> Option<String> {
    // Check function name
    if span_contains(&f.span, line, col, f.name.len()) {
        return Some(f.name.clone());
    }
    // Check params
    for p in &f.params {
        if span_contains(&p.span, line, col, p.name.len()) {
            return Some(p.name.clone());
        }
    }
    // Check body
    find_name_in_block(&f.body, line, col)
}

fn find_name_in_block(block: &ast::Block, line: usize, col: usize) -> Option<String> {
    for stmt in &block.statements {
        if let Some(n) = find_name_in_stmt(stmt, line, col) {
            return Some(n);
        }
    }
    None
}

fn find_name_in_stmt(stmt: &ast::Statement, line: usize, col: usize) -> Option<String> {
    match stmt {
        ast::Statement::VarDecl(v) => {
            if span_contains(&v.span, line, col, v.name.len()) {
                return Some(v.name.clone());
            }
            find_name_in_expr(&v.value, line, col)
        }
        ast::Statement::Assign(a) => {
            if let Some(n) = find_name_in_expr(&a.target, line, col) { return Some(n); }
            find_name_in_expr(&a.value, line, col)
        }
        ast::Statement::Return(r) => r.value.as_ref().and_then(|e| find_name_in_expr(e, line, col)),
        ast::Statement::Expr(e) => find_name_in_expr(&e.expr, line, col),
        ast::Statement::While(w) => {
            if let Some(n) = find_name_in_expr(&w.condition, line, col) { return Some(n); }
            find_name_in_block(&w.body, line, col)
        }
        ast::Statement::For(f) => {
            if span_contains(&f.span, line, col, f.name.len()) {
                return Some(f.name.clone());
            }
            if let Some(n) = find_name_in_expr(&f.iterable, line, col) { return Some(n); }
            find_name_in_block(&f.body, line, col)
        }
        ast::Statement::Loop(l) => find_name_in_block(&l.body, line, col),
        ast::Statement::Spawn(s) => find_name_in_block(&s.body, line, col),
        _ => None,
    }
}

fn find_name_in_expr(expr: &ast::Expr, line: usize, col: usize) -> Option<String> {
    match expr {
        ast::Expr::Name(n) => {
            if span_contains(&n.span, line, col, n.value.len()) {
                Some(n.value.clone())
            } else {
                None
            }
        }
        ast::Expr::Call(c) => {
            if let Some(n) = find_name_in_expr(&c.callee, line, col) { return Some(n); }
            for arg in &c.args { if let Some(n) = find_name_in_expr(arg, line, col) { return Some(n); } }
            None
        }
        ast::Expr::Member(m) => {
            if span_contains(&m.span, line, col, m.name.len()) {
                return Some(m.name.clone());
            }
            find_name_in_expr(&m.object, line, col)
        }
        ast::Expr::Binary(b) => {
            if let Some(n) = find_name_in_expr(&b.left, line, col) { return Some(n); }
            find_name_in_expr(&b.right, line, col)
        }
        ast::Expr::Unary(u) => find_name_in_expr(&u.value, line, col),
        ast::Expr::If(i) => {
            if let Some(n) = find_name_in_expr(&i.condition, line, col) { return Some(n); }
            if let Some(n) = find_name_in_block(&i.then_branch, line, col) { return Some(n); }
            if let Some(ast::ElseBranch::Block(eb)) = &i.else_branch { return find_name_in_block(eb, line, col); }
            None
        }
        ast::Expr::Match(m) => {
            if let Some(n) = find_name_in_expr(&m.subject, line, col) { return Some(n); }
            for arm in &m.arms {
                if let ast::ArmBody::Block(b) = &arm.body {
                    if let Some(n) = find_name_in_block(b, line, col) { return Some(n); }
                }
            }
            None
        }
        ast::Expr::List(l) => {
            for item in &l.items { if let Some(n) = find_name_in_expr(item, line, col) { return Some(n); } }
            None
        }
        ast::Expr::Lambda(l) => {
            for p in &l.params {
                if span_contains(&p.span, line, col, p.name.len()) { return Some(p.name.clone()); }
            }
            find_name_in_block(&l.body, line, col)
        }
        ast::Expr::FString(_) => None,
        ast::Expr::Move(m) => find_name_in_expr(&m.value, line, col),
        ast::Expr::Ref(r) => find_name_in_expr(&r.value, line, col),
        ast::Expr::MutRef(r) => find_name_in_expr(&r.value, line, col),
        ast::Expr::Question(q) => find_name_in_expr(&q.value, line, col),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn format_function_sig(f: &ast::FunctionDecl) -> String {
    let receiver = f.receiver_type.as_ref().map(|t| format!("{t}.")).unwrap_or_default();
    let params: Vec<String> = f.params.iter().map(|p| {
        let conv = p.convention.as_ref().map(|c| format!("{c} ")).unwrap_or_default();
        let ty = p.type_name.as_deref().unwrap_or("_");
        format!("{conv}{}: {ty}", p.name)
    }).collect();
    let ret = f.return_type.as_ref().map(|t| format!(" -> {t}")).unwrap_or_default();
    format!("fn {receiver}{}({}){ret}", f.name, params.join(", "))
}

fn format_extern_fn(ef: &ast::ExternFnDecl) -> String {
    let params: Vec<String> = ef.params.iter().map(|p| {
        let ty = p.type_name.as_deref().unwrap_or("_");
        format!("{}: {ty}", p.name)
    }).collect();
    let ret = ef.return_type.as_ref().map(|t| format!(" -> {t}")).unwrap_or_default();
    format!("extern fn {}({}){ret}", ef.name, params.join(", "))
}

fn format_data_class(d: &ast::DataClassDecl) -> String {
    let fields: Vec<String> = d.fields.iter().map(|f| {
        let ty = f.type_name.as_deref().unwrap_or("_");
        format!("{} {}: {ty}", if f.mutable { "var" } else { "val" }, f.name)
    }).collect();
    format!("data class {}({})", d.name, fields.join(", "))
}

fn format_enum(e: &ast::EnumDecl) -> String {
    let variants: Vec<String> = e.variants.iter().map(|v| {
        if !v.payload_types.is_empty() {
            format!("{}({})", v.name, v.payload_types.join(", "))
        } else {
            v.name.clone()
        }
    }).collect();
    format!("enum {} {{ {} }}", e.name, variants.join(", "))
}
