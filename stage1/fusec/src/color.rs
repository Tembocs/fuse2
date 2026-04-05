use std::io::IsTerminal;

/// Controls whether ANSI colour codes are emitted in diagnostic output.
#[derive(Clone, Copy, Debug)]
pub enum ColorMode {
    /// Use colour when stderr is a TTY and NO_COLOR / TERM=dumb are not set.
    Auto,
    /// Always emit ANSI escape codes, even when piped.
    Always,
    /// Never emit ANSI escape codes.
    Never,
}

/// Applies ANSI SGR colour codes to diagnostic text.
///
/// When disabled (either explicitly via `Never` or because `Auto` detected
/// a non-TTY / NO_COLOR environment), all methods return the input unchanged.
pub struct Painter {
    enabled: bool,
}

impl Painter {
    pub fn new(mode: ColorMode) -> Self {
        let enabled = match mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => {
                std::io::stderr().is_terminal()
                    && std::env::var_os("NO_COLOR").is_none()
                    && std::env::var("TERM").as_deref() != Ok("dumb")
            }
        };
        Self { enabled }
    }

    /// Bold red — used for `error` label and error carets.
    pub fn error(&self, text: &str) -> String {
        self.wrap("\x1b[1;31m", text)
    }

    /// Bold yellow — used for `warning` label and warning carets.
    pub fn warning(&self, text: &str) -> String {
        self.wrap("\x1b[1;33m", text)
    }

    /// Cyan — used for `note` label.
    pub fn note(&self, text: &str) -> String {
        self.wrap("\x1b[36m", text)
    }

    /// Dim (grey) — used for line numbers, `|` margin, `-->` arrow.
    pub fn dim(&self, text: &str) -> String {
        self.wrap("\x1b[2m", text)
    }

    /// Bold white — used for highlighted spans in source.
    pub fn bold(&self, text: &str) -> String {
        self.wrap("\x1b[1m", text)
    }

    fn wrap(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("{code}{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}
