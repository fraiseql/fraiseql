"""Configuration for entity-aware query routing."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal


@dataclass
class EntityRoutingConfig:
    """Configuration for entity-aware query routing.

    This configuration enables intelligent query routing based on entity complexity,
    optimizing performance while ensuring cache consistency.
    """

    turbo_entities: list[str]
    """Entities that should use turbo mode (have materialized views, benefit from caching)."""

    normal_entities: list[str]
    """Entities that should use normal mode (simple, real-time data preferred)."""

    mixed_query_strategy: Literal["normal", "turbo", "split"] = "normal"
    """How to handle queries containing both entity types:
    - normal: Always use normal mode for consistency
    - turbo: Always use turbo mode for performance
    - split: Split query into separate parts (future feature)
    """

    auto_routing_enabled: bool = True
    """Enable automatic query routing based on entity classification."""

    def __post_init__(self) -> None:
        """Validate configuration after initialization."""
        self._validate_configuration()

    def _validate_configuration(self) -> None:
        """Validate the routing configuration for common issues."""
        overlapping = set(self.turbo_entities) & set(self.normal_entities)
        if overlapping:
            raise ValueError(f"Entities cannot be in both turbo and normal lists: {overlapping}")

    def is_turbo_entity(self, entity_name: str) -> bool:
        """Check if an entity should use turbo mode."""
        return entity_name in self.turbo_entities

    def is_normal_entity(self, entity_name: str) -> bool:
        """Check if an entity should use normal mode."""
        return entity_name in self.normal_entities

    def get_entity_mode(self, entity_name: str) -> Literal["turbo", "normal", "unknown"]:
        """Get the execution mode for an entity."""
        if self.is_turbo_entity(entity_name):
            return "turbo"
        if self.is_normal_entity(entity_name):
            return "normal"
        return "unknown"
