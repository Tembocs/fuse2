use std::collections::HashMap;
use std::path::PathBuf;

use crate::ast::nodes::{DataClassDecl, EnumDecl, FunctionDecl, ImportDecl};

#[derive(Clone, Debug)]
pub struct Module {
    pub path: PathBuf,
    pub filename: String,
    pub imports: Vec<ImportDecl>,
    pub functions: Vec<FunctionDecl>,
    pub data_classes: Vec<DataClassDecl>,
    pub enums: Vec<EnumDecl>,
    pub extension_functions: HashMap<(String, String), FunctionDecl>,
}
