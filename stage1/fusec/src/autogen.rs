//! Auto-generation of interface method implementations from field metadata.
//!
//! When a `data class` or `@value struct` declares `implements Equatable`
//! (or Hashable, Comparable, Printable, Debuggable) without providing the
//! required methods manually, the compiler generates them from field names
//! and types at compile time. Generated functions are real `FunctionDecl`
//! AST nodes — they compile, inline, and optimize identically to hand-written
//! code. Zero runtime overhead.

use crate::ast::nodes::*;
use crate::error::Span;

// ---------------------------------------------------------------------------
// Type classification
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TypeKind {
    DataClass,
    ValueStruct,
    PlainStruct,
    Enum,
}

pub fn classify_type(annotations: &[Annotation], is_struct: bool) -> TypeKind {
    if !is_struct {
        return TypeKind::DataClass;
    }
    if annotations.iter().any(|a| a.is("value")) {
        TypeKind::ValueStruct
    } else {
        TypeKind::PlainStruct
    }
}

/// Returns true if the given (type_kind, interface_name) pair supports
/// automatic method generation from field metadata.
pub fn can_auto_generate(kind: TypeKind, interface: &str) -> bool {
    match kind {
        TypeKind::DataClass | TypeKind::ValueStruct => matches!(
            interface,
            "Equatable" | "Hashable" | "Comparable" | "Printable" | "Debuggable"
        ),
        TypeKind::PlainStruct => false,
        TypeKind::Enum => matches!(
            interface,
            "Equatable" | "Hashable" | "Printable" | "Debuggable"
        ),
    }
}

// ---------------------------------------------------------------------------
// Synthetic span used for all generated AST nodes.
// ---------------------------------------------------------------------------

fn syn() -> Span {
    Span::new(0, 0)
}

// ---------------------------------------------------------------------------
// AST builder helpers
// ---------------------------------------------------------------------------

fn make_name(n: &str) -> Expr {
    Expr::Name(Name { value: n.to_string(), span: syn() })
}

fn make_self() -> Expr {
    make_name("self")
}

fn make_other() -> Expr {
    make_name("other")
}

fn make_int(n: i64) -> Expr {
    Expr::Literal(Literal { value: LiteralValue::Int(n), span: syn() })
}

fn make_bool(b: bool) -> Expr {
    Expr::Literal(Literal { value: LiteralValue::Bool(b), span: syn() })
}

fn make_member(object: Expr, field: &str) -> Expr {
    Expr::Member(Member {
        object: Box::new(object),
        name: field.to_string(),
        optional: false,
        span: syn(),
    })
}

fn make_binary(left: Expr, op: &str, right: Expr) -> Expr {
    Expr::Binary(BinaryOp {
        left: Box::new(left),
        op: op.to_string(),
        right: Box::new(right),
        span: syn(),
    })
}

fn make_unary(op: &str, value: Expr) -> Expr {
    Expr::Unary(UnaryOp {
        op: op.to_string(),
        value: Box::new(value),
        span: syn(),
    })
}

fn make_call(callee: Expr, args: Vec<Expr>) -> Expr {
    Expr::Call(Call {
        callee: Box::new(callee),
        args,
        span: syn(),
    })
}

fn make_method_call(object: Expr, method: &str, args: Vec<Expr>) -> Expr {
    make_call(make_member(object, method), args)
}

fn make_ref(value: Expr) -> Expr {
    Expr::Ref(RefExpr { value: Box::new(value), span: syn() })
}

fn make_self_param() -> Param {
    Param {
        convention: Some("ref".to_string()),
        name: "self".to_string(),
        type_name: None,
        variadic: false,
        span: syn(),
    }
}

fn make_other_param(type_name: &str) -> Param {
    Param {
        convention: Some("ref".to_string()),
        name: "other".to_string(),
        type_name: Some(type_name.to_string()),
        variadic: false,
        span: syn(),
    }
}

fn make_var_decl(name: &str, mutable: bool, value: Expr) -> Statement {
    Statement::VarDecl(VarDecl {
        annotations: Vec::new(),
        mutable,
        name: name.to_string(),
        type_name: None,
        value,
        span: syn(),
    })
}

fn make_return(value: Expr) -> Statement {
    Statement::Return(ReturnStmt { value: Some(value), span: syn() })
}

fn make_expr_stmt(expr: Expr) -> Statement {
    Statement::Expr(ExprStmt { expr, span: syn() })
}

fn make_assign(target: Expr, value: Expr) -> Statement {
    Statement::Assign(Assign { target, value, span: syn() })
}

fn make_if(condition: Expr, then_body: Vec<Statement>, else_body: Option<Vec<Statement>>) -> Expr {
    Expr::If(IfExpr {
        condition: Box::new(condition),
        then_branch: Block { statements: then_body, span: syn() },
        else_branch: else_body.map(|stmts| ElseBranch::Block(Block { statements: stmts, span: syn() })),
        span: syn(),
    })
}

fn make_function(
    name: &str,
    receiver_type: &str,
    params: Vec<Param>,
    return_type: &str,
    body: Vec<Statement>,
) -> FunctionDecl {
    FunctionDecl {
        name: name.to_string(),
        type_params: Vec::new(),
        params,
        return_type: Some(return_type.to_string()),
        body: Block { statements: body, span: syn() },
        is_pub: false,
        annotations: Vec::new(),
        receiver_type: Some(receiver_type.to_string()),
        span: syn(),
    }
}

// ---------------------------------------------------------------------------
// Equatable: eq
// ---------------------------------------------------------------------------

/// Generate `fn Type.eq(ref self, ref other: Type) -> Bool`
/// Returns: `self.f1 == other.f1 and self.f2 == other.f2 and ...`
pub fn generate_eq(type_name: &str, fields: &[FieldDecl]) -> FunctionDecl {
    let body = if fields.is_empty() {
        vec![make_expr_stmt(make_bool(true))]
    } else {
        let mut expr = make_binary(
            make_member(make_self(), &fields[0].name),
            "==",
            make_member(make_other(), &fields[0].name),
        );
        for field in &fields[1..] {
            let cmp = make_binary(
                make_member(make_self(), &field.name),
                "==",
                make_member(make_other(), &field.name),
            );
            expr = make_binary(expr, "and", cmp);
        }
        vec![make_expr_stmt(expr)]
    };
    make_function(
        "eq",
        type_name,
        vec![make_self_param(), make_other_param(type_name)],
        "Bool",
        body,
    )
}

// ---------------------------------------------------------------------------
// Hashable: hash
// ---------------------------------------------------------------------------

/// Generate `fn Type.hash(ref self) -> Int`
/// Algorithm: `var h = 17; h = h * 31 + self.f.hash(); ...; h`
pub fn generate_hash(type_name: &str, fields: &[FieldDecl]) -> FunctionDecl {
    let mut stmts = vec![make_var_decl("h", true, make_int(17))];
    for field in fields {
        // h = h * 31 + self.field.hash()
        let field_hash = make_method_call(make_member(make_self(), &field.name), "hash", vec![]);
        let new_h = make_binary(
            make_binary(make_name("h"), "*", make_int(31)),
            "+",
            field_hash,
        );
        stmts.push(make_assign(make_name("h"), new_h));
    }
    stmts.push(make_expr_stmt(make_name("h")));
    make_function("hash", type_name, vec![make_self_param()], "Int", stmts)
}

// ---------------------------------------------------------------------------
// Comparable: compareTo
// ---------------------------------------------------------------------------

/// Generate `fn Type.compareTo(ref self, ref other: Type) -> Int`
/// Field-wise comparison: first non-zero wins.
pub fn generate_compare_to(type_name: &str, fields: &[FieldDecl]) -> FunctionDecl {
    let mut stmts = Vec::new();
    for field in fields {
        // val cmp = self.field.compareTo(ref other.field)
        let cmp_call = make_method_call(
            make_member(make_self(), &field.name),
            "compareTo",
            vec![make_ref(make_member(make_other(), &field.name))],
        );
        let var_name = format!("cmp_{}", field.name);
        stmts.push(make_var_decl(&var_name, false, cmp_call));
        // if cmp != 0 { return cmp }
        let check = make_if(
            make_binary(make_name(&var_name), "!=", make_int(0)),
            vec![make_return(make_name(&var_name))],
            None,
        );
        stmts.push(make_expr_stmt(check));
    }
    stmts.push(make_expr_stmt(make_int(0)));
    make_function(
        "compareTo",
        type_name,
        vec![make_self_param(), make_other_param(type_name)],
        "Int",
        stmts,
    )
}

// ---------------------------------------------------------------------------
// Printable: toString
// ---------------------------------------------------------------------------

/// Generate `fn Type.toString(ref self) -> String`
/// Returns: `f"TypeName({self.f1}, {self.f2})"`
pub fn generate_to_string(type_name: &str, fields: &[FieldDecl]) -> FunctionDecl {
    let template = if fields.is_empty() {
        format!("{type_name}()")
    } else {
        let parts: Vec<String> = fields
            .iter()
            .map(|f| format!("{{self.{}}}", f.name))
            .collect();
        format!("{type_name}({})", parts.join(", "))
    };
    let body = vec![make_expr_stmt(Expr::FString(FString {
        template,
        span: syn(),
    }))];
    make_function("toString", type_name, vec![make_self_param()], "String", body)
}

// ---------------------------------------------------------------------------
// Debuggable: debugString
// ---------------------------------------------------------------------------

/// Generate `fn Type.debugString(ref self) -> String`
/// Returns: `"TypeName { " + f"f1: {self.f1}, f2: {self.f2}" + " }"`
/// Uses concatenation for literal braces since f-strings don't support brace escaping.
pub fn generate_debug_string(type_name: &str, fields: &[FieldDecl]) -> FunctionDecl {
    let body = if fields.is_empty() {
        let s = Expr::Literal(Literal {
            value: LiteralValue::String(format!("{type_name} {{}}")),
            span: syn(),
        });
        vec![make_expr_stmt(s)]
    } else {
        let parts: Vec<String> = fields
            .iter()
            .map(|f| format!("{}: {{self.{}}}", f.name, f.name))
            .collect();
        // "TypeName { " + f"f1: {self.f1}, ..." + " }"
        let prefix = Expr::Literal(Literal {
            value: LiteralValue::String(format!("{type_name} {{ ")),
            span: syn(),
        });
        let middle = Expr::FString(FString {
            template: parts.join(", "),
            span: syn(),
        });
        let suffix = Expr::Literal(Literal {
            value: LiteralValue::String(" }".to_string()),
            span: syn(),
        });
        let concat = make_binary(make_binary(prefix, "+", middle), "+", suffix);
        vec![make_expr_stmt(concat)]
    };
    make_function("debugString", type_name, vec![make_self_param()], "String", body)
}
