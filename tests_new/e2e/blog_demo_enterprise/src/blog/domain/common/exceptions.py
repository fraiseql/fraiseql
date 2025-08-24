"""
Domain layer exceptions.

Pure domain exceptions without infrastructure dependencies.
"""
from typing import Optional, Dict, Any


class DomainError(Exception):
    """Base class for all domain errors."""
    
    def __init__(self, message: str, error_code: Optional[str] = None, metadata: Optional[Dict[str, Any]] = None):
        super().__init__(message)
        self.message = message
        self.error_code = error_code or self.__class__.__name__.upper()
        self.metadata = metadata or {}


class DomainValidationError(DomainError):
    """Raised when domain validation rules are violated."""
    pass


class BusinessRuleViolationError(DomainError):
    """Raised when business rules are violated."""
    pass


class EntityNotFoundError(DomainError):
    """Raised when an entity is not found."""
    pass


class ConcurrencyError(DomainError):
    """Raised when concurrent modifications conflict."""
    pass