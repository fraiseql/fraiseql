"""
User domain value objects.

Pure value objects for user management.
"""
import re
from dataclasses import dataclass
from typing import Pattern

from ..common.base_classes import ValueObject
from ..common.exceptions import DomainValidationError


@dataclass(frozen=True)
class Email(ValueObject):
    """Email address value object."""
    
    value: str
    
    # Email validation pattern
    EMAIL_PATTERN: Pattern = re.compile(r'^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$')
    
    def __post_init__(self):
        if not self.value or not self.value.strip():
            raise DomainValidationError("Email cannot be empty")
        
        # Normalize to lowercase
        normalized = self.value.strip().lower()
        
        if len(normalized) > 254:  # RFC 5321 limit
            raise DomainValidationError("Email address too long")
        
        if not self.EMAIL_PATTERN.match(normalized):
            raise DomainValidationError("Invalid email format")
        
        object.__setattr__(self, 'value', normalized)
    
    def __str__(self) -> str:
        return self.value
    
    @property
    def domain(self) -> str:
        """Extract domain part of email."""
        return self.value.split('@')[1]
    
    @property
    def local_part(self) -> str:
        """Extract local part of email."""
        return self.value.split('@')[0]


@dataclass(frozen=True)
class Username(ValueObject):
    """Username value object."""
    
    value: str
    
    # Username pattern: alphanumeric, underscores, hyphens
    USERNAME_PATTERN: Pattern = re.compile(r'^[a-zA-Z0-9_-]+$')
    
    def __post_init__(self):
        if not self.value or not self.value.strip():
            raise DomainValidationError("Username cannot be empty")
        
        # Normalize to lowercase
        normalized = self.value.strip().lower()
        
        if len(normalized) < 2 or len(normalized) > 30:
            raise DomainValidationError("Username must be between 2 and 30 characters")
        
        if not self.USERNAME_PATTERN.match(normalized):
            raise DomainValidationError(
                "Username can only contain letters, numbers, underscores, and hyphens"
            )
        
        # Reserved usernames
        reserved = {
            'admin', 'administrator', 'root', 'system', 'api', 'www', 'mail', 'ftp',
            'support', 'help', 'info', 'blog', 'user', 'users', 'account', 'accounts'
        }
        if normalized in reserved:
            raise DomainValidationError(f"'{normalized}' is a reserved username")
        
        object.__setattr__(self, 'value', normalized)
    
    def __str__(self) -> str:
        return self.value


@dataclass(frozen=True)
class FullName(ValueObject):
    """Full name value object."""
    
    value: str
    
    def __post_init__(self):
        if not self.value or not self.value.strip():
            raise DomainValidationError("Full name cannot be empty")
        
        # Normalize whitespace
        normalized = ' '.join(self.value.strip().split())
        
        if len(normalized) < 2 or len(normalized) > 100:
            raise DomainValidationError("Full name must be between 2 and 100 characters")
        
        object.__setattr__(self, 'value', normalized)
    
    def __str__(self) -> str:
        return self.value
    
    @property
    def first_name(self) -> str:
        """Extract first name."""
        parts = self.value.split()
        return parts[0] if parts else ""
    
    @property
    def last_name(self) -> str:
        """Extract last name."""
        parts = self.value.split()
        return parts[-1] if len(parts) > 1 else ""