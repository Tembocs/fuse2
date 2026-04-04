use std::collections::HashMap;
use std::path::Path;

use crate::ast::nodes as fa;

pub const ENTRY_SYMBOL: &str = "fuse_user_entry";

#[derive(Clone, Debug, Default)]
pub struct ProgramLayout {
    data: HashMap<String, DataLayout>,
}

impl ProgramLayout {
    pub fn new(data_classes: impl IntoIterator<Item = fa::DataClassDecl>) -> Self {
        let mut layout = Self::default();
        for data in data_classes {
            layout
                .data
                .insert(data.name.clone(), DataLayout::new(&data));
        }
        layout
    }

    pub fn data_layout(&self, type_name: &str) -> Option<&DataLayout> {
        self.data.get(canonical_type_name(type_name))
    }
}

#[derive(Clone, Debug)]
pub struct DataLayout {
    field_indices: HashMap<String, usize>,
    field_count: usize,
}

impl DataLayout {
    fn new(data: &fa::DataClassDecl) -> Self {
        let field_indices = data
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| (field.name.clone(), index))
            .collect();
        Self {
            field_indices,
            field_count: data.fields.len(),
        }
    }

    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.field_indices.get(name).copied()
    }

    pub fn field_count(&self) -> usize {
        self.field_count
    }
}

pub fn abi_name() -> &'static str {
    "stage1-cranelift-runtime"
}

pub fn canonical_type_name(type_name: &str) -> &str {
    type_name.split('<').next().unwrap_or(type_name)
}

pub fn function_symbol(module_path: &Path, name: &str) -> String {
    format!("fuse_fn_{}_{}", sanitize_path(module_path), sanitize_name(name))
}

pub fn destructor_symbol(module_path: &Path, type_name: &str) -> String {
    format!(
        "fuse_del_{}_{}",
        sanitize_path(module_path),
        sanitize_name(type_name)
    )
}

pub fn data_type_name(module_path: &Path, type_name: &str) -> String {
    format!("{}::{}", sanitize_path(module_path), type_name)
}

fn sanitize_path(path: &Path) -> String {
    path.components()
        .map(|component| sanitize_name(&component.as_os_str().to_string_lossy()))
        .collect::<Vec<_>>()
        .join("_")
}

fn sanitize_name(value: &str) -> String {
    value.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}
