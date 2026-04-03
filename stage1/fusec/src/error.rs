use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    kind: &'static str,
    message: String,
    filename: String,
    span: Span,
    hint: Option<String>,
}

impl Diagnostic {
    pub fn error(
        message: impl Into<String>,
        filename: impl Into<String>,
        span: Span,
        hint: Option<String>,
    ) -> Self {
        Self {
            kind: "error",
            message: message.into(),
            filename: filename.into(),
            span,
            hint,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn render(&self) -> String {
        let mut lines = vec![format!("{}: {}", self.kind, self.message)];
        if let Some(hint) = &self.hint {
            lines.push(format!("       {}", hint));
        }
        lines.push(format!(
            "  --> {}:{}:{}",
            self.filename, self.span.line, self.span.column
        ));
        lines.join("\n")
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

impl std::error::Error for Diagnostic {}
