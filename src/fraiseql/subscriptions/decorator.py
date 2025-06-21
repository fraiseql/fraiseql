"""Subscription decorator for GraphQL subscriptions."""

import inspect
from collections.abc import AsyncGenerator, Callable
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
    fn._is_subscription = True
    fn._field_def = field

    return fn
