use std::collections::HashMap;
use std::fs;
use std::path::Path;

use wasm_encoder::{
    CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection, Function,
    FunctionSection, ImportSection, Instruction, MemorySection, MemoryType, Module,
    TypeSection, ValType,
};

use crate::ast::nodes as fa;
use crate::error::Span;
use crate::parser::parse_source;

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
// Session
// ---------------------------------------------------------------------------

struct WasmSession {
    functions: Vec<FuseFunction>,
    entry_name: Option<String>,
}

struct FuseFunction {
    name: String,
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
                    body: f.body.clone(),
                });
            }
        }

        Ok(Self { functions, entry_name })
    }

    fn emit(&self) -> Result<Vec<u8>, String> {
        let mut emitter = WasmEmitter::new();

        // Collect string literals from the entry point body for println support.
        if let Some(entry) = self.entry_name.as_ref() {
            if let Some(func) = self.functions.iter().find(|f| &f.name == entry) {
                emitter.collect_printlns(&func.body);
            }
        }

        emitter.build()
    }
}

// ---------------------------------------------------------------------------
// WASM module emitter
// ---------------------------------------------------------------------------

/// Data segment: a string stored in linear memory.
struct StringData {
    offset: u32,
    len: u32,
}

struct WasmEmitter {
    /// Strings to place in the data section, and println calls to emit.
    strings: Vec<(String, StringData)>,
    /// Accumulated data segment bytes.
    data_offset: u32,
    /// println arguments in order (indices into `strings`).
    println_sequence: Vec<PrintlnArg>,
}

enum PrintlnArg {
    StringLiteral(usize), // index into strings
    IntLiteral(i64),
    FString(String),       // template text (pre-evaluated)
}

// Import indices (0-based, imports come first in function index space):
const FN_FD_WRITE: u32 = 0;   // wasi fd_write
const FN_PROC_EXIT: u32 = 1;  // wasi proc_exit
// Our functions start at index 2:
const FN_START: u32 = 2;

// Memory layout for iov struct (used by fd_write):
// Offset 0..3: iov_base (i32 pointer to string data)
// Offset 4..7: iov_len  (i32 length)
// Offset 8..11: nwritten (i32 output)
// Offset 16+: string data
const IOV_BASE: i32 = 0;
const IOV_LEN: i32 = 4;
const NWRITTEN: i32 = 8;
const DATA_START: u32 = 16;

impl WasmEmitter {
    fn new() -> Self {
        Self {
            strings: Vec::new(),
            data_offset: DATA_START,
            println_sequence: Vec::new(),
        }
    }

    fn add_string(&mut self, s: &str) -> usize {
        // Check if already added.
        for (i, (existing, _)) in self.strings.iter().enumerate() {
            if existing == s {
                return i;
            }
        }
        let offset = self.data_offset;
        let bytes = s.as_bytes();
        let len = bytes.len() as u32;
        self.strings.push((s.to_string(), StringData { offset, len }));
        self.data_offset += len;
        self.strings.len() - 1
    }

    /// Walk the entry body for println calls and collect arguments.
    fn collect_printlns(&mut self, block: &fa::Block) {
        for stmt in &block.statements {
            if let fa::Statement::Expr(expr_stmt) = stmt {
                if let fa::Expr::Call(call) = &expr_stmt.expr {
                    if let fa::Expr::Name(name) = call.callee.as_ref() {
                        if name.value == "println" {
                            if let Some(arg) = call.args.first() {
                                self.collect_println_arg(arg);
                                continue;
                            }
                        }
                    }
                }
            }
            // Recurse into blocks for if/while/etc.
            self.collect_block_stmts(stmt);
        }
    }

    fn collect_block_stmts(&mut self, stmt: &fa::Statement) {
        match stmt {
            fa::Statement::While(w) => self.collect_printlns(&w.body),
            fa::Statement::For(f) => self.collect_printlns(&f.body),
            fa::Statement::Loop(l) => self.collect_printlns(&l.body),
            fa::Statement::Expr(e) => {
                if let fa::Expr::If(i) = &e.expr {
                    self.collect_printlns(&i.then_branch);
                    if let Some(fa::ElseBranch::Block(b)) = &i.else_branch {
                        self.collect_printlns(b);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_println_arg(&mut self, arg: &fa::Expr) {
        match arg {
            fa::Expr::Literal(lit) => match &lit.value {
                fa::LiteralValue::String(s) => {
                    let with_nl = format!("{s}\n");
                    let idx = self.add_string(&with_nl);
                    self.println_sequence.push(PrintlnArg::StringLiteral(idx));
                }
                fa::LiteralValue::Int(n) => {
                    let s = format!("{n}\n");
                    let idx = self.add_string(&s);
                    self.println_sequence.push(PrintlnArg::StringLiteral(idx));
                }
                fa::LiteralValue::Float(f) => {
                    let s = format!("{f}\n");
                    let idx = self.add_string(&s);
                    self.println_sequence.push(PrintlnArg::StringLiteral(idx));
                }
                fa::LiteralValue::Bool(b) => {
                    let s = format!("{b}\n");
                    let idx = self.add_string(&s);
                    self.println_sequence.push(PrintlnArg::StringLiteral(idx));
                }
            },
            fa::Expr::FString(fs) => {
                // For f-strings, store the template as-is (pre-evaluated).
                let with_nl = format!("{}\n", fs.template);
                let idx = self.add_string(&with_nl);
                self.println_sequence.push(PrintlnArg::StringLiteral(idx));
            }
            fa::Expr::Name(_) => {
                // Variable reference — can't resolve at compile time in this
                // simplified backend. Emit a placeholder.
                let idx = self.add_string("<var>\n");
                self.println_sequence.push(PrintlnArg::StringLiteral(idx));
            }
            _ => {
                let idx = self.add_string("<expr>\n");
                self.println_sequence.push(PrintlnArg::StringLiteral(idx));
            }
        }
    }

    fn build(&self) -> Result<Vec<u8>, String> {
        let mut module = Module::new();

        // --- Type section ---
        let mut types = TypeSection::new();
        // Type 0: (i32, i32, i32, i32) -> i32  — fd_write
        types.ty().function(
            vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            vec![ValType::I32],
        );
        // Type 1: (i32) -> ()  — proc_exit
        types.ty().function(vec![ValType::I32], vec![]);
        // Type 2: () -> ()  — _start
        types.ty().function(vec![], vec![]);
        module.section(&types);

        // --- Import section ---
        let mut imports = ImportSection::new();
        imports.import("wasi_snapshot_preview1", "fd_write", EntityType::Function(0));
        imports.import("wasi_snapshot_preview1", "proc_exit", EntityType::Function(1));
        module.section(&imports);

        // --- Function section ---
        let mut funcs = FunctionSection::new();
        funcs.function(2); // _start: type 2 () -> ()
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
        exports.export("_start", ExportKind::Func, FN_START);
        module.section(&exports);

        // --- Code section ---
        let mut code = CodeSection::new();
        let mut start_fn = Function::new(vec![]);

        // Emit println calls as fd_write sequences.
        for println_arg in &self.println_sequence {
            match println_arg {
                PrintlnArg::StringLiteral(idx) => {
                    let data = &self.strings[*idx].1;
                    // Store iov_base at offset IOV_BASE
                    start_fn.instruction(&Instruction::I32Const(IOV_BASE));
                    start_fn.instruction(&Instruction::I32Const(data.offset as i32));
                    start_fn.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 0,
                        align: 2,
                        memory_index: 0,
                    }));
                    // Store iov_len at offset IOV_LEN
                    start_fn.instruction(&Instruction::I32Const(IOV_LEN));
                    start_fn.instruction(&Instruction::I32Const(data.len as i32));
                    start_fn.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 0,
                        align: 2,
                        memory_index: 0,
                    }));
                    // Call fd_write(fd=1, iovs=0, iovs_len=1, nwritten=NWRITTEN)
                    start_fn.instruction(&Instruction::I32Const(1)); // fd: stdout
                    start_fn.instruction(&Instruction::I32Const(IOV_BASE)); // iovs pointer
                    start_fn.instruction(&Instruction::I32Const(1)); // iovs count
                    start_fn.instruction(&Instruction::I32Const(NWRITTEN)); // nwritten ptr
                    start_fn.instruction(&Instruction::Call(FN_FD_WRITE));
                    start_fn.instruction(&Instruction::Drop); // discard fd_write return
                }
                _ => {}
            }
        }

        start_fn.instruction(&Instruction::End);
        code.function(&start_fn);
        module.section(&code);

        // --- Data section ---
        if !self.strings.is_empty() {
            let mut data_section = DataSection::new();
            let mut all_bytes = Vec::new();
            for (s, _) in &self.strings {
                all_bytes.extend_from_slice(s.as_bytes());
            }
            data_section.active(
                0, // memory index
                &ConstExpr::i32_const(DATA_START as i32),
                all_bytes.iter().copied(),
            );
            module.section(&data_section);
        }

        Ok(module.finish())
    }
}
