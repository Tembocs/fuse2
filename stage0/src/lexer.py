from __future__ import annotations

import sys
from pathlib import Path

if __package__ in (None, ''):
    sys.path.insert(0, str(Path(__file__).resolve().parent))

from errors import FuseSyntaxError
from fuse_token import KEYWORDS, Token


class Lexer:
    def __init__(self, source: str, filename: str):
        self.source = source
        self.filename = filename
        self.index = 0
        self.line = 1
        self.column = 1

    def peek(self, offset: int = 0) -> str:
        pos = self.index + offset
        if pos >= len(self.source):
            return '\0'
        return self.source[pos]

    def advance(self) -> str:
        ch = self.peek()
        self.index += 1
        if ch == '\n':
            self.line += 1
            self.column = 1
        else:
            self.column += 1
        return ch

    def make(self, kind: str, text: str, line: int, column: int) -> Token:
        return Token(kind, text, line, column)

    def error(self, message: str, line: int | None = None, column: int | None = None):
        raise FuseSyntaxError(message, self.filename, line or self.line, column or self.column)

    def skip_ws_and_comments(self):
        while True:
            ch = self.peek()
            if ch in ' \t\r\n':
                self.advance()
                continue
            if ch == '/' and self.peek(1) == '/':
                while self.peek() not in ('\n', '\0'):
                    self.advance()
                continue
            break

    def read_number(self) -> Token:
        line, col = self.line, self.column
        text = ''
        while self.peek().isdigit():
            text += self.advance()
        if self.peek() == '.' and self.peek(1).isdigit():
            text += self.advance()
            while self.peek().isdigit():
                text += self.advance()
            return self.make('FLOAT', text, line, col)
        return self.make('INT', text, line, col)

    def read_ident(self) -> Token:
        line, col = self.line, self.column
        text = ''
        while self.peek().isalnum() or self.peek() == '_':
            text += self.advance()
        return self.make(KEYWORDS.get(text, 'IDENT'), text, line, col)

    def read_string(self, formatted: bool = False) -> Token:
        line, col = self.line, self.column
        if formatted:
            self.advance()
        self.advance()  # opening quote
        parts: list[str] = []
        while True:
            ch = self.peek()
            if ch == '\0':
                self.error('unterminated string literal', line, col)
            if ch == '"':
                self.advance()
                break
            if ch == '\\':
                self.advance()
                nxt = self.advance()
                parts.append({'n': '\n', 't': '\t', '"': '"', '\\': '\\'}.get(nxt, nxt))
                continue
            parts.append(self.advance())
        return self.make('FSTRING' if formatted else 'STRING', ''.join(parts), line, col)

    def tokenize(self) -> list[Token]:
        tokens: list[Token] = []
        while True:
            self.skip_ws_and_comments()
            line, col = self.line, self.column
            ch = self.peek()
            if ch == '\0':
                tokens.append(self.make('EOF', '', line, col))
                return tokens
            if ch.isdigit():
                tokens.append(self.read_number())
                continue
            if ch.isalpha() or ch == '_':
                if ch == 'f' and self.peek(1) == '"':
                    tokens.append(self.read_string(formatted=True))
                else:
                    tokens.append(self.read_ident())
                continue
            if ch == '"':
                tokens.append(self.read_string())
                continue

            two = ch + self.peek(1)
            pairs = {
                '=>': 'FATARROW',
                '->': 'ARROW',
                '?.': 'QDOT',
                '?:': 'ELVIS',
                '::': 'COLONCOLON',
                '==': 'EQEQ',
                '!=': 'NE',
                '<=': 'LE',
                '>=': 'GE',
            }
            if two in pairs:
                self.advance(); self.advance()
                tokens.append(self.make(pairs[two], two, line, col))
                continue
            singles = {
                '(': 'LPAREN', ')': 'RPAREN', '{': 'LBRACE', '}': 'RBRACE', '[': 'LBRACKET', ']': 'RBRACKET',
                ',': 'COMMA', ';': 'SEMI', ':': 'COLON', '.': 'DOT', '?': 'QUESTION', '@': 'AT',
                '=': 'EQ', '+': 'PLUS', '-': 'MINUS', '*': 'STAR', '/': 'SLASH', '%': 'PERCENT',
                '<': 'LT', '>': 'GT',
            }
            if ch in singles:
                self.advance()
                tokens.append(self.make(singles[ch], ch, line, col))
                continue
            self.error(f'unexpected character `{ch}`', line, col)


def lex(source: str, filename: str) -> list[Token]:
    return Lexer(source, filename).tokenize()
