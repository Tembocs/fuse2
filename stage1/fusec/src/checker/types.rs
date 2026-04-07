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
        || expected == "!" || actual == "!"
        || (expected.starts_with("Result") && actual.starts_with("Result"))
        || (expected.starts_with("Option") && actual.starts_with("Option"))
        || (expected.starts_with("List") && actual.starts_with("List"))
        || (expected.starts_with("Map") && actual.starts_with("Map"))
        || (expected.starts_with("Chan") && actual.starts_with("Chan"))
        || (expected.starts_with("Shared") && actual.starts_with("Shared"))
}

/// All numeric primitive types recognized by the language.
pub fn is_numeric_type(ty: &str) -> bool {
    matches!(
        ty,
        "Int" | "Float" | "Float32" | "Int8" | "UInt8" | "Int32" | "UInt32" | "UInt64"
    )
}

/// All primitive types recognized by the language.
pub fn is_primitive_type(ty: &str) -> bool {
    is_numeric_type(ty) || matches!(ty, "Bool" | "String" | "Unit")
}
