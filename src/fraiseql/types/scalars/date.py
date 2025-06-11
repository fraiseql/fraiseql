"""ISO 8601 Date scalar for FraiseQL."""

from datetime import date
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode, ValueNode

from fraiseql.types.definitions import ScalarMarker


def serialize_date(value: Any) -> str:
    """Serialize a Python `date` to an ISO 8601 string."""
    if not isinstance(value, date):
        msg = f"Date cannot represent non-date value: {value!r}"
        raise GraphQLError(msg)
    return value.isoformat()


def parse_date_value(value: Any) -> date | None:
    """Parse an ISO 8601 date string into a Python `date`."""
    if value is None:
        return None

    if not isinstance(value, str):
        msg = f"Date cannot represent non-string value: {value!r}"
        raise GraphQLError(msg)

    try:
        return date.fromisoformat(value)
    except ValueError as e:
        msg = f"Invalid ISO 8601 Date: {value!r}"
        raise GraphQLError(msg) from e


def parse_date_literal(
    ast: ValueNode,
    variables: dict[str, object] | None = None,
) -> date | None:
    """Parse a Date literal from GraphQL AST."""
    _ = variables
    if isinstance(ast, StringValueNode):
        return parse_date_value(ast.value)
    msg = f"Date cannot represent non-string literal: {getattr(ast, 'value', None)!r}"
    raise GraphQLError(msg)


DateScalar = GraphQLScalarType(
    name="Date",
    description="An ISO 8601-compliant Date scalar (yyyy-mm-dd).",
    serialize=serialize_date,
    parse_value=parse_date_value,
    parse_literal=parse_date_literal,
)


class DateField(str, ScalarMarker):
    """Python marker for the GraphQL Date scalar."""

    __slots__ = ()

    def __repr__(self) -> str:
        """Return a string identifier for debugging."""
        return "Date"
