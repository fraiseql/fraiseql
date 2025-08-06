"""Schema configuration for FraiseQL."""

from __future__ import annotations

from dataclasses import dataclass
from typing import ClassVar


@dataclass
class SchemaConfig:
    """Configuration for GraphQL schema generation."""

    camel_case_fields: bool = True
    """Whether to convert snake_case field names to camelCase in GraphQL schema (default: True)."""

    _instance: ClassVar[SchemaConfig | None] = None

    @classmethod
    def get_instance(cls) -> SchemaConfig:
        """Get or create the singleton instance."""
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    @classmethod
    def set_config(cls, **kwargs) -> None:
        """Update the configuration."""
        instance = cls.get_instance()
        for key, value in kwargs.items():
            if hasattr(instance, key):
                setattr(instance, key, value)

    @classmethod
    def reset(cls) -> None:
        """Reset to default configuration."""
        cls._instance = None
