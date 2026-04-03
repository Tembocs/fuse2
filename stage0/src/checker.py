from __future__ import annotations

import copy
import sys
from dataclasses import dataclass
from pathlib import Path

if __package__ in (None, ''):
    sys.path.insert(0, str(Path(__file__).resolve().parent))

import fuse_ast as fa
from common import resolve_import_path
from errors import CheckError
from parser import parse_source
from values import FuseOption, FuseResult


@dataclass
class Symbol:
    kind: str
    node: object
    is_pub: bool = False


class ModuleInfo:
    def __init__(self, path: Path, program: fa.Program):
        self.path = path
        self.program = program
        self.symbols: dict[str, Symbol] = {}
        self.imports: list[fa.ImportDecl] = []
        self.extension_functions: dict[tuple[str, str], fa.FunctionDecl] = {}


class Checker:
    def __init__(self):
        self.module_cache: dict[Path, ModuleInfo] = {}
        self.errors: list[CheckError] = []

    def check_file(self, path: str | Path) -> list[CheckError]:
        self.errors = []
        module = self.load_module(Path(path).resolve())
        self.check_module(module)
        return self.errors

    def load_module(self, path: Path) -> ModuleInfo:
        if path in self.module_cache:
            return self.module_cache[path]
        source = path.read_text(encoding='utf-8')
        program = parse_source(source, path.name)
        module = ModuleInfo(path, program)
        self.module_cache[path] = module
        for decl in program.declarations:
            if isinstance(decl, fa.ImportDecl):
                module.imports.append(decl)
            elif isinstance(decl, fa.FunctionDecl):
                if decl.receiver_type:
                    module.extension_functions[(decl.receiver_type, decl.name)] = decl
                else:
                    module.symbols[decl.name] = Symbol('fn', decl, decl.is_pub)
            elif isinstance(decl, fa.DataClassDecl):
                module.symbols[decl.name] = Symbol('data', decl, decl.is_pub)
            elif isinstance(decl, fa.EnumDecl):
                module.symbols[decl.name] = Symbol('enum', decl, decl.is_pub)
        return module

    def add_error(self, node, message: str, hint: str | None = None, filename: str | None = None):
        self.errors.append(CheckError(message, filename or getattr(node, 'filename', '') or self.current_file.name, getattr(node, 'line', 1), getattr(node, 'column', 1), hint))

    def check_module(self, module: ModuleInfo):
        self.current_file = module.path
        for imp in module.imports:
            self.check_import(module, imp)
        for sym in module.symbols.values():
            if sym.kind == 'fn':
                self.check_function(module, sym.node)
            elif sym.kind == 'data':
                for method in sym.node.methods:
                    self.check_function(module, method, owner=sym.node)

    def check_import(self, module: ModuleInfo, imp: fa.ImportDecl):
        try:
            target_path = resolve_import_path(module.path, imp.module_path)
            target = self.load_module(target_path)
        except FileNotFoundError:
            self.add_error(imp, f'cannot resolve import `{imp.module_path}`', filename=module.path.name)
            return
        if imp.items:
            for item in imp.items:
                symbol = target.symbols.get(item)
                if symbol is None or not symbol.is_pub:
                    self.add_error(imp, f'cannot import non-pub item `{item}`', filename=module.path.name)

    def builtins(self) -> dict[str, str]:
        return {
            'println': 'Unit',
            'Some': 'Option',
            'None': 'Option',
            'Ok': 'Result',
            'Err': 'Result',
        }

    def check_function(self, module: ModuleInfo, fn: fa.FunctionDecl, owner: fa.DataClassDecl | None = None):
        scope: dict[str, dict] = {}
        loop_depth = 0
        owner_name = owner.name if owner else None
        for param in fn.params:
            scope[param.name] = {
                'mutable': True,
                'param_convention': param.convention,
                'type': param.type_name,
                'moved': False,
            }
        for stmt in fn.body.statements:
            loop_depth = self.check_statement(module, stmt, scope, loop_depth, fn, owner_name)
        if fn.return_type:
            inferred = self.infer_block_type(module, fn.body, scope, owner_name)
            if inferred and not self.type_matches(fn.return_type, inferred):
                self.add_error(fn, f'type mismatch: expected `{fn.return_type}`, found `{inferred}`', filename=module.path.name)

    def infer_block_type(self, module: ModuleInfo, block: fa.Block, scope: dict, owner_name: str | None):
        if not block.statements:
            return 'Unit'
        last = block.statements[-1]
        if isinstance(last, fa.ExprStmt):
            return self.infer_expr_type(module, last.expr, scope, owner_name)
        if isinstance(last, fa.ReturnStmt) and last.value is not None:
            return self.infer_expr_type(module, last.value, scope, owner_name)
        return 'Unit'

    def check_statement(self, module: ModuleInfo, stmt, scope: dict, loop_depth: int, fn: fa.FunctionDecl, owner_name: str | None):
        if isinstance(stmt, fa.VarDecl):
            ty = stmt.type_name or self.infer_expr_type(module, stmt.value, scope, owner_name)
            self.check_expr(module, stmt.value, scope, owner_name)
            scope[stmt.name] = {'mutable': stmt.mutable, 'param_convention': None, 'type': ty, 'moved': False}
        elif isinstance(stmt, fa.Assign):
            self.check_expr(module, stmt.value, scope, owner_name)
            self.check_assignment_target(module, stmt.target, scope, owner_name, stmt)
        elif isinstance(stmt, fa.ReturnStmt):
            if stmt.value is not None:
                self.check_expr(module, stmt.value, scope, owner_name)
        elif isinstance(stmt, fa.WhileStmt):
            self.check_expr(module, stmt.condition, scope, owner_name)
            child = copy.deepcopy(scope)
            for inner in stmt.body.statements:
                self.check_statement(module, inner, child, loop_depth + 1, fn, owner_name)
        elif isinstance(stmt, fa.ForStmt):
            self.check_expr(module, stmt.iterable, scope, owner_name)
            child = copy.deepcopy(scope)
            child[stmt.name] = {'mutable': True, 'param_convention': None, 'type': None, 'moved': False}
            for inner in stmt.body.statements:
                self.check_statement(module, inner, child, loop_depth + 1, fn, owner_name)
        elif isinstance(stmt, fa.LoopStmt):
            child = copy.deepcopy(scope)
            for inner in stmt.body.statements:
                self.check_statement(module, inner, child, loop_depth + 1, fn, owner_name)
        elif isinstance(stmt, (fa.BreakStmt, fa.ContinueStmt)):
            if loop_depth <= 0:
                self.add_error(stmt, f'`{type(stmt).__name__.replace("Stmt", "").lower()}` outside loop', filename=module.path.name)
        elif isinstance(stmt, fa.DeferStmt):
            self.check_expr(module, stmt.expr, scope, owner_name)
        elif isinstance(stmt, fa.ExprStmt):
            if isinstance(stmt.expr, fa.IfExpr):
                self.check_expr(module, stmt.expr.condition, scope, owner_name)
                inner_a = copy.deepcopy(scope)
                for inner in stmt.expr.then_branch.statements:
                    self.check_statement(module, inner, inner_a, loop_depth, fn, owner_name)
                if stmt.expr.else_branch is not None:
                    if isinstance(stmt.expr.else_branch, fa.Block):
                        inner_b = copy.deepcopy(scope)
                        for inner in stmt.expr.else_branch.statements:
                            self.check_statement(module, inner, inner_b, loop_depth, fn, owner_name)
                    else:
                        self.check_expr(module, stmt.expr.else_branch, scope, owner_name)
            else:
                self.check_expr(module, stmt.expr, scope, owner_name)
        return loop_depth

    def check_assignment_target(self, module: ModuleInfo, target, scope: dict, owner_name: str | None, stmt):
        if isinstance(target, fa.Name):
            binding = scope.get(target.value)
            if binding and not binding['mutable']:
                self.add_error(stmt, f'cannot assign to immutable binding `{target.value}`', filename=module.path.name)
            return
        if isinstance(target, fa.Member):
            root = self.root_name(target.object)
            if root and root in scope:
                binding = scope[root]
                if binding.get('param_convention') == 'ref':
                    self.add_error(stmt, f'cannot assign through `ref` parameter `{root}`', filename=module.path.name)
                    return
                root_type = binding.get('type')
                data_decl = self.find_data_decl(module, root_type)
                if data_decl:
                    field = next((f for f in data_decl.fields if f.name == target.name), None)
                    if field and not field.mutable:
                        self.add_error(stmt, f'cannot assign to immutable field `{target.name}`', filename=module.path.name)
            self.check_expr(module, target.object, scope, owner_name)

    def root_name(self, expr):
        while isinstance(expr, fa.Member):
            expr = expr.object
        if isinstance(expr, fa.Name):
            return expr.value
        return None

    def find_data_decl(self, module: ModuleInfo, type_name: str | None):
        if not type_name:
            return None
        if type_name in {'Result', 'Option'}:
            return None
        for info in self.module_cache.values():
            sym = info.symbols.get(type_name.split('<')[0])
            if sym and sym.kind == 'data':
                return sym.node
        return None

    def check_expr(self, module: ModuleInfo, expr, scope: dict, owner_name: str | None):
        if isinstance(expr, (fa.Literal, fa.FString)):
            return
        if isinstance(expr, fa.Name):
            if expr.value == 'None':
                return
            binding = scope.get(expr.value)
            if binding and binding['moved']:
                self.add_error(expr, f'cannot use `{expr.value}` after `move`', filename=module.path.name)
            return
        if isinstance(expr, fa.ListExpr):
            for item in expr.items:
                self.check_expr(module, item, scope, owner_name)
            return
        if isinstance(expr, fa.UnaryOp):
            self.check_expr(module, expr.value, scope, owner_name)
            return
        if isinstance(expr, fa.BinaryOp):
            self.check_expr(module, expr.left, scope, owner_name)
            self.check_expr(module, expr.right, scope, owner_name)
            return
        if isinstance(expr, fa.Member):
            self.check_expr(module, expr.object, scope, owner_name)
            root = self.root_name(expr)
            if root and root in scope and scope[root]['moved'] and not isinstance(expr.object, fa.Name):
                self.add_error(expr, f'cannot use `{root}` after `move`', filename=module.path.name)
            return
        if isinstance(expr, fa.MoveExpr):
            root = self.root_name(expr.value)
            if root:
                self.check_expr(module, expr.value, scope, owner_name)
            if root and root in scope:
                convention = scope[root].get('param_convention')
                if convention in {'ref', 'mutref'}:
                    self.add_error(expr, f'cannot move from `{convention}` parameter `{root}`', filename=module.path.name)
                scope[root]['moved'] = True
            return
        if isinstance(expr, (fa.RefExpr, fa.MutRefExpr, fa.QuestionExpr)):
            self.check_expr(module, expr.value, scope, owner_name)
            return
        if isinstance(expr, fa.IfExpr):
            self.check_expr(module, expr.condition, scope, owner_name)
            inner_a = copy.deepcopy(scope)
            for stmt in expr.then_branch.statements:
                self.check_statement(module, stmt, inner_a, 0, None, owner_name)
            if expr.else_branch is not None:
                if isinstance(expr.else_branch, fa.Block):
                    inner_b = copy.deepcopy(scope)
                    for stmt in expr.else_branch.statements:
                        self.check_statement(module, stmt, inner_b, 0, None, owner_name)
                else:
                    self.check_expr(module, expr.else_branch, scope, owner_name)
            return
        if isinstance(expr, fa.WhenExpr):
            has_else = False
            for cond, body in expr.arms:
                if cond == 'else':
                    has_else = True
                else:
                    self.check_expr(module, cond, scope, owner_name)
                self.check_expr(module, body, scope, owner_name) if not isinstance(body, fa.Block) else [self.check_statement(module, s, copy.deepcopy(scope), 0, None, owner_name) for s in body.statements]
            if not has_else:
                self.add_error(expr, '`when` requires an `else` arm', filename=module.path.name)
            return
        if isinstance(expr, fa.MatchExpr):
            self.check_expr(module, expr.subject, scope, owner_name)
            for arm in expr.arms:
                if isinstance(arm.body, fa.Block):
                    local = copy.deepcopy(scope)
                    for stmt in arm.body.statements:
                        self.check_statement(module, stmt, local, 0, None, owner_name)
                else:
                    self.check_expr(module, arm.body, copy.deepcopy(scope), owner_name)
            self.check_match_exhaustiveness(module, expr, scope, owner_name)
            return
        if isinstance(expr, fa.Call):
            self.check_expr(module, expr.callee, scope, owner_name)
            for arg in expr.args:
                self.check_expr(module, arg, scope, owner_name)
            self.check_call(module, expr, scope, owner_name)
            return

    def check_call(self, module: ModuleInfo, call: fa.Call, scope: dict, owner_name: str | None):
        callee_name = None
        decl = None
        if isinstance(call.callee, fa.Name):
            callee_name = call.callee.value
            decl = self.resolve_function(module, callee_name)
        elif isinstance(call.callee, fa.Member):
            recv_type = self.infer_expr_type(module, call.callee.object, scope, owner_name)
            decl = self.resolve_extension(recv_type, call.callee.name)
        if decl:
            for param, arg in zip(decl.params, call.args):
                if param.convention == 'mutref' and not isinstance(arg, fa.MutRefExpr):
                    self.add_error(arg, f'`mutref` must be explicit at the call site for `{param.name}`', 'did you mean `mutref`?', filename=module.path.name)
                if param.convention == 'ref' and isinstance(arg, fa.MutRefExpr):
                    self.add_error(arg, f'`ref` parameter `{param.name}` cannot receive `mutref`', filename=module.path.name)
        elif callee_name in self.builtins() or self.find_data_decl(module, callee_name):
            return

    def resolve_function(self, module: ModuleInfo, name: str):
        for info in self.module_cache.values():
            sym = info.symbols.get(name)
            if sym and sym.kind == 'fn':
                return sym.node
        return None

    def resolve_extension(self, recv_type: str | None, name: str):
        if not recv_type:
            return None
        recv_key = recv_type.split('<')[0]
        for info in self.module_cache.values():
            decl = info.extension_functions.get((recv_key, name))
            if decl:
                return decl
        return None

    def check_match_exhaustiveness(self, module: ModuleInfo, match_expr: fa.MatchExpr, scope: dict, owner_name: str | None):
        ty = self.infer_expr_type(module, match_expr.subject, scope, owner_name)
        covered = set()
        wildcard = False
        for arm in match_expr.arms:
            pat = arm.pattern
            if isinstance(pat, fa.WildcardPattern):
                wildcard = True
            elif isinstance(pat, fa.VariantPattern):
                covered.add(pat.name.split('.')[-1])
            elif isinstance(pat, fa.LiteralPattern):
                covered.add(repr(pat.value))
        if wildcard:
            return
        if ty and ty.startswith('Result') and not {'Ok', 'Err'}.issubset(covered):
            self.add_error(match_expr, 'non-exhaustive match for `Result`', filename=module.path.name)
        elif ty and ty.startswith('Option') and not {'Some', 'None'}.issubset(covered):
            self.add_error(match_expr, 'non-exhaustive match for `Option`', filename=module.path.name)
        elif ty == 'Bool' and not {repr(True), repr(False)}.issubset(covered):
            self.add_error(match_expr, 'non-exhaustive match for `Bool`', filename=module.path.name)

    def infer_expr_type(self, module: ModuleInfo, expr, scope: dict, owner_name: str | None) -> str | None:
        if isinstance(expr, fa.Literal):
            if isinstance(expr.value, bool):
                return 'Bool'
            if isinstance(expr.value, int):
                return 'Int'
            if isinstance(expr.value, float):
                return 'Float'
            if isinstance(expr.value, str):
                return 'String'
        if isinstance(expr, fa.FString):
            return 'String'
        if isinstance(expr, fa.Name):
            if expr.value == 'None':
                return 'Option'
            if expr.value in scope:
                return scope[expr.value].get('type')
            if expr.value in {'Some', 'Ok', 'Err'}:
                return None
            sym = module.symbols.get(expr.value)
            if sym and sym.kind == 'data':
                return expr.value
            return self.builtins().get(expr.value)
        if isinstance(expr, fa.ListExpr):
            return 'List'
        if isinstance(expr, fa.Member):
            obj_ty = self.infer_expr_type(module, expr.object, scope, owner_name)
            if expr.optional:
                return 'Option'
            decl = self.find_data_decl(module, obj_ty)
            if decl:
                field = next((f for f in decl.fields if f.name == expr.name), None)
                if field:
                    return field.type_name
            if obj_ty == 'String' and expr.name == 'isEmpty':
                return None
            return None
        if isinstance(expr, fa.BinaryOp):
            if expr.op in {'==', '!=', '<', '>', '<=', '>='}:
                return 'Bool'
            if expr.op in {'and', 'or'}:
                return 'Bool'
            if expr.op == '?:':
                left = self.infer_expr_type(module, expr.left, scope, owner_name)
                if left == 'Option':
                    return self.infer_expr_type(module, expr.right, scope, owner_name)
                return left
            return self.infer_expr_type(module, expr.left, scope, owner_name)
        if isinstance(expr, fa.UnaryOp):
            return self.infer_expr_type(module, expr.value, scope, owner_name)
        if isinstance(expr, fa.IfExpr):
            return self.infer_block_type(module, expr.then_branch, scope, owner_name)
        if isinstance(expr, fa.QuestionExpr):
            inner = self.infer_expr_type(module, expr.value, scope, owner_name) or ''
            if inner.startswith('Result<'):
                args = inner[7:-1].split(',', 1)
                return args[0].strip()
            if inner.startswith('Option<'):
                return inner[7:-1].strip()
            return None
        if isinstance(expr, fa.Call):
            if isinstance(expr.callee, fa.Name):
                name = expr.callee.value
                if name == 'Some':
                    inner = self.infer_expr_type(module, expr.args[0], scope, owner_name) or 'Any'
                    return f'Option<{inner}>'
                if name == 'Ok':
                    inner = self.infer_expr_type(module, expr.args[0], scope, owner_name) or 'Any'
                    return f'Result<{inner}, Any>'
                if name == 'Err':
                    inner = self.infer_expr_type(module, expr.args[0], scope, owner_name) or 'Any'
                    return f'Result<Any, {inner}>'
                sym = module.symbols.get(name)
                if sym and sym.kind == 'data':
                    return name
                fn = self.resolve_function(module, name)
                if fn:
                    return fn.return_type
            if isinstance(expr.callee, fa.Member):
                recv_type = self.infer_expr_type(module, expr.callee.object, scope, owner_name)
                if recv_type == 'String' and expr.callee.name == 'toUpper':
                    return 'String'
                if recv_type == 'String' and expr.callee.name == 'isEmpty':
                    return 'Bool'
                ext = self.resolve_extension(recv_type, expr.callee.name)
                if ext:
                    return ext.return_type
        if isinstance(expr, fa.MatchExpr):
            if expr.arms:
                first = expr.arms[0].body
                return self.infer_expr_type(module, first, scope, owner_name) if not isinstance(first, fa.Block) else self.infer_block_type(module, first, scope, owner_name)
        if isinstance(expr, fa.MoveExpr):
            return self.infer_expr_type(module, expr.value, scope, owner_name)
        if isinstance(expr, (fa.RefExpr, fa.MutRefExpr)):
            return self.infer_expr_type(module, expr.value, scope, owner_name)
        return None

    def type_matches(self, expected: str, actual: str) -> bool:
        if expected == actual:
            return True
        if expected.startswith('Result') and actual.startswith('Result'):
            return True
        if expected.startswith('Option') and actual.startswith('Option'):
            return True
        return False
