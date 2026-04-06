from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Token:
    kind: str
    text: str
    line: int
    column: int

    def __repr__(self) -> str:
        return f'Token({self.kind!r}, {self.text!r}, {self.line}:{self.column})'


KEYWORDS = {
    'fn': 'FN',
    'val': 'VAL',
    'var': 'VAR',
    'ref': 'REF',
    'mutref': 'MUTREF',
    'owned': 'OWNED',
    'move': 'MOVE',
    'struct': 'STRUCT',
    'data': 'DATA',
    'class': 'CLASS',
    'enum': 'ENUM',
    'match': 'MATCH',
    'when': 'WHEN',
    'if': 'IF',
    'else': 'ELSE',
    'for': 'FOR',
    'in': 'IN',
    'loop': 'LOOP',
    'return': 'RETURN',
    'defer': 'DEFER',
    'while': 'WHILE',
    'break': 'BREAK',
    'continue': 'CONTINUE',
    'extern': 'EXTERN',
    'pub': 'PUB',
    'import': 'IMPORT',
    'true': 'TRUE',
    'false': 'FALSE',
    'and': 'AND',
    'or': 'OR',
    'not': 'NOT',
}
