"""Configuration for query complexity analysis."""

from dataclasses import dataclass, field
from typing import ClassVar


@dataclass
class ComplexityConfig:
    """Configuration for query complexity analysis.

    This allows fine-tuning the complexity scoring algorithm
    for different use cases and performance requirements.
    """

    # Field scoring
    base_field_cost: int = 1
    """Base cost for each field in the query"""

    # Depth scoring
    depth_multiplier: float = 1.5
    """Multiplier for depth scoring (depth ^ multiplier)"""

    max_depth_penalty: int = 100
    """Maximum penalty for depth to prevent overflow"""

    # Array field scoring
    array_field_multiplier: int = 10
    """Base multiplier for array fields"""

    array_depth_factor: float = 1.2
    """How much depth affects array scoring"""

    # Type diversity
    type_diversity_cost: int = 2
    """Cost per unique type accessed"""

    # Cache thresholds
    simple_query_threshold: int = 10
    """Queries below this are considered simple"""

    moderate_query_threshold: int = 50
    """Queries below this are considered moderate"""

    complex_query_threshold: int = 200
    """Queries above this are considered too complex to cache"""

    # Cache weights
    simple_query_weight: float = 0.1
    """Cache weight for simple queries"""

    moderate_query_weight: float = 0.5
    """Cache weight for moderate queries"""

    complex_query_weight: float = 2.0
    """Cache weight for complex queries"""

    # Array detection patterns
    array_field_patterns: list[str] = field(
        default_factory=lambda: [
            "items",
            "list",
            "all",
            "many",
            "collection",
            "users",
            "posts",
            "comments",
            "replies",
            "reactions",
            "messages",
            "notifications",
            "events",
            "records",
        ],
    )
    """Patterns that indicate array fields"""

    # Default instance
    _default: ClassVar["ComplexityConfig | None"] = None

    @classmethod
    def get_default(cls) -> "ComplexityConfig":
        """Get the default configuration instance."""
        if cls._default is None:
            cls._default = cls()
        return cls._default

    @classmethod
    def set_default(cls, config: "ComplexityConfig") -> None:
        """Set the default configuration instance."""
        cls._default = config

    def is_array_field(self, field_name: str) -> bool:
        """Check if a field name indicates an array field.

        Args:
            field_name: The field name to check

        Returns:
            True if the field is likely an array
        """
        # Check if field ends with 's' (plural)
        if field_name.endswith("s") and len(field_name) > 2:
            return True

        # Check patterns
        field_lower = field_name.lower()
        return any(pattern in field_lower for pattern in self.array_field_patterns)

    def calculate_depth_penalty(self, depth: int) -> int:
        """Calculate the depth penalty with bounds checking.

        Args:
            depth: The nesting depth

        Returns:
            The depth penalty score
        """
        if depth == 0:
            return 0

        # Use multiplier with max cap
        penalty = int(depth**self.depth_multiplier)
        return min(penalty, self.max_depth_penalty)

    def calculate_array_penalty(self, depth: int, count: int) -> int:
        """Calculate the array field penalty.

        Args:
            depth: The depth at which arrays appear
            count: Number of array fields

        Returns:
            The array penalty score
        """
        if count == 0:
            return 0

        # Base multiplier adjusted by depth
        depth_factor = self.array_depth_factor**depth
        return int(count * self.array_field_multiplier * depth_factor)

    def get_cache_weight(self, total_score: int) -> float:
        """Get the cache weight based on complexity score.

        Args:
            total_score: The total complexity score

        Returns:
            Cache weight for the query
        """
        if total_score < self.simple_query_threshold:
            return self.simple_query_weight
        if total_score < self.moderate_query_threshold:
            return self.moderate_query_weight
        if total_score < self.complex_query_threshold:
            return self.complex_query_weight
        # Very complex queries get exponentially higher weights
        return self.complex_query_weight * (total_score / self.complex_query_threshold)


# Preset configurations
STRICT_CONFIG = ComplexityConfig(
    depth_multiplier=2.0,
    array_field_multiplier=15,
    complex_query_threshold=150,
)
"""Strict configuration for limited resources"""

BALANCED_CONFIG = ComplexityConfig()
"""Balanced default configuration"""

RELAXED_CONFIG = ComplexityConfig(
    depth_multiplier=1.2,
    array_field_multiplier=5,
    complex_query_threshold=500,
)
"""Relaxed configuration for powerful servers"""
