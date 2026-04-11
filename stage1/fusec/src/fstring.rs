//! F-string template parsing shared between the codegen and evaluator
//! f-string re-scanners. The lexer stores the raw characters between the
//! `f"` and the closing `"` in the token's text, with `{{` and `}}`
//! preserved as doubled braces. This helper walks that template and
//! produces a sequence of literal and interpolation chunks, resolving
//! `{{` → `{` and `}}` → `}`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FStringPart<'a> {
    Literal(String),
    Interp(&'a str),
}

/// Split an f-string template into alternating literal and interpolation
/// parts. `{{` and `}}` outside interpolations are resolved to literal
/// `{` and `}`. A single `{` begins an interpolation whose body extends
/// to the matching `}` (with nested brace balancing for map literals).
pub fn parse_fstring_template(template: &str) -> Result<Vec<FStringPart<'_>>, String> {
    let mut parts: Vec<FStringPart<'_>> = Vec::new();
    let mut literal = String::new();
    let mut rest = template;
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix("{{") {
            literal.push('{');
            rest = stripped;
            continue;
        }
        if let Some(stripped) = rest.strip_prefix("}}") {
            literal.push('}');
            rest = stripped;
            continue;
        }
        if rest.starts_with('{') {
            if !literal.is_empty() {
                parts.push(FStringPart::Literal(std::mem::take(&mut literal)));
            }
            let after = &rest[1..];
            let end = fstring_brace_end(after)
                .ok_or_else(|| "unterminated f-string interpolation".to_string())?;
            parts.push(FStringPart::Interp(&after[..end]));
            rest = &after[end + 1..];
            continue;
        }
        let c = rest.chars().next().unwrap();
        literal.push(c);
        rest = &rest[c.len_utf8()..];
    }
    if !literal.is_empty() {
        parts.push(FStringPart::Literal(literal));
    }
    Ok(parts)
}

/// Find the closing `}` for an f-string interpolation, accounting for
/// nested brace pairs (e.g. map literals or nested blocks inside the
/// interpolation expression). Returns the byte offset of the terminating
/// `}` within `s`, or `None` if unterminated.
pub fn fstring_brace_end(s: &str) -> Option<usize> {
    let mut depth: usize = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' if depth == 0 => return Some(i),
            '}' => depth -= 1,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literal(s: &str) -> FStringPart<'static> {
        FStringPart::Literal(s.to_string())
    }

    #[test]
    fn plain_literal_only() {
        let parts = parse_fstring_template("hello world").unwrap();
        assert_eq!(parts, vec![literal("hello world")]);
    }

    #[test]
    fn single_interpolation() {
        let parts = parse_fstring_template("x = {x}").unwrap();
        assert_eq!(
            parts,
            vec![literal("x = "), FStringPart::Interp("x")]
        );
    }

    #[test]
    fn escaped_open_brace() {
        // B10.2.3: `{{hello` lexes the leading `{{` as a literal `{`.
        let parts = parse_fstring_template("{{hello").unwrap();
        assert_eq!(parts, vec![literal("{hello")]);
    }

    #[test]
    fn escaped_close_brace() {
        let parts = parse_fstring_template("hello}}").unwrap();
        assert_eq!(parts, vec![literal("hello}")]);
    }

    #[test]
    fn escaped_brace_pair() {
        // B10.1 success criterion: `f"{{hello}}"` lexes as `{hello}`.
        let parts = parse_fstring_template("{{hello}}").unwrap();
        assert_eq!(parts, vec![literal("{hello}")]);
    }

    #[test]
    fn escape_then_interpolation() {
        // `f"{{x = {x}}}"` → `{x = ` + Interp(x) + `}`
        let parts = parse_fstring_template("{{x = {x}}}").unwrap();
        assert_eq!(
            parts,
            vec![
                literal("{x = "),
                FStringPart::Interp("x"),
                literal("}"),
            ]
        );
    }

    #[test]
    fn interpolation_with_nested_map_literal() {
        // The map literal's braces are balanced inside the interp.
        let parts = parse_fstring_template("{ {a: 1} }").unwrap();
        assert_eq!(parts, vec![FStringPart::Interp(" {a: 1} ")]);
    }

    #[test]
    fn escape_surrounding_nested_interp() {
        // `f"{{ { {a: 1} } }}"` → `{ ` + Interp(` {a: 1} `) + ` }`
        let parts = parse_fstring_template("{{ { {a: 1} } }}").unwrap();
        assert_eq!(
            parts,
            vec![
                literal("{ "),
                FStringPart::Interp(" {a: 1} "),
                literal(" }"),
            ]
        );
    }

    #[test]
    fn lone_close_brace_is_legacy_literal() {
        // Lone `}` at literal position is accepted as a literal `}` to
        // preserve pre-B10 lenient behavior for existing programs.
        let parts = parse_fstring_template("a}b").unwrap();
        assert_eq!(parts, vec![literal("a}b")]);
    }

    #[test]
    fn unterminated_interpolation_errors() {
        assert!(parse_fstring_template("hello {x").is_err());
    }

    #[test]
    fn buildwrapper_cargo_toml_fragment() {
        // Exercises the canonical case from stage2/src/main.fuse:465.
        let parts =
            parse_fstring_template("fuse-runtime = {{ path = \"{runtimePath}\" }}").unwrap();
        assert_eq!(
            parts,
            vec![
                literal("fuse-runtime = { path = \""),
                FStringPart::Interp("runtimePath"),
                literal("\" }"),
            ]
        );
    }
}
