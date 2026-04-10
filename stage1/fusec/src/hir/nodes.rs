use std::collections::BTreeMap;
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
    // BTreeMap so the codegen `for ((recv, name), function) in
    // module.extension_functions.clone()` loop in load_module_recursive
    // (object_backend.rs) iterates in deterministic order. See B1.2 audit.
    pub extension_functions: BTreeMap<(String, String), FunctionDecl>,
}
