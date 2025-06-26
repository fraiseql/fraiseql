"""FraiseQL error handling module."""

from .user_friendly import (
    FraiseQLError,
    InvalidFieldTypeError,
    MissingDatabaseViewError,
    MissingTypeHintError,
    MutationNotFoundError,
    SQLGenerationError,
)

from .exceptions import (
    FraiseQLException,
    PartialInstantiationError,
    WhereClauseError,
    QueryValidationError,
    DatabaseQueryError,
    TypeRegistrationError,
    ResolverError,
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
