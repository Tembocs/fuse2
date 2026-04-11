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
    // B9.2 — track both `<>` AND `()` depth. The old version only
    // incremented on `<` / `>`, so a generic argument that was itself
    // a tuple (e.g. `Result<(Int,String),String>`) was split on the
    // tuple's *internal* comma, corrupting every downstream consumer
    // (`result_ok_type`, `option_inner_type`, `list_inner_type`, etc.)
    // and silently giving pattern-bound variables the wrong type —
    // which then surfaced as `cannot infer member `0`` when code tried
    // to access a tuple field on the bound variable. Paren depth
    // keeps the tuple intact.
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

#[cfg(test)]
mod split_generic_args_tests {
    use super::split_generic_args;

    #[test]
    fn splits_flat_args() {
        assert_eq!(
            split_generic_args("Map<String,Int>"),
            Some(vec!["String".to_string(), "Int".to_string()])
        );
    }

    #[test]
    fn splits_nested_generic_arg() {
        assert_eq!(
            split_generic_args("Map<String,List<Int>>"),
            Some(vec!["String".to_string(), "List<Int>".to_string()])
        );
    }

    #[test]
    fn splits_result_wrapping_tuple() {
        // B9.2 regression guard — the tuple's internal comma must
        // not split the outer Result<Ok, Err>.
        assert_eq!(
            split_generic_args("Result<(Int,String),String>"),
            Some(vec!["(Int,String)".to_string(), "String".to_string()])
        );
    }

    #[test]
    fn splits_option_wrapping_tuple() {
        assert_eq!(
            split_generic_args("Option<(Int,String)>"),
            Some(vec!["(Int,String)".to_string()])
        );
    }

    #[test]
    fn splits_result_wrapping_nested_tuple() {
        assert_eq!(
            split_generic_args("Result<(Int,(String,Bool)),String>"),
            Some(vec![
                "(Int,(String,Bool))".to_string(),
                "String".to_string(),
            ])
        );
    }
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

pub fn list_inner_type(type_name: &str) -> Option<String> {
    let args = split_generic_args(type_name)?;
    (type_name.trim().trim_end_matches('>').contains("List") && args.len() == 1)
        .then(|| args[0].clone())
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

/// B7.3 — Unify the types produced by each arm of a `match` (or
/// `when`) expression used in expression position. The rules are
/// enumerated in `docs/fuse-stage2-parity-plan.md` Wave B7 and
/// mirrored in section 1.8 of `docs/fuse-language-guide-2.md`:
///
/// - **U1 — Identity.** All arms with the same concrete type → that
///   type.
/// - **U2 — Empty list promotion.** A mix of `List<Unknown>` / `List`
///   and `List<X>` → `List<X>`.
/// - **U3 — Option None promotion.** A mix of `Option<Unknown>` and
///   `Option<X>` → `Option<X>`.
/// - **U4 — Result half promotion.** A mix of `Result<T, Unknown>`
///   and `Result<Unknown, E>` → `Result<T, E>`. Either "half-known"
///   form may appear without the other; the known half is preserved.
/// - **U5 — Incompatible concrete arms.** Returns `None`. The
///   checker (B7.4) reports the diagnostic; codegen continues with
///   a None type so later passes do not panic.
/// - **U6 — No information.** Every arm reported `None` → `None`.
///
/// Arms whose body inference produced `None` (no type info) are
/// ignored when computing the unified type — they neither contribute
/// nor block unification. If *every* arm is `None`, the result is
/// `None` (Rule U6).
///
/// The helper is pure and operates on surface type-name strings. It
/// does not know anything about the checker's binding tables, the
/// codegen's runtime values, or Cranelift IR. Both the checker and
/// the codegen call it so they agree on arm unification semantics.
pub fn unify_match_arm_types(arm_types: &[Option<String>]) -> Option<String> {
    let knowns: Vec<&str> = arm_types
        .iter()
        .filter_map(|t| t.as_deref())
        .collect();
    if knowns.is_empty() {
        return None; // U6
    }

    // Identity fast path (U1). If every known arm is the same
    // string, we are done.
    if knowns.iter().all(|t| *t == knowns[0]) {
        return Some(knowns[0].to_string());
    }

    // U2 — Empty list promotion. Any arm whose type is bare `List`
    // or `List<Unknown>` is promoted to a sibling's concrete
    // `List<X>`. If no sibling is concrete, promotion fails.
    if knowns.iter().all(|t| is_list_type(t)) {
        let concrete_list = knowns
            .iter()
            .find(|t| {
                let inner = list_inner_type(t);
                inner.is_some() && inner.as_deref() != Some("Unknown")
            })
            .copied();
        if let Some(c) = concrete_list {
            return Some(c.to_string());
        }
        // All-unknown list arms — fall through to identity; if
        // every arm was "List" or "List<Unknown>" the identity
        // branch above already handled it, so this is a mix we
        // cannot promote.
        return Some("List<Unknown>".to_string());
    }

    // U3 — Option<None> promotion.
    if knowns.iter().all(|t| is_option_type(t)) {
        let concrete_opt = knowns
            .iter()
            .find(|t| {
                let inner = option_inner_type(t);
                inner.is_some() && inner.as_deref() != Some("Unknown")
            })
            .copied();
        if let Some(c) = concrete_opt {
            return Some(c.to_string());
        }
        return Some("Option<Unknown>".to_string());
    }

    // U4 — Result half promotion.
    if knowns.iter().all(|t| is_result_type(t)) {
        // Collect known T halves and known E halves.
        let mut ok_ty: Option<String> = None;
        let mut err_ty: Option<String> = None;
        for t in &knowns {
            if let Some(a) = result_ok_type(t) {
                if a != "Unknown" && ok_ty.is_none() {
                    ok_ty = Some(a);
                }
            }
            if let Some(e) = result_err_type(t) {
                if e != "Unknown" && err_ty.is_none() {
                    err_ty = Some(e);
                }
            }
        }
        let t = ok_ty.unwrap_or_else(|| "Unknown".to_string());
        let e = err_ty.unwrap_or_else(|| "Unknown".to_string());
        return Some(format!("Result<{t}, {e}>"));
    }

    // U5 — Incompatible concrete arms.
    None
}

fn is_list_type(t: &str) -> bool {
    let canonical_head = t.trim().split('<').next().unwrap_or("").trim();
    canonical_head == "List"
}

fn is_option_type(t: &str) -> bool {
    let canonical_head = t.trim().split('<').next().unwrap_or("").trim();
    canonical_head == "Option"
}

fn is_result_type(t: &str) -> bool {
    let canonical_head = t.trim().split('<').next().unwrap_or("").trim();
    canonical_head == "Result"
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

    // -----------------------------------------------------------------
    // unify_match_arm_types (B7.3)
    // -----------------------------------------------------------------

    fn types(values: &[&str]) -> Vec<Option<String>> {
        values.iter().map(|v| Some(v.to_string())).collect()
    }

    #[test]
    fn unify_u1_identity() {
        // All arms produce the same concrete type.
        assert_eq!(
            unify_match_arm_types(&types(&["Int", "Int", "Int"])),
            Some("Int".to_string())
        );
        assert_eq!(
            unify_match_arm_types(&types(&["List<String>", "List<String>"])),
            Some("List<String>".to_string())
        );
    }

    #[test]
    fn unify_u2_empty_list_promotion() {
        // `[]` has `List<Unknown>`. A sibling `List<String>`
        // promotes the unknowns to `List<String>`.
        assert_eq!(
            unify_match_arm_types(&types(&["List<String>", "List<Unknown>"])),
            Some("List<String>".to_string())
        );
        assert_eq!(
            unify_match_arm_types(&types(&["List<Unknown>", "List<Int>"])),
            Some("List<Int>".to_string())
        );
        // Bare `List` (no angle brackets) is the canonical empty.
        assert_eq!(
            unify_match_arm_types(&types(&["List<Int>", "List"])),
            Some("List<Int>".to_string())
        );
        // All unknown → stay unknown, but still resolved.
        assert_eq!(
            unify_match_arm_types(&types(&["List<Unknown>", "List<Unknown>"])),
            Some("List<Unknown>".to_string())
        );
    }

    #[test]
    fn unify_u3_option_none_promotion() {
        // `None` inferred as `Option<Unknown>`, promoted by
        // sibling `Option<Int>`.
        assert_eq!(
            unify_match_arm_types(&types(&["Option<Int>", "Option<Unknown>"])),
            Some("Option<Int>".to_string())
        );
        assert_eq!(
            unify_match_arm_types(&types(&["Option<Unknown>", "Option<String>"])),
            Some("Option<String>".to_string())
        );
    }

    #[test]
    fn unify_u4_result_half_promotion() {
        // `Ok(x)` → `Result<Int, Unknown>`, `Err(s)` → `Result<Unknown, String>`.
        assert_eq!(
            unify_match_arm_types(&types(&["Result<Int, Unknown>", "Result<Unknown, String>"])),
            Some("Result<Int, String>".to_string())
        );
        // Full symmetry.
        assert_eq!(
            unify_match_arm_types(&types(&["Result<Unknown, String>", "Result<Int, Unknown>"])),
            Some("Result<Int, String>".to_string())
        );
        // Third arm with fully concrete Result<Int, String>
        // should not displace the existing unification.
        assert_eq!(
            unify_match_arm_types(&types(&[
                "Result<Int, Unknown>",
                "Result<Unknown, String>",
                "Result<Int, String>"
            ])),
            Some("Result<Int, String>".to_string())
        );
    }

    #[test]
    fn unify_u5_incompatible_arms_returns_none() {
        // Mix of unrelated concrete types.
        assert_eq!(unify_match_arm_types(&types(&["Int", "String"])), None);
        // Mix of List and Option.
        assert_eq!(unify_match_arm_types(&types(&["List<Int>", "Option<Int>"])), None);
        // Mix of Result and Option.
        assert_eq!(unify_match_arm_types(&types(&["Result<Int, String>", "Option<Int>"])), None);
    }

    #[test]
    fn unify_u6_no_information() {
        // Every arm reported None → None.
        assert_eq!(unify_match_arm_types(&[None, None, None]), None);
        assert_eq!(unify_match_arm_types(&[]), None);
    }

    #[test]
    fn unify_ignores_unknown_arms_when_others_are_concrete() {
        // Three arms: one concrete, two unknown. The concrete
        // arm drives the unification; the None arms are ignored.
        assert_eq!(
            unify_match_arm_types(&[Some("Int".to_string()), None, None]),
            Some("Int".to_string())
        );
    }

    #[test]
    fn unify_mixed_list_handles_whitespace() {
        // Parser-emitted type strings sometimes have spaces
        // inside angle brackets. The helper must still unify
        // them correctly.
        assert_eq!(
            unify_match_arm_types(&types(&["List<String>", "List< Unknown >"])),
            Some("List<String>".to_string())
        );
    }
}
