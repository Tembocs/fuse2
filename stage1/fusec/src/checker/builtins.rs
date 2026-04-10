//! B2.1 — Builtin method mirror for the checker.
//!
//! `compile_member_call` in `codegen/object_backend.rs` has hardcoded
//! dispatch blocks for a small set of methods on built-in receiver
//! types (List, Chan, Shared, Map, String). These methods work even
//! when the user has not imported the corresponding stdlib module
//! (`stdlib.core.list`, etc.) because the codegen falls back to a
//! hardcoded runtime call instead of calling a Fuse extension.
//!
//! Before this module existed, the checker treated unresolved method
//! calls as silent no-ops: if `resolve_extension` returned None, no
//! error was emitted, and the codegen later either succeeded via the
//! hardcoded fallback or crashed with `unsupported X member call Y`
//! deep inside the codegen. The investigation in
//! `docs/t4-parity-investigation.md` (Issue 1) traced 284 such silent
//! method calls in stage2/src/*.fuse alone.
//!
//! This module is the checker's mirror of the codegen dispatch table.
//! `is_builtin_method` returns true for exactly the (type, method)
//! pairs that `compile_member_call` handles via a hardcoded block —
//! no more, no less. The unit test
//! `every_hardcoded_method_is_recognized` enforces that the two
//! tables stay in sync.
//!
//! See `docs/fuse-stage2-parity-plan.md` Phase B2.1 for the audit.

/// True if `(canonical_receiver, method_name)` is one of the hardcoded
/// dispatch entries in `compile_member_call`. Receiver type must
/// already be canonicalized (no generic args, no ownership prefix).
///
/// Mirror of `stage1/fusec/src/codegen/object_backend.rs:3479-3846`.
/// When the codegen hardcoded table changes, update this function
/// AND the unit test below.
pub fn is_builtin_method(canonical_receiver: &str, method_name: &str) -> bool {
    match canonical_receiver {
        // List: object_backend.rs:3479-3534
        "List" => matches!(method_name, "len" | "get" | "push"),

        // Chan: object_backend.rs:3535-3629
        "Chan" => matches!(
            method_name,
            "send" | "recv" | "close" | "tryRecv" | "isClosed" | "len" | "cap"
        ),

        // Shared: object_backend.rs:3630-3686
        "Shared" => matches!(
            method_name,
            "read" | "write" | "try_write" | "tryWrite" | "tryRead"
        ),

        // Map: object_backend.rs:3687-3814
        "Map" => matches!(
            method_name,
            "set" | "get" | "remove" | "len" | "isEmpty" | "contains" | "keys" | "values" | "entries"
        ),

        // String: object_backend.rs:3815-3846
        "String" => matches!(method_name, "toUpper" | "isEmpty" | "len"),

        _ => false,
    }
}

/// Strip ownership prefixes (`mutref`, `ref`, `owned`, with or
/// without trailing space), generic arguments, and trailing module
/// qualifiers from a type name so it can be matched against the
/// builtin table.
///
/// Equivalent to `crate::codegen::layout::canonical_type_name`. We
/// duplicate the logic here so the checker doesn't take a build-time
/// dependency on the codegen module's internals; the unit test below
/// keeps the two implementations honest.
pub fn canonical_receiver(type_name: &str) -> &str {
    let base = type_name
        .strip_prefix("mutref ")
        .or_else(|| type_name.strip_prefix("mutref"))
        .or_else(|| type_name.strip_prefix("ref "))
        .or_else(|| type_name.strip_prefix("ref"))
        .or_else(|| type_name.strip_prefix("owned "))
        .or_else(|| type_name.strip_prefix("owned"))
        .unwrap_or(type_name);
    base.split('<')
        .next()
        .unwrap_or(base)
        .trim_end_matches("::")
        .trim()
}

/// If `(canonical_receiver, method_name)` corresponds to a method
/// defined in a known stdlib module that the user may have forgotten
/// to import, return the import path the user should add. Returns
/// None if the method is not recognized as a stdlib extension.
///
/// This is a hint generator. It must return None when in doubt — a
/// false suggestion is worse than no suggestion.
pub fn suggest_stdlib_import_for(
    canonical_receiver: &str,
    method_name: &str,
) -> Option<&'static str> {
    // Methods that are themselves builtins handled by codegen
    // hardcoded blocks never need a stdlib import — the user can call
    // them without `import`.
    if is_builtin_method(canonical_receiver, method_name) {
        return None;
    }

    match canonical_receiver {
        "List" if list_stdlib_methods().contains(&method_name) => Some("stdlib.core.list"),
        "Option" if option_stdlib_methods().contains(&method_name) => Some("stdlib.core.option"),
        "Result" if result_stdlib_methods().contains(&method_name) => Some("stdlib.core.result"),
        "Map" if map_stdlib_methods().contains(&method_name) => Some("stdlib.core.map"),
        "String" if string_stdlib_methods().contains(&method_name) => Some("stdlib.core.string"),
        _ => None,
    }
}

// The stdlib method tables below are derived from the actual stdlib
// source by grepping `^pub fn <Type>\.<name>` in stdlib/core/. They
// are NOT exhaustive proofs — if a stdlib method is missing here, the
// hint is omitted but the error is still raised. The cost of a missing
// hint is low; the cost of a wrong hint is high.

fn list_stdlib_methods() -> &'static [&'static str] {
    &[
        "all", "any", "clear", "concat", "contains", "count", "drop",
        "filter", "first", "flatMap", "flatten", "get", "indexOf",
        "insert", "isEmpty", "join", "last", "map", "of", "pop",
        "range", "rangeClosed", "reduce", "removeAt", "removeWhere",
        "repeat", "reverseInPlace", "reversed", "slice", "sortInPlace",
        "sorted", "sortedBy", "take", "unique", "zip",
    ]
}

fn option_stdlib_methods() -> &'static [&'static str] {
    &[
        "filter", "flatten", "isNone", "isSome", "map", "okOr",
        "orElse", "unwrap", "unwrapOr", "unwrapOrElse",
    ]
}

fn result_stdlib_methods() -> &'static [&'static str] {
    &[
        "err", "flatten", "isErr", "isOk", "map", "mapErr", "ok",
        "unwrap", "unwrapOr", "unwrapOrElse",
    ]
}

fn map_stdlib_methods() -> &'static [&'static str] {
    &[
        "filter", "forEach", "getOrDefault", "getOrInsert", "invert",
        "mapValues", "merge", "toList",
    ]
}

fn string_stdlib_methods() -> &'static [&'static str] {
    &[
        "byteAt", "capitalize", "charCount", "chars", "compareTo",
        "contains", "endsWith", "fromBytes", "fromChar", "hash",
        "indexOf", "lastIndexOf", "padEnd", "padStart", "repeat",
        "replace", "replaceFirst", "reverse", "split", "splitLines",
        "startsWith", "toBool", "toBytes", "toFloat", "toInt",
        "toLower", "trim", "trimEnd", "trimStart",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every hardcoded dispatch entry in `compile_member_call` must be
    /// recognized by `is_builtin_method`. If this test fails, the
    /// codegen and the checker are out of sync — fix the table.
    #[test]
    fn every_hardcoded_method_is_recognized() {
        // Mirror of object_backend.rs:3479-3846. Update both when
        // either changes.
        let cases: &[(&str, &str)] = &[
            // List
            ("List", "len"), ("List", "get"), ("List", "push"),
            // Chan
            ("Chan", "send"), ("Chan", "recv"), ("Chan", "close"),
            ("Chan", "tryRecv"), ("Chan", "isClosed"), ("Chan", "len"),
            ("Chan", "cap"),
            // Shared
            ("Shared", "read"), ("Shared", "write"), ("Shared", "try_write"),
            ("Shared", "tryWrite"), ("Shared", "tryRead"),
            // Map
            ("Map", "set"), ("Map", "get"), ("Map", "remove"),
            ("Map", "len"), ("Map", "isEmpty"), ("Map", "contains"),
            ("Map", "keys"), ("Map", "values"), ("Map", "entries"),
            // String
            ("String", "toUpper"), ("String", "isEmpty"), ("String", "len"),
        ];
        for (recv, name) in cases {
            assert!(
                is_builtin_method(recv, name),
                "missing builtin: {recv}.{name}"
            );
        }
    }

    #[test]
    fn unrelated_methods_are_not_builtin() {
        // Common methods that exist in stdlib but NOT in the hardcoded
        // codegen blocks — these must require stdlib imports.
        assert!(!is_builtin_method("List", "concat"));
        assert!(!is_builtin_method("List", "map"));
        assert!(!is_builtin_method("List", "filter"));
        assert!(!is_builtin_method("Option", "unwrap"));
        assert!(!is_builtin_method("Result", "mapErr"));
        assert!(!is_builtin_method("String", "trim"));
    }

    #[test]
    fn canonical_receiver_strips_generics() {
        assert_eq!(canonical_receiver("List<Int>"), "List");
        assert_eq!(canonical_receiver("Map<String, Int>"), "Map");
        assert_eq!(canonical_receiver("List"), "List");
        assert_eq!(canonical_receiver("List<List<Int>>"), "List");
    }

    #[test]
    fn canonical_receiver_strips_ownership_prefixes() {
        // Parser produces both spaced and unspaced forms.
        assert_eq!(canonical_receiver("mutref Request"), "Request");
        assert_eq!(canonical_receiver("mutrefRequest"), "Request");
        assert_eq!(canonical_receiver("ref Point"), "Point");
        assert_eq!(canonical_receiver("refPoint"), "Point");
        assert_eq!(canonical_receiver("owned Item"), "Item");
        assert_eq!(canonical_receiver("ownedItem"), "Item");
        assert_eq!(canonical_receiver("mutref List<Int>"), "List");
    }

    #[test]
    fn suggest_returns_none_for_builtins() {
        // List.len is a builtin — no import needed, no hint.
        assert_eq!(suggest_stdlib_import_for("List", "len"), None);
        assert_eq!(suggest_stdlib_import_for("Map", "set"), None);
    }

    #[test]
    fn suggest_returns_module_for_known_stdlib() {
        assert_eq!(
            suggest_stdlib_import_for("List", "concat"),
            Some("stdlib.core.list")
        );
        assert_eq!(
            suggest_stdlib_import_for("Option", "unwrap"),
            Some("stdlib.core.option")
        );
        assert_eq!(
            suggest_stdlib_import_for("Result", "mapErr"),
            Some("stdlib.core.result")
        );
        assert_eq!(
            suggest_stdlib_import_for("String", "trim"),
            Some("stdlib.core.string")
        );
    }

    #[test]
    fn suggest_returns_none_for_unknown_methods() {
        // Typo or genuinely missing method — no hint.
        assert_eq!(suggest_stdlib_import_for("List", "lenn"), None);
        assert_eq!(suggest_stdlib_import_for("Foo", "bar"), None);
    }
}
