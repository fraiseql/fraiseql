"""Core type definitions for FraiseQL."""

from typing import Any, Callable, Dict, Optional, Type
from dataclasses import dataclass


@dataclass
class FieldDefinition:
    """Base field definition."""
    name: str
    resolver: Callable
    return_type: Type
    args: Dict[str, Any]
    description: Optional[str] = None


@dataclass  
class QueryField(FieldDefinition):
    """Query field definition."""
    pass


@dataclass
class MutationField(FieldDefinition):
    """Mutation field definition."""
    pass


@dataclass
class SubscriptionField(FieldDefinition):
    """Subscription field definition."""
    pass