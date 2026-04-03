pub fn builtin_return_type(name: &str) -> Option<&'static str> {
    match name {
        "println" => Some("Unit"),
        "Some" | "None" => Some("Option"),
        "Ok" | "Err" => Some("Result"),
        _ => None,
    }
}

pub fn type_matches(expected: &str, actual: &str) -> bool {
    expected == actual
        || (expected.starts_with("Result") && actual.starts_with("Result"))
        || (expected.starts_with("Option") && actual.starts_with("Option"))
}
