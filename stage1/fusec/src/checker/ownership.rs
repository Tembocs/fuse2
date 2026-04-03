use crate::ast::nodes::Expr;

pub(crate) fn root_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name(name) => Some(&name.value),
        Expr::Member(member) => root_name(&member.object),
        _ => None,
    }
}
