use std::fmt;

use crate::color::Painter;

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

/// Severity of a diagnostic message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Option<String>,
    message: String,
    filename: String,
    pub span: Span,
    hint: Option<String>,
    help: Option<String>,
    span_label: Option<String>,
}

impl Diagnostic {
    pub fn error(
        message: impl Into<String>,
        filename: impl Into<String>,
        span: Span,
        hint: Option<String>,
    ) -> Self {
        Self {
            severity: Severity::Error,
            code: None,
            message: message.into(),
            filename: filename.into(),
            span,
            hint,
            help: None,
            span_label: None,
        }
    }

    pub fn warning(
        message: impl Into<String>,
        filename: impl Into<String>,
        span: Span,
        hint: Option<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            code: None,
            message: message.into(),
            filename: filename.into(),
            span,
            hint,
            help: None,
            span_label: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_span_label(mut self, label: impl Into<String>) -> Self {
        self.span_label = Some(label.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    pub fn is_warning(&self) -> bool {
        self.severity == Severity::Warning
    }

    // -----------------------------------------------------------------------
    // Backward-compatible render (used by existing tests)
    // -----------------------------------------------------------------------

    pub fn render(&self) -> String {
        let mut lines = vec![format!("{}: {}", self.severity.label(), self.message)];
        if let Some(hint) = &self.hint {
            lines.push(format!("       {hint}"));
        }
        lines.push(format!(
            "  --> {}:{}:{}",
            self.filename, self.span.line, self.span.column
        ));
        lines.join("\n")
    }

    // -----------------------------------------------------------------------
    // Long format — rich context with source lines, carets, and help
    // -----------------------------------------------------------------------

    pub fn render_long(&self, source: Option<&str>, painter: &Painter) -> String {
        let severity_str = self.severity.label();
        let code_str = self
            .code
            .as_ref()
            .map(|c| format!("[{c}]"))
            .unwrap_or_default();

        // Header: severity[code]: message
        let header_label = match self.severity {
            Severity::Error => painter.error(&format!("{severity_str}{code_str}")),
            Severity::Warning => painter.warning(&format!("{severity_str}{code_str}")),
            Severity::Note => painter.note(&format!("{severity_str}{code_str}")),
        };
        let mut out = format!("{header_label}: {}\n", self.message);

        // Location arrow: --> file:line:col
        out.push_str(&format!(
            "  {} {}:{}:{}\n",
            painter.dim("-->"),
            self.filename,
            self.span.line,
            self.span.column
        ));

        // Context lines with caret
        if let Some(source) = source {
            let lines: Vec<&str> = source.lines().collect();
            let target_line = self.span.line.saturating_sub(1); // 0-indexed

            // Determine gutter width from line numbers we'll display
            let last_line_num = (target_line + 2).min(lines.len());
            let gutter_width = format!("{last_line_num}").len();

            // Empty gutter line
            out.push_str(&format!(
                " {} {}\n",
                " ".repeat(gutter_width),
                painter.dim("|")
            ));

            // Context: one line before (if available)
            if target_line > 0 {
                if let Some(prev) = lines.get(target_line - 1) {
                    out.push_str(&format!(
                        " {} {} {prev}\n",
                        painter.dim(&format!("{:>gutter_width$}", self.span.line - 1)),
                        painter.dim("|"),
                    ));
                }
            }

            // Target line
            if let Some(line_text) = lines.get(target_line) {
                out.push_str(&format!(
                    " {} {} {}\n",
                    painter.dim(&format!("{:>gutter_width$}", self.span.line)),
                    painter.dim("|"),
                    painter.bold(line_text),
                ));

                // Caret line
                let col = self.span.column.saturating_sub(1);
                let caret_padding = " ".repeat(col);
                let caret = "^".repeat(estimate_span_width(line_text, col));
                let label = self.span_label.as_deref().unwrap_or("");
                let caret_str = if label.is_empty() {
                    caret.clone()
                } else {
                    format!("{caret} {label}")
                };
                let colored_caret = match self.severity {
                    Severity::Error => painter.error(&caret_str),
                    Severity::Warning => painter.warning(&caret_str),
                    Severity::Note => painter.note(&caret_str),
                };
                out.push_str(&format!(
                    " {} {} {caret_padding}{colored_caret}\n",
                    " ".repeat(gutter_width),
                    painter.dim("|"),
                ));
            }

            // Context: one line after (if available)
            if let Some(next) = lines.get(target_line + 1) {
                out.push_str(&format!(
                    " {} {} {next}\n",
                    painter.dim(&format!("{:>gutter_width$}", self.span.line + 1)),
                    painter.dim("|"),
                ));
            }
        }

        // Hint (legacy field)
        if let Some(hint) = &self.hint {
            out.push_str(&format!(
                "   {} {}: {hint}\n",
                painter.dim("="),
                painter.note("note")
            ));
        }

        // Help
        if let Some(help) = &self.help {
            out.push_str(&format!(
                "   {} {}: {help}\n",
                painter.dim("="),
                painter.note("help")
            ));
        }

        out
    }

    // -----------------------------------------------------------------------
    // Short format — one line per diagnostic for editor integration
    // -----------------------------------------------------------------------

    pub fn render_short(&self) -> String {
        let severity = self.severity.label();
        let code_str = self
            .code
            .as_ref()
            .map(|c| format!("[{c}]"))
            .unwrap_or_default();
        format!(
            "{}:{}:{}: {severity}{code_str}: {}",
            self.filename, self.span.line, self.span.column, self.message
        )
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

impl std::error::Error for Diagnostic {}

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------

/// Render a summary line like `error: 3 errors found — compilation stopped`.
pub fn render_summary(diagnostics: &[Diagnostic], painter: &Painter) -> String {
    let errors = diagnostics.iter().filter(|d| d.is_error()).count();
    let warnings = diagnostics.iter().filter(|d| d.is_warning()).count();
    if errors == 0 && warnings == 0 {
        return String::new();
    }
    let mut parts = Vec::new();
    if errors > 0 {
        parts.push(format!(
            "{} {}",
            errors,
            if errors == 1 { "error" } else { "errors" }
        ));
    }
    if warnings > 0 {
        parts.push(format!(
            "{} {}",
            warnings,
            if warnings == 1 { "warning" } else { "warnings" }
        ));
    }
    let summary = parts.join(", ");
    if errors > 0 {
        painter.error(&format!("error: {summary} found — compilation stopped"))
    } else {
        painter.warning(&format!("warning: {summary} found"))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Estimate the width of the span to underline with carets.
/// Uses a heuristic: from the column to the next word boundary or end of line.
fn estimate_span_width(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    if col >= chars.len() {
        return 1;
    }
    let mut end = col;
    // Walk forward until whitespace, punctuation, or end of line
    while end < chars.len() && !chars[end].is_whitespace() && !is_delimiter(chars[end]) {
        end += 1;
    }
    let width = end - col;
    if width == 0 { 1 } else { width }
}

fn is_delimiter(ch: char) -> bool {
    matches!(ch, '(' | ')' | '{' | '}' | '[' | ']' | ',' | ';' | ':')
}
