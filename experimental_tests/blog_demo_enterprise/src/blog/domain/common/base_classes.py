"""
Base domain classes for entities and value objects.

Following DDD principles with pure Python classes.
"""
from abc import ABC, abstractmethod
from typing import Any, Generic, TypeVar, Union
from uuid import UUID
from dataclasses import dataclass, field
from datetime import datetime


T = TypeVar('T')


@dataclass(frozen=True)
class ValueObject:
    """Base class for all value objects."""

    def __eq__(self, other: Any) -> bool:
        if not isinstance(other, self.__class__):
            return False
        return self.__dict__ == other.__dict__


@dataclass(frozen=True)
class EntityId(ValueObject, Generic[T]):
    """Base class for entity identifiers."""

    value: UUID

    def __str__(self) -> str:
        return str(self.value)

    def __hash__(self) -> int:
        return hash(self.value)


@dataclass
class Entity(ABC):
    """Base class for all domain entities."""

    id: EntityId
    created_at: datetime = field(default_factory=datetime.utcnow, init=False)
    updated_at: datetime = field(default_factory=datetime.utcnow, init=False)
    version: int = field(default=1, init=False)

    def __eq__(self, other: Any) -> bool:
        if not isinstance(other, self.__class__):
            return False
        return self.id == other.id

    def __hash__(self) -> int:
        return hash(self.id)

    def _update_timestamp(self) -> None:
        """Update the entity's timestamp."""
        self.updated_at = datetime.utcnow()
        self.version += 1


@dataclass
class AggregateRoot(Entity):
    """Base class for aggregate roots."""

    _domain_events: list = field(default_factory=list, init=False)

    def add_domain_event(self, event: Any) -> None:
        """Add a domain event to be published."""
        self._domain_events.append(event)

    def clear_domain_events(self) -> list:
        """Clear and return domain events."""
        events = self._domain_events.copy()
        self._domain_events.clear()
        return events
