"""Subscription decorator for GraphQL subscriptions."""

import inspect
from collections.abc import AsyncGenerator, Callable
from functools import wraps
from typing import Any, TypeVar

from fraiseql.core.types import SubscriptionField

F = TypeVar("F", bound=Callable[..., Any])


def subscription(fn: F) -> F:
    """Decorator to mark a function as a GraphQL subscription.

    Example:
        @subscription
        async def task_updates(info, project_id: UUID) -> AsyncGenerator[Task, None]:
            async for task in watch_project_tasks(project_id):
                yield task
    """
    if not inspect.isasyncgenfunction(fn):
        msg = (
            f"Subscription {fn.__name__} must be an async generator function "
            f"(use 'async def' and 'yield')"
        )
        raise TypeError(
            msg,
        )

    # Extract type hints
    hints = inspect.get_annotations(fn)
    return_type = hints.get("return", Any)

    # Parse AsyncGenerator type
    if hasattr(return_type, "__origin__") and return_type.__origin__ is AsyncGenerator:
        yield_type = return_type.__args__[0] if return_type.__args__ else Any
    else:
        # Try to infer from first yield
        yield_type = Any

    # Create subscription field
    field = SubscriptionField(
        name=fn.__name__,
        resolver=fn,
        return_type=yield_type,
        args=hints,
        description=fn.__doc__,
    )

    # Register with schema builder
    from fraiseql.gql.schema_builder import SchemaRegistry

    schema_registry = SchemaRegistry.get_instance()
    schema_registry.register_subscription(fn)

    # Add metadata
    fn.__fraiseql_subscription__ = True
    fn._field_def = field

    return fn


def websocket_auth(
    required: bool = True,
    roles: list[str] | None = None,
    permissions: list[str] | None = None,
) -> Callable[[F], F]:
    """Decorator to add authentication requirements to subscriptions.

    Can be used standalone or combined with @subscription decorator.
    Auto-inherits authentication configuration from FraiseQLConfig if not explicitly set.

    Args:
        required: Whether authentication is required (default: True)
        roles: List of required roles for access (optional)
        permissions: List of required permissions for access (optional)

    Returns:
        Decorated function with auth metadata

    Examples:
        # Require authentication
        @subscription
        @websocket_auth(required=True)
        async def protected_stream(info):
            yield data

        # Require specific roles
        @subscription
        @websocket_auth(required=True, roles=["admin"])
        async def admin_stream(info):
            yield admin_data

        # Standalone (can be applied before @subscription)
        @websocket_auth(required=True, roles=["user"])
        @subscription
        async def user_stream(info):
            yield user_data
    """

    def decorator(fn: F) -> F:
        """Apply auth requirements to function."""
        # Store auth requirements as function attributes
        fn._websocket_auth_required = required  # type: ignore[attr-defined]
        fn._websocket_auth_roles = roles or []  # type: ignore[attr-defined]
        fn._websocket_auth_permissions = permissions or []  # type: ignore[attr-defined]

        # Add a wrapper that validates auth at execution time
        @wraps(fn)
        async def wrapper(*args: Any, **kwargs: Any) -> Any:
            """Wrapper that enforces auth before calling subscription."""
            # The actual auth enforcement happens in the subscription executor
            # This wrapper just marks that auth was applied
            return await fn(*args, **kwargs)

        # Preserve auth metadata on wrapper
        wrapper._websocket_auth_required = required  # type: ignore[attr-defined]
        wrapper._websocket_auth_roles = roles or []  # type: ignore[attr-defined]
        wrapper._websocket_auth_permissions = permissions or []  # type: ignore[attr-defined]

        # If function already has subscription metadata, preserve it
        if hasattr(fn, "__fraiseql_subscription__"):
            wrapper.__fraiseql_subscription__ = fn.__fraiseql_subscription__  # type: ignore[attr-defined]

        if hasattr(fn, "_field_def"):
            wrapper._field_def = fn._field_def  # type: ignore[attr-defined]

        return wrapper  # type: ignore[return-value]

    return decorator
