"""Schema registry for managing GraphQL type registrations."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

from fraiseql.config.schema_config import SchemaConfig

if TYPE_CHECKING:
    from collections.abc import Callable

    from graphql import GraphQLEnumType

    from fraiseql.gql.builders.registry_health import RegistryHealthResult

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
        self.config: Any = None  # FraiseQLConfig instance

    @classmethod
    def get_instance(cls) -> SchemaRegistry:
        """Get or create the singleton registry instance."""
        if cls._instance is None:
            logger.debug("Creating new SchemaRegistry instance")
            cls._instance = cls()
        else:
            logger.debug("Returning existing SchemaRegistry instance")
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
        self.config = None
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

        # Register type with database repository if it has sql_source
        if hasattr(typ, "__fraiseql_definition__") and typ.__fraiseql_definition__.sql_source:
            from fraiseql.db import register_type_for_view

            sql_source = typ.__fraiseql_definition__.sql_source
            logger.debug(
                "Registering type '%s' with repository for view '%s'",
                typ.__name__,
                sql_source,
            )
            register_type_for_view(sql_source, typ)

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
        """Register a query function as a GraphQL field with smart deduplication.

        This method implements smart deduplication to prevent registry corruption
        that occurs when the same function is registered multiple times through
        different paths (e.g., decorator auto-registration + explicit registration).

        Args:
            query_fn: The query function to register
        """
        name = query_fn.__name__

        # Check for duplicate registration
        if name in self._queries:
            existing_fn = self._queries[name]

            # Case 1: Exact same function instance (same memory address)
            if existing_fn is query_fn:
                logger.debug(
                    "Skipping duplicate registration of identical function '%s' from module '%s'",
                    name,
                    query_fn.__module__ if hasattr(query_fn, "__module__") else "unknown",
                )
                return

            # Case 2: Same function from same module (likely import chain duplication)
            if (
                hasattr(existing_fn, "__module__")
                and hasattr(query_fn, "__module__")
                and existing_fn.__module__ == query_fn.__module__
                and hasattr(existing_fn, "__code__")
                and hasattr(query_fn, "__code__")
                and existing_fn.__code__.co_code == query_fn.__code__.co_code
            ):
                logger.debug(
                    "Skipping duplicate registration of same function '%s' from module '%s' "
                    "(detected via code comparison)",
                    name,
                    query_fn.__module__,
                )
                return

            # Case 3: Different function with same name - legitimate override (but warn)
            prev_module = (
                existing_fn.__module__ if hasattr(existing_fn, "__module__") else "unknown"
            )
            new_module = query_fn.__module__ if hasattr(query_fn, "__module__") else "unknown"
            logger.warning(
                "Query '%s' is being overwritten with different implementation. "
                "Previous module: %s, New module: %s. "
                "This may indicate a naming conflict.",
                name,
                prev_module,
                new_module,
            )
        else:
            logger.debug(
                "Registering query '%s' from module '%s'",
                name,
                query_fn.__module__ if hasattr(query_fn, "__module__") else "unknown",
            )

        # Register the function
        self._queries[query_fn.__name__] = query_fn

        # Log current registry state
        logger.debug("Current queries in registry: %s", list(self._queries.keys()))

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

    def health_check(self) -> RegistryHealthResult:
        """Perform comprehensive health check on the registry.

        This method helps diagnose registry corruption and other issues
        that can cause the "Available views: []" error in production.

        Returns:
            RegistryHealthResult with detailed health information and diagnostics
        """
        from fraiseql.gql.builders.registry_health import RegistryHealthChecker

        checker = RegistryHealthChecker(self)
        return checker.check_health()

    def generate_diagnostic_report(self) -> str:
        """Generate a comprehensive diagnostic report for troubleshooting.

        Returns:
            A detailed string report with registry state, issues, and recommendations
        """
        from fraiseql.gql.builders.registry_health import RegistryHealthChecker

        checker = RegistryHealthChecker(self)
        return checker.generate_diagnostic_report()

    def validate_registry_integrity(self) -> None:
        """Validate registry integrity and raise detailed error if corrupted.

        This method provides much better error messages than the cryptic
        "Available views: []" error users currently see.

        Raises:
            RuntimeError: If critical registry corruption is detected
        """
        health = self.health_check()

        if health.has_critical_issues:
            # Create a detailed error message instead of cryptic "Available views: []"
            error_lines = [
                "FraiseQL Registry Corruption Detected!",
                "",
                "Registry Health Status: CRITICAL",
                f"Total registrations: {health.diagnostic_info.get('total_registrations', 0)}",
                "",
                "Critical Issues Found:",
            ]

            for i, issue in enumerate(health.issues, 1):
                error_lines.append(f"  {i}. {issue}")

            if health.warnings:
                error_lines.extend(
                    [
                        "",
                        "Additional Warnings:",
                    ]
                )
                for i, warning in enumerate(health.warnings, 1):
                    error_lines.append(f"  {i}. {warning}")

            error_lines.extend(
                [
                    "",
                    "Diagnostic Information:",
                    f"  Queries registered: {health.diagnostic_info.get('query_count', 0)}",
                    f"  Mutations registered: {health.diagnostic_info.get('mutation_count', 0)}",
                    f"  Subscriptions: {health.diagnostic_info.get('subscription_count', 0)}",
                    f"  Types registered: {health.diagnostic_info.get('type_count', 0)}",
                    "",
                    "Common Solutions:",
                    "  - Check for duplicate @fraiseql.query decorator usage",
                    "  - Check create_fraiseql_app() queries parameter for duplicates",
                    "  - Review import chains for circular or duplicate imports",
                    "  - Ensure database connection is established",
                    "",
                    "For full diagnostic report, call registry.generate_diagnostic_report()",
                ]
            )

            raise RuntimeError("\n".join(error_lines))
