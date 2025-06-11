"""Mutations module for FraiseQL."""

from .decorators import failure, resolve_union_annotation, result, success
from .mutation_decorator import mutation
from .parser import parse_mutation_result
from .types import MutationResult

__all__ = [
    "MutationResult",
    "failure",
    "mutation",
    "parse_mutation_result",
    "resolve_union_annotation",
    "result",
    "success",
]
