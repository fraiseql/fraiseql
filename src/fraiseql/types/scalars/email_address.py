"""Missing docstring."""

# fraiseql/types/scalars/email_address.py

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import ValueNode

from fraiseql.types.definitions import ScalarMarker

_EMAIL_REGEX = re.compile(r"^[^@]+@[^@]+\.[^@]+$")


def serialize_email_address(value: Any) -> str:
    """Missing docstring."""
    return value


def parse_email_address(value: Any) -> str:
    """Missing docstring."""
    if not isinstance(value, str) or not _EMAIL_REGEX.match(value):
        msg = f"Invalid email address: {value}"
        raise GraphQLError(msg)
    return value


def parse_email_literal(ast: ValueNode, _vars: dict[str, Any] | None = None) -> str:
    """Missing docstring."""
    _ = _vars
    from graphql.language import StringValueNode

    if isinstance(ast, StringValueNode):
        return parse_email_address(ast.value)
    msg = "EmailAddress must be a string"
    raise GraphQLError(msg)


EmailAddressScalar = GraphQLScalarType(
    name="EmailAddress",
    description="A validated email address conforming to standard format",
    serialize=serialize_email_address,
    parse_value=parse_email_address,
    parse_literal=parse_email_literal,
)


class EmailAddressField(ScalarMarker):
    """Represents a validated email address."""

    __slots__ = ()

    def __repr__(self) -> str:
        """Missing docstring."""
        return "EmailAddress"
