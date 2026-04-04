pub mod object_backend;
pub mod layout;
pub mod type_names;
pub use object_backend as cranelift;

pub fn backend_name() -> &'static str {
    object_backend::backend_name()
}

pub fn compile_path_to_native(input: &std::path::Path, output: &std::path::Path) -> Result<(), String> {
    object_backend::compile_path_to_native(input, output)
}
