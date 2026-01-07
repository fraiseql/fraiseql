"""Registration hooks for auto-registering decorators with AxumRegistry.

This module provides safe registration hooks that decorators can use to
automatically register with AxumRegistry when auto_register=True.

Hooks are designed to be called during decorator execution and safely handle
the case where AxumRegistry is not available (e.g., when Axum is not installed).

Examples:
    In a decorator (e.g., @fraiseql.type):
    ```python
    from fraiseql.axum.registration_hooks import register_type_hook

    def fraise_type(_cls=None, *, auto_register=True, ...):
        def wrapper(cls):
            # ... existing decorator logic ...

            # Register to AxumRegistry if Axum is available and enabled
            if auto_register:
                register_type_hook(cls)

            return cls

        return wrapper(_cls) if _cls else wrapper
    ```
"""

import logging
from typing import Any, Callable

logger = logging.getLogger(__name__)


def _get_registry_safe() -> "Any | None":
    """Safely import and get AxumRegistry instance.

    Returns None if AxumRegistry is not available (e.g., Axum not installed).

    Returns:
        AxumRegistry instance or None
    """
    try:
        from fraiseql.axum.registry import AxumRegistry

        return AxumRegistry.get_instance()
    except ImportError:
        # AxumRegistry not available (Axum not installed)
        return None
    except Exception as e:
        logger.debug(f"Failed to get AxumRegistry: {e}")
        return None


def register_type_hook(type_: type[Any]) -> None:
    """Register a type with AxumRegistry (if available).

    This is called by @fraiseql.type decorator when auto_register=True.

    Args:
        type_: The GraphQL type to register

    Examples:
        ```python
        from fraiseql.axum.registration_hooks import register_type_hook

        @fraiseql.type
        class User:
            id: ID
            name: str

        # Called automatically by decorator, but can also be called manually
        register_type_hook(User)
        ```
    """
    if type_ is None:
        return

    registry = _get_registry_safe()
    if registry is None:
        return

    try:
        registry.register_type(type_)
        logger.debug(f"Registered type {type_.__name__} to AxumRegistry")
    except Exception as e:
        type_name = getattr(type_, "__name__", str(type_))
        logger.warning(f"Failed to register type {type_name}: {e}")


def register_input_hook(input_: type[Any]) -> None:
    """Register an input type with AxumRegistry (if available).

    This is called by @fraiseql.input decorator when auto_register=True.

    Args:
        input_: The GraphQL input type to register

    Examples:
        ```python
        from fraiseql.axum.registration_hooks import register_input_hook

        @fraiseql.input
        class CreateUserInput:
            name: str
            email: str

        # Called automatically by decorator
        register_input_hook(CreateUserInput)
        ```
    """
    registry = _get_registry_safe()
    if registry is None:
        return

    try:
        registry.register_input(input_)
        logger.debug(f"Registered input {input_.__name__} to AxumRegistry")
    except Exception as e:
        logger.warning(f"Failed to register input {input_.__name__}: {e}")


def register_enum_hook(enum_: type[Any]) -> None:
    """Register an enum type with AxumRegistry (if available).

    This is called by @fraiseql.enum decorator when auto_register=True.

    Args:
        enum_: The GraphQL enum to register

    Examples:
        ```python
        from fraiseql.axum.registration_hooks import register_enum_hook

        @fraiseql.enum
        class UserRole:
            ADMIN = "admin"
            USER = "user"

        # Called automatically by decorator
        register_enum_hook(UserRole)
        ```
    """
    registry = _get_registry_safe()
    if registry is None:
        return

    try:
        registry.register_enum(enum_)
        logger.debug(f"Registered enum {enum_.__name__} to AxumRegistry")
    except Exception as e:
        logger.warning(f"Failed to register enum {enum_.__name__}: {e}")


def register_interface_hook(interface_: type[Any]) -> None:
    """Register an interface with AxumRegistry (if available).

    This is called by @fraiseql.interface decorator when auto_register=True.

    Args:
        interface_: The GraphQL interface to register

    Examples:
        ```python
        from fraiseql.axum.registration_hooks import register_interface_hook

        @fraiseql.interface
        class Node:
            id: ID

        # Called automatically by decorator
        register_interface_hook(Node)
        ```
    """
    registry = _get_registry_safe()
    if registry is None:
        return

    try:
        registry.register_interface(interface_)
        logger.debug(f"Registered interface {interface_.__name__} to AxumRegistry")
    except Exception as e:
        logger.warning(f"Failed to register interface {interface_.__name__}: {e}")


def register_query_hook(query_: Callable[..., Any]) -> None:
    """Register a query with AxumRegistry (if available).

    This is called by @fraiseql.query decorator when auto_register=True.

    Args:
        query_: The GraphQL query function to register

    Examples:
        ```python
        from fraiseql.axum.registration_hooks import register_query_hook

        @fraiseql.query
        async def get_users() -> list[User]:
            ...

        # Called automatically by decorator
        register_query_hook(get_users)
        ```
    """
    registry = _get_registry_safe()
    if registry is None:
        return

    try:
        registry.register_query(query_)
        logger.debug(f"Registered query {query_.__name__} to AxumRegistry")
    except Exception as e:
        logger.warning(f"Failed to register query {query_.__name__}: {e}")


def register_mutation_hook(mutation_: Callable[..., Any] | type[Any]) -> None:
    """Register a mutation with AxumRegistry (if available).

    This is called by @fraiseql.mutation decorator when auto_register=True.

    Args:
        mutation_: The GraphQL mutation function or class to register

    Examples:
        ```python
        from fraiseql.axum.registration_hooks import register_mutation_hook

        @fraiseql.mutation
        async def create_user(input: CreateUserInput) -> User:
            ...

        # Called automatically by decorator
        register_mutation_hook(create_user)
        ```
    """
    registry = _get_registry_safe()
    if registry is None:
        return

    try:
        registry.register_mutation(mutation_)
        logger.debug(f"Registered mutation {mutation_.__name__} to AxumRegistry")
    except Exception as e:
        logger.warning(f"Failed to register mutation {mutation_.__name__}: {e}")


def register_subscription_hook(subscription_: Callable[..., Any]) -> None:
    """Register a subscription with AxumRegistry (if available).

    This is called by @fraiseql.subscription decorator when auto_register=True.

    Args:
        subscription_: The GraphQL subscription function to register

    Examples:
        ```python
        from fraiseql.axum.registration_hooks import register_subscription_hook

        @fraiseql.subscription
        async def on_user_created() -> User:
            ...

        # Called automatically by decorator
        register_subscription_hook(on_user_created)
        ```
    """
    registry = _get_registry_safe()
    if registry is None:
        return

    try:
        registry.register_subscription(subscription_)
        logger.debug(f"Registered subscription {subscription_.__name__} to AxumRegistry")
    except Exception as e:
        logger.warning(f"Failed to register subscription {subscription_.__name__}: {e}")
