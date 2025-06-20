"""Custom exceptions for FraiseQL."""


class FraiseQLError(Exception):
    """Base exception for FraiseQL errors."""


class SchemaError(FraiseQLError):
    """Raised when there's an error in schema construction."""


class ValidationError(FraiseQLError):
    """Raised when validation fails."""


class AuthenticationError(FraiseQLError):
    """Raised when authentication fails."""


class AuthorizationError(FraiseQLError):
    """Raised when authorization fails."""


class ConfigurationError(FraiseQLError):
    """Raised when configuration is invalid."""


class ComplexityLimitExceededError(FraiseQLError):
    """Raised when query complexity exceeds limits."""


class FilterError(FraiseQLError):
    """Raised when filter expression is invalid."""


class N1QueryDetectedError(FraiseQLError):
    """Raised when N+1 query pattern is detected."""


class WebSocketError(FraiseQLError):
    """Raised when WebSocket operations fail."""
