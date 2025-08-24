"""Core exceptions for Blog Demo Application.

Following PrintOptim Backend exception patterns for consistent error handling.
"""

from typing import Any, Optional


class BlogException(Exception):
    """Base exception for all blog application errors."""

    def __init__(
        self,
        message: str,
        code: Optional[str] = None,
        details: Optional[dict[str, Any]] = None
    ):
        super().__init__(message)
        self.message = message
        self.code = code or "BLOG_ERROR"
        self.details = details or {}


class BlogValidationError(BlogException):
    """Raised when validation fails."""

    def __init__(
        self,
        message: str,
        field: Optional[str] = None,
        value: Optional[Any] = None,
        details: Optional[dict[str, Any]] = None
    ):
        super().__init__(message, "VALIDATION_ERROR", details)
        self.field = field
        self.value = value


class BlogNotFoundError(BlogException):
    """Raised when a requested entity is not found."""

    def __init__(
        self,
        entity_type: str,
        identifier: Any,
        details: Optional[dict[str, Any]] = None
    ):
        message = f"{entity_type} not found: {identifier}"
        super().__init__(message, "NOT_FOUND", details)
        self.entity_type = entity_type
        self.identifier = identifier


class BlogDuplicateError(BlogException):
    """Raised when attempting to create a duplicate entity."""

    def __init__(
        self,
        entity_type: str,
        field: str,
        value: Any,
        existing_id: Optional[Any] = None,
        details: Optional[dict[str, Any]] = None
    ):
        message = f"Duplicate {entity_type}: {field} '{value}' already exists"
        super().__init__(message, "DUPLICATE_ERROR", details)
        self.entity_type = entity_type
        self.field = field
        self.value = value
        self.existing_id = existing_id


class BlogAuthorizationError(BlogException):
    """Raised when user lacks required permissions."""

    def __init__(
        self,
        action: str,
        resource: str,
        user_id: Optional[str] = None,
        details: Optional[dict[str, Any]] = None
    ):
        message = f"Not authorized to {action} {resource}"
        super().__init__(message, "AUTHORIZATION_ERROR", details)
        self.action = action
        self.resource = resource
        self.user_id = user_id


class BlogBusinessLogicError(BlogException):
    """Raised when business logic constraints are violated."""

    def __init__(
        self,
        message: str,
        constraint: Optional[str] = None,
        details: Optional[dict[str, Any]] = None
    ):
        super().__init__(message, "BUSINESS_LOGIC_ERROR", details)
        self.constraint = constraint


class BlogDataIntegrityError(BlogException):
    """Raised when data integrity constraints are violated."""

    def __init__(
        self,
        message: str,
        constraint: Optional[str] = None,
        details: Optional[dict[str, Any]] = None
    ):
        super().__init__(message, "DATA_INTEGRITY_ERROR", details)
        self.constraint = constraint


class BlogConfigurationError(BlogException):
    """Raised when there are configuration issues."""

    def __init__(
        self,
        message: str,
        setting: Optional[str] = None,
        details: Optional[dict[str, Any]] = None
    ):
        super().__init__(message, "CONFIGURATION_ERROR", details)
        self.setting = setting
