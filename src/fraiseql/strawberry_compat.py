"""Strawberry GraphQL compatibility layer.

This module provides compatibility imports and adapters to ease migration
from Strawberry GraphQL to FraiseQL.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import TYPE_CHECKING, Any, TypeVar

import fraiseql

if TYPE_CHECKING:
    import builtins

# Type variable for decorators
F = TypeVar("F", bound=Callable[..., Any])


class StrawberryCompatibility:
    """Compatibility layer that mimics Strawberry's API using FraiseQL."""

    @staticmethod
    def type(cls_arg: Any = None, **kwargs) -> Any:
        """Strawberry @strawberry.type compatibility."""
        if cls_arg is None:
            # Called with arguments: @strawberry.type(name="CustomName")
            def decorator(cls: Any) -> Any:
                return fraiseql.type(cls)

            return decorator
        # Called without arguments: @strawberry.type
        return fraiseql.type(cls_arg)

    @staticmethod
    def input(cls_arg: builtins.type[Any] | None = None, **kwargs) -> Any:
        """Strawberry @strawberry.input compatibility."""
        if cls_arg is None:

            def decorator(cls: Any) -> Any:
                return fraiseql.input(cls)

            return decorator
        return fraiseql.input(cls_arg)

    @staticmethod
    def enum(cls_arg: builtins.type[Any] | None = None, **kwargs) -> Any:
        """Strawberry @strawberry.enum compatibility."""
        if cls_arg is None:

            def decorator(cls: Any) -> Any:
                return fraiseql.enum(cls)

            return decorator
        return fraiseql.enum(cls_arg)

    @staticmethod
    def interface(cls_arg: builtins.type[Any] | None = None, **kwargs) -> Any:
        """Strawberry @strawberry.interface compatibility."""
        if cls_arg is None:

            def decorator(cls: Any) -> Any:
                return fraiseql.interface(cls)

            return decorator
        return fraiseql.interface(cls_arg)

    @staticmethod
    def field(
        fn: Callable[..., Any] | None = None,
        *,
        resolver: Callable[..., Any] | None = None,
        description: str | None = None,
        **kwargs,
    ) -> Any:
        """Strawberry @strawberry.field compatibility."""
        if fn is None:

            def decorator(fn: Callable[..., Any]) -> Callable[..., Any]:
                return fraiseql.field(fn, resolver=resolver, description=description)

            return decorator
        return fraiseql.field(fn, resolver=resolver, description=description)

    @staticmethod
    def mutation(fn: Callable[..., Any] | None = None, **kwargs) -> Any:
        """Strawberry @strawberry.mutation compatibility."""
        if fn is None:

            def decorator(fn: Callable[..., Any]) -> Callable[..., Any]:
                return fraiseql.mutation(fn)

            return decorator
        return fraiseql.mutation(fn)

    @staticmethod
    def query(fn: Callable[..., Any] | None = None, **kwargs) -> Any:
        """Strawberry @strawberry.query compatibility."""
        if fn is None:

            def decorator(fn: Callable[..., Any]) -> Callable[..., Any]:
                return fraiseql.query(fn)

            return decorator
        return fraiseql.query(fn)

    @staticmethod
    def subscription(fn: Callable[..., Any] | None = None, **kwargs) -> Any:
        """Strawberry @strawberry.subscription compatibility."""
        if fn is None:

            def decorator(fn: Callable[..., Any]) -> Callable[..., Any]:
                return fraiseql.subscription(fn)

            return decorator
        return fraiseql.subscription(fn)


# Create a strawberry-like module interface
strawberry = StrawberryCompatibility()

# For more direct compatibility, also expose individual functions
# Note: 'type' shadows built-in, so we use '__all__' to control exports
__all__ = [
    "enum",
    "field",
    "input",
    "interface",
    "mutation",
    "query",
    "strawberry",
    "subscription",
    "type",
]

type = strawberry.type  # noqa: A001
input = strawberry.input  # noqa: A001
enum = strawberry.enum
interface = strawberry.interface
field = strawberry.field
mutation = strawberry.mutation
query = strawberry.query
subscription = strawberry.subscription
