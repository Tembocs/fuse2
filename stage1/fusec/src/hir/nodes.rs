use std::collections::HashMap;
use std::path::PathBuf;

use crate::ast::nodes::{ConstDecl, DataClassDecl, EnumDecl, ExternFnDecl, FunctionDecl, ImportDecl, InterfaceDecl, StructDecl};

#[derive(Clone, Debug)]
pub struct Module {
    pub path: PathBuf,
    pub filename: String,
    pub imports: Vec<ImportDecl>,
    pub functions: Vec<FunctionDecl>,
    pub data_classes: Vec<DataClassDecl>,
    pub enums: Vec<EnumDecl>,
    pub extern_fns: Vec<ExternFnDecl>,
    pub structs: Vec<StructDecl>,
    pub consts: Vec<ConstDecl>,
    pub interfaces: Vec<InterfaceDecl>,
    pub extension_functions: HashMap<(String, String), FunctionDecl>,
}
