"""GraphQL ID scalar backed by UUID."""

from __future__ import annotations

import uuid
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode, ValueNode

from fraiseql.types.definitions import ScalarMarker


# Serialization functions (reuse UUID logic since ID = UUID)
def serialize_id(value: Any) -> str:
    """Serialize an ID (UUID) to string."""
    if isinstance(value, uuid.UUID):
        return str(value)
    if isinstance(value, str):
        try:
            uuid.UUID(value)
            return value
        except ValueError:
            pass
    msg = f"ID cannot represent non-UUID value: {value!r}"
    raise GraphQLError(msg)


def parse_id_value(value: Any) -> uuid.UUID:
    """Parse an ID string into a UUID object."""
    if isinstance(value, str):
        try:
            return uuid.UUID(value)
        except ValueError:
            msg = f"Invalid ID string provided: {value!r}"
            raise GraphQLError(msg) from None
    msg = f"ID cannot represent non-string value: {value!r}"
    raise GraphQLError(msg)


def parse_id_literal(ast: ValueNode, variables: dict[str, object] | None = None) -> uuid.UUID:
    """Parse an ID literal from GraphQL AST."""
    _ = variables
    if isinstance(ast, StringValueNode):
        return parse_id_value(ast.value)
    msg = f"ID cannot represent non-string literal: {getattr(ast, 'value', None)!r}"
    raise GraphQLError(msg)


# NOTE: We do NOT create a custom IDScalar here because "ID" is a reserved type name
# in graphql-core and cannot be redefined. Use GraphQLID (the built-in scalar) instead.
# This module only exports IDField for Python type compatibility.


# Python Type Marker
class IDField(str, ScalarMarker):
    """FraiseQL ID marker used for Python-side typing and introspection.

    Represents opaque identifiers, backed by UUID in PostgreSQL.
    """

    __slots__ = ()

    def __repr__(self) -> str:
        """Return a user-friendly type name for introspection and debugging."""
        return "ID"
