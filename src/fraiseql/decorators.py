"""Additional decorators for FraiseQL."""

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
) -> F | Callable[[F], F]:
    """Decorator to mark a method as a GraphQL field resolver.
    
    This is used with the QueryRoot pattern to define field resolvers.
    
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
        return func
    
    if method is None:
        return decorator
    return decorator(method)