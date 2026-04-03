from __future__ import annotations

import copy
import re
import sys
from dataclasses import dataclass
from pathlib import Path

if __package__ in (None, ''):
    sys.path.insert(0, str(Path(__file__).resolve().parent))

import fuse_ast as fa
from common import resolve_import_path
from environment import Environment
from errors import FuseRuntimeError
from parser import parse_source
from values import BoundMethod, DataInstance, FuseOption, FuseResult, ModuleValue, NativeFunction, UserFunction


class ReturnSignal(Exception):
    def __init__(self, value):
        self.value = value


class BreakSignal(Exception):
    pass


class ContinueSignal(Exception):
    pass


@dataclass
class DataDef:
    decl: fa.DataClassDecl
    methods: dict[str, fa.FunctionDecl]


class ModuleRuntime:
    def __init__(self, path: Path, program: fa.Program):
        self.path = path
        self.program = program
        self.functions: dict[str, fa.FunctionDecl] = {}
        self.extensions: dict[tuple[str, str], fa.FunctionDecl] = {}
        self.data_defs: dict[str, DataDef] = {}
        self.exports: dict[str, object] = {}
        self.imports: list[fa.ImportDecl] = []


class Evaluator:
    def __init__(self):
        self.modules: dict[Path, ModuleRuntime] = {}
        self.stdout: list[str] = []

    def load_module(self, path: str | Path) -> ModuleRuntime:
        path = Path(path).resolve()
        if path in self.modules:
            return self.modules[path]
        program = parse_source(path.read_text(encoding='utf-8'), path.name)
        module = ModuleRuntime(path, program)
        self.modules[path] = module
        for decl in program.declarations:
            if isinstance(decl, fa.ImportDecl):
                module.imports.append(decl)
            elif isinstance(decl, fa.FunctionDecl):
                if decl.receiver_type:
                    module.extensions[(decl.receiver_type, decl.name)] = decl
                else:
                    module.functions[decl.name] = decl
                    if decl.is_pub:
                        module.exports[decl.name] = decl
            elif isinstance(decl, fa.DataClassDecl):
                module.data_defs[decl.name] = DataDef(decl, {m.name: m for m in decl.methods})
                if decl.is_pub:
                    module.exports[decl.name] = decl
        return module

    def base_env(self) -> Environment:
        env = Environment()
        env.define('println', NativeFunction('println', self.builtin_println), mutable=False, type_name='Unit', destroy=False)
        env.define('Some', NativeFunction('Some', lambda v: FuseOption(True, v)), mutable=False, destroy=False)
        env.define('Ok', NativeFunction('Ok', lambda v: FuseResult(True, v)), mutable=False, destroy=False)
        env.define('Err', NativeFunction('Err', lambda v: FuseResult(False, v)), mutable=False, destroy=False)
        env.define('None', FuseOption(False, None), mutable=False, type_name='Option', destroy=False)
        return env

    def module_env(self, module: ModuleRuntime) -> Environment:
        env = self.base_env()
        for name, decl in module.functions.items():
            env.define(name, UserFunction(str(module.path), decl), mutable=False, destroy=False)
        for name, data_def in module.data_defs.items():
            env.define(name, NativeFunction(name, lambda *args, _name=name: self.construct(module, _name, list(args))), mutable=False, destroy=False)
        for imp in module.imports:
            imported = self.load_module(resolve_import_path(module.path, imp.module_path))
            if imp.items:
                for item in imp.items:
                    if item in imported.exports:
                        value = imported.exports[item]
                        if isinstance(value, fa.FunctionDecl):
                            value = UserFunction(str(imported.path), value)
                        elif isinstance(value, fa.DataClassDecl):
                            value = NativeFunction(item, lambda *args, _name=item, _module=imported: self.construct(_module, _name, list(args)))
                        env.define(item, value, mutable=False, destroy=False)
            else:
                env.define(imp.module_path.split('.')[-1], ModuleValue(imp.module_path, imported.exports), mutable=False, destroy=False)
        return env

    def construct(self, module: ModuleRuntime, name: str, args: list[object]):
        data_def = module.data_defs[name]
        fields = {}
        for field, arg in zip(data_def.decl.fields, args):
            fields[field.name] = arg
        return DataInstance(name, fields, data_def.methods, [f.name for f in data_def.decl.fields])

    def builtin_println(self, value):
        self.stdout.append(self.stringify(value))
        return None

    def stringify(self, value):
        if isinstance(value, bool):
            return 'true' if value else 'false'
        if isinstance(value, FuseOption):
            return 'Some(' + self.stringify(value.value) + ')' if value.is_some else 'None'
        if isinstance(value, FuseResult):
            tag = 'Ok' if value.is_ok else 'Err'
            return f'{tag}({self.stringify(value.value)})'
        if isinstance(value, DataInstance):
            return repr(value)
        if value is None:
            return 'Unit'
        return str(value)

    def eval_file(self, path: str | Path):
        self.stdout = []
        module = self.load_module(path)
        env = self.module_env(module)
        entry = None
        for fn in module.functions.values():
            if 'entrypoint' in fn.decorators:
                entry = fn
                break
        if entry is None:
            raise FuseRuntimeError('missing @entrypoint function', Path(path).name)
        self.call_user_function(module, entry, [], env)
        return '\n'.join(self.stdout)

    def call_user_function(self, module: ModuleRuntime, fn: fa.FunctionDecl, args: list[object], caller_env: Environment | None = None, receiver=None):
        env = Environment(self.module_env(module) if caller_env is None else caller_env)
        deferred: list[tuple[fa.ExprStmt | fa.DeferStmt | object, Environment]] = []
        if receiver is not None:
            env.define('self', receiver, mutable=True, type_name=getattr(receiver, 'type_name', None), destroy=False)
        for param, arg in zip(fn.params, args):
            passed = arg
            destroy = False
            if isinstance(arg, tuple) and len(arg) == 2 and arg[0] == '__move__':
                passed = arg[1]
                destroy = True
            elif param.convention == 'owned':
                passed = copy.deepcopy(arg)
                destroy = True
            env.define(param.name, passed, mutable=True, type_name=param.type_name, destroy=destroy)
        try:
            result = self.eval_block(module, fn.body, env, deferred)
        except ReturnSignal as signal:
            self.destroy_remaining(env)
            self.run_defers(module, deferred)
            return signal.value
        self.destroy_remaining(env)
        self.run_defers(module, deferred)
        return result

    def eval_block(self, module: ModuleRuntime, block: fa.Block, env: Environment, deferred):
        future = self.compute_future_uses(block.statements)
        last_result = None
        for idx, stmt in enumerate(block.statements):
            last_result = self.eval_statement(module, stmt, env, deferred)
            self.destroy_unused(env, future[idx], deferred)
        if block.statements and isinstance(block.statements[-1], fa.ExprStmt):
            return last_result
        return None

    def compute_future_uses(self, statements):
        future = [set() for _ in statements]
        seen: set[str] = set()
        for idx in range(len(statements) - 1, -1, -1):
            future[idx] = set(seen)
            seen.update(self.collect_stmt_names(statements[idx]))
        return future

    def collect_stmt_names(self, stmt):
        if isinstance(stmt, fa.VarDecl):
            return self.collect_expr_names(stmt.value)
        if isinstance(stmt, fa.Assign):
            return self.collect_expr_names(stmt.target) | self.collect_expr_names(stmt.value)
        if isinstance(stmt, fa.ReturnStmt):
            return self.collect_expr_names(stmt.value) if stmt.value else set()
        if isinstance(stmt, fa.WhileStmt):
            names = self.collect_expr_names(stmt.condition)
            for inner in stmt.body.statements:
                names |= self.collect_stmt_names(inner)
            return names
        if isinstance(stmt, fa.ForStmt):
            names = self.collect_expr_names(stmt.iterable)
            for inner in stmt.body.statements:
                names |= self.collect_stmt_names(inner)
            return names
        if isinstance(stmt, fa.LoopStmt):
            names = set()
            for inner in stmt.body.statements:
                names |= self.collect_stmt_names(inner)
            return names
        if isinstance(stmt, fa.DeferStmt):
            return self.collect_expr_names(stmt.expr)
        if isinstance(stmt, fa.ExprStmt):
            return self.collect_expr_names(stmt.expr)
        return set()

    def collect_expr_names(self, expr):
        if expr is None:
            return set()
        if isinstance(expr, fa.Name):
            return {expr.value}
        if isinstance(expr, fa.Literal):
            return set()
        if isinstance(expr, fa.FString):
            names = set()
            for match in re.finditer(r'\{([^{}]+)\}', expr.template):
                names.add(match.group(1).strip().split('.')[0])
            return names
        if isinstance(expr, fa.Member):
            return self.collect_expr_names(expr.object)
        if isinstance(expr, (fa.MoveExpr, fa.RefExpr, fa.MutRefExpr, fa.QuestionExpr, fa.UnaryOp)):
            return self.collect_expr_names(expr.value)
        if isinstance(expr, fa.BinaryOp):
            return self.collect_expr_names(expr.left) | self.collect_expr_names(expr.right)
        if isinstance(expr, fa.Call):
            names = self.collect_expr_names(expr.callee)
            for arg in expr.args:
                names |= self.collect_expr_names(arg)
            return names
        if isinstance(expr, fa.ListExpr):
            names = set()
            for item in expr.items:
                names |= self.collect_expr_names(item)
            return names
        if isinstance(expr, fa.IfExpr):
            names = self.collect_expr_names(expr.condition)
            for stmt in expr.then_branch.statements:
                names |= self.collect_stmt_names(stmt)
            if isinstance(expr.else_branch, fa.Block):
                for stmt in expr.else_branch.statements:
                    names |= self.collect_stmt_names(stmt)
            elif expr.else_branch is not None:
                names |= self.collect_expr_names(expr.else_branch)
            return names
        if isinstance(expr, fa.MatchExpr):
            names = self.collect_expr_names(expr.subject)
            for arm in expr.arms:
                names |= self.collect_expr_names(arm.body) if not isinstance(arm.body, fa.Block) else set().union(*[self.collect_stmt_names(s) for s in arm.body.statements])
            return names
        return set()

    def destroy_unused(self, env: Environment, future_names: set[str], deferred):
        defer_names = set()
        for expr, _ in deferred:
            defer_names |= self.collect_expr_names(expr)
        for name, binding in list(env.values.items()):
            if name in future_names or name in defer_names or binding.moved or not binding.destroy:
                continue
            if isinstance(binding.value, DataInstance):
                self.destroy_value(binding.value)
                binding.moved = True

    def destroy_remaining(self, env: Environment):
        for binding in env.values.values():
            if binding.moved or not binding.destroy:
                continue
            if isinstance(binding.value, DataInstance):
                self.destroy_value(binding.value)
                binding.moved = True

    def destroy_value(self, value):
        if not isinstance(value, DataInstance) or value.destroyed:
            return
        value.destroyed = True
        method = value.methods.get('__del__')
        if method:
            module = self.find_module_for_method(method)
            if module is not None:
                self.call_user_function(module, method, [value], self.module_env(module))

    def run_defers(self, module: ModuleRuntime, deferred):
        while deferred:
            expr, env = deferred.pop()
            self.eval_expr(module, expr, env)

    def find_module_for_method(self, method: fa.FunctionDecl):
        for module in self.modules.values():
            if any(method is m for data in module.data_defs.values() for m in data.methods.values()) or any(method is fn for fn in module.functions.values()) or any(method is fn for fn in module.extensions.values()):
                return module
        return None

    def eval_statement(self, module: ModuleRuntime, stmt, env: Environment, deferred):
        if isinstance(stmt, fa.VarDecl):
            value = self.eval_expr(module, stmt.value, env)
            env.define(stmt.name, value, stmt.mutable, stmt.type_name)
            return value
        if isinstance(stmt, fa.Assign):
            value = self.eval_expr(module, stmt.value, env)
            if isinstance(stmt.target, fa.Name):
                env.set(stmt.target.value, value)
                return value
            if isinstance(stmt.target, fa.Member):
                obj = self.eval_expr(module, stmt.target.object, env)
                if isinstance(obj, DataInstance):
                    obj.fields[stmt.target.name] = value
                    return value
                raise FuseRuntimeError(f'cannot assign member `{stmt.target.name}`', module.path.name, stmt.line, stmt.column)
        if isinstance(stmt, fa.ReturnStmt):
            raise ReturnSignal(self.eval_expr(module, stmt.value, env) if stmt.value is not None else None)
        if isinstance(stmt, fa.BreakStmt):
            raise BreakSignal()
        if isinstance(stmt, fa.ContinueStmt):
            raise ContinueSignal()
        if isinstance(stmt, fa.WhileStmt):
            while self.truthy(self.eval_expr(module, stmt.condition, env)):
                try:
                    self.eval_block(module, stmt.body, Environment(env), deferred)
                except BreakSignal:
                    break
                except ContinueSignal:
                    continue
            return None
        if isinstance(stmt, fa.ForStmt):
            iterable = self.eval_expr(module, stmt.iterable, env)
            for item in iterable:
                child = Environment(env)
                child.define(stmt.name, item, True)
                try:
                    self.eval_block(module, stmt.body, child, deferred)
                except BreakSignal:
                    break
                except ContinueSignal:
                    continue
            return None
        if isinstance(stmt, fa.LoopStmt):
            while True:
                try:
                    self.eval_block(module, stmt.body, Environment(env), deferred)
                except BreakSignal:
                    break
                except ContinueSignal:
                    continue
            return None
        if isinstance(stmt, fa.DeferStmt):
            deferred.append((stmt.expr, Environment(env)))
            return None
        if isinstance(stmt, fa.ExprStmt):
            return self.eval_expr(module, stmt.expr, env)
        return None

    def eval_expr(self, module: ModuleRuntime, expr, env: Environment):
        if isinstance(expr, fa.Literal):
            return expr.value
        if isinstance(expr, fa.FString):
            return self.render_fstring(expr.template, module, env)
        if isinstance(expr, fa.Name):
            if expr.value == 'None':
                return FuseOption(False, None)
            try:
                return env.get(expr.value)
            except KeyError:
                raise FuseRuntimeError(f'unknown name `{expr.value}`', module.path.name, expr.line, expr.column)
            except RuntimeError as err:
                raise FuseRuntimeError(str(err).replace('error: ', ''), module.path.name, expr.line, expr.column)
        if isinstance(expr, fa.ListExpr):
            return [self.eval_expr(module, item, env) for item in expr.items]
        if isinstance(expr, fa.UnaryOp):
            value = self.eval_expr(module, expr.value, env)
            return -value if expr.op == '-' else (not self.truthy(value))
        if isinstance(expr, fa.BinaryOp):
            if expr.op == '?:':
                left = self.eval_expr(module, expr.left, env)
                if isinstance(left, FuseOption):
                    return left.value if left.is_some else self.eval_expr(module, expr.right, env)
                return left
            left = self.eval_expr(module, expr.left, env)
            right = self.eval_expr(module, expr.right, env)
            if expr.op == '+':
                return left + right
            if expr.op == '-':
                return left - right
            if expr.op == '*':
                return left * right
            if expr.op == '/':
                return left // right if isinstance(left, int) and isinstance(right, int) else left / right
            if expr.op == '%':
                return left % right
            if expr.op == '==':
                return left == right
            if expr.op == '!=':
                return left != right
            if expr.op == '<':
                return left < right
            if expr.op == '>':
                return left > right
            if expr.op == '<=':
                return left <= right
            if expr.op == '>=':
                return left >= right
            if expr.op == 'and':
                return self.truthy(left) and self.truthy(right)
            if expr.op == 'or':
                return self.truthy(left) or self.truthy(right)
            raise FuseRuntimeError(f'unsupported operator `{expr.op}`', module.path.name, expr.line, expr.column)
        if isinstance(expr, fa.Member):
            obj = self.eval_expr(module, expr.object, env)
            if expr.optional:
                if isinstance(obj, FuseOption):
                    if not obj.is_some:
                        return FuseOption(False, None)
                    return FuseOption(True, self.resolve_member(obj.value, expr.name, module, env, expr))
                return FuseOption(False, None)
            return self.resolve_member(obj, expr.name, module, env, expr)
        if isinstance(expr, fa.MoveExpr):
            if isinstance(expr.value, fa.Name):
                value = env.get(expr.value.value)
                env.mark_moved(expr.value.value)
                return ('__move__', value)
            value = self.eval_expr(module, expr.value, env)
            return ('__move__', value)
        if isinstance(expr, (fa.RefExpr, fa.MutRefExpr)):
            return self.eval_expr(module, expr.value, env)
        if isinstance(expr, fa.QuestionExpr):
            value = self.eval_expr(module, expr.value, env)
            if isinstance(value, FuseResult):
                if value.is_ok:
                    return value.value
                raise ReturnSignal(value)
            if isinstance(value, FuseOption):
                if value.is_some:
                    return value.value
                raise ReturnSignal(value)
            return value
        if isinstance(expr, fa.Call):
            callee = self.eval_expr(module, expr.callee, env)
            args = [self.eval_expr(module, arg, env) for arg in expr.args]
            return self.call_value(module, callee, args)
        if isinstance(expr, fa.IfExpr):
            if self.truthy(self.eval_expr(module, expr.condition, env)):
                return self.eval_block(module, expr.then_branch, Environment(env), [])
            if expr.else_branch is None:
                return None
            if isinstance(expr.else_branch, fa.Block):
                return self.eval_block(module, expr.else_branch, Environment(env), [])
            return self.eval_expr(module, expr.else_branch, env)
        if isinstance(expr, fa.MatchExpr):
            subject = self.eval_expr(module, expr.subject, env)
            for arm in expr.arms:
                matched, bindings = self.match_pattern(arm.pattern, subject)
                if matched:
                    child = Environment(env)
                    for key, value in bindings.items():
                        child.define(key, value, True)
                    if isinstance(arm.body, fa.Block):
                        return self.eval_block(module, arm.body, child, [])
                    return self.eval_expr(module, arm.body, child)
            raise FuseRuntimeError('non-exhaustive match', module.path.name, expr.line, expr.column)
        if isinstance(expr, fa.WhenExpr):
            for cond, body in expr.arms:
                if cond == 'else' or self.truthy(self.eval_expr(module, cond, env)):
                    return self.eval_block(module, body, Environment(env), []) if isinstance(body, fa.Block) else self.eval_expr(module, body, env)
            return None
        raise FuseRuntimeError('unsupported expression', module.path.name, getattr(expr, 'line', 1), getattr(expr, 'column', 1))

    def call_value(self, module: ModuleRuntime, callee, args):
        if isinstance(callee, NativeFunction):
            return callee(*[arg[1] if isinstance(arg, tuple) and arg[0] == '__move__' else arg for arg in args])
        if isinstance(callee, BoundMethod):
            if isinstance(callee.method, fa.FunctionDecl):
                target_module = self.find_module_for_method(callee.method)
                return self.call_user_function(target_module, callee.method, [callee.receiver] + args, self.module_env(target_module))
            return callee.method(*args)
        if isinstance(callee, UserFunction):
            target_module = self.load_module(callee.module)
            return self.call_user_function(target_module, callee.decl, args, self.module_env(target_module))
        if callable(callee):
            return callee(*args)
        raise FuseRuntimeError(f'cannot call value `{callee}`')

    def resolve_member(self, obj, name: str, module: ModuleRuntime, env: Environment, expr):
        if isinstance(obj, DataInstance):
            if name in obj.fields:
                return obj.fields[name]
            if name in obj.methods:
                return BoundMethod(obj, obj.methods[name])
        if isinstance(obj, ModuleValue):
            value = obj.exports.get(name)
            if isinstance(value, fa.FunctionDecl):
                return UserFunction(obj.name, value)
            return value
        if isinstance(obj, str):
            if name == 'toUpper':
                return NativeFunction('toUpper', lambda: obj.upper())
            if name == 'isEmpty':
                return NativeFunction('isEmpty', lambda: len(obj) == 0)
        recv_type = self.runtime_type(obj)
        ext = self.find_extension(recv_type, name)
        if ext:
            return BoundMethod(obj, ext)
        raise FuseRuntimeError(f'unknown member `{name}`', module.path.name, expr.line, expr.column)

    def find_extension(self, recv_type: str, name: str):
        for module in self.modules.values():
            decl = module.extensions.get((recv_type, name))
            if decl:
                return decl
        return None

    def runtime_type(self, value):
        if isinstance(value, bool):
            return 'Bool'
        if isinstance(value, int):
            return 'Int'
        if isinstance(value, float):
            return 'Float'
        if isinstance(value, str):
            return 'String'
        if isinstance(value, list):
            return 'List'
        if isinstance(value, DataInstance):
            return value.type_name
        if isinstance(value, FuseOption):
            return 'Option'
        if isinstance(value, FuseResult):
            return 'Result'
        return type(value).__name__

    def truthy(self, value):
        if isinstance(value, FuseOption):
            return value.is_some
        if isinstance(value, FuseResult):
            return value.is_ok
        return bool(value)

    def match_pattern(self, pattern, value):
        if isinstance(pattern, fa.WildcardPattern):
            return True, {}
        if isinstance(pattern, fa.NamePattern):
            return True, {pattern.name: value}
        if isinstance(pattern, fa.LiteralPattern):
            return value == pattern.value, {}
        if isinstance(pattern, fa.VariantPattern):
            name = pattern.name.split('.')[-1]
            if name == 'Some' and isinstance(value, FuseOption) and value.is_some:
                if not pattern.args:
                    return True, {}
                return self.match_pattern(pattern.args[0], value.value)
            if name == 'None' and isinstance(value, FuseOption) and not value.is_some:
                return True, {}
            if name == 'Ok' and isinstance(value, FuseResult) and value.is_ok:
                if not pattern.args:
                    return True, {}
                return self.match_pattern(pattern.args[0], value.value)
            if name == 'Err' and isinstance(value, FuseResult) and not value.is_ok:
                if not pattern.args:
                    return True, {}
                return self.match_pattern(pattern.args[0], value.value)
            if isinstance(value, DataInstance) and value.type_name == name and not pattern.args:
                return True, {}
        return False, {}

    def render_fstring(self, template: str, module: ModuleRuntime, env: Environment):
        def replace(match):
            expr = match.group(1).strip()
            parts = expr.split('.')
            try:
                value = env.get(parts[0])
            except Exception:
                value = ''
            for part in parts[1:]:
                if isinstance(value, DataInstance):
                    value = value.fields.get(part)
                elif isinstance(value, FuseOption) and value.is_some:
                    value = value.value
                    if isinstance(value, DataInstance):
                        value = value.fields.get(part)
                else:
                    value = getattr(value, part, '') if not isinstance(value, dict) else value.get(part)
            return self.stringify(value)
        return re.sub(r'\{([^{}]+)\}', replace, template)
