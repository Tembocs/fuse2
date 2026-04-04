use std::path::PathBuf;

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
    fusec::codegen::compile_path_to_native(input, output)
}
