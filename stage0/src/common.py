from __future__ import annotations

from pathlib import Path


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def resolve_import_path(current_file: str | Path, module_path: str) -> Path:
    current = Path(current_file)
    rel = Path(*module_path.split('.')).with_suffix('.fuse')
    candidates = [
        current.parent / rel,
        current.parent / 'src' / rel.relative_to('src') if module_path.startswith('src.') else current.parent / 'src' / rel,
        repo_root() / rel,
        repo_root() / 'tests' / 'fuse' / 'core' / 'modules' / rel,
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate
    raise FileNotFoundError(f'cannot resolve import `{module_path}` from {current_file}')


def extract_expected_block(path: Path):
    lines = path.read_text(encoding='utf-8').splitlines()
    if not lines or not lines[0].startswith('// EXPECTED '):
        return None, []
    kind = lines[0][3:].strip().rstrip(':')
    expected = []
    for line in lines[1:]:
        if line.startswith('// '):
            expected.append(line[3:])
            continue
        if line.startswith('//'):
            expected.append(line[2:])
            continue
        break
    return kind, expected
