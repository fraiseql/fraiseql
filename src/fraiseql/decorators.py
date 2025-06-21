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


# Field decorator for QueryRoot pattern
@overload
def field(method: F) -> F: ...


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
    track_n1: bool = True,  # Enable N+1 tracking by default
) -> F | Callable[[F], F]:
    """Decorator to mark a method as a GraphQL field resolver.

    This is used with the QueryRoot pattern to define field resolvers.
    In development mode, this decorator also tracks field resolutions
    for N+1 query detection.

    Usage:
        @fraiseql.type
        class QueryRoot:
            @fraiseql.field
            def version(self, root, info) -> str:
                return "1.0.0"

            @fraiseql.field(description="Get current user")
            async def me(self, root, info) -> User:
                return await get_current_user(info)
    """

    def decorator(func: F) -> F:
        # Store metadata on the method
        func.__fraiseql_field__ = True
        func.__fraiseql_field_resolver__ = resolver or func
        func.__fraiseql_field_description__ = description

        if not track_n1:
            return func

        # Wrap the resolver to track N+1 queries
        if asyncio.iscoroutinefunction(func):

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
                        await detector.track_field_resolution(info, info.field_name, execution_time)
                        return result  # noqa: TRY300
                    except Exception:
                        execution_time = time.time() - start_time
                        await detector.track_field_resolution(info, info.field_name, execution_time)
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
                        # Create task to track asynchronously
                        _ = asyncio.create_task(  # noqa: RUF006
                            detector.track_field_resolution(info, info.field_name, execution_time),
                        )
                        return result  # noqa: TRY300
                    except Exception:
                        execution_time = time.time() - start_time
                        _ = asyncio.create_task(  # noqa: RUF006
                            detector.track_field_resolution(info, info.field_name, execution_time),
                        )
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

        # Copy type annotations
        if hasattr(func, "__annotations__"):
            wrapped_func.__annotations__ = func.__annotations__.copy()

        return wrapped_func  # type: ignore[return-value]

    if method is None:
        return decorator
    return decorator(method)
