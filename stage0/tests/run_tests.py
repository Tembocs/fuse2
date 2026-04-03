from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / 'stage0' / 'src'
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from common import extract_expected_block
from main import check_file, run_file


def main() -> int:
    failures = 0
    files = [p for p in (ROOT / 'tests' / 'fuse' / 'core').rglob('*.fuse') if 'src' not in p.parts]
    for path in sorted(files):
        kind, expected = extract_expected_block(path)
        if kind is None:
            print(f'MISSING EXPECTED BLOCK: {path.relative_to(ROOT)}')
            failures += 1
            continue
        actual = check_file(path) if 'ERROR' in kind else run_file(path)
        if actual.strip() != '\n'.join(expected).strip():
            failures += 1
            print(f'FAIL: {path.relative_to(ROOT)}')
            print('EXPECTED:')
            print('\n'.join(expected))
            print('ACTUAL:')
            print(actual)
    if failures == 0:
        print('PASS: all core tests')
    return 1 if failures else 0


if __name__ == '__main__':
    raise SystemExit(main())
