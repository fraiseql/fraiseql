"""Additional decorators for FraiseQL."""

import asyncio
import time
from collections.abc import Callable
from typing import Any, TypeVar, overload

from fraiseql.gql.schema_builder import SchemaRegistry

F = TypeVar("F", bound=Callable[..., Any])


@overload
def query(fn: F) -> F: ...


@overload
def query() -> Callable[[F], F]: ...


def query(fn: F | None = None) -> F | Callable[[F], F]:
    """Decorator to mark a function as a GraphQL query.

    This is a convenience decorator that registers the function with the schema.
    It's equivalent to passing the function in the queries list to create_fraiseql_app.

    Usage:
        @fraiseql.query
        async def get_user(info, id: UUID) -> User:
            db = info.context["db"]
            return await db.get_user(id)

        # Now you can just pass types, not queries
        app = create_fraiseql_app(
            types=[User, Post],
            # queries=[get_user] - no longer needed!
        )
    """

    def decorator(func: F) -> F:
        # Register with schema
        registry = SchemaRegistry.get_instance()
        registry.register_query(func)
        return func

    if fn is None:
        return decorator
    return decorator(fn)


@overload
def subscription(fn: F) -> F: ...


@overload
def subscription() -> Callable[[F], F]: ...


def subscription(fn: F | None = None) -> F | Callable[[F], F]:
    """Decorator to mark a function as a GraphQL subscription.

    This is a convenience decorator that registers the function with the schema.

    Usage:
        @fraiseql.subscription
        async def on_post_created(info) -> AsyncGenerator[Post, None]:
            # Subscribe to post creation events
            async for post in post_events():
                yield post
    """

    def decorator(func: F) -> F:
        # Register with schema
        registry = SchemaRegistry.get_instance()
        registry.register_subscription(func)
        return func

    if fn is None:
        return decorator
    return decorator(fn)


@overload
def field(
    method: F,
    *,
    resolver: Callable[..., Any] | None = None,
    description: str | None = None,
) -> F: ...


@overload
def field(
    *,
    resolver: Callable[..., Any] | None = None,
    description: str | None = None,
) -> Callable[[F], F]: ...


def field(
    method: F | None = None,
    *,
    resolver: Callable[..., Any] | None = None,
    description: str | None = None,
) -> F | Callable[[F], F]:
    """Decorator to mark a method as a GraphQL field with optional resolver.

    This decorator should be applied to methods of @fraise_type decorated classes.
    It allows defining custom field resolvers and adding field descriptions.

    Args:
        method: The method to decorate (when used without parentheses)
        resolver: Optional custom resolver function
        description: Field description for GraphQL schema

    Returns:
        Decorated method with field metadata

    Example:
        @fraise_type
        class User:
            name: str

            @field(description="User's full display name")
            def display_name(self) -> str:
                return f"User: {self.name}"

            @field(resolver=fetch_posts_for_user)
            async def posts(self) -> list[Post]:
                # This will use fetch_posts_for_user as resolver
                pass
    """

    def decorator(func: F) -> F:
        # Determine if the function is async
        is_async = asyncio.iscoroutinefunction(func)

        if is_async:

            async def async_wrapped_resolver(root, info, *args, **kwargs):
                # Check if N+1 detector is available in context
                detector = None
                if info and hasattr(info, "context") and info.context:
                    detector = getattr(info.context, "get", lambda x: None)("n1_detector")
                if detector and detector.enabled:
                    start_time = time.time()
                    try:
                        # Call the original method - if it's a bound method, use root as self
                        if hasattr(func, "__self__"):
                            result = await func(info, *args, **kwargs)
                        else:
                            result = await func(root, info, *args, **kwargs)
                        execution_time = time.time() - start_time
                        # Track field resolution without blocking
                        # Using create_task is safe here as detector manages its own lifecycle
                        task = asyncio.create_task(
                            detector.track_field_resolution(info, info.field_name, execution_time),
                        )
                        # Add error handler to prevent unhandled exceptions
                        task.add_done_callback(lambda t: t.exception() if t.done() else None)
                        return result  # noqa: TRY300
                    except Exception:
                        execution_time = time.time() - start_time
                        task = asyncio.create_task(
                            detector.track_field_resolution(info, info.field_name, execution_time),
                        )
                        task.add_done_callback(lambda t: t.exception() if t.done() else None)
                        raise
                # Call the original method - if it's a bound method, use root as self
                elif hasattr(func, "__self__"):
                    return await func(info, *args, **kwargs)
                else:
                    return await func(root, info, *args, **kwargs)

            wrapped_func = async_wrapped_resolver

        else:

            def sync_wrapped_resolver(root, info, *args, **kwargs):
                # Check if N+1 detector is available in context
                detector = None
                if info and hasattr(info, "context") and info.context:
                    detector = getattr(info.context, "get", lambda x: None)("n1_detector")
                if detector and detector.enabled:
                    start_time = time.time()
                    try:
                        # Call the original method - if it's a bound method, use root as self
                        if hasattr(func, "__self__"):
                            result = func(info, *args, **kwargs)
                        else:
                            result = func(root, info, *args, **kwargs)
                        execution_time = time.time() - start_time
                        # Track field resolution without blocking
                        # Using create_task is safe here as detector manages its own lifecycle
                        task = asyncio.create_task(
                            detector.track_field_resolution(info, info.field_name, execution_time),
                        )
                        # Add error handler to prevent unhandled exceptions
                        task.add_done_callback(lambda t: t.exception() if t.done() else None)
                        return result  # noqa: TRY300
                    except Exception:
                        execution_time = time.time() - start_time
                        task = asyncio.create_task(
                            detector.track_field_resolution(info, info.field_name, execution_time),
                        )
                        task.add_done_callback(lambda t: t.exception() if t.done() else None)
                        raise
                # Call the original method - if it's a bound method, use root as self
                elif hasattr(func, "__self__"):
                    return func(info, *args, **kwargs)
                else:
                    return func(root, info, *args, **kwargs)

            wrapped_func = sync_wrapped_resolver

        # Copy over the metadata
        wrapped_func.__fraiseql_field__ = True
        wrapped_func.__fraiseql_field_resolver__ = resolver or wrapped_func
        wrapped_func.__fraiseql_field_description__ = description
        wrapped_func.__name__ = func.__name__
        wrapped_func.__doc__ = func.__doc__

        # Store the original function for field authorization
        wrapped_func.__fraiseql_original_func__ = func

        # Copy type annotations
        if hasattr(func, "__annotations__"):
            wrapped_func.__annotations__ = func.__annotations__.copy()

        return wrapped_func  # type: ignore[return-value]

    if method is None:
        return decorator
    return decorator(method)
