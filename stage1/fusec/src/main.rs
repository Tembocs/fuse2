use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(first) = args.next() else {
        eprintln!("usage: fusec --check <file.fuse>");
        eprintln!("   or: fusec <file.fuse> -o <output>");
        return std::process::ExitCode::from(1);
    };
    if first != "--check" {
        return compile_entry(first, args.collect());
    }
    let Some(path) = args.next() else {
        eprintln!("usage: fusec --check <file.fuse>");
        return std::process::ExitCode::from(1);
    };
    if args.next().is_some() {
        eprintln!("usage: fusec --check <file.fuse>");
        return std::process::ExitCode::from(1);
    }

    let output = fusec::check_path_output(&PathBuf::from(path));
    if output.is_empty() {
        std::process::ExitCode::SUCCESS
    } else {
        println!("{output}");
        std::process::ExitCode::from(1)
    }
}

fn compile_entry(input: String, rest: Vec<String>) -> std::process::ExitCode {
    let path = PathBuf::from(input);
    let mut output_path = None;
    let mut idx = 0;
    while idx < rest.len() {
        match rest[idx].as_str() {
            "-o" => {
                idx += 1;
                let Some(value) = rest.get(idx) else {
                    eprintln!("missing value after `-o`");
                    return std::process::ExitCode::from(1);
                };
                output_path = Some(PathBuf::from(value));
            }
            other => {
                eprintln!("unexpected argument `{other}`");
                return std::process::ExitCode::from(1);
            }
        }
        idx += 1;
    }
    let Some(output_path) = output_path else {
        eprintln!("usage: fusec <file.fuse> -o <output>");
        return std::process::ExitCode::from(1);
    };

    let diagnostics = fusec::check_path_output(&path);
    if !diagnostics.is_empty() {
        println!("{diagnostics}");
        return std::process::ExitCode::from(1);
    }

    match compile_to_native(&path, &output_path) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::from(1)
        }
    }
}

fn compile_to_native(input: &PathBuf, output: &PathBuf) -> Result<(), String> {
    let source = std::fs::read_to_string(input)
        .map_err(|error| format!("failed to read `{}`: {error}", input.display()))?;
    let repo_root = fusec::common::repo_root();
    let stage1_root = repo_root.join("stage1");
    let generated_root = stage1_root.join("target").join("generated");
    std::fs::create_dir_all(&generated_root)
        .map_err(|error| format!("failed to create generated directory: {error}"))?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock error: {error}"))?
        .as_millis();
    let workdir = generated_root.join(format!(
        "{}-{stamp}",
        input.file_stem().and_then(|part| part.to_str()).unwrap_or("program")
    ));
    let src_dir = workdir.join("src");
    std::fs::create_dir_all(&src_dir)
        .map_err(|error| format!("failed to create generated source directory: {error}"))?;

    let fusec_path = escape_path(&stage1_root.join("fusec"));
    let cargo_toml = format!(
        "[package]\nname = \"fuse-generated-{stamp}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n\n[dependencies]\nfusec = {{ path = \"{fusec_path}\" }}\n"
    );
    std::fs::write(workdir.join("Cargo.toml"), cargo_toml)
        .map_err(|error| format!("failed to write generated Cargo.toml: {error}"))?;

    let source_literal = source.replace("\"\"\"", "\\\"\\\"\\\"");
    let source_path = escape_string(&input.canonicalize().unwrap_or_else(|_| input.clone()));
    let launcher = format!(
        "const SOURCE: &str = r#\"{source_literal}\"#;\nconst SOURCE_PATH: &str = r#\"{source_path}\"#;\n\nfn main() {{\n    std::process::exit(fusec::run_embedded_source(SOURCE, SOURCE_PATH));\n}}\n"
    );
    std::fs::write(src_dir.join("main.rs"), launcher)
        .map_err(|error| format!("failed to write generated main.rs: {error}"))?;

    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .current_dir(&workdir)
        .status()
        .map_err(|error| format!("failed to launch cargo build: {error}"))?;
    if !status.success() {
        return Err("generated launcher build failed".to_string());
    }

    let built = workdir.join("target").join("release").join(format!(
        "{}{}",
        workdir
            .file_name()
            .and_then(|part| part.to_str())
            .unwrap_or("fuse-generated"),
        std::env::consts::EXE_SUFFIX
    ));
    let built = if built.exists() {
        built
    } else {
        let fallback = std::fs::read_dir(workdir.join("target").join("release"))
            .map_err(|error| format!("failed to inspect generated release directory: {error}"))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.extension().and_then(|part| part.to_str()) == Some("exe"))
            .ok_or_else(|| "could not locate generated executable".to_string())?;
        fallback
    };

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create output directory: {error}"))?;
    }
    std::fs::copy(&built, output)
        .map_err(|error| format!("failed to copy generated executable: {error}"))?;
    Ok(())
}

fn escape_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn escape_string(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}
