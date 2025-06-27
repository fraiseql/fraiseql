"""FraiseQL error handling module."""

from .exceptions import (
    DatabaseQueryError,
    FraiseQLException,
    PartialInstantiationError,
    QueryValidationError,
    ResolverError,
    TypeRegistrationError,
    WhereClauseError,
)
from .user_friendly import (
    FraiseQLError,
    InvalidFieldTypeError,
    MissingDatabaseViewError,
    MissingTypeHintError,
    MutationNotFoundError,
    SQLGenerationError,
)

__all__ = [
    # User-friendly errors
    "FraiseQLError",
    "InvalidFieldTypeError",
    "MissingDatabaseViewError",
    "MissingTypeHintError",
    "MutationNotFoundError",
    "SQLGenerationError",
    # Enhanced exceptions with context
    "FraiseQLException",
    "PartialInstantiationError",
    "WhereClauseError",
    "QueryValidationError",
    "DatabaseQueryError",
    "TypeRegistrationError",
    "ResolverError",
]
