pub mod ast;
pub mod checker;
pub mod color;
pub mod common;
pub mod codegen;
pub mod error;
pub mod evaluator;
pub mod hir;
pub mod lexer;
pub mod parser;

use std::path::Path;

pub use error::{Diagnostic, Severity, Span};
pub use lexer::lex;
pub use parser::parse_source;

pub fn check_path(path: &Path) -> Vec<Diagnostic> {
    checker::check_file(path)
}

pub fn check_path_with_options(path: &Path, warn_unused: bool) -> Vec<Diagnostic> {
    checker::check_file_with_options(path, warn_unused)
}

pub fn check_file(path: &Path) -> Result<(), Vec<Diagnostic>> {
    let diagnostics = check_path(path);
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

pub fn check_path_output(path: &Path) -> String {
    check_path(path)
        .into_iter()
        .map(|diag| diag.render())
        .collect::<Vec<_>>()
        .join("\n")
}
