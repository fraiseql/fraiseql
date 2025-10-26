"""Global registry for FraiseQL types and fields."""

from __future__ import annotations

from threading import Lock
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from fraiseql.core.types import MutationField, QueryField, SubscriptionField


class TypeRegistry:
    """Global registry for GraphQL types and fields."""

    _instance: TypeRegistry | None = None
    _lock = Lock()

    def __new__(cls):
        """Create or return singleton instance."""
        with cls._lock:
            if cls._instance is None:
                cls._instance = super().__new__(cls)
                cls._instance._initialized = False
            return cls._instance

    def __init__(self) -> None:
        if self._initialized:
            return

        self._queries: dict[str, QueryField] = {}
        self._mutations: dict[str, MutationField] = {}
        self._subscriptions: dict[str, SubscriptionField] = {}
        self._types: dict[str, Any] = {}
        self._initialized = True

    def register_query(self, field: QueryField) -> None:
        """Register a query field."""
        self._queries[field.name] = field

    def register_mutation(self, field: MutationField) -> None:
        """Register a mutation field."""
        self._mutations[field.name] = field

    def register_subscription(self, field: SubscriptionField) -> None:
        """Register a subscription field."""
        self._subscriptions[field.name] = field

    def register_type(self, name: str, type_def: Any) -> None:
        """Register a GraphQL type."""
        self._types[name] = type_def

    def get_queries(self) -> dict[str, QueryField]:
        """Get all registered queries."""
        return self._queries.copy()

    def get_mutations(self) -> dict[str, MutationField]:
        """Get all registered mutations."""
        return self._mutations.copy()

    def get_subscriptions(self) -> dict[str, SubscriptionField]:
        """Get all registered subscriptions."""
        return self._subscriptions.copy()

    def get_types(self) -> dict[str, Any]:
        """Get all registered types."""
        return self._types.copy()

    def clear(self) -> None:
        """Clear all registrations."""
        self._queries.clear()
        self._mutations.clear()
        self._subscriptions.clear()
        self._types.clear()


def get_registry() -> TypeRegistry:
    """Get the global type registry."""
    return TypeRegistry()
