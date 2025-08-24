"""
Taxonomy domain value objects.

Pure value objects for tags and categories.
"""
import re
from dataclasses import dataclass
from typing import Pattern

from ..common.base_classes import ValueObject
from ..common.exceptions import DomainValidationError


@dataclass(frozen=True)
class TagName(ValueObject):
    """Tag name value object."""
    
    value: str
    
    # Tag name pattern: letters, numbers, spaces, basic punctuation
    TAG_NAME_PATTERN: Pattern = re.compile(r'^[a-zA-Z0-9\s\-_\.]+$')
    
    def __post_init__(self):
        if not self.value or not self.value.strip():
            raise DomainValidationError("Tag name cannot be empty")
        
        # Normalize whitespace but preserve case
        normalized = ' '.join(self.value.strip().split())
        
        if len(normalized) < 1 or len(normalized) > 50:
            raise DomainValidationError("Tag name must be between 1 and 50 characters")
        
        if not self.TAG_NAME_PATTERN.match(normalized):
            raise DomainValidationError(
                "Tag name can only contain letters, numbers, spaces, hyphens, underscores, and periods"
            )
        
        # Reserved tag names
        reserved = {'all', 'none', 'null', 'undefined', 'admin', 'system'}
        if normalized.lower() in reserved:
            raise DomainValidationError(f"'{normalized}' is a reserved tag name")
        
        object.__setattr__(self, 'value', normalized)
    
    def __str__(self) -> str:
        return self.value
    
    @property
    def slug(self) -> str:
        """Generate a slug from the tag name."""
        # Convert to lowercase and replace spaces/special chars with hyphens
        slug = re.sub(r'[^a-z0-9]+', '-', self.value.lower())
        # Remove leading/trailing hyphens and multiple consecutive hyphens
        slug = re.sub(r'^-+|-+$', '', slug)
        slug = re.sub(r'-+', '-', slug)
        return slug


@dataclass(frozen=True)
class TagDescription(ValueObject):
    """Tag description value object."""
    
    value: str
    
    def __post_init__(self):
        # Description is optional, so empty is allowed
        normalized = self.value.strip() if self.value else ""
        
        if len(normalized) > 500:
            raise DomainValidationError("Tag description cannot exceed 500 characters")
        
        object.__setattr__(self, 'value', normalized)
    
    def __str__(self) -> str:
        return self.value
    
    @property
    def is_empty(self) -> bool:
        """Check if description is empty."""
        return not self.value


@dataclass(frozen=True)
class TagColor(ValueObject):
    """Tag color value object for UI theming."""
    
    value: str
    
    # Hex color pattern
    HEX_COLOR_PATTERN: Pattern = re.compile(r'^#[0-9A-Fa-f]{6}$')
    
    # Predefined color palette
    PREDEFINED_COLORS = {
        'red': '#dc3545',
        'orange': '#fd7e14',
        'yellow': '#ffc107',
        'green': '#28a745',
        'teal': '#20c997',
        'cyan': '#17a2b8',
        'blue': '#007bff',
        'indigo': '#6610f2',
        'purple': '#6f42c1',
        'pink': '#e83e8c',
        'gray': '#6c757d',
        'dark': '#343a40'
    }
    
    def __post_init__(self):
        if not self.value or not self.value.strip():
            # Default to gray if no color specified
            normalized = self.PREDEFINED_COLORS['gray']
        else:
            color_input = self.value.strip().lower()
            
            # Check if it's a predefined color name
            if color_input in self.PREDEFINED_COLORS:
                normalized = self.PREDEFINED_COLORS[color_input]
            # Check if it's a valid hex color
            elif self.HEX_COLOR_PATTERN.match(self.value.strip()):
                normalized = self.value.strip().upper()
            else:
                raise DomainValidationError(
                    f"Invalid color format. Use hex format (#RRGGBB) or one of: "
                    f"{', '.join(sorted(self.PREDEFINED_COLORS.keys()))}"
                )
        
        object.__setattr__(self, 'value', normalized)
    
    def __str__(self) -> str:
        return self.value
    
    @property
    def rgb_tuple(self) -> tuple[int, int, int]:
        """Convert hex color to RGB tuple."""
        hex_color = self.value.lstrip('#')
        return tuple(int(hex_color[i:i+2], 16) for i in (0, 2, 4))
    
    @property
    def is_light(self) -> bool:
        """Determine if color is light (for text contrast)."""
        r, g, b = self.rgb_tuple
        # Calculate relative luminance
        luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255
        return luminance > 0.5