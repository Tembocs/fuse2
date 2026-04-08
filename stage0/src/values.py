from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable


@dataclass
class FuseResult:
    is_ok: bool
    value: Any

    def __repr__(self) -> str:
        tag = 'Ok' if self.is_ok else 'Err'
        return f'{tag}({self.value!r})'


@dataclass
class FuseOption:
    is_some: bool
    value: Any | None = None

    def __repr__(self) -> str:
        return f'Some({self.value!r})' if self.is_some else 'None'


@dataclass
class DataInstance:
    type_name: str
    fields: dict[str, Any]
    methods: dict[str, Any] = field(default_factory=dict)
    field_order: list[str] = field(default_factory=list)
    destroyed: bool = False

    def __repr__(self) -> str:
        parts = ', '.join(repr(self.fields[name]) for name in self.field_order)
        return f'{self.type_name}({parts})'

    def __eq__(self, other: Any) -> bool:
        return isinstance(other, DataInstance) and self.type_name == other.type_name and self.fields == other.fields


@dataclass
class NativeFunction:
    name: str
    fn: Callable[..., Any]

    def __call__(self, *args, **kwargs):
        return self.fn(*args, **kwargs)


@dataclass
class UserFunction:
    module: str
    decl: Any


@dataclass
class BoundMethod:
    receiver: Any
    method: Any


@dataclass
class ModuleValue:
    name: str
    exports: dict[str, Any]


@dataclass
class EnumType:
    """Runtime representation of an enum type. Allows EnumType.Variant access."""
    name: str
    variants: dict[str, str]  # variant_name -> "EnumName.VariantName"

    def __repr__(self) -> str:
        return self.name


@dataclass
class EnumVariantValue:
    """Runtime value of a simple enum variant (no payload)."""
    enum_name: str
    variant_name: str

    def __repr__(self) -> str:
        return self.variant_name

    def __eq__(self, other: Any) -> bool:
        return isinstance(other, EnumVariantValue) and self.enum_name == other.enum_name and self.variant_name == other.variant_name

    def __hash__(self) -> int:
        return hash((self.enum_name, self.variant_name))
