"""Base domain classes for entities and value objects.

Following DDD principles with pure Python classes for enterprise blog domain.
"""

from abc import ABC
from dataclasses import dataclass, field
from datetime import datetime
from uuid import UUID, uuid4


@dataclass(frozen=True)
class ValueObject:
    """Base class for all value objects."""

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, self.__class__):
            return False
        return self.__dict__ == other.__dict__


@dataclass(frozen=True)
class EntityId(ValueObject):
    """Base class for entity identifiers."""

    value: UUID = field(default_factory=uuid4)

    def __str__(self) -> str:
        return str(self.value)

    def __hash__(self) -> int:
        return hash(self.value)


@dataclass
class Entity(ABC):
    """Base class for all domain entities."""

    id: EntityId
    organization_id: UUID  # Multi-tenant support
    created_at: datetime = field(default_factory=datetime.utcnow, init=False)
    updated_at: datetime = field(default_factory=datetime.utcnow, init=False)
    created_by: UUID | None = None
    updated_by: UUID | None = None
    version: int = field(default=1, init=False)

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, self.__class__):
            return False
        return self.id == other.id

    def __hash__(self) -> int:
        return hash(self.id)

    def _update_timestamp(self) -> None:
        """Update the entity's timestamp and version."""
        self.updated_at = datetime.utcnow()
        self.version += 1


@dataclass
class DomainEvent:
    """Base class for all domain events."""

    event_id: UUID = field(default_factory=uuid4, init=False)
    occurred_at: datetime = field(default_factory=datetime.utcnow, init=False)
    version: int = field(default=1, init=False)
    correlation_id: UUID | None = field(default=None, init=False)
    causation_id: UUID | None = field(default=None, init=False)


@dataclass
class AggregateRoot(Entity):
    """Base class for aggregate roots with domain events."""

    _domain_events: list[DomainEvent] = field(default_factory=list, init=False)

    def emit_event(self, event: DomainEvent) -> None:
        """Add a domain event to be published."""
        self._domain_events.append(event)

    def get_uncommitted_events(self) -> list[DomainEvent]:
        """Get uncommitted domain events."""
        return self._domain_events.copy()

    def mark_events_as_committed(self) -> None:
        """Clear domain events after they've been committed."""
        self._domain_events.clear()


@dataclass(frozen=True)
class BusinessRule:
    """Base class for business rules."""

    def is_broken(self) -> bool:
        """Check if the business rule is violated."""
        raise NotImplementedError

    def get_message(self) -> str:
        """Get error message when rule is broken."""
        raise NotImplementedError


class DomainException(Exception):
    """Base exception for domain-related errors."""

    def __init__(self, message: str, details: dict | None = None):
        super().__init__(message)
        self.message = message
        self.details = details or {}


class BusinessRuleViolationException(DomainException):
    """Exception thrown when a business rule is violated."""

    def __init__(self, rule: BusinessRule):
        super().__init__(rule.get_message(), {"rule": rule.__class__.__name__})
        self.rule = rule
