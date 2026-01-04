"""GraphQL ID scalar type with UUID validation.

This module provides:
- ID: NewType for Python type annotations (Strawberry-style syntax)
- IDScalar: GraphQL scalar named "ID" that enforces UUID format

FraiseQL is opinionated: IDs must be valid UUIDs.
The scalar is named "ID" for Apollo/Relay cache compatibility.
"""

import uuid
from typing import Any, NewType

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode, ValueNode


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
    msg = f"ID must be a valid UUID, got: {value!r}"
    raise GraphQLError(msg)


def parse_id_value(value: Any) -> uuid.UUID:
    """Parse an ID string into a UUID object."""
    if isinstance(value, str):
        try:
            return uuid.UUID(value)
        except ValueError:
            msg = f"ID must be a valid UUID string, got: {value!r}"
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


# GraphQL Scalar named "ID" for cache compatibility, but enforces UUID
IDScalar = GraphQLScalarType(
    name="ID",
    description="Unique identifier (UUID format). Compatible with Apollo/Relay caching.",
    serialize=serialize_id,
    parse_value=parse_id_value,
    parse_literal=parse_id_literal,
)

# Python type annotation (Strawberry-style)
ID = NewType("ID", str)
"""GraphQL ID type with UUID validation.

Usage:
    @fraiseql.type
    class User:
        id: ID  # Must be a valid UUID
        name: str
"""
