"""Schema configuration for FraiseQL."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, ClassVar


class IDPolicy(str, Enum):
    """Policy for GraphQL ID scalar type behavior.

    FraiseQL provides two ID policies:

    - UUID: IDs must be valid UUIDs, enforced at GraphQL layer (default)
    - OPAQUE: IDs accept any string, following GraphQL spec

    Example:
        >>> from fraiseql.config.schema_config import SchemaConfig, IDPolicy
        >>> SchemaConfig.set_config(id_policy=IDPolicy.OPAQUE)
    """

    UUID = "uuid"
    """IDs must be valid UUIDs. This is FraiseQL's opinionated default."""

    OPAQUE = "opaque"
    """IDs accept any string. This follows the GraphQL specification."""

    def enforces_uuid(self) -> bool:
        """Check if this policy enforces UUID format for IDs.

        Returns:
            True if IDs must be valid UUIDs, False otherwise.
        """
        return self == IDPolicy.UUID


@dataclass
class SchemaConfig:
    """Configuration for GraphQL schema generation."""

    camel_case_fields: bool = True
    """Whether to convert snake_case field names to camelCase in GraphQL schema (default: True)."""

    id_policy: IDPolicy = field(default=IDPolicy.UUID)
    """Policy for ID scalar type behavior (default: UUID enforcement)."""

    _instance: ClassVar[SchemaConfig | None] = None

    @classmethod
    def get_instance(cls) -> SchemaConfig:
        """Get or create the singleton instance."""
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    @classmethod
    def set_config(cls, **kwargs: Any) -> None:
        """Update the configuration."""
        instance = cls.get_instance()
        for key, value in kwargs.items():
            if hasattr(instance, key):
                setattr(instance, key, value)

    @classmethod
    def reset(cls) -> None:
        """Reset to default configuration."""
        cls._instance = None
