"""Core type definitions for FraiseQL."""

from collections.abc import Callable
from dataclasses import dataclass
from typing import Any


@dataclass
class FieldDefinition:
    """Base field definition."""

    name: str
    resolver: Callable
    return_type: type
    args: dict[str, Any]
    description: str | None = None


@dataclass
class QueryField(FieldDefinition):
    """Query field definition."""


@dataclass
class MutationField(FieldDefinition):
    """Mutation field definition."""


@dataclass
class SubscriptionField(FieldDefinition):
    """Subscription field definition."""
