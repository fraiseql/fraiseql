"""Schema registry for managing GraphQL type registrations."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

from fraiseql.config.schema_config import SchemaConfig

if TYPE_CHECKING:
    from collections.abc import Callable

    from graphql import GraphQLEnumType

logger = logging.getLogger(__name__)


class SchemaRegistry:
    """Singleton registry for GraphQL query types and mutation resolvers."""

    _instance = None

    def __init__(self) -> None:
        """Initialize empty registries for types, mutations, enums, and interfaces."""
        self._types: dict[type, type] = {}
        self._mutations: dict[str, Callable[..., Any]] = {}
        self._queries: dict[str, Callable[..., Any]] = {}
        self._subscriptions: dict[str, Callable[..., Any]] = {}
        self._enums: dict[type, GraphQLEnumType] = {}
        self._interfaces: dict[type, type] = {}

    @classmethod
    def get_instance(cls) -> SchemaRegistry:
        """Get or create the singleton registry instance."""
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    def clear(self) -> None:
        """Clear all registries and caches.

        This method clears:
        - SchemaRegistry's internal types and mutations
        - SchemaConfig settings
        - Registered enums
        - Mutation decorator registries
        - GraphQL type caches
        """
        logger.debug("Clearing the registry...")
        # Clear SchemaRegistry's own registries
        self._types.clear()
        self._mutations.clear()
        self._queries.clear()
        self._subscriptions.clear()
        self._enums.clear()
        self._interfaces.clear()
        logger.debug("Registry after clearing: %s", list(self._types.keys()))

        # Clear mutation decorator registries
        from fraiseql.mutations.decorators import clear_mutation_registries

        clear_mutation_registries()

        # Reset SchemaConfig to defaults
        SchemaConfig.reset()

        # Clear GraphQL type cache since field names might change
        from fraiseql.core.graphql_type import _graphql_type_cache

        _graphql_type_cache.clear()

    def register_type(self, typ: type) -> None:
        """Register a Python type as a GraphQL query type.

        Args:
            typ: The decorated Python type to register.
        """
        if typ in self._types:
            logger.debug("Type '%s' is already registered in the schema.", typ.__name__)
        else:
            logger.debug("Registering type '%s' to the schema.", typ.__name__)

        self._types[typ] = typ
        logger.debug("Current registry: %s", list(self._types.keys()))

    def register_enum(self, enum_cls: type, graphql_enum: GraphQLEnumType) -> None:
        """Register a Python Enum class as a GraphQL enum type.

        Args:
            enum_cls: The Python Enum class decorated with @fraise_enum.
            graphql_enum: The corresponding GraphQL enum type.
        """
        if enum_cls in self._enums:
            logger.debug("Enum '%s' is already registered in the schema.", enum_cls.__name__)
        else:
            logger.debug("Registering enum '%s' to the schema.", enum_cls.__name__)

        self._enums[enum_cls] = graphql_enum

    def register_interface(self, interface_cls: type) -> None:
        """Register a Python class as a GraphQL interface type.

        Args:
            interface_cls: The Python class decorated with @fraise_interface.
        """
        if interface_cls in self._interfaces:
            logger.debug(
                "Interface '%s' is already registered in the schema.",
                interface_cls.__name__,
            )
        else:
            logger.debug("Registering interface '%s' to the schema.", interface_cls.__name__)

        self._interfaces[interface_cls] = interface_cls

    def deregister(self, typename: str) -> None:
        """Deregister a type by its name to avoid name conflicts in subsequent tests."""
        types_to_remove = [key for key, value in self._types.items() if value.__name__ == typename]
        for key in types_to_remove:
            del self._types[key]
            logger.debug("Deregistered type '%s' from the schema.", typename)

    def register_mutation(self, mutation_or_fn: type | Callable[..., Any]) -> None:
        """Register a mutation class or resolver function as a GraphQL mutation.

        Args:
            mutation_or_fn: The mutation class or resolver function to register.
        """
        if hasattr(mutation_or_fn, "__fraiseql_mutation__"):
            # Check if it's a simple function-based mutation
            if (
                hasattr(mutation_or_fn, "__fraiseql_resolver__")
                and mutation_or_fn.__fraiseql_resolver__ is mutation_or_fn
            ):
                # It's a function-based mutation decorated with @mutation
                self._mutations[mutation_or_fn.__name__] = mutation_or_fn
            else:
                # It's a @mutation decorated class
                # Register the resolver function
                resolver_fn = mutation_or_fn.__fraiseql_resolver__
                self._mutations[resolver_fn.__name__] = resolver_fn
                # Also register the success and error types
                definition = mutation_or_fn.__fraiseql_mutation__
                if hasattr(definition, "success_type") and definition.success_type:
                    self.register_type(definition.success_type)
                if hasattr(definition, "error_type") and definition.error_type:
                    self.register_type(definition.error_type)
        else:
            # Legacy: direct resolver function
            self._mutations[mutation_or_fn.__name__] = mutation_or_fn

    def register_query(self, query_fn: Callable[..., Any]) -> None:
        """Register a query function as a GraphQL field."""
        self._queries[query_fn.__name__] = query_fn

    def register_subscription(self, subscription_fn: Callable[..., Any]) -> None:
        """Register a subscription function as a GraphQL field."""
        self._subscriptions[subscription_fn.__name__] = subscription_fn

    @property
    def types(self) -> dict[type, type]:
        """Get registered types."""
        return self._types

    @property
    def mutations(self) -> dict[str, Callable[..., Any]]:
        """Get registered mutations."""
        return self._mutations

    @property
    def queries(self) -> dict[str, Callable[..., Any]]:
        """Get registered queries."""
        return self._queries

    @property
    def subscriptions(self) -> dict[str, Callable[..., Any]]:
        """Get registered subscriptions."""
        return self._subscriptions

    @property
    def enums(self) -> dict[type, GraphQLEnumType]:
        """Get registered enums."""
        return self._enums

    @property
    def interfaces(self) -> dict[type, type]:
        """Get registered interfaces."""
        return self._interfaces
