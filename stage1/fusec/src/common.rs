use std::fs;
use std::path::{Path, PathBuf};

pub fn repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("FUSE_STAGE1_ROOT") {
        let root = PathBuf::from(root);
        if looks_like_repo_root(&root) {
            return root;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            let candidate = ancestor.to_path_buf();
            if looks_like_repo_root(&candidate) {
                return candidate;
            }
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("stage1/fusec must be nested under the repository root")
        .to_path_buf()
}

fn looks_like_repo_root(path: &Path) -> bool {
    path.join("stage1").is_dir() && path.join("stdlib").is_dir()
}

pub fn resolve_import_path(current_file: &Path, module_path: &str) -> Option<PathBuf> {
    let rel = module_path
        .split('.')
        .fold(PathBuf::new(), |path, part| path.join(part))
        .with_extension("fuse");
    let mut candidates = vec![
        current_file.parent()?.join(&rel),
        current_file.parent()?.join("src").join(&rel),
        repo_root().join(&rel),
        repo_root().join("stdlib").join(&rel),
        repo_root().join("stdlib").join("full").join(&rel),
        repo_root().join("stdlib").join("core").join(&rel),
        repo_root()
            .join("tests")
            .join("fuse")
            .join("core")
            .join("modules")
            .join(&rel),
    ];
    if module_path.starts_with("src.") {
        let from_src = module_path
            .split('.')
            .skip(1)
            .fold(PathBuf::new(), |path, part| path.join(part))
            .with_extension("fuse");
        candidates.push(current_file.parent()?.join("src").join(from_src));
    }
    candidates.into_iter().find(|candidate| candidate.exists())
}

pub fn extract_expected_block(path: &Path) -> Option<(String, String)> {
    let content = fs::read_to_string(path).ok()?;
    let mut lines = content.lines();
    let first = lines.next()?;
    if !first.starts_with("// EXPECTED ") {
        return None;
    }
    let kind = first[3..].trim().trim_end_matches(':').to_owned();
    let mut expected = Vec::new();
    for line in lines {
        if let Some(rest) = line.strip_prefix("// ") {
            expected.push(rest.to_owned());
            continue;
        }
        if let Some(rest) = line.strip_prefix("//") {
            expected.push(rest.to_owned());
            continue;
        }
        break;
    }
    Some((kind, expected.join("\n")))
}

pub fn core_test_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_fuse_files(&repo_root().join("tests").join("fuse").join("core"), &mut files);
    files.retain(|path| !path.components().any(|component| component.as_os_str() == "src"));
    files.sort();
    files
}

pub fn full_test_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_fuse_files(&repo_root().join("tests").join("fuse").join("full"), &mut files);
    files.retain(|path| !path.components().any(|component| component.as_os_str() == "src"));
    files.sort();
    files
}

fn collect_fuse_files(path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        if child.is_dir() {
            collect_fuse_files(&child, files);
        } else if child.extension().and_then(|ext| ext.to_str()) == Some("fuse") {
            files.push(child);
        }
    }
}
