from __future__ import annotations

import sys
from pathlib import Path

if __package__ in (None, ''):
    sys.path.insert(0, str(Path(__file__).resolve().parent))

import fuse_ast as fa
from errors import FuseSyntaxError
from lexer import lex
from fuse_token import Token


class Parser:
    def __init__(self, tokens: list[Token], filename: str):
        self.tokens = tokens
        self.filename = filename
        self.index = 0

    def peek(self, offset: int = 0) -> Token:
        pos = self.index + offset
        if pos >= len(self.tokens):
            return self.tokens[-1]
        return self.tokens[pos]

    def match(self, *kinds: str) -> Token | None:
        if self.peek().kind in kinds:
            tok = self.peek()
            self.index += 1
            return tok
        return None

    def expect(self, kind: str, message: str | None = None) -> Token:
        tok = self.peek()
        if tok.kind != kind:
            raise FuseSyntaxError(message or f'expected {kind}, found {tok.kind}', self.filename, tok.line, tok.column)
        self.index += 1
        return tok

    def parse(self) -> fa.Program:
        decls = []
        while self.peek().kind != 'EOF':
            decls.append(self.parse_top_level())
        return fa.Program(decls, self.filename)

    def parse_top_level(self):
        decorators = []
        while self.match('AT'):
            name = self.expect('IDENT', 'expected decorator name')
            decorators.append(name.text)
        is_pub = bool(self.match('PUB'))
        tok = self.peek()
        if tok.kind == 'IMPORT':
            return self.parse_import()
        if tok.kind == 'FN':
            fn = self.parse_function()
            fn.decorators = decorators
            fn.is_pub = is_pub
            return fn
        if tok.kind == 'DATA':
            decl = self.parse_data_class()
            decl.decorators = decorators
            decl.is_pub = is_pub
            return decl
        if tok.kind == 'ENUM':
            decl = self.parse_enum()
            decl.is_pub = is_pub
            return decl
        raise FuseSyntaxError(f'unexpected top-level token {tok.kind}', self.filename, tok.line, tok.column)

    def parse_import(self) -> fa.ImportDecl:
        start = self.expect('IMPORT')
        parts = [self.expect('IDENT', 'expected import path segment').text]
        while self.peek().kind == 'DOT' and self.peek(1).kind == 'IDENT':
            self.index += 1
            parts.append(self.expect('IDENT').text)
        items = None
        if self.peek().kind == 'DOT' and self.peek(1).kind == 'LBRACE':
            self.index += 1
            self.expect('LBRACE', 'expected `{` after import path')
            items = [self.expect('IDENT').text]
            while self.match('COMMA'):
                items.append(self.expect('IDENT').text)
            self.expect('RBRACE', 'expected `}` after import items')
        return fa.ImportDecl('.'.join(parts), items, start.line, start.column)

    def parse_function(self) -> fa.FunctionDecl:
        start = self.expect('FN')
        first = self.expect('IDENT', 'expected function name or receiver type')
        receiver_type = None
        name = first.text
        if self.match('DOT'):
            receiver_type = first.text
            name = self.expect('IDENT', 'expected extension function name').text
        self.expect('LPAREN', 'expected `(` after function name')
        params = []
        if self.peek().kind != 'RPAREN':
            while True:
                params.append(self.parse_param())
                if not self.match('COMMA'):
                    break
        self.expect('RPAREN', 'expected `)` after parameters')
        return_type = None
        if self.match('ARROW'):
            return_type = self.parse_type_name(stop={'LBRACE', 'FATARROW'})
        if self.match('FATARROW'):
            expr = self.parse_expression()
            body = fa.Block([fa.ExprStmt(expr, expr.line, expr.column)], expr.line, expr.column)
        else:
            body = self.parse_block()
        return fa.FunctionDecl(name, params, return_type, body, receiver_type=receiver_type, line=start.line, column=start.column)

    def parse_param(self) -> fa.Param:
        start = self.peek()
        convention = None
        if self.peek().kind in ('REF', 'MUTREF', 'OWNED'):
            convention = self.peek().text
            self.index += 1
        name = self.expect('IDENT', 'expected parameter name')
        type_name = None
        if self.match('COLON'):
            type_name = self.parse_type_name(stop={'COMMA', 'RPAREN'})
        return fa.Param(convention, name.text, type_name, start.line, start.column)

    def parse_type_name(self, stop: set[str]) -> str:
        parts: list[str] = []
        depth = 0
        while True:
            tok = self.peek()
            if tok.kind == 'EOF':
                break
            if depth == 0 and tok.kind in stop:
                break
            if tok.kind in {'LT', 'LPAREN', 'LBRACKET'}:
                depth += 1
            elif tok.kind in {'GT', 'RPAREN', 'RBRACKET'}:
                depth -= 1
            parts.append(tok.text)
            self.index += 1
        return ''.join(parts).strip()

    def parse_data_class(self) -> fa.DataClassDecl:
        start = self.expect('DATA')
        self.expect('CLASS', 'expected `class` after `data`')
        name = self.expect('IDENT', 'expected data class name')
        self.expect('LPAREN')
        fields = []
        if self.peek().kind != 'RPAREN':
            while True:
                mutable_tok = self.peek()
                if mutable_tok.kind not in ('VAL', 'VAR'):
                    raise FuseSyntaxError('expected `val` or `var` field', self.filename, mutable_tok.line, mutable_tok.column)
                self.index += 1
                field_name = self.expect('IDENT', 'expected field name')
                type_name = None
                if self.match('COLON'):
                    type_name = self.parse_type_name(stop={'COMMA', 'RPAREN'})
                fields.append(fa.FieldDecl(mutable_tok.kind == 'VAR', field_name.text, type_name, mutable_tok.line, mutable_tok.column))
                if not self.match('COMMA'):
                    break
        self.expect('RPAREN')
        methods = []
        if self.match('LBRACE'):
            while self.peek().kind != 'RBRACE':
                decorators = []
                while self.match('AT'):
                    decorators.append(self.expect('IDENT').text)
                is_pub = bool(self.match('PUB'))
                fn = self.parse_function()
                fn.decorators = decorators
                fn.is_pub = is_pub
                methods.append(fn)
            self.expect('RBRACE')
        return fa.DataClassDecl(name.text, fields, methods, line=start.line, column=start.column)

    def parse_enum(self) -> fa.EnumDecl:
        start = self.expect('ENUM')
        name = self.expect('IDENT', 'expected enum name')
        self.expect('LBRACE')
        variants = []
        while self.peek().kind != 'RBRACE':
            vname = self.expect('IDENT', 'expected variant name')
            arity = 0
            if self.match('LPAREN'):
                if self.peek().kind != 'RPAREN':
                    arity = 1
                    while self.match('COMMA'):
                        arity += 1
                        self.parse_type_name(stop={'COMMA', 'RPAREN'})
                    if self.peek(-1).kind != 'COMMA':
                        self.parse_type_name(stop={'COMMA', 'RPAREN'})
                self.expect('RPAREN')
            variants.append(fa.EnumVariant(vname.text, arity, vname.line, vname.column))
            self.match('COMMA')
        self.expect('RBRACE')
        return fa.EnumDecl(name.text, variants, line=start.line, column=start.column)

    def parse_block(self) -> fa.Block:
        start = self.expect('LBRACE', 'expected `{` to start block')
        statements = []
        while self.peek().kind != 'RBRACE':
            statements.append(self.parse_statement())
            self.match('SEMI')
        self.expect('RBRACE', 'expected `}` to close block')
        return fa.Block(statements, start.line, start.column)

    def parse_statement(self):
        tok = self.peek()
        if tok.kind in ('VAL', 'VAR'):
            return self.parse_var_decl()
        if tok.kind == 'RETURN':
            self.index += 1
            if self.peek().kind in ('RBRACE', 'SEMI'):
                return fa.ReturnStmt(None, tok.line, tok.column)
            value = self.parse_expression()
            return fa.ReturnStmt(value, tok.line, tok.column)
        if tok.kind == 'BREAK':
            self.index += 1
            return fa.BreakStmt(tok.line, tok.column)
        if tok.kind == 'CONTINUE':
            self.index += 1
            return fa.ContinueStmt(tok.line, tok.column)
        if tok.kind == 'WHILE':
            return self.parse_while()
        if tok.kind == 'FOR':
            return self.parse_for()
        if tok.kind == 'LOOP':
            return self.parse_loop()
        if tok.kind == 'DEFER':
            self.index += 1
            expr = self.parse_expression()
            return fa.DeferStmt(expr, tok.line, tok.column)
        expr = self.parse_expression()
        if self.match('EQ'):
            value = self.parse_expression()
            return fa.Assign(expr, value, tok.line, tok.column)
        return fa.ExprStmt(expr, tok.line, tok.column)

    def parse_var_decl(self):
        start = self.peek()
        self.index += 1
        name = self.expect('IDENT', 'expected binding name')
        type_name = None
        if self.match('COLON'):
            type_name = self.parse_type_name(stop={'EQ'})
        self.expect('EQ', 'expected `=` in binding')
        value = self.parse_expression()
        return fa.VarDecl(start.kind == 'VAR', name.text, type_name, value, start.line, start.column)

    def parse_while(self):
        start = self.expect('WHILE')
        cond = self.parse_expression()
        body = self.parse_block()
        return fa.WhileStmt(cond, body, start.line, start.column)

    def parse_for(self):
        start = self.expect('FOR')
        name = self.expect('IDENT', 'expected loop variable')
        self.expect('IN', 'expected `in` in for loop')
        iterable = self.parse_expression()
        body = self.parse_block()
        return fa.ForStmt(name.text, iterable, body, start.line, start.column)

    def parse_loop(self):
        start = self.expect('LOOP')
        return fa.LoopStmt(self.parse_block(), start.line, start.column)

    def parse_expression(self):
        return self.parse_elvis()

    def parse_elvis(self):
        expr = self.parse_or()
        while self.match('ELVIS'):
            op = self.tokens[self.index - 1]
            right = self.parse_or()
            expr = fa.BinaryOp(expr, '?:', right, op.line, op.column)
        return expr

    def parse_or(self):
        expr = self.parse_and()
        while self.match('OR'):
            op = self.tokens[self.index - 1]
            right = self.parse_and()
            expr = fa.BinaryOp(expr, 'or', right, op.line, op.column)
        return expr

    def parse_and(self):
        expr = self.parse_equality()
        while self.match('AND'):
            op = self.tokens[self.index - 1]
            right = self.parse_equality()
            expr = fa.BinaryOp(expr, 'and', right, op.line, op.column)
        return expr

    def parse_equality(self):
        expr = self.parse_compare()
        while self.peek().kind in ('EQEQ', 'NE'):
            op = self.peek(); self.index += 1
            right = self.parse_compare()
            expr = fa.BinaryOp(expr, op.text, right, op.line, op.column)
        return expr

    def parse_compare(self):
        expr = self.parse_term()
        while self.peek().kind in ('LT', 'GT', 'LE', 'GE'):
            op = self.peek(); self.index += 1
            right = self.parse_term()
            expr = fa.BinaryOp(expr, op.text, right, op.line, op.column)
        return expr

    def parse_term(self):
        expr = self.parse_factor()
        while self.peek().kind in ('PLUS', 'MINUS'):
            op = self.peek(); self.index += 1
            right = self.parse_factor()
            expr = fa.BinaryOp(expr, op.text, right, op.line, op.column)
        return expr

    def parse_factor(self):
        expr = self.parse_unary()
        while self.peek().kind in ('STAR', 'SLASH', 'PERCENT'):
            op = self.peek(); self.index += 1
            right = self.parse_unary()
            expr = fa.BinaryOp(expr, op.text, right, op.line, op.column)
        return expr

    def parse_unary(self):
        tok = self.peek()
        if tok.kind in ('MINUS', 'NOT'):
            self.index += 1
            return fa.UnaryOp(tok.text, self.parse_unary(), tok.line, tok.column)
        if tok.kind == 'MOVE':
            self.index += 1
            return fa.MoveExpr(self.parse_unary(), tok.line, tok.column)
        if tok.kind == 'REF':
            self.index += 1
            return fa.RefExpr(self.parse_unary(), tok.line, tok.column)
        if tok.kind == 'MUTREF':
            self.index += 1
            return fa.MutRefExpr(self.parse_unary(), tok.line, tok.column)
        return self.parse_postfix()

    def parse_postfix(self):
        expr = self.parse_primary()
        while True:
            if self.match('LPAREN'):
                args = []
                if self.peek().kind != 'RPAREN':
                    while True:
                        args.append(self.parse_expression())
                        if not self.match('COMMA'):
                            break
                rparen = self.expect('RPAREN', 'expected `)` after arguments')
                expr = fa.Call(expr, args, rparen.line, rparen.column)
                continue
            if self.match('DOT'):
                name = self.expect('IDENT', 'expected member name after `.`')
                expr = fa.Member(expr, name.text, False, name.line, name.column)
                continue
            if self.match('QDOT'):
                name = self.expect('IDENT', 'expected member name after `?.`')
                expr = fa.Member(expr, name.text, True, name.line, name.column)
                continue
            if self.match('QUESTION'):
                tok = self.tokens[self.index - 1]
                expr = fa.QuestionExpr(expr, tok.line, tok.column)
                continue
            break
        return expr

    def parse_primary(self):
        tok = self.peek()
        if tok.kind == 'INT':
            self.index += 1
            return fa.Literal(int(tok.text), tok.line, tok.column)
        if tok.kind == 'FLOAT':
            self.index += 1
            return fa.Literal(float(tok.text), tok.line, tok.column)
        if tok.kind == 'STRING':
            self.index += 1
            return fa.Literal(tok.text, tok.line, tok.column)
        if tok.kind == 'FSTRING':
            self.index += 1
            return fa.FString(tok.text, tok.line, tok.column)
        if tok.kind == 'TRUE':
            self.index += 1
            return fa.Literal(True, tok.line, tok.column)
        if tok.kind == 'FALSE':
            self.index += 1
            return fa.Literal(False, tok.line, tok.column)
        if tok.kind == 'IDENT':
            self.index += 1
            return fa.Name(tok.text, tok.line, tok.column)
        if tok.kind == 'LPAREN':
            self.index += 1
            expr = self.parse_expression()
            self.expect('RPAREN', 'expected `)`')
            return expr
        if tok.kind == 'LBRACKET':
            self.index += 1
            items = []
            if self.peek().kind != 'RBRACKET':
                while True:
                    items.append(self.parse_expression())
                    if not self.match('COMMA'):
                        break
            self.expect('RBRACKET', 'expected `]`')
            return fa.ListExpr(items, tok.line, tok.column)
        if tok.kind == 'IF':
            return self.parse_if_expr()
        if tok.kind == 'MATCH':
            return self.parse_match_expr()
        if tok.kind == 'WHEN':
            return self.parse_when_expr()
        raise FuseSyntaxError(f'unexpected token {tok.kind}', self.filename, tok.line, tok.column)

    def parse_if_expr(self):
        start = self.expect('IF')
        cond = self.parse_expression()
        then_branch = self.parse_block()
        else_branch = None
        if self.match('ELSE'):
            if self.peek().kind == 'IF':
                else_branch = self.parse_if_expr()
            else:
                else_branch = self.parse_block()
        return fa.IfExpr(cond, then_branch, else_branch, start.line, start.column)

    def parse_match_expr(self):
        start = self.expect('MATCH')
        subject = self.parse_expression()
        self.expect('LBRACE')
        arms = []
        while self.peek().kind != 'RBRACE':
            pat = self.parse_pattern()
            arrow = self.expect('FATARROW', 'expected `=>` in match arm')
            body = self.parse_block() if self.peek().kind == 'LBRACE' else self.parse_expression()
            arms.append(fa.MatchArm(pat, body, arrow.line, arrow.column))
            self.match('COMMA')
        self.expect('RBRACE')
        return fa.MatchExpr(subject, arms, start.line, start.column)

    def parse_when_expr(self):
        start = self.expect('WHEN')
        self.expect('LBRACE')
        arms = []
        while self.peek().kind != 'RBRACE':
            if self.peek().kind == 'ELSE':
                self.index += 1
                cond = 'else'
                arrow = self.expect('FATARROW')
            else:
                cond = self.parse_expression()
                arrow = self.expect('FATARROW')
            body = self.parse_block() if self.peek().kind == 'LBRACE' else self.parse_expression()
            arms.append((cond, body))
            self.match('COMMA')
        self.expect('RBRACE')
        return fa.WhenExpr(arms, start.line, start.column)

    def parse_pattern(self):
        tok = self.peek()
        if tok.kind == 'IDENT' and tok.text == '_':
            self.index += 1
            return fa.WildcardPattern(tok.line, tok.column)
        if tok.kind in ('INT', 'STRING', 'TRUE', 'FALSE'):
            expr = self.parse_primary()
            value = expr.value if isinstance(expr, fa.Literal) else expr.template
            return fa.LiteralPattern(value, tok.line, tok.column)
        if tok.kind == 'IDENT':
            self.index += 1
            name = tok.text
            if self.match('DOT'):
                name += '.' + self.expect('IDENT').text
            if self.match('LPAREN'):
                args = []
                if self.peek().kind != 'RPAREN':
                    while True:
                        args.append(self.parse_pattern())
                        if not self.match('COMMA'):
                            break
                self.expect('RPAREN')
                return fa.VariantPattern(name, args, tok.line, tok.column)
            if name and name[0].isupper():
                return fa.VariantPattern(name, [], tok.line, tok.column)
            return fa.NamePattern(name, tok.line, tok.column)
        raise FuseSyntaxError('unsupported match pattern', self.filename, tok.line, tok.column)


def parse_source(source: str, filename: str) -> fa.Program:
    return Parser(lex(source, filename), filename).parse()


def main(argv: list[str] | None = None) -> int:
    argv = argv or sys.argv[1:]
    if not argv:
        print('usage: parser.py <file.fuse>')
        return 1
    path = Path(argv[0])
    program = parse_source(path.read_text(encoding='utf-8'), path.name)
    print(program)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
