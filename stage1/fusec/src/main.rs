use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use fusec::color::{ColorMode, Painter};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Mode {
    Help,
    Version,
    Check,
    Run,
    Compile,
    Repl,
    Emit(EmitStage),
}

#[derive(Debug)]
enum EmitStage {
    Tokens,
    Ast,
    Hir,
    Ir,
}

#[derive(Debug, Clone, Copy)]
enum ErrorFormat {
    Long,
    Short,
}

#[derive(Debug)]
struct Args {
    mode: Mode,
    file: Option<PathBuf>,
    output: Option<PathBuf>,
    color: ColorMode,
    error_format: ErrorFormat,
    warn_unused: bool,
    deny_warnings: bool,
}

// ---------------------------------------------------------------------------
// Arg parsing
// ---------------------------------------------------------------------------

fn parse_args(raw: &[String]) -> Result<Args, String> {
    if raw.is_empty() {
        return Err("no arguments provided".to_string());
    }

    let mut mode: Option<Mode> = None;
    let mut file: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut color = ColorMode::Auto;
    let mut error_format = ErrorFormat::Long;
    let mut warn_unused = false;
    let mut deny_warnings = false;

    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--help" | "-h" => mode = Some(Mode::Help),
            "--version" | "-V" => mode = Some(Mode::Version),
            "--check" => mode = Some(Mode::Check),
            "--run" => mode = Some(Mode::Run),
            "--repl" => mode = Some(Mode::Repl),
            "--emit" => {
                i += 1;
                let stage_str = raw.get(i).ok_or_else(|| {
                    "missing emit stage after `--emit`; expected one of: tokens, ast, hir, ir"
                        .to_string()
                })?;
                let stage = match stage_str.as_str() {
                    "tokens" => EmitStage::Tokens,
                    "ast" => EmitStage::Ast,
                    "hir" => EmitStage::Hir,
                    "ir" => EmitStage::Ir,
                    other => {
                        return Err(format!(
                            "unknown emit stage `{other}`; expected one of: tokens, ast, hir, ir"
                        ))
                    }
                };
                mode = Some(Mode::Emit(stage));
            }
            "-o" => {
                i += 1;
                let path = raw
                    .get(i)
                    .ok_or_else(|| "missing output path after `-o`".to_string())?;
                output = Some(PathBuf::from(path));
            }
            "--color" => {
                i += 1;
                let value = raw.get(i).ok_or_else(|| {
                    "missing value after `--color`; expected: auto, always, never".to_string()
                })?;
                color = match value.as_str() {
                    "auto" => ColorMode::Auto,
                    "always" => ColorMode::Always,
                    "never" => ColorMode::Never,
                    other => {
                        return Err(format!(
                            "invalid color mode `{other}`; expected: auto, always, never"
                        ))
                    }
                };
            }
            "--error-format" => {
                i += 1;
                let value = raw.get(i).ok_or_else(|| {
                    "missing value after `--error-format`; expected: short, long".to_string()
                })?;
                error_format = match value.as_str() {
                    "short" => ErrorFormat::Short,
                    "long" => ErrorFormat::Long,
                    other => {
                        return Err(format!(
                            "invalid error format `{other}`; expected: short, long"
                        ))
                    }
                };
            }
            "--warn-unused" => warn_unused = true,
            "--deny-warnings" => deny_warnings = true,
            arg if arg.starts_with('-') => {
                return Err(format!("unexpected argument `{arg}`"));
            }
            _ => {
                if file.is_some() {
                    return Err("multi-file compilation is not yet supported; import between modules is handled by the compiler".to_string());
                }
                file = Some(PathBuf::from(&raw[i]));
            }
        }
        i += 1;
    }

    // Infer mode from context when no explicit mode flag was given.
    let mode = match mode {
        Some(m) => m,
        None => {
            if file.is_some() && output.is_some() {
                Mode::Compile
            } else if file.is_some() {
                return Err(
                    "missing output path; use `-o <path>` to specify the output binary"
                        .to_string(),
                );
            } else {
                return Err("no arguments provided".to_string());
            }
        }
    };

    // Validate mode-specific requirements.
    match &mode {
        Mode::Check | Mode::Run => {
            if file.is_none() {
                return Err(format!("{} requires a file argument", mode_name(&mode)));
            }
        }
        Mode::Emit(_) => {
            if file.is_none() {
                return Err("--emit requires a file argument".to_string());
            }
        }
        Mode::Compile => {
            if file.is_none() {
                return Err("missing file argument".to_string());
            }
            if output.is_none() {
                return Err(
                    "missing output path; use `-o <path>` to specify the output binary"
                        .to_string(),
                );
            }
        }
        Mode::Repl => {
            if file.is_some() {
                return Err("--repl does not accept a file argument".to_string());
            }
        }
        Mode::Help | Mode::Version => {}
    }

    Ok(Args {
        mode,
        file,
        output,
        color,
        error_format,
        warn_unused,
        deny_warnings,
    })
}

fn mode_name(mode: &Mode) -> &'static str {
    match mode {
        Mode::Help => "--help",
        Mode::Version => "--version",
        Mode::Check => "--check",
        Mode::Run => "--run",
        Mode::Compile => "compile",
        Mode::Repl => "--repl",
        Mode::Emit(_) => "--emit",
    }
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

fn dispatch(args: Args) -> ExitCode {
    match args.mode {
        Mode::Help => {
            print_help();
            ExitCode::SUCCESS
        }
        Mode::Version => {
            println!("fusec {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Mode::Check => run_check(&args),
        Mode::Run => run_interpret(&args),
        Mode::Compile => run_compile(&args),
        Mode::Repl => run_repl(&args),
        Mode::Emit(ref stage) => run_emit(stage, &args),
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&raw) {
        Ok(args) => dispatch(args),
        Err(message) => {
            eprintln!("error: {message}");
            eprintln!();
            eprintln!("usage: fusec <file.fuse> -o <output>");
            eprintln!("   or: fusec --check <file.fuse>");
            eprintln!("   or: fusec --help for full usage");
            ExitCode::from(2)
        }
    }
}

// ---------------------------------------------------------------------------
// --help
// ---------------------------------------------------------------------------

fn print_help() {
    println!(
        "\
fusec — the Fuse compiler

USAGE:
  fusec --check  <file.fuse>           Type-check only, no output
  fusec --run    <file.fuse>           Check and interpret
  fusec          <file.fuse> -o <out>  Compile to native binary
  fusec --repl                         Start interactive REPL
  fusec --emit   <stage> <file.fuse>   Print intermediate representation

EMIT STAGES:
  tokens    Token stream (after lexing)
  ast       Abstract syntax tree (after parsing)
  hir       High-level IR with type annotations (after HIR lowering)
  ir        Cranelift IR text (after codegen, Stage 1 only)

OPTIONS:
  -o <path>                     Output path for compiled binary (required in compile mode)
  --color auto|always|never     Diagnostic colour output (default: auto)
  --error-format short|long     Diagnostic verbosity (default: long)
  --warn-unused                 Warn on unused bindings, parameters, and imports
  --deny-warnings               Exit 1 if any warnings are produced

FLAGS:
  -h, --help                    Print this message and exit
  -V, --version                 Print version and exit

EXAMPLES:
  fusec --check src/main.fuse
  fusec src/main.fuse -o bin/main
  fusec --run examples/hello.fuse
  fusec --emit ast src/main.fuse
  fusec --check src/main.fuse --color never --error-format short"
    );
}

// ---------------------------------------------------------------------------
// --check
// ---------------------------------------------------------------------------

fn run_check(args: &Args) -> ExitCode {
    let path = args.file.as_ref().expect("file validated in parse_args");
    let source = fs::read_to_string(path).ok();
    let diagnostics = fusec::check_path(path);

    let has_errors = diagnostics.iter().any(|d| d.is_error());
    let has_warnings = diagnostics.iter().any(|d| d.is_warning());

    if !diagnostics.is_empty() {
        let output = render_diagnostics(&diagnostics, source.as_deref(), args);
        eprintln!("{output}");
    }

    if has_errors {
        ExitCode::from(1)
    } else if has_warnings && args.deny_warnings {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

// ---------------------------------------------------------------------------
// compile (<file> -o <out>)
// ---------------------------------------------------------------------------

fn run_compile(args: &Args) -> ExitCode {
    let path = args.file.as_ref().expect("file validated in parse_args");
    let output_path = args.output.as_ref().expect("output validated in parse_args");

    let source = fs::read_to_string(path).ok();
    let diagnostics = fusec::check_path(path);
    if !diagnostics.is_empty() {
        let rendered = render_diagnostics(&diagnostics, source.as_deref(), args);
        eprintln!("{rendered}");
        return ExitCode::from(1);
    }

    match fusec::codegen::compile_path_to_native(path, output_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}

// ---------------------------------------------------------------------------
// --run
// ---------------------------------------------------------------------------

fn run_interpret(args: &Args) -> ExitCode {
    let path = args.file.as_ref().expect("file validated in parse_args");

    // Check first — if errors, do not interpret.
    let source = fs::read_to_string(path).ok();
    let diagnostics = fusec::check_path(path);
    if !diagnostics.is_empty() {
        let rendered = render_diagnostics(&diagnostics, source.as_deref(), args);
        eprintln!("{rendered}");
        return ExitCode::from(1);
    }

    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {e}", path.display());
            return ExitCode::from(1);
        }
    };
    let path_str = path.to_string_lossy();
    let exit_code = fusec::evaluator::run_embedded_source(&source, &path_str);
    ExitCode::from(exit_code as u8)
}

// ---------------------------------------------------------------------------
// --emit
// ---------------------------------------------------------------------------

fn run_emit(stage: &EmitStage, args: &Args) -> ExitCode {
    let path = args.file.as_ref().expect("file validated in parse_args");
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {e}", path.display());
            return ExitCode::from(1);
        }
    };
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("input.fuse");

    match stage {
        EmitStage::Tokens => emit_tokens(&source, filename),
        EmitStage::Ast => emit_ast(&source, filename),
        EmitStage::Hir => emit_hir(path),
        EmitStage::Ir => emit_ir(path, args),
    }
}

fn emit_tokens(source: &str, filename: &str) -> ExitCode {
    match fusec::lexer::lex(source, filename) {
        Ok(tokens) => {
            for token in &tokens {
                if token.kind == fusec::lexer::TokenKind::Eof {
                    break;
                }
                println!(
                    "{:>5}:{:<3} {:<14} '{}'",
                    token.span.line,
                    token.span.column,
                    format!("{:?}", token.kind),
                    token.text
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", error.render());
            ExitCode::from(1)
        }
    }
}

fn emit_ast(source: &str, filename: &str) -> ExitCode {
    match fusec::parser::parse_source(source, filename) {
        Ok(program) => {
            print_ast_program(&program);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", error.render());
            ExitCode::from(1)
        }
    }
}

fn emit_hir(path: &PathBuf) -> ExitCode {
    let diagnostics = fusec::check_path(path);
    if !diagnostics.is_empty() {
        for diag in &diagnostics {
            eprintln!("{}", diag.render());
        }
        return ExitCode::from(1);
    }

    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {e}", path.display());
            return ExitCode::from(1);
        }
    };
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("input.fuse");
    match fusec::parser::parse_source(&source, filename) {
        Ok(program) => {
            let module = fusec::hir::lower_program(&program, path.clone());
            print_hir_module(&module);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", error.render());
            ExitCode::from(1)
        }
    }
}

fn emit_ir(path: &PathBuf, _args: &Args) -> ExitCode {
    let diagnostics = fusec::check_path(path);
    if !diagnostics.is_empty() {
        for diag in &diagnostics {
            eprintln!("{}", diag.render());
        }
        return ExitCode::from(1);
    }

    eprintln!("--emit ir is not yet implemented");
    ExitCode::from(1)
}

// ---------------------------------------------------------------------------
// --repl
// ---------------------------------------------------------------------------

fn run_repl(_args: &Args) -> ExitCode {
    eprintln!("--repl is not yet implemented");
    ExitCode::from(1)
}

// ---------------------------------------------------------------------------
// AST printer
// ---------------------------------------------------------------------------

fn print_ast_program(program: &fusec::ast::nodes::Program) {
    println!("Module [1:1]");
    for decl in &program.declarations {
        print_ast_decl(decl, 1);
    }
}

fn print_ast_decl(decl: &fusec::ast::nodes::Declaration, depth: usize) {
    let indent = "  ".repeat(depth);
    match decl {
        fusec::ast::nodes::Declaration::Import(import) => {
            println!("{indent}Import '{}'", import.module_path);
        }
        fusec::ast::nodes::Declaration::Function(func) => {
            println!(
                "{indent}FnDecl '{}' [{}:{}]",
                func.name, func.span.line, func.span.column
            );
            let inner = "  ".repeat(depth + 1);
            let params: Vec<String> = func
                .params
                .iter()
                .map(|p| {
                    let conv = p.convention.as_deref().unwrap_or("");
                    let ty = p.type_name.as_deref().unwrap_or("?");
                    if conv.is_empty() {
                        format!("{}: {ty}", p.name)
                    } else {
                        format!("{}: {conv} {ty}", p.name)
                    }
                })
                .collect();
            println!("{inner}Params [{}]", params.join(", "));
            let ret = func.return_type.as_deref().unwrap_or("Unit");
            println!("{inner}ReturnType {ret}");
            println!("{inner}Body");
            for stmt in &func.body.statements {
                print_ast_stmt(stmt, depth + 2);
            }
        }
        fusec::ast::nodes::Declaration::DataClass(data) => {
            println!(
                "{indent}DataClass '{}' [{}:{}]",
                data.name, data.span.line, data.span.column
            );
            let inner = "  ".repeat(depth + 1);
            for field in &data.fields {
                let mutability = if field.mutable { "var" } else { "val" };
                let ty = field.type_name.as_deref().unwrap_or("?");
                println!("{inner}Field {mutability} {}: {ty}", field.name);
            }
        }
        fusec::ast::nodes::Declaration::Enum(e) => {
            println!(
                "{indent}Enum '{}' [{}:{}]",
                e.name, e.span.line, e.span.column
            );
        }
    }
}

fn print_ast_stmt(stmt: &fusec::ast::nodes::Statement, depth: usize) {
    let indent = "  ".repeat(depth);
    match stmt {
        fusec::ast::nodes::Statement::VarDecl(v) => {
            let kw = if v.mutable { "VarDecl" } else { "ValDecl" };
            let ty = v
                .type_name
                .as_deref()
                .map(|t| format!(": {t}"))
                .unwrap_or_default();
            println!(
                "{indent}{kw} '{}'{ty} [{}:{}]",
                v.name, v.span.line, v.span.column
            );
            print_ast_expr(&v.value, depth + 1, "Init");
        }
        fusec::ast::nodes::Statement::Assign(a) => {
            println!("{indent}Assign [{}:{}]", a.span.line, a.span.column);
            print_ast_expr(&a.target, depth + 1, "Target");
            print_ast_expr(&a.value, depth + 1, "Value");
        }
        fusec::ast::nodes::Statement::Return(r) => {
            println!("{indent}Return [{}:{}]", r.span.line, r.span.column);
            if let Some(value) = &r.value {
                print_ast_expr(value, depth + 1, "Value");
            }
        }
        fusec::ast::nodes::Statement::Expr(e) => {
            print_ast_expr(&e.expr, depth, "");
        }
        fusec::ast::nodes::Statement::While(w) => {
            println!("{indent}While [{}:{}]", w.span.line, w.span.column);
            print_ast_expr(&w.condition, depth + 1, "Condition");
            for s in &w.body.statements {
                print_ast_stmt(s, depth + 1);
            }
        }
        fusec::ast::nodes::Statement::For(f) => {
            println!(
                "{indent}For '{}' [{}:{}]",
                f.name, f.span.line, f.span.column
            );
            print_ast_expr(&f.iterable, depth + 1, "Iterable");
            for s in &f.body.statements {
                print_ast_stmt(s, depth + 1);
            }
        }
        fusec::ast::nodes::Statement::Loop(l) => {
            println!("{indent}Loop [{}:{}]", l.span.line, l.span.column);
            for s in &l.body.statements {
                print_ast_stmt(s, depth + 1);
            }
        }
        fusec::ast::nodes::Statement::Break(span) => {
            println!("{indent}Break [{}:{}]", span.line, span.column);
        }
        fusec::ast::nodes::Statement::Continue(span) => {
            println!("{indent}Continue [{}:{}]", span.line, span.column);
        }
        fusec::ast::nodes::Statement::Spawn(s) => {
            println!("{indent}Spawn [{}:{}]", s.span.line, s.span.column);
            for st in &s.body.statements {
                print_ast_stmt(st, depth + 1);
            }
        }
        fusec::ast::nodes::Statement::Defer(d) => {
            println!("{indent}Defer [{}:{}]", d.span.line, d.span.column);
            print_ast_expr(&d.expr, depth + 1, "Expr");
        }
    }
}

fn print_ast_expr(expr: &fusec::ast::nodes::Expr, depth: usize, label: &str) {
    let indent = "  ".repeat(depth);
    let prefix = if label.is_empty() {
        String::new()
    } else {
        format!("{label} ")
    };
    match expr {
        fusec::ast::nodes::Expr::Literal(lit) => {
            println!("{indent}{prefix}{:?}", lit.value);
        }
        fusec::ast::nodes::Expr::FString(f) => {
            println!("{indent}{prefix}FString ({} parts)", f.template.len());
        }
        fusec::ast::nodes::Expr::Name(n) => {
            println!("{indent}{prefix}Name '{}'", n.value);
        }
        fusec::ast::nodes::Expr::Call(c) => {
            println!("{indent}{prefix}Call");
            print_ast_expr(&c.callee, depth + 1, "Callee");
            for arg in &c.args {
                print_ast_expr(arg, depth + 1, "Arg");
            }
        }
        fusec::ast::nodes::Expr::Binary(b) => {
            println!("{indent}{prefix}Binary '{}'", b.op);
            print_ast_expr(&b.left, depth + 1, "Left");
            print_ast_expr(&b.right, depth + 1, "Right");
        }
        fusec::ast::nodes::Expr::Unary(u) => {
            println!("{indent}{prefix}Unary '{}'", u.op);
            print_ast_expr(&u.value, depth + 1, "");
        }
        fusec::ast::nodes::Expr::Member(m) => {
            println!("{indent}{prefix}Member '.{}'", m.name);
            print_ast_expr(&m.object, depth + 1, "Object");
        }
        fusec::ast::nodes::Expr::List(l) => {
            println!("{indent}{prefix}List ({} items)", l.items.len());
        }
        fusec::ast::nodes::Expr::If(if_expr) => {
            println!("{indent}{prefix}If");
            print_ast_expr(&if_expr.condition, depth + 1, "Condition");
        }
        fusec::ast::nodes::Expr::Match(m) => {
            println!(
                "{indent}{prefix}Match ({} arms)",
                m.arms.len()
            );
        }
        fusec::ast::nodes::Expr::When(w) => {
            println!(
                "{indent}{prefix}When ({} arms)",
                w.arms.len()
            );
        }
        fusec::ast::nodes::Expr::Move(m) => {
            println!("{indent}{prefix}Move");
            print_ast_expr(&m.value, depth + 1, "");
        }
        fusec::ast::nodes::Expr::Ref(r) => {
            println!("{indent}{prefix}Ref");
            print_ast_expr(&r.value, depth + 1, "");
        }
        fusec::ast::nodes::Expr::MutRef(r) => {
            println!("{indent}{prefix}MutRef");
            print_ast_expr(&r.value, depth + 1, "");
        }
        fusec::ast::nodes::Expr::Await(a) => {
            println!("{indent}{prefix}Await");
            print_ast_expr(&a.value, depth + 1, "");
        }
        fusec::ast::nodes::Expr::Question(q) => {
            println!("{indent}{prefix}Question");
            print_ast_expr(&q.value, depth + 1, "");
        }
    }
}

// ---------------------------------------------------------------------------
// HIR printer
// ---------------------------------------------------------------------------

fn print_hir_module(module: &fusec::hir::Module) {
    println!("Module '{}'", module.filename);
    for func in &module.functions {
        let ret = func.return_type.as_deref().unwrap_or("Unit");
        let params: Vec<String> = func
            .params
            .iter()
            .map(|p| {
                let conv = p.convention.as_deref().unwrap_or("");
                let ty = p.type_name.as_deref().unwrap_or("?");
                if conv.is_empty() {
                    format!("{}: {ty}", p.name)
                } else {
                    format!("{}: {conv} {ty}", p.name)
                }
            })
            .collect();
        println!("  FnDecl '{}' ({}) -> {ret}", func.name, params.join(", "));
        for stmt in &func.body.statements {
            print_ast_stmt(stmt, 2);
        }
    }
    for data in &module.data_classes {
        println!("  DataClass '{}'", data.name);
        for field in &data.fields {
            let ty = field.type_name.as_deref().unwrap_or("?");
            println!("    {} {}: {ty}", if field.mutable { "var" } else { "val" }, field.name);
        }
    }
}

// ---------------------------------------------------------------------------
// Diagnostic rendering
// ---------------------------------------------------------------------------

fn render_diagnostics(diagnostics: &[fusec::error::Diagnostic], source: Option<&str>, args: &Args) -> String {
    let painter = Painter::new(args.color);
    let rendered: Vec<String> = diagnostics
        .iter()
        .map(|d| match args.error_format {
            ErrorFormat::Long => d.render_long(source, &painter),
            ErrorFormat::Short => d.render_short(),
        })
        .collect();
    let mut out = rendered.join("\n");
    let summary = fusec::error::render_summary(diagnostics, &painter);
    if !summary.is_empty() {
        out.push('\n');
        out.push_str(&summary);
    }
    out
}
