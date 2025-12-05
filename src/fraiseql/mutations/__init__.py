"""Mutations module for FraiseQL."""

from .decorators import failure, resolve_union_annotation, result, success
from .error_config import (
    ALWAYS_DATA_CONFIG,
    DEFAULT_ERROR_CONFIG,
    STRICT_STATUS_CONFIG,
    MutationErrorConfig,
)
from .mutation_decorator import mutation
from .types import MutationResult

__all__ = [
    "ALWAYS_DATA_CONFIG",
    "DEFAULT_ERROR_CONFIG",
    "STRICT_STATUS_CONFIG",
    "MutationErrorConfig",
    "MutationResult",
    "failure",
    "mutation",
    "resolve_union_annotation",
    "result",
    "success",
]
