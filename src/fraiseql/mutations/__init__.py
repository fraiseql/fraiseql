"""Mutations module for FraiseQL."""

from .decorators import failure, resolve_union_annotation, result, success
from .error_config import (
    ALWAYS_DATA_CONFIG,
    DEFAULT_ERROR_CONFIG,
    PRINTOPTIM_ERROR_CONFIG,
    MutationErrorConfig,
)
from .mutation_decorator import mutation
from .parser import parse_mutation_result
from .types import MutationResult

__all__ = [
    "ALWAYS_DATA_CONFIG",
    "DEFAULT_ERROR_CONFIG",
    "PRINTOPTIM_ERROR_CONFIG",
    "MutationErrorConfig",
    "MutationResult",
    "failure",
    "mutation",
    "parse_mutation_result",
    "resolve_union_annotation",
    "result",
    "success",
]
