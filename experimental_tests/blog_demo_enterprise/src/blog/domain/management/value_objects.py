"""
Management domain value objects.

Pure value objects for organization management.
"""
import re
from dataclasses import dataclass
from typing import Pattern

from ..common.base_classes import ValueObject
from ..common.exceptions import DomainValidationError


@dataclass(frozen=True)
class OrganizationName(ValueObject):
    """Organization name value object."""

    value: str

    def __post_init__(self):
        if not self.value or not self.value.strip():
            raise DomainValidationError("Organization name cannot be empty")

        if len(self.value.strip()) > 200:
            raise DomainValidationError("Organization name cannot exceed 200 characters")

        # Store normalized value - collapse multiple spaces
        normalized = ' '.join(self.value.strip().split())
        object.__setattr__(self, 'value', normalized)

    def __str__(self) -> str:
        return self.value


@dataclass(frozen=True)
class OrganizationIdentifier(ValueObject):
    """Organization identifier (subdomain) value object."""

    value: str

    # URL-safe identifier pattern
    IDENTIFIER_PATTERN: Pattern = re.compile(r'^[a-z0-9][a-z0-9-]*[a-z0-9]$')

    def __post_init__(self):
        if not self.value or not self.value.strip():
            raise DomainValidationError("Organization identifier cannot be empty")

        # Normalize to lowercase
        normalized = self.value.strip().lower()

        if len(normalized) < 2 or len(normalized) > 50:
            raise DomainValidationError("Organization identifier must be between 2 and 50 characters")

        if not self.IDENTIFIER_PATTERN.match(normalized):
            raise DomainValidationError(
                "Invalid organization identifier format. Must contain only lowercase letters, "
                "numbers, and hyphens, and cannot start or end with a hyphen."
            )

        # Reserved identifiers
        reserved = {'api', 'www', 'admin', 'support', 'help', 'blog', 'mail', 'ftp'}
        if normalized in reserved:
            raise DomainValidationError(f"'{normalized}' is a reserved identifier")

        object.__setattr__(self, 'value', normalized)

    def __str__(self) -> str:
        return self.value


@dataclass(frozen=True)
class ContactEmail(ValueObject):
    """Contact email value object."""

    value: str

    # Basic email pattern
    EMAIL_PATTERN: Pattern = re.compile(r'^[^@]+@[^@]+\.[^@]+$')

    def __post_init__(self):
        if not self.value or not self.value.strip():
            raise DomainValidationError("Contact email cannot be empty")

        # Normalize to lowercase
        normalized = self.value.strip().lower()

        if not self.EMAIL_PATTERN.match(normalized):
            raise DomainValidationError("Invalid email format")

        object.__setattr__(self, 'value', normalized)

    def __str__(self) -> str:
        return self.value

    @property
    def domain(self) -> str:
        """Extract domain part of email."""
        return self.value.split('@')[1]
