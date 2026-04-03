from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class Program:
    declarations: list[Any]
    filename: str


@dataclass
class ImportDecl:
    module_path: str
    items: list[str] | None
    line: int
    column: int


@dataclass
class Param:
    convention: str | None
    name: str
    type_name: str | None
    line: int
    column: int


@dataclass
class FieldDecl:
    mutable: bool
    name: str
    type_name: str | None
    line: int
    column: int


@dataclass
class FunctionDecl:
    name: str
    params: list[Param]
    return_type: str | None
    body: Any
    is_pub: bool = False
    decorators: list[str] = field(default_factory=list)
    receiver_type: str | None = None
    line: int = 1
    column: int = 1


@dataclass
class DataClassDecl:
    name: str
    fields: list[FieldDecl]
    methods: list[FunctionDecl]
    is_pub: bool = False
    decorators: list[str] = field(default_factory=list)
    line: int = 1
    column: int = 1


@dataclass
class EnumVariant:
    name: str
    arity: int
    line: int
    column: int


@dataclass
class EnumDecl:
    name: str
    variants: list[EnumVariant]
    is_pub: bool = False
    line: int = 1
    column: int = 1


@dataclass
class Block:
    statements: list[Any]
    line: int
    column: int


@dataclass
class VarDecl:
    mutable: bool
    name: str
    type_name: str | None
    value: Any
    line: int
    column: int


@dataclass
class Assign:
    target: Any
    value: Any
    line: int
    column: int


@dataclass
class ReturnStmt:
    value: Any | None
    line: int
    column: int


@dataclass
class BreakStmt:
    line: int
    column: int


@dataclass
class ContinueStmt:
    line: int
    column: int


@dataclass
class WhileStmt:
    condition: Any
    body: Block
    line: int
    column: int


@dataclass
class ForStmt:
    name: str
    iterable: Any
    body: Block
    line: int
    column: int


@dataclass
class LoopStmt:
    body: Block
    line: int
    column: int


@dataclass
class DeferStmt:
    expr: Any
    line: int
    column: int


@dataclass
class ExprStmt:
    expr: Any
    line: int
    column: int


@dataclass
class IfExpr:
    condition: Any
    then_branch: Block
    else_branch: Block | Any | None
    line: int
    column: int


@dataclass
class MatchArm:
    pattern: Any
    body: Any
    line: int
    column: int


@dataclass
class MatchExpr:
    subject: Any
    arms: list[MatchArm]
    line: int
    column: int


@dataclass
class WhenExpr:
    arms: list[tuple[Any | str, Any]]
    line: int
    column: int


@dataclass
class Literal:
    value: Any
    line: int
    column: int


@dataclass
class FString:
    template: str
    line: int
    column: int


@dataclass
class Name:
    value: str
    line: int
    column: int


@dataclass
class ListExpr:
    items: list[Any]
    line: int
    column: int


@dataclass
class UnaryOp:
    op: str
    value: Any
    line: int
    column: int


@dataclass
class BinaryOp:
    left: Any
    op: str
    right: Any
    line: int
    column: int


@dataclass
class Call:
    callee: Any
    args: list[Any]
    line: int
    column: int


@dataclass
class Member:
    object: Any
    name: str
    optional: bool
    line: int
    column: int


@dataclass
class MoveExpr:
    value: Any
    line: int
    column: int


@dataclass
class RefExpr:
    value: Any
    line: int
    column: int


@dataclass
class MutRefExpr:
    value: Any
    line: int
    column: int


@dataclass
class QuestionExpr:
    value: Any
    line: int
    column: int


@dataclass
class WildcardPattern:
    line: int
    column: int


@dataclass
class LiteralPattern:
    value: Any
    line: int
    column: int


@dataclass
class NamePattern:
    name: str
    line: int
    column: int


@dataclass
class VariantPattern:
    name: str
    args: list[Any]
    line: int
    column: int
