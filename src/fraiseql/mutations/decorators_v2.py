"""Enhanced mutation decorators with explicit registration.

This module provides a builder pattern for mutation configuration
that reduces coupling and makes dependencies explicit.
"""

from collections.abc import Callable
from dataclasses import dataclass
from functools import wraps
from typing import TypeVar

from .registry_v2 import ResultRegistry

T = TypeVar("T")


@dataclass
class MutationMetadata:
    """Metadata for a mutation function."""

    function: Callable
    result_type: type
    error_type: type
    sql_function: str
    description: str | None = None


class MutationBuilder:
    """Builder pattern for mutation configuration.

    This builder allows explicit configuration of mutations with
    dependency injection for the registry.
    """

    def __init__(self, registry: ResultRegistry) -> None:
        """Initialize with a registry instance.

        Args:
            registry: The registry to use for type registration
        """
        self.registry = registry
        self.mutations: dict[str, MutationMetadata] = {}

    def mutation(
        self,
        *,
        result_type: type,
        error_type: type,
        sql_function: str | None = None,
        description: str | None = None,
    ):
        """Explicit mutation decorator with dependency injection.

        Args:
            result_type: The success result type
            error_type: The error result type
            sql_function: Optional SQL function name (defaults to graphql.{function_name})
            description: Optional description for documentation

        Returns:
            Decorator function
        """

        def decorator(func: Callable) -> Callable:
            # Register types with injected registry
            self.registry.register(result_type, error_type)

            # Determine SQL function name
            sql_func_name = f"graphql.{func.__name__}" if sql_function is None else sql_function

            # Store mutation metadata
            metadata = MutationMetadata(
                function=func,
                result_type=result_type,
                error_type=error_type,
                sql_function=sql_func_name,
                description=description or func.__doc__,
            )
            self.mutations[func.__name__] = metadata

            @wraps(func)
            async def wrapper(*args, **kwargs):
                return await func(*args, **kwargs)

            # Attach metadata for introspection
            wrapper._mutation_metadata = metadata
            wrapper._is_mutation = True

            return wrapper

        return decorator

    def get_mutations(self) -> dict[str, MutationMetadata]:
        """Get all registered mutations."""
        return self.mutations.copy()

    def get_mutation(self, name: str) -> MutationMetadata | None:
        """Get a specific mutation by name."""
        return self.mutations.get(name)

    def clear(self) -> None:
        """Clear all registered mutations."""
        self.mutations.clear()


def create_mutations(registry: ResultRegistry) -> MutationBuilder:
    """Factory function to create a mutation builder.

    Args:
        registry: The registry to use for type registration

    Returns:
        A new MutationBuilder instance

    Usage:
        registry = ScopedResultRegistry()
        builder = create_mutations(registry)

        @builder.mutation(
            result_type=CreateUserSuccess,
            error_type=CreateUserError,
            sql_function="users.create"
        )
        async def create_user(input: CreateUserInput, repo: Repository):
            # Implementation
            pass
    """
    return MutationBuilder(registry)


class MutationRegistry:
    """Registry for mutation functions with explicit registration."""

    def __init__(self) -> None:
        self._mutations: dict[str, MutationMetadata] = {}
        self._builders: list[MutationBuilder] = []

    def register_builder(self, builder: MutationBuilder) -> None:
        """Register a mutation builder."""
        self._builders.append(builder)
        # Merge mutations from builder
        self._mutations.update(builder.get_mutations())

    def register_mutation(self, name: str, metadata: MutationMetadata) -> None:
        """Register a single mutation."""
        self._mutations[name] = metadata

    def get_mutation(self, name: str) -> MutationMetadata | None:
        """Get a mutation by name."""
        return self._mutations.get(name)

    def get_all_mutations(self) -> dict[str, MutationMetadata]:
        """Get all registered mutations."""
        return self._mutations.copy()

    def clear(self) -> None:
        """Clear all registrations."""
        self._mutations.clear()
        self._builders.clear()
        for builder in self._builders:
            builder.clear()


# Example migration helper
def migrate_mutation_decorator(registry: ResultRegistry):
    """Helper to migrate from global decorators to explicit registration.

    Args:
        registry: The registry to use

    Returns:
        A mutation decorator function compatible with the old API

    Usage:
        # Old style (implicit global registry):
        @mutation
        async def create_user(...): ...

        # New style (explicit registry):
        registry = ScopedResultRegistry()
        mutation = migrate_mutation_decorator(registry)

        @mutation
        async def create_user(...): ...
    """
    builder = MutationBuilder(registry)

    def mutation(func: Callable) -> Callable:
        # Infer types from function signature if possible
        # This is a simplified version - real implementation would
        # need more sophisticated type inference
        return builder.mutation(
            result_type=type("DummySuccess", (), {}),
            error_type=type("DummyError", (), {}),
        )(func)

    return mutation
