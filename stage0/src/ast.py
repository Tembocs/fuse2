from __future__ import annotations

import importlib.util
import sysconfig
from pathlib import Path

_stdlib_ast = Path(sysconfig.get_path('stdlib')) / 'ast.py'
_spec = importlib.util.spec_from_file_location('_stdlib_ast', _stdlib_ast)
_module = importlib.util.module_from_spec(_spec)
assert _spec and _spec.loader
_spec.loader.exec_module(_module)
for _name in dir(_module):
    globals()[_name] = getattr(_module, _name)
