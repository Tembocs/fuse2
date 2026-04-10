use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::ast::nodes::{Declaration, Program as AstProgram};

use super::Module;

pub fn lower_program(program: &AstProgram, path: PathBuf) -> Module {
    let mut imports = Vec::new();
    let mut functions = Vec::new();
    let mut data_classes = Vec::new();
    let mut enums = Vec::new();
    let mut extern_fns = Vec::new();
    let mut structs = Vec::new();
    let mut consts = Vec::new();
    let mut interfaces = Vec::new();
    // BTreeMap (not HashMap) for determinism — see B1.2 audit. The
    // codegen iterates this map in load_module_recursive.
    let mut extension_functions = BTreeMap::new();

    for declaration in &program.declarations {
        match declaration {
            Declaration::Import(import_decl) => imports.push(import_decl.clone()),
            Declaration::Function(function) => {
                if let Some(receiver_type) = &function.receiver_type {
                    extension_functions.insert((receiver_type.clone(), function.name.clone()), function.clone());
                } else {
                    functions.push(function.clone());
                }
            }
            Declaration::DataClass(data_class) => data_classes.push(data_class.clone()),
            Declaration::Enum(enum_decl) => enums.push(enum_decl.clone()),
            Declaration::ExternFn(extern_fn) => extern_fns.push(extern_fn.clone()),
            Declaration::Struct(struct_decl) => structs.push(struct_decl.clone()),
            Declaration::Const(const_decl) => consts.push(const_decl.clone()),
            Declaration::Interface(iface) => interfaces.push(iface.clone()),
        }
    }

    Module {
        path,
        filename: program.filename.clone(),
        imports,
        functions,
        data_classes,
        enums,
        extern_fns,
        structs,
        consts,
        interfaces,
        extension_functions,
    }
}
