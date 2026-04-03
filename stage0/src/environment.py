from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass
class Binding:
    value: Any
    mutable: bool
    type_name: str | None = None
    moved: bool = False
    destroy: bool = True


class Environment:
    def __init__(self, parent: 'Environment | None' = None):
        self.parent = parent
        self.values: dict[str, Binding] = {}

    def define(self, name: str, value: Any, mutable: bool, type_name: str | None = None, destroy: bool = True):
        self.values[name] = Binding(value, mutable, type_name, False, destroy)

    def resolve(self, name: str) -> Binding:
        if name in self.values:
            return self.values[name]
        if self.parent is not None:
            return self.parent.resolve(name)
        raise KeyError(name)

    def get(self, name: str) -> Any:
        binding = self.resolve(name)
        if binding.moved:
            raise RuntimeError(f'cannot use `{name}` after `move`')
        return binding.value

    def set(self, name: str, value: Any):
        binding = self.resolve(name)
        if not binding.mutable:
            raise RuntimeError(f'cannot assign to immutable binding `{name}`')
        binding.value = value

    def mark_moved(self, name: str):
        binding = self.resolve(name)
        binding.moved = True

    def contains_local(self, name: str) -> bool:
        return name in self.values
