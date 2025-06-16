"""Global registry for FraiseQL types and fields."""

from typing import Dict, List, Any, Optional
from threading import Lock

from fraiseql.core.types import QueryField, MutationField, SubscriptionField


class TypeRegistry:
    """Global registry for GraphQL types and fields."""
    
    _instance: Optional['TypeRegistry'] = None
    _lock = Lock()
    
    def __new__(cls):
        with cls._lock:
            if cls._instance is None:
                cls._instance = super().__new__(cls)
                cls._instance._initialized = False
            return cls._instance
    
    def __init__(self):
        if self._initialized:
            return
        
        self._queries: Dict[str, QueryField] = {}
        self._mutations: Dict[str, MutationField] = {}
        self._subscriptions: Dict[str, SubscriptionField] = {}
        self._types: Dict[str, Any] = {}
        self._initialized = True
    
    def register_query(self, field: QueryField):
        """Register a query field."""
        self._queries[field.name] = field
    
    def register_mutation(self, field: MutationField):
        """Register a mutation field."""
        self._mutations[field.name] = field
    
    def register_subscription(self, field: SubscriptionField):
        """Register a subscription field."""
        self._subscriptions[field.name] = field
    
    def register_type(self, name: str, type_def: Any):
        """Register a GraphQL type."""
        self._types[name] = type_def
    
    def get_queries(self) -> Dict[str, QueryField]:
        """Get all registered queries."""
        return self._queries.copy()
    
    def get_mutations(self) -> Dict[str, MutationField]:
        """Get all registered mutations."""
        return self._mutations.copy()
    
    def get_subscriptions(self) -> Dict[str, SubscriptionField]:
        """Get all registered subscriptions."""
        return self._subscriptions.copy()
    
    def get_types(self) -> Dict[str, Any]:
        """Get all registered types."""
        return self._types.copy()
    
    def clear(self):
        """Clear all registrations."""
        self._queries.clear()
        self._mutations.clear()
        self._subscriptions.clear()
        self._types.clear()


def get_registry() -> TypeRegistry:
    """Get the global type registry."""
    return TypeRegistry()