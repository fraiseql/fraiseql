"""Custom JSON scalar type for FraiseQL."""

import json
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import (
    BooleanValueNode,
    FloatValueNode,
    IntValueNode,
    ListValueNode,
    NullValueNode,
    ObjectValueNode,
    StringValueNode,
    ValueNode,
)

from fraiseql.types.definitions import ScalarMarker

# Python-side recursive JSON type
JSONData = str | int | float | bool | None | dict[str, "JSONData"] | list["JSONData"]


def serialize_json(
    value: Any,
) -> str | dict[str, Any] | list[Any] | int | float | bool | None:
    """Serialize JSON-compatible data.

    Validates that the value is JSON-serializable before returning it.
    This prevents non-serializable objects from causing issues later
    in the serialization pipeline.
    """
    # Check if the value is JSON-serializable
    try:
        # Use json.dumps to validate serializability
        json.dumps(value)
        return value  # noqa: TRY300
    except (TypeError, ValueError) as e:
        msg = f"Value is not JSON-serializable: {type(value).__name__} - {e}"
        raise GraphQLError(msg) from e


def parse_json_value(
    value: Any,
) -> str | dict[str, Any] | list[Any] | int | float | bool | None:
    """Parse value that must conform to JSON structure."""
    # If value is one of the expected types, return it
    if isinstance(value, dict | list | str | int | float | bool | type(None)):
        return value  # type: ignore[return-value]

    # If it's an unsupported type, raise an error
    msg = f"JSON cannot represent value: {value!r}"
    raise GraphQLError(msg)


def parse_json_literal(
    ast: ValueNode,
    variables: dict[str, Any] | None = None,
) -> str | dict[str, Any] | list[Any] | int | float | bool | None:
    """Parse JSON from a GraphQL literal.

    Accepts:
    - String literals containing JSON
    - Numeric literals (int/float)
    - Boolean literals
    - Null literals
    - Object literals (GraphQL object notation)
    - List literals (GraphQL list notation)

    Args:
        ast: The GraphQL AST node to parse
        variables: Optional variables dict (not used for JSON parsing)

    Returns:
        The parsed JSON value as a Python object

    Raises:
        GraphQLError: If the literal cannot be parsed as JSON
    """
    _ = variables  # Not used here

    if isinstance(ast, StringValueNode):
        # If it's a string, try to parse it as JSON
        # If it fails, raise an error for non-JSON strings
        try:
            return json.loads(ast.value)
        except json.JSONDecodeError as e:
            msg = f"JSON cannot represent non-JSON string: {ast.value!r}"
            raise GraphQLError(msg) from e

    if isinstance(ast, IntValueNode):
        # IntValueNode stores the value as a string, convert to int
        return int(ast.value)

    if isinstance(ast, FloatValueNode):
        # FloatValueNode stores the value as a string, convert to float
        return float(ast.value)

    if isinstance(ast, BooleanValueNode):
        # BooleanValueNode already has a boolean value
        return ast.value

    if isinstance(ast, NullValueNode):
        # NullValueNode represents JSON null
        return None

    if isinstance(ast, ObjectValueNode):
        # JSON scalar should only accept string literals containing JSON,
        # not GraphQL object notation
        msg = "Use a String literal containing JSON, not GraphQL object notation"
        raise GraphQLError(msg)

    if isinstance(ast, ListValueNode):
        # JSON scalar should only accept string literals containing JSON,
        # not GraphQL list notation
        msg = "Use a String literal containing JSON, not GraphQL list notation"
        raise GraphQLError(msg)

    msg = f"JSON cannot represent literal of type {type(ast).__name__}"
    raise GraphQLError(msg)


# GraphQL scalar definition
JSONScalar = GraphQLScalarType(
    name="JSON",
    description="The `JSON` scalar type represents JSON values as specified by ECMA-404.",
    serialize=serialize_json,
    parse_value=parse_json_value,
    parse_literal=parse_json_literal,
)


# Python marker for use in dataclasses
class JSONField(dict[str, Any], ScalarMarker):
    """Python marker for the GraphQL JSON scalar."""

    __slots__ = ()

    def __repr__(self) -> str:
        """String representation used in type annotations and debug output."""
        return "JSON"
