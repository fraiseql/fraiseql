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
    "DatabaseQueryError",
    # User-friendly errors
    "FraiseQLError",
    # Enhanced exceptions with context
    "FraiseQLException",
    "InvalidFieldTypeError",
    "MissingDatabaseViewError",
    "MissingTypeHintError",
    "MutationNotFoundError",
    "PartialInstantiationError",
    "QueryValidationError",
    "ResolverError",
    "SQLGenerationError",
    "TypeRegistrationError",
    "WhereClauseError",
]
