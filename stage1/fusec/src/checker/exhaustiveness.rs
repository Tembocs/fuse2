use std::collections::HashSet;

use crate::ast::nodes::{LiteralValue, MatchExpr, Pattern};

#[derive(Default)]
pub(crate) struct MatchCoverage {
    pub(crate) covered: HashSet<String>,
    pub(crate) wildcard: bool,
}

pub(crate) fn collect_match_coverage(match_expr: &MatchExpr) -> MatchCoverage {
    let mut coverage = MatchCoverage::default();
    for arm in &match_expr.arms {
        match &arm.pattern {
            Pattern::Wildcard(_) => coverage.wildcard = true,
            Pattern::Variant(pattern) => {
                coverage
                    .covered
                    .insert(pattern.name.rsplit('.').next().unwrap_or(&pattern.name).to_string());
            }
            Pattern::Literal(pattern) => {
                let value = match &pattern.value {
                    LiteralValue::Bool(true) => "True".to_string(),
                    LiteralValue::Bool(false) => "False".to_string(),
                    LiteralValue::Int(value) => value.to_string(),
                    LiteralValue::Float(value) => value.to_string(),
                    LiteralValue::String(value) => value.clone(),
                };
                coverage.covered.insert(value);
            }
            Pattern::Name(_) => {}
            Pattern::Tuple(_) => { coverage.wildcard = true; }
        }
    }
    coverage
}

pub(crate) fn missing_match_message(
    ty: &str,
    covered: &HashSet<String>,
    wildcard: bool,
) -> Option<&'static str> {
    if wildcard {
        return None;
    }
    if ty.starts_with("Result") && !(covered.contains("Ok") && covered.contains("Err")) {
        return Some("non-exhaustive match for `Result`");
    }
    if ty.starts_with("Option") && !(covered.contains("Some") && covered.contains("None")) {
        return Some("non-exhaustive match for `Option`");
    }
    if ty == "Bool" && !(covered.contains("true") && covered.contains("false")) {
        return Some("non-exhaustive match for `Bool`");
    }
    None
}
