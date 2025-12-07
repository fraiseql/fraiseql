"""FraiseQL Mutations - v1.8.0

Breaking Changes in v1.8.0:
---------------------------
- Validation failures now return Error type (not Success with null entity)
- Error type includes `code` field (422, 404, 409, 500)
- Success type entity is always non-null
- Removed `error_as_data_prefixes` from error config

See docs/migrations/v1.8.0.md for migration guide.
"""

from .decorators import failure, resolve_union_annotation, result, success
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
    "failure",
    # Decorators
    "mutation",
    "resolve_union_annotation",
    "result",
    "success",
]

# Version check warning
import warnings

warnings.warn(
    "FraiseQL v1.8.0 includes breaking changes to mutation error handling. "
    "See docs/migrations/v1.8.0.md for migration guide.",
    FutureWarning,
    stacklevel=2,
)
