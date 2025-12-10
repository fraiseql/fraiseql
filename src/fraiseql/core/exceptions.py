"""Custom exceptions for FraiseQL.

This module defines the exception hierarchy used throughout FraiseQL.
All FraiseQL exceptions inherit from FraiseQLError for easy catching.

Exception Hierarchy:
    FraiseQLError (base)
    ├── SchemaError - GraphQL schema construction issues
    ├── ValidationError - Input validation failures
    ├── AuthenticationError - User authentication failures
    ├── AuthorizationError - Permission/access control failures
    ├── ConfigurationError - Invalid configuration settings
    ├── ComplexityLimitExceededError - Query complexity limits exceeded
    ├── FilterError - Invalid filter expressions
    ├── N1QueryDetectedError - N+1 query pattern detected
    └── WebSocketError - WebSocket operation failures
"""


class FraiseQLError(Exception):
    """Base exception for all FraiseQL errors.

    This is the root exception class for FraiseQL. All other FraiseQL
    exceptions inherit from this class, allowing applications to catch
    all FraiseQL-related errors with a single except clause.
    """


class SchemaError(FraiseQLError):
    """Raised when there's an error in GraphQL schema construction.

    This exception is raised when:
    - Type definitions are invalid or incomplete
    - Field definitions conflict with GraphQL spec
    - Schema validation fails during construction
    """


class ValidationError(FraiseQLError):
    """Raised when input validation fails.

    This exception is raised when:
    - User input doesn't match expected types
    - Required fields are missing
    - Field values fail validation rules
    """


class AuthenticationError(FraiseQLError):
    """Raised when user authentication fails.

    This exception is raised when:
    - Invalid credentials are provided
    - Authentication tokens are expired or invalid
    - User authentication is required but not provided
    """


class AuthorizationError(FraiseQLError):
    """Raised when authorization/permission checks fail.

    This exception is raised when:
    - User lacks required permissions
    - Role-based access control denies access
    - Field-level authorization fails
    """


class ConfigurationError(FraiseQLError):
    """Raised when FraiseQL configuration is invalid.

    This exception is raised when:
    - Required configuration settings are missing
    - Configuration values are invalid
    - Database connection configuration is incorrect
    """


class ComplexityLimitExceededError(FraiseQLError):
    """Raised when GraphQL query complexity exceeds configured limits.

    This exception is raised when:
    - Query depth exceeds maximum allowed depth
    - Query breadth (number of fields) exceeds limits
    - Computed complexity score exceeds threshold
    """


class FilterError(FraiseQLError):
    """Raised when filter expressions are invalid.

    This exception is raised when:
    - Where clause syntax is incorrect
    - Filter operators are not supported for field types
    - Filter values don't match expected types
    """


class N1QueryDetectedError(FraiseQLError):
    """Raised when N+1 query patterns are detected.

    This exception is raised when:
    - Resolvers make inefficient database queries
    - DataLoader is not used for batching requests
    - Query optimization detects N+1 patterns
    """


class WebSocketError(FraiseQLError):
    """Raised when WebSocket operations fail.

    This exception is raised when:
    - WebSocket connections cannot be established
    - Subscription operations fail
    - Real-time communication errors occur
    """
