"""Defines a custom GraphQL UUID scalar for FraiseQL.

This module provides a strict UUID scalar (`UUIDScalar`) with full serialize/parse logic,
and a `UUIDField` marker type for Python-side typing and introspection.
"""

import uuid
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import (
    StringValueNode,
    ValueNode,
)

from fraiseql.types.definitions import ScalarMarker


def serialize_uuid(value: Any) -> str:
    """Serialize a UUID object to a string."""
    if isinstance(value, uuid.UUID):
        return str(value)
    if isinstance(value, str):
        try:
            uuid.UUID(value)
            return value  # noqa: TRY300
        except ValueError:
            pass
    msg = f"UUID cannot represent non-UUID value: {value!r}"
    raise GraphQLError(msg)


def parse_uuid_value(value: Any) -> uuid.UUID:
    """Parse a UUID string into a UUID object."""
    if isinstance(value, str):
        try:
            return uuid.UUID(value)
        except ValueError:
            msg = f"Invalid UUID string provided: {value!r}"
            raise GraphQLError(msg) from None
    msg = f"UUID cannot represent non-string value: {value!r}"
    raise GraphQLError(msg)


def parse_uuid_literal(ast: ValueNode, variables: dict[str, object] | None = None) -> uuid.UUID:
    """Parse a UUID literal from the GraphQL AST."""
    _ = variables
    if isinstance(ast, StringValueNode):
        return parse_uuid_value(ast.value)
    msg = f"UUID cannot represent non-string literal: {getattr(ast, 'value', None)!r}"
    raise GraphQLError(msg)


UUIDScalar = GraphQLScalarType(
    name="UUID",
    description="A globally unique identifier in UUID format.",
    serialize=serialize_uuid,
    parse_value=parse_uuid_value,
    parse_literal=parse_uuid_literal,
)


class UUIDField(str, ScalarMarker):
    """FraiseQL UUID marker used for Python-side typing and introspection."""

    __slots__ = ()

    def __repr__(self) -> str:
        """Return a user-friendly type name for introspection and debugging."""
        return "UUID"
