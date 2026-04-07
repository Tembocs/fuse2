use std::collections::HashMap;
use std::fs;
use std::path::Path;

use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
    ImportSection, Instruction, MemorySection, MemoryType, Module, TypeSection, ValType,
};

use crate::ast::nodes as fa;
use crate::common::resolve_import_path;
use crate::hir::lower_program;
use crate::parser::parse_source;

use super::layout::ProgramLayout;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn compile_path_to_wasm(input: &Path, output: &Path) -> Result<(), String> {
    let input = input.canonicalize().unwrap_or_else(|_| input.to_path_buf());
    let session = WasmSession::load(&input)?;
    let wasm_bytes = session.emit()?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create output directory: {e}"))?;
    }
    fs::write(output, &wasm_bytes)
        .map_err(|e| format!("failed to write .wasm file: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Session: load and compile
// ---------------------------------------------------------------------------

struct WasmSession {
    root_path: std::path::PathBuf,
    functions: Vec<FuseFunction>,
    entry_name: Option<String>,
}

struct FuseFunction {
    name: String,
    params: Vec<fa::Param>,
    return_type: Option<String>,
    body: fa::Block,
}

impl WasmSession {
    fn load(path: &Path) -> Result<Self, String> {
        let source = fs::read_to_string(path)
            .map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
        let filename = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let program = parse_source(&source, &filename)
            .map_err(|d| d.render())?;

        let mut functions = Vec::new();
        let mut entry_name = None;

        for decl in &program.declarations {
            if let fa::Declaration::Function(f) = decl {
                if f.annotations.iter().any(|a| a.is("entrypoint")) {
                    entry_name = Some(f.name.clone());
                }
                functions.push(FuseFunction {
                    name: f.name.clone(),
                    params: f.params.clone(),
                    return_type: f.return_type.clone(),
                    body: f.body.clone(),
                });
            }
        }

        Ok(Self {
            root_path: path.to_path_buf(),
            functions,
            entry_name,
        })
    }

    fn emit(&self) -> Result<Vec<u8>, String> {
        let mut module = Module::new();

        // --- Type section ---
        let mut types = TypeSection::new();

        // Type 0: () -> i32 (entry function returning exit code)
        types.ty().function(vec![], vec![ValType::I32]);

        // Type 1: (i32) -> () (fd_write-like import for future I/O)
        types.ty().function(vec![ValType::I32], vec![]);

        module.section(&types);

        // --- Import section ---
        // Import WASI fd_write for future println support.
        let mut imports = ImportSection::new();
        imports.import(
            "wasi_snapshot_preview1",
            "proc_exit",
            EntityType::Function(1), // type 1: (i32) -> ()
        );
        module.section(&imports);

        // --- Function section ---
        let mut funcs = FunctionSection::new();

        // Function 0 (after 1 import): _start, type 0
        funcs.function(0);

        // Add user functions (all as type 0 for now — simplified)
        for _ in &self.functions {
            funcs.function(0);
        }
        module.section(&funcs);

        // --- Memory section ---
        let mut memory = MemorySection::new();
        memory.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        module.section(&memory);

        // --- Export section ---
        let mut exports = ExportSection::new();
        exports.export("memory", ExportKind::Memory, 0);
        // _start is function index 1 (index 0 is the proc_exit import)
        exports.export("_start", ExportKind::Func, 1);
        module.section(&exports);

        // --- Code section ---
        let mut code = CodeSection::new();

        // _start function: compile the entry point body, then return 0
        let mut start_fn = Function::new(vec![]);
        // For now: push exit code 0 and return.
        // Future phases will compile the actual Fuse function body to WASM instructions.
        start_fn.instruction(&Instruction::I32Const(0));
        start_fn.instruction(&Instruction::End);
        code.function(&start_fn);

        // Stub bodies for user functions (return 0)
        for _f in &self.functions {
            let mut func = Function::new(vec![]);
            func.instruction(&Instruction::I32Const(0));
            func.instruction(&Instruction::End);
            code.function(&func);
        }

        module.section(&code);

        Ok(module.finish())
    }
}
