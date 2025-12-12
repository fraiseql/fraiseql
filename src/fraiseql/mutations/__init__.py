"""FraiseQL Mutations

Core mutation functionality for FraiseQL, including decorators, error handling,
and response type management.
"""

from .decorators import error, resolve_union_annotation, result, success
from .error_config import (
    ALWAYS_DATA_CONFIG,
    DEFAULT_ERROR_CONFIG,
    STRICT_STATUS_CONFIG,
    MutationErrorConfig,
)
from .mutation_decorator import mutation
from .types import MutationError, MutationResult, MutationSuccess

__all__ = [
    "ALWAYS_DATA_CONFIG",  # Deprecated
    "DEFAULT_ERROR_CONFIG",
    "STRICT_STATUS_CONFIG",  # Deprecated
    # Types
    "MutationError",
    # Error configuration
    "MutationErrorConfig",
    "MutationResult",
    "MutationSuccess",
    "error",
    # Decorators
    "mutation",
    "resolve_union_annotation",
    "result",
    "success",
]
