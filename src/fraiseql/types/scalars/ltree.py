"""Custom GraphQL scalar types for FraiseQL."""

from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode, ValueNode

from fraiseql.types.definitions import ScalarMarker


def serialize_ltree(value: Any) -> str:
    """Serialize a PostgreSQL ltree path."""
    if isinstance(value, str):
        return value
    msg = f"LTreePath cannot represent non-string value: {value!r}"
    raise GraphQLError(msg)


def parse_ltree_value(value: Any) -> str:
    """Parse a ltree path string."""
    if isinstance(value, str):
        return value
    msg = f"Invalid input for LTreePath: {value!r}"
    raise GraphQLError(msg)


def parse_ltree_literal(ast: ValueNode, variables: dict[str, Any] | None = None) -> str:
    """Parse a ltree path literal."""
    _ = variables
    if isinstance(ast, StringValueNode):
        return ast.value
    msg = f"Invalid input for LTreePath: {getattr(ast, 'value', None)!r}"
    raise GraphQLError(msg)


LTreeScalar = GraphQLScalarType(
    name="LTreePath",
    description="Scalar for PostgreSQL `ltree` hierarchical path strings.",
    serialize=serialize_ltree,
    parse_value=parse_ltree_value,
    parse_literal=parse_ltree_literal,
)


class LTreeField(str, ScalarMarker):
    """FraiseQL UUID marker used for Python-side typing and introspection."""

    __slots__ = ()

    def __repr__(self) -> str:
        """Missing docstring."""
        return "UUID"
