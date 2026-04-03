from __future__ import annotations

from dataclasses import dataclass


class FuseError(Exception):
    pass


@dataclass
class FuseSyntaxError(FuseError):
    message: str
    filename: str
    line: int
    column: int

    def __str__(self) -> str:
        return format_diagnostic('error', self.message, self.filename, self.line, self.column)


@dataclass
class CheckError:
    message: str
    filename: str
    line: int
    column: int
    hint: str | None = None

    def render(self) -> str:
        return format_diagnostic('error', self.message, self.filename, self.line, self.column, self.hint)

    def __str__(self) -> str:
        return self.render()


@dataclass
class FuseRuntimeError(FuseError):
    message: str
    filename: str = '<runtime>'
    line: int = 1
    column: int = 1

    def __str__(self) -> str:
        return format_diagnostic('error', self.message, self.filename, self.line, self.column)


def format_diagnostic(kind: str, message: str, filename: str, line: int, column: int, hint: str | None = None) -> str:
    out = [f'{kind}: {message}', f'  --> {filename}:{line}:{column}']
    if hint:
        out.insert(1, f'       {hint}')
    return '\n'.join(out)
