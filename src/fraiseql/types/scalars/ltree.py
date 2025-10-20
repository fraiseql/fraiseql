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
    description="""Hierarchical path strings using PostgreSQL's ltree data type.

    LTree paths represent hierarchical relationships using dot-separated labels.
    Each label can contain alphanumeric characters, underscores, and hyphens.

    Examples:
    - "top" (single level)
    - "top.science" (two levels)
    - "top.science.physics.quantum" (four levels)

    Supports advanced hierarchical operations like ancestor/descendant relationships,
    pattern matching with wildcards, and path analysis functions.

    Path Format Rules:
    - Labels separated by dots (.)
    - Labels: A-Z, a-z, 0-9, _, -
    - Maximum label length: 256 characters
    - Maximum path length: 65,535 bytes
    - Case-sensitive by default
    """,
    serialize=serialize_ltree,
    parse_value=parse_ltree_value,
    parse_literal=parse_ltree_literal,
)


class LTreeField(str, ScalarMarker):
    """FraiseQL field type for PostgreSQL ltree hierarchical paths.

    Use this type to declare fields that store hierarchical path data.
    Enables powerful filtering operations for tree-like data structures.

    Common use cases:
    - Category hierarchies (product categories, taxonomies)
    - Organizational charts (reporting structures)
    - File system paths (directory trees)
    - Geographic hierarchies (country > state > city)
    - Classification systems (library classifications)

    Supports 23+ filtering operators including hierarchical relationships,
    pattern matching, and path analysis operations.
    """

    __slots__ = ()

    def __repr__(self) -> str:
        """Missing docstring."""
        return "UUID"
