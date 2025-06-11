"""Enum support for FraiseQL GraphQL schemas.

This module provides the @fraise_enum decorator for defining GraphQL enum types
from Python Enum classes. It handles registration, type conversion, and
serialization/deserialization of enum values.
"""

from collections.abc import Callable
from enum import Enum
from typing import TypeVar, overload

from graphql import GraphQLEnumType, GraphQLEnumValue

T = TypeVar("T", bound=type[Enum])


@overload
def fraise_enum(_cls: None = None) -> Callable[[T], T]: ...
@overload
def fraise_enum(_cls: T) -> T: ...


def fraise_enum(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator for GraphQL enum types.

    Converts a Python Enum class into a GraphQL enum type that can be used
    in queries, mutations, and type definitions.

    Example:
        @fraise_enum
        class UserRole(Enum):
            ADMIN = "admin"
            USER = "user"
            GUEST = "guest"

        @fraise_type
        class User:
            name: str
            role: UserRole
    """

    def wrap(cls: T) -> T:
        if not issubclass(cls, Enum):
            msg = f"@fraise_enum can only be used on Enum classes, not {cls.__name__}"
            raise TypeError(msg)

        # Import here to avoid circular imports
        from fraiseql.gql.schema_builder import SchemaRegistry

        # Create GraphQL enum type
        enum_values = {}
        for member in cls:
            # Use the enum member name as the GraphQL value name
            # GraphQL will serialize based on the GraphQLEnumValue's internal value
            enum_values[member.name] = GraphQLEnumValue(
                value=member.name,  # Store the GraphQL name
                description=getattr(member, "_description", None),
            )

        graphql_enum = GraphQLEnumType(
            name=cls.__name__,
            values=enum_values,
            description=cls.__doc__,
        )

        # Store the GraphQL type on the class for later retrieval
        cls.__graphql_type__ = graphql_enum

        # Register with schema
        SchemaRegistry.get_instance().register_enum(cls, graphql_enum)

        return cls

    return wrap if _cls is None else wrap(_cls)
