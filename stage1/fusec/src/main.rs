use std::path::PathBuf;

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(first) = args.next() else {
        eprintln!("usage: fusec --check <file.fuse>");
        return std::process::ExitCode::from(1);
    };
    if first != "--check" {
        eprintln!("usage: fusec --check <file.fuse>");
        return std::process::ExitCode::from(1);
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
