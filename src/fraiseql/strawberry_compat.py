"""Strawberry GraphQL compatibility layer.

This module provides compatibility imports and adapters to ease migration
from Strawberry GraphQL to FraiseQL.
"""

from typing import Any, Callable, TypeVar

import fraiseql

# Type variable for decorators
F = TypeVar("F", bound=Callable[..., Any])


class StrawberryCompatibility:
    """Compatibility layer that mimics Strawberry's API using FraiseQL."""
    
    @staticmethod
    def type(cls: type = None, **kwargs) -> Any:
        """Strawberry @strawberry.type compatibility."""
        if cls is None:
            # Called with arguments: @strawberry.type(name="CustomName")
            def decorator(cls: type) -> type:
                return fraiseql.type(cls)
            return decorator
        else:
            # Called without arguments: @strawberry.type
            return fraiseql.type(cls)
    
    @staticmethod
    def input(cls: type = None, **kwargs) -> Any:
        """Strawberry @strawberry.input compatibility."""
        if cls is None:
            def decorator(cls: type) -> type:
                return fraiseql.input(cls)
            return decorator
        else:
            return fraiseql.input(cls)
    
    @staticmethod
    def enum(cls: type = None, **kwargs) -> Any:
        """Strawberry @strawberry.enum compatibility."""
        if cls is None:
            def decorator(cls: type) -> type:
                return fraiseql.enum(cls)
            return decorator
        else:
            return fraiseql.enum(cls)
    
    @staticmethod
    def interface(cls: type = None, **kwargs) -> Any:
        """Strawberry @strawberry.interface compatibility."""
        if cls is None:
            def decorator(cls: type) -> type:
                return fraiseql.interface(cls)
            return decorator
        else:
            return fraiseql.interface(cls)
    
    @staticmethod
    def field(
        fn: F = None,
        *,
        resolver: Callable[..., Any] = None,
        description: str = None,
        **kwargs
    ) -> Any:
        """Strawberry @strawberry.field compatibility."""
        if fn is None:
            def decorator(fn: F) -> F:
                return fraiseql.field(
                    fn,
                    resolver=resolver,
                    description=description
                )
            return decorator
        else:
            return fraiseql.field(
                fn,
                resolver=resolver,
                description=description
            )
    
    @staticmethod
    def mutation(fn: F = None, **kwargs) -> Any:
        """Strawberry @strawberry.mutation compatibility."""
        if fn is None:
            def decorator(fn: F) -> F:
                return fraiseql.mutation(fn)
            return decorator
        else:
            return fraiseql.mutation(fn)
    
    @staticmethod
    def query(fn: F = None, **kwargs) -> Any:
        """Strawberry @strawberry.query compatibility."""
        if fn is None:
            def decorator(fn: F) -> F:
                return fraiseql.query(fn)
            return decorator
        else:
            return fraiseql.query(fn)
    
    @staticmethod
    def subscription(fn: F = None, **kwargs) -> Any:
        """Strawberry @strawberry.subscription compatibility."""
        if fn is None:
            def decorator(fn: F) -> F:
                return fraiseql.subscription(fn)
            return decorator
        else:
            return fraiseql.subscription(fn)


# Create a strawberry-like module interface
strawberry = StrawberryCompatibility()

# For more direct compatibility, also expose individual functions
type = strawberry.type
input = strawberry.input
enum = strawberry.enum
interface = strawberry.interface
field = strawberry.field
mutation = strawberry.mutation
query = strawberry.query
subscription = strawberry.subscription