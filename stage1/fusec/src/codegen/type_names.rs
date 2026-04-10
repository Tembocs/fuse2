use std::collections::HashMap;

/// Builtin generic type parameter table. Returns the formal type
/// parameter names for built-in generic types whose declarations do
/// not live in the loaded modules (List, Map, Option, Result, Chan,
/// Shared, Set). Returns None for any other type — callers should
/// fall back to looking up the type in the loaded modules' data
/// classes, structs, and enums.
///
/// This table is the single source of truth for builtin type-param
/// shapes used by `BuildSession::type_params_for_type` (B4.2) and the
/// substitution at extension call sites (B4.3). See
/// `docs/fuse-stage2-parity-plan.md` Phase B4.1.
pub fn builtin_type_params(canonical: &str) -> Option<Vec<String>> {
    match canonical {
        "List" | "Option" | "Chan" | "Shared" | "Set" => Some(vec!["T".to_string()]),
        "Map" => Some(vec!["K".to_string(), "V".to_string()]),
        "Result" => Some(vec!["T".to_string(), "E".to_string()]),
        _ => None,
    }
}

/// Build a substitution map from formal type parameter names to
/// concrete type argument strings. The two slices are zipped position-
/// wise; if they differ in length, only the common prefix is used.
/// An empty input pair yields an empty map.
pub fn build_type_param_map(
    type_params: &[String],
    concrete_args: &[String],
) -> HashMap<String, String> {
    type_params
        .iter()
        .zip(concrete_args.iter())
        .map(|(param, arg)| (param.clone(), arg.clone()))
        .collect()
}

/// Substitute type parameters in `type_name` with their concrete
/// values from `params`. The substitution is **whole-word**: an
/// identifier in `type_name` is replaced only when it matches a key
/// in `params` exactly. The identifier `Tail` will not match the
/// parameter `T`, even though `Tail` starts with `T`.
///
/// Identifier boundary rules: an identifier starts with an ASCII
/// letter or underscore and continues with ASCII alphanumerics or
/// underscores. Generic delimiters `<` `>` `,` ` ` are not part of
/// identifiers, so `List<T>` correctly substitutes `T` while
/// preserving the angle brackets.
///
/// If `params` is empty, the input is returned unchanged.
pub fn substitute_generics(type_name: &str, params: &HashMap<String, String>) -> String {
    if params.is_empty() {
        return type_name.to_string();
    }
    let bytes = type_name.as_bytes();
    let mut result = String::with_capacity(type_name.len());
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i];
        if is_ident_start(ch) {
            let start = i;
            i += 1;
            while i < bytes.len() && is_ident_continue(bytes[i]) {
                i += 1;
            }
            let ident = &type_name[start..i];
            if let Some(replacement) = params.get(ident) {
                result.push_str(replacement);
            } else {
                result.push_str(ident);
            }
        } else {
            // Non-identifier byte (`<`, `>`, `,`, ` `, etc.) — emit
            // verbatim. Type names are ASCII-safe so the byte index
            // matches a char boundary.
            result.push(ch as char);
            i += 1;
        }
    }
    result
}

#[inline]
fn is_ident_start(ch: u8) -> bool {
    ch.is_ascii_alphabetic() || ch == b'_'
}

#[inline]
fn is_ident_continue(ch: u8) -> bool {
    ch.is_ascii_alphanumeric() || ch == b'_'
}

pub fn split_generic_args(type_name: &str) -> Option<Vec<String>> {
    let start = type_name.find('<')?;
    let end = type_name.rfind('>')?;
    let inner = &type_name[start + 1..end];
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in inner.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }
    Some(args)
}

pub fn option_inner_type(type_name: &str) -> Option<String> {
    let args = split_generic_args(type_name)?;
    (type_name.trim().trim_end_matches('>').contains("Option") && args.len() == 1)
        .then(|| args[0].clone())
}

pub fn result_ok_type(type_name: &str) -> Option<String> {
    let args = split_generic_args(type_name)?;
    (type_name.trim().trim_end_matches('>').contains("Result") && args.len() == 2)
        .then(|| args[0].clone())
}

pub fn result_err_type(type_name: &str) -> Option<String> {
    let args = split_generic_args(type_name)?;
    (type_name.trim().trim_end_matches('>').contains("Result") && args.len() == 2)
        .then(|| args[1].clone())
}

pub fn chan_inner_type(type_name: &str) -> Option<String> {
    let args = split_generic_args(type_name)?;
    (type_name.trim().trim_end_matches('>').contains("Chan") && args.len() == 1)
        .then(|| args[0].clone())
}

pub fn shared_inner_type(type_name: &str) -> Option<String> {
    let args = split_generic_args(type_name)?;
    (type_name.trim().trim_end_matches('>').contains("Shared") && args.len() == 1)
        .then(|| args[0].clone())
}

/// Split a tuple type string "(T1,T2,...)" into element types.
pub fn split_tuple_types(type_name: &str) -> Option<Vec<String>> {
    let s = type_name.trim();
    if !s.starts_with('(') || !s.ends_with(')') {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    let mut types = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in inner.chars() {
        match ch {
            '<' | '(' => {
                depth += 1;
                current.push(ch);
            }
            '>' | ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                types.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        types.push(current.trim().to_string());
    }
    Some(types)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // -----------------------------------------------------------------
    // builtin_type_params (B4.1)
    // -----------------------------------------------------------------

    #[test]
    fn builtin_type_params_for_list_is_t() {
        assert_eq!(builtin_type_params("List"), Some(vec!["T".to_string()]));
    }

    #[test]
    fn builtin_type_params_for_map_is_k_v() {
        assert_eq!(
            builtin_type_params("Map"),
            Some(vec!["K".to_string(), "V".to_string()])
        );
    }

    #[test]
    fn builtin_type_params_for_result_is_t_e() {
        assert_eq!(
            builtin_type_params("Result"),
            Some(vec!["T".to_string(), "E".to_string()])
        );
    }

    #[test]
    fn builtin_type_params_covers_all_builtin_generics() {
        for ty in ["List", "Option", "Chan", "Shared", "Set"] {
            assert_eq!(
                builtin_type_params(ty),
                Some(vec!["T".to_string()]),
                "{ty} should have one type param T"
            );
        }
    }

    #[test]
    fn builtin_type_params_returns_none_for_user_types() {
        assert_eq!(builtin_type_params("MyData"), None);
        assert_eq!(builtin_type_params("Foo"), None);
        assert_eq!(builtin_type_params(""), None);
    }

    // -----------------------------------------------------------------
    // build_type_param_map (B4.1)
    // -----------------------------------------------------------------

    #[test]
    fn build_map_zips_params_with_args() {
        let formal = vec!["K".to_string(), "V".to_string()];
        let concrete = vec!["String".to_string(), "Int".to_string()];
        let m = build_type_param_map(&formal, &concrete);
        assert_eq!(m.get("K"), Some(&"String".to_string()));
        assert_eq!(m.get("V"), Some(&"Int".to_string()));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn build_map_handles_empty_inputs() {
        assert!(build_type_param_map(&[], &[]).is_empty());
        assert!(build_type_param_map(&["T".to_string()], &[]).is_empty());
        assert!(build_type_param_map(&[], &["Int".to_string()]).is_empty());
    }

    // -----------------------------------------------------------------
    // substitute_generics (B4.1) — the load-bearing function. The
    // canonical edge case from the plan is "Tail with param T must
    // remain Tail, not Intail".
    // -----------------------------------------------------------------

    #[test]
    fn substitute_replaces_single_param() {
        assert_eq!(
            substitute_generics("Option<T>", &map(&[("T", "Int")])),
            "Option<Int>"
        );
    }

    #[test]
    fn substitute_replaces_multiple_params() {
        assert_eq!(
            substitute_generics("Result<T, E>", &map(&[("T", "Int"), ("E", "String")])),
            "Result<Int, String>"
        );
    }

    #[test]
    fn substitute_handles_unspaced_arglist() {
        // Parser produces unspaced concatenation; substitution must
        // handle both forms identically.
        assert_eq!(
            substitute_generics("Map<K,V>", &map(&[("K", "String"), ("V", "Int")])),
            "Map<String,Int>"
        );
    }

    #[test]
    fn substitute_replaces_in_nested_generics() {
        assert_eq!(
            substitute_generics("List<Option<T>>", &map(&[("T", "Int")])),
            "List<Option<Int>>"
        );
        assert_eq!(
            substitute_generics(
                "Result<List<T>, E>",
                &map(&[("T", "Int"), ("E", "String")])
            ),
            "Result<List<Int>, String>"
        );
    }

    #[test]
    fn substitute_returns_input_when_no_match() {
        assert_eq!(
            substitute_generics("Int", &map(&[("T", "Int")])),
            "Int"
        );
        assert_eq!(
            substitute_generics("List<String>", &map(&[("T", "Int")])),
            "List<String>"
        );
    }

    #[test]
    fn substitute_does_not_match_identifier_prefix() {
        // The canonical regression test from the plan: param `T`
        // must NOT replace inside the identifier `Tail`.
        assert_eq!(
            substitute_generics("Tail", &map(&[("T", "Int")])),
            "Tail"
        );
        assert_eq!(
            substitute_generics("List<Tail>", &map(&[("T", "Int")])),
            "List<Tail>"
        );
        // And the symmetric case: identifier `KMap` should not be
        // affected by param `K`.
        assert_eq!(
            substitute_generics("KMap<V>", &map(&[("K", "String"), ("V", "Int")])),
            "KMap<Int>"
        );
    }

    #[test]
    fn substitute_does_not_match_identifier_suffix() {
        // Param `T` must not replace inside `MyT` either.
        assert_eq!(
            substitute_generics("MyT<U>", &map(&[("T", "Int"), ("U", "Int")])),
            "MyT<Int>"
        );
    }

    #[test]
    fn substitute_returns_input_when_map_is_empty() {
        let empty: HashMap<String, String> = HashMap::new();
        assert_eq!(substitute_generics("Option<T>", &empty), "Option<T>");
    }

    #[test]
    fn substitute_handles_underscores_in_identifiers() {
        // Identifiers can contain underscores; substitution must
        // treat `T_inner` as a single identifier.
        assert_eq!(
            substitute_generics("Box<T_inner>", &map(&[("T", "Int")])),
            "Box<T_inner>"
        );
    }
}
