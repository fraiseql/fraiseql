"""Registry system for FraiseQL Axum server.

Provides centralized registration for GraphQL items (types, mutations, queries,
subscriptions). Supports both manual registration and auto-registration via
decorators.
"""

import logging
import threading
from typing import Any, Callable

logger = logging.getLogger(__name__)


class AxumRegistry:
    """Centralized registry for GraphQL items in Axum server.

    This is a singleton class that maintains separate registries for:
    - Types (via @fraiseql.type)
    - Inputs (via @fraiseql.input)
    - Enums (via @fraiseql.enum)
    - Interfaces (via @fraiseql.interface)
    - Mutations (via @fraiseql.mutation)
    - Queries (via @fraiseql.query)
    - Subscriptions (via @fraiseql.subscription)

    The registry supports both:
    1. **Manual registration**: Explicitly call register_* methods
    2. **Auto-registration**: Decorators register automatically (Phase D.3)

    Thread-safe: Uses singleton pattern with locking for initialization.

    Examples:
        Get singleton instance:
        ```python
        registry = AxumRegistry.get_instance()
        ```

        Register types manually:
        ```python
        from fraiseql import type as fraiseql_type

        @fraiseql_type
        class User:
            id: ID
            name: str

        registry = AxumRegistry.get_instance()
        registry.register_type(User)
        ```

        Query registry:
        ```python
        types = registry.get_registered_types()
        mutations = registry.get_registered_mutations()
        ```

        Test isolation:
        ```python
        # In pytest fixture
        AxumRegistry.get_instance().clear()
        ```
    """

    _instance: "AxumRegistry | None" = None
    _lock = threading.Lock()

    def __new__(cls) -> "AxumRegistry":
        """Ensure singleton pattern with thread safety."""
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._initialized = False
        return cls._instance

    def __init__(self) -> None:
        """Initialize registry storage (only once)."""
        if self._initialized:
            return

        # Type registries - store by name for fast lookup
        self._types: dict[str, type[Any]] = {}
        self._inputs: dict[str, type[Any]] = {}
        self._enums: dict[str, type[Any]] = {}
        self._interfaces: dict[str, type[Any]] = {}

        # Query/mutation/subscription registries - store by name
        self._mutations: dict[str, type[Any]] = {}
        self._queries: dict[str, Callable[..., Any]] = {}
        self._subscriptions: dict[str, Callable[..., Any]] = {}

        self._initialized = True
        logger.debug("AxumRegistry initialized")

    @classmethod
    def get_instance(cls) -> "AxumRegistry":
        """Get the singleton registry instance.

        Returns:
            The global AxumRegistry instance.

        Examples:
            ```python
            registry = AxumRegistry.get_instance()
            ```
        """
        if cls._instance is None:
            cls()  # Calls __new__ and __init__
        return cls._instance

    # ===== Type Registration =====

    def register_type(self, type_: type[Any]) -> None:
        """Register a GraphQL type.

        Args:
            type_: A @fraiseql.type decorated class

        Examples:
            ```python
            from fraiseql import type as fraiseql_type

            @fraiseql_type
            class User:
                id: ID
                name: str

            registry = AxumRegistry.get_instance()
            registry.register_type(User)
            ```
        """
        type_name = getattr(type_, "__name__", str(type_))
        self._types[type_name] = type_
        logger.debug(f"Registered type: {type_name}")

    def register_input(self, input_: type[Any]) -> None:
        """Register a GraphQL input type.

        Args:
            input_: A @fraiseql.input decorated class

        Examples:
            ```python
            from fraiseql import input as fraiseql_input

            @fraiseql_input
            class CreateUserInput:
                name: str
                email: str

            registry = AxumRegistry.get_instance()
            registry.register_input(CreateUserInput)
            ```
        """
        input_name = getattr(input_, "__name__", str(input_))
        self._inputs[input_name] = input_
        logger.debug(f"Registered input: {input_name}")

    def register_enum(self, enum_: type[Any]) -> None:
        """Register a GraphQL enum type.

        Args:
            enum_: A @fraiseql.enum decorated class

        Examples:
            ```python
            from fraiseql import enum as fraiseql_enum

            @fraiseql_enum
            class UserRole:
                ADMIN = "admin"
                USER = "user"

            registry = AxumRegistry.get_instance()
            registry.register_enum(UserRole)
            ```
        """
        enum_name = getattr(enum_, "__name__", str(enum_))
        self._enums[enum_name] = enum_
        logger.debug(f"Registered enum: {enum_name}")

    def register_interface(self, interface_: type[Any]) -> None:
        """Register a GraphQL interface type.

        Args:
            interface_: A @fraiseql.interface decorated class

        Examples:
            ```python
            from fraiseql import interface as fraiseql_interface

            @fraiseql_interface
            class Node:
                id: ID

            registry = AxumRegistry.get_instance()
            registry.register_interface(Node)
            ```
        """
        interface_name = getattr(interface_, "__name__", str(interface_))
        self._interfaces[interface_name] = interface_
        logger.debug(f"Registered interface: {interface_name}")

    # ===== Query/Mutation/Subscription Registration =====

    def register_query(self, query_: Callable[..., Any]) -> None:
        """Register a GraphQL query.

        Args:
            query_: A @fraiseql.query decorated function

        Examples:
            ```python
            from fraiseql import query

            @query
            async def get_users() -> list[User]:
                ...

            registry = AxumRegistry.get_instance()
            registry.register_query(get_users)
            ```
        """
        query_name = getattr(query_, "__name__", str(query_))
        self._queries[query_name] = query_
        logger.debug(f"Registered query: {query_name}")

    def register_mutation(self, mutation_: Callable[..., Any] | type[Any]) -> None:
        """Register a GraphQL mutation.

        Args:
            mutation_: A @fraiseql.mutation decorated function or class

        Examples:
            ```python
            from fraiseql import mutation

            @mutation
            async def create_user(input: CreateUserInput) -> User:
                ...

            registry = AxumRegistry.get_instance()
            registry.register_mutation(create_user)
            ```
        """
        mutation_name = getattr(mutation_, "__name__", str(mutation_))
        self._mutations[mutation_name] = mutation_
        logger.debug(f"Registered mutation: {mutation_name}")

    def register_subscription(self, subscription_: Callable[..., Any]) -> None:
        """Register a GraphQL subscription.

        Args:
            subscription_: A @fraiseql.subscription decorated function

        Examples:
            ```python
            from fraiseql import subscription

            @subscription
            async def on_user_created() -> User:
                ...

            registry = AxumRegistry.get_instance()
            registry.register_subscription(on_user_created)
            ```
        """
        sub_name = getattr(subscription_, "__name__", str(subscription_))
        self._subscriptions[sub_name] = subscription_
        logger.debug(f"Registered subscription: {sub_name}")

    # ===== Batch Registration =====

    def register_types(self, types: list[type[Any]]) -> None:
        """Register multiple GraphQL types at once.

        Args:
            types: List of @fraiseql.type decorated classes

        Examples:
            ```python
            registry = AxumRegistry.get_instance()
            registry.register_types([User, Post, Comment])
            ```
        """
        for type_ in types:
            self.register_type(type_)

    def register_mutations(self, mutations: list[Callable[..., Any] | type[Any]]) -> None:
        """Register multiple mutations at once.

        Args:
            mutations: List of @fraiseql.mutation decorated items

        Examples:
            ```python
            registry = AxumRegistry.get_instance()
            registry.register_mutations([create_user, update_user, delete_user])
            ```
        """
        for mutation_ in mutations:
            self.register_mutation(mutation_)

    def register_queries(self, queries: list[Callable[..., Any]]) -> None:
        """Register multiple queries at once.

        Args:
            queries: List of @fraiseql.query decorated functions

        Examples:
            ```python
            registry = AxumRegistry.get_instance()
            registry.register_queries([get_users, get_posts])
            ```
        """
        for query_ in queries:
            self.register_query(query_)

    def register_subscriptions(self, subscriptions: list[Callable[..., Any]]) -> None:
        """Register multiple subscriptions at once.

        Args:
            subscriptions: List of @fraiseql.subscription decorated functions

        Examples:
            ```python
            registry = AxumRegistry.get_instance()
            registry.register_subscriptions([on_user_created, on_post_updated])
            ```
        """
        for subscription_ in subscriptions:
            self.register_subscription(subscription_)

    # ===== Introspection =====

    def get_registered_types(self) -> dict[str, type[Any]]:
        """Get all registered GraphQL types.

        Returns:
            Dictionary mapping type names to type classes

        Examples:
            ```python
            registry = AxumRegistry.get_instance()
            types = registry.get_registered_types()
            for name, type_class in types.items():
                print(f"Type: {name}")
            ```
        """
        return dict(self._types)

    def get_registered_inputs(self) -> dict[str, type[Any]]:
        """Get all registered GraphQL input types.

        Returns:
            Dictionary mapping input names to input classes
        """
        return dict(self._inputs)

    def get_registered_enums(self) -> dict[str, type[Any]]:
        """Get all registered GraphQL enums.

        Returns:
            Dictionary mapping enum names to enum classes
        """
        return dict(self._enums)

    def get_registered_interfaces(self) -> dict[str, type[Any]]:
        """Get all registered GraphQL interfaces.

        Returns:
            Dictionary mapping interface names to interface classes
        """
        return dict(self._interfaces)

    def get_registered_mutations(self) -> dict[str, Callable[..., Any] | type[Any]]:
        """Get all registered mutations.

        Returns:
            Dictionary mapping mutation names to mutation callables/classes

        Examples:
            ```python
            registry = AxumRegistry.get_instance()
            mutations = registry.get_registered_mutations()
            for name, mutation_fn in mutations.items():
                print(f"Mutation: {name}")
            ```
        """
        return dict(self._mutations)

    def get_registered_queries(self) -> dict[str, Callable[..., Any]]:
        """Get all registered queries.

        Returns:
            Dictionary mapping query names to query functions

        Examples:
            ```python
            registry = AxumRegistry.get_instance()
            queries = registry.get_registered_queries()
            for name, query_fn in queries.items():
                print(f"Query: {name}")
            ```
        """
        return dict(self._queries)

    def get_registered_subscriptions(self) -> dict[str, Callable[..., Any]]:
        """Get all registered subscriptions.

        Returns:
            Dictionary mapping subscription names to subscription functions
        """
        return dict(self._subscriptions)

    # ===== Utility Methods =====

    def count_registered(self) -> dict[str, int]:
        """Get count of all registered items.

        Returns:
            Dictionary with counts for each item type

        Examples:
            ```python
            registry = AxumRegistry.get_instance()
            counts = registry.count_registered()
            print(f"Types: {counts['types']}, Mutations: {counts['mutations']}")
            ```
        """
        return {
            "types": len(self._types),
            "inputs": len(self._inputs),
            "enums": len(self._enums),
            "interfaces": len(self._interfaces),
            "mutations": len(self._mutations),
            "queries": len(self._queries),
            "subscriptions": len(self._subscriptions),
            "total": (
                len(self._types)
                + len(self._inputs)
                + len(self._enums)
                + len(self._interfaces)
                + len(self._mutations)
                + len(self._queries)
                + len(self._subscriptions)
            ),
        }

    def summary(self) -> str:
        """Get a human-readable summary of registered items.

        Returns:
            Formatted summary string

        Examples:
            ```python
            registry = AxumRegistry.get_instance()
            print(registry.summary())
            # Output:
            # AxumRegistry Summary:
            #   Types: 5 (User, Post, Comment, Author, Tag)
            #   Inputs: 3 (CreateUserInput, UpdateUserInput, FilterInput)
            #   Mutations: 4 (createUser, updateUser, deleteUser, publishPost)
            #   Queries: 3 (getUsers, getPosts, getComments)
            #   Subscriptions: 2 (onUserCreated, onPostPublished)
            ```
        """
        counts = self.count_registered()
        summary_lines = ["AxumRegistry Summary:"]

        if self._types:
            type_names = ", ".join(self._types.keys())
            summary_lines.append(f"  Types: {counts['types']} ({type_names})")

        if self._inputs:
            input_names = ", ".join(self._inputs.keys())
            summary_lines.append(f"  Inputs: {counts['inputs']} ({input_names})")

        if self._enums:
            enum_names = ", ".join(self._enums.keys())
            summary_lines.append(f"  Enums: {counts['enums']} ({enum_names})")

        if self._interfaces:
            iface_names = ", ".join(self._interfaces.keys())
            summary_lines.append(f"  Interfaces: {counts['interfaces']} ({iface_names})")

        if self._mutations:
            mut_names = ", ".join(self._mutations.keys())
            summary_lines.append(f"  Mutations: {counts['mutations']} ({mut_names})")

        if self._queries:
            query_names = ", ".join(self._queries.keys())
            summary_lines.append(f"  Queries: {counts['queries']} ({query_names})")

        if self._subscriptions:
            sub_names = ", ".join(self._subscriptions.keys())
            summary_lines.append(f"  Subscriptions: {counts['subscriptions']} ({sub_names})")

        if counts["total"] == 0:
            summary_lines.append("  (empty)")

        return "\n".join(summary_lines)

    def clear(self) -> None:
        """Clear all registered items.

        This is primarily used for test isolation. Call in test fixtures
        to reset the registry between tests.

        Examples:
            ```python
            import pytest
            from fraiseql.axum.registry import AxumRegistry

            @pytest.fixture(autouse=True)
            def clear_registry():
                AxumRegistry.get_instance().clear()
                yield
                AxumRegistry.get_instance().clear()

            def test_registry():
                registry = AxumRegistry.get_instance()
                assert registry.count_registered()['total'] == 0
            ```
        """
        self._types.clear()
        self._inputs.clear()
        self._enums.clear()
        self._interfaces.clear()
        self._mutations.clear()
        self._queries.clear()
        self._subscriptions.clear()
        logger.debug("AxumRegistry cleared")

    def to_lists(
        self,
    ) -> tuple[
        list[type[Any]],
        list[type[Any]],
        list[Callable[..., Any]],
        list[Callable[..., Any]],
    ]:
        """Convert registry to lists for AxumServer registration.

        This method enables backward compatibility with AxumServer's
        list-based registration API.

        Returns:
            Tuple of (types, mutations, queries, subscriptions) as lists

        Examples:
            ```python
            registry = AxumRegistry.get_instance()
            types, mutations, queries, subscriptions = registry.to_lists()

            # Pass to AxumServer
            server.register_types(types)
            server.register_mutations(mutations)
            server.register_queries(queries)
            server.register_subscriptions(subscriptions)
            ```
        """
        return (
            list(self._types.values()),
            list(self._mutations.values()),
            list(self._queries.values()),
            list(self._subscriptions.values()),
        )
