"""
Content domain value objects.

Pure value objects for posts, comments and related content.
"""
import re
from dataclasses import dataclass
from typing import Pattern

from ..common.base_classes import ValueObject
from ..common.exceptions import DomainValidationError


@dataclass(frozen=True)
class Slug(ValueObject):
    """URL slug value object."""
    
    value: str
    
    # Slug pattern: lowercase letters, numbers, hyphens
    SLUG_PATTERN: Pattern = re.compile(r'^[a-z0-9]+(?:-[a-z0-9]+)*$')
    
    def __post_init__(self):
        if not self.value or not self.value.strip():
            raise DomainValidationError("Slug cannot be empty")
        
        # Normalize to lowercase and remove extra spaces
        normalized = self.value.strip().lower()
        
        if len(normalized) < 1 or len(normalized) > 100:
            raise DomainValidationError("Slug must be between 1 and 100 characters")
        
        if not self.SLUG_PATTERN.match(normalized):
            raise DomainValidationError(
                "Slug can only contain lowercase letters, numbers, and hyphens. "
                "It cannot start or end with a hyphen."
            )
        
        object.__setattr__(self, 'value', normalized)
    
    def __str__(self) -> str:
        return self.value
    
    @classmethod
    def from_title(cls, title: str) -> 'Slug':
        """Generate a slug from a title."""
        if not title or not title.strip():
            raise DomainValidationError("Title cannot be empty")
        
        # Convert to lowercase and replace spaces/special chars with hyphens
        slug = re.sub(r'[^a-z0-9]+', '-', title.strip().lower())
        # Remove leading/trailing hyphens and multiple consecutive hyphens
        slug = re.sub(r'^-+|-+$', '', slug)
        slug = re.sub(r'-+', '-', slug)
        
        if not slug:
            raise DomainValidationError("Cannot generate valid slug from title")
        
        return cls(slug)


@dataclass(frozen=True)
class Title(ValueObject):
    """Post/comment title value object."""
    
    value: str
    
    def __post_init__(self):
        if not self.value or not self.value.strip():
            raise DomainValidationError("Title cannot be empty")
        
        # Normalize whitespace
        normalized = ' '.join(self.value.strip().split())
        
        if len(normalized) < 1 or len(normalized) > 200:
            raise DomainValidationError("Title must be between 1 and 200 characters")
        
        object.__setattr__(self, 'value', normalized)
    
    def __str__(self) -> str:
        return self.value


@dataclass(frozen=True)
class Content(ValueObject):
    """Post/comment content value object."""
    
    value: str
    
    def __post_init__(self):
        if not self.value or not self.value.strip():
            raise DomainValidationError("Content cannot be empty")
        
        # Keep original formatting but strip leading/trailing whitespace
        normalized = self.value.strip()
        
        if len(normalized) < 1:
            raise DomainValidationError("Content cannot be empty")
        
        if len(normalized) > 50000:  # 50KB limit for content
            raise DomainValidationError("Content exceeds maximum length of 50,000 characters")
        
        object.__setattr__(self, 'value', normalized)
    
    def __str__(self) -> str:
        return self.value
    
    @property
    def word_count(self) -> int:
        """Calculate approximate word count."""
        return len(self.value.split())
    
    @property
    def character_count(self) -> int:
        """Get character count."""
        return len(self.value)
    
    def excerpt(self, length: int = 150) -> str:
        """Get content excerpt."""
        if len(self.value) <= length:
            return self.value
        
        # Find the last space before the length limit
        excerpt = self.value[:length]
        last_space = excerpt.rfind(' ')
        if last_space > length * 0.8:  # If space is reasonably close to limit
            return excerpt[:last_space] + "..."
        else:
            return excerpt + "..."


@dataclass(frozen=True)
class PostStatus(ValueObject):
    """Post status value object."""
    
    value: str
    
    # Valid post statuses
    VALID_STATUSES = {'draft', 'published', 'archived', 'deleted'}
    
    def __post_init__(self):
        if not self.value or not self.value.strip():
            raise DomainValidationError("Post status cannot be empty")
        
        normalized = self.value.strip().lower()
        
        if normalized not in self.VALID_STATUSES:
            raise DomainValidationError(
                f"Invalid post status. Must be one of: {', '.join(sorted(self.VALID_STATUSES))}"
            )
        
        object.__setattr__(self, 'value', normalized)
    
    def __str__(self) -> str:
        return self.value
    
    def is_published(self) -> bool:
        """Check if post is published."""
        return self.value == 'published'
    
    def is_draft(self) -> bool:
        """Check if post is draft."""
        return self.value == 'draft'
    
    def is_archived(self) -> bool:
        """Check if post is archived."""
        return self.value == 'archived'
    
    def is_deleted(self) -> bool:
        """Check if post is deleted."""
        return self.value == 'deleted'