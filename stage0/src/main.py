from __future__ import annotations

import sys
from pathlib import Path

if __package__ in (None, ''):
    sys.path.insert(0, str(Path(__file__).resolve().parent))

from checker import Checker
from evaluator import Evaluator


def check_file(path: str | Path) -> str:
    checker = Checker()
    errors = checker.check_file(path)
    return '\n'.join(error.render() for error in errors)


def run_file(path: str | Path) -> str:
    checker = Checker()
    errors = checker.check_file(path)
    if errors:
        return '\n'.join(error.render() for error in errors)
    evaluator = Evaluator()
    return evaluator.eval_file(path)


def main(argv: list[str] | None = None) -> int:
    argv = argv or sys.argv[1:]
    if not argv:
        print('usage: main.py [--check] <file.fuse>')
        return 1
    check_only = False
    if argv[0] == '--check':
        check_only = True
        argv = argv[1:]
    if not argv:
        print('usage: main.py [--check] <file.fuse>')
        return 1
    path = Path(argv[0])
    output = check_file(path) if check_only else run_file(path)
    if output:
        print(output)
    return 1 if check_only and output else 0


if __name__ == '__main__':
    raise SystemExit(main())
