"""Phone number scalar type for E.164 validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# E.164: International phone number format (+[country][number], 7-15 digits total)
_PHONE_NUMBER_REGEX = re.compile(r"^\+[1-9]\d{6,14}$")


def serialize_phone_number(value: Any) -> str | None:
    """Serialize phone number to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _PHONE_NUMBER_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid phone number: {value}. Must be E.164 format "
            "(e.g., '+1234567890', '+447911123456')"
        )

    return value_str


def parse_phone_number_value(value: Any) -> str:
    """Parse phone number from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Phone number must be a string, got {type(value).__name__}")

    if not _PHONE_NUMBER_REGEX.match(value):
        raise GraphQLError(
            f"Invalid phone number: {value}. Must be E.164 format "
            "(e.g., '+1234567890', '+447911123456')"
        )

    return value


def parse_phone_number_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse phone number from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Phone number must be a string")

    return parse_phone_number_value(ast.value)


PhoneNumberScalar = GraphQLScalarType(
    name="PhoneNumber",
    description=(
        "E.164 international phone number format. "
        "Must start with '+' followed by country code and subscriber number. "
        "Total length: 7-15 digits. "
        "Examples: +1234567890, +447911123456. "
        "See: https://en.wikipedia.org/wiki/E.164"
    ),
    serialize=serialize_phone_number,
    parse_value=parse_phone_number_value,
    parse_literal=parse_phone_number_literal,
)


class PhoneNumberField(str, ScalarMarker):
    """E.164 international phone number.

    This scalar validates that the phone number follows E.164 standard:
    - Starts with '+' followed by country code
    - Total length: 7-15 digits
    - No spaces, hyphens, or other formatting

    Example:
        >>> from fraiseql.types import PhoneNumber
        >>>
        >>> @fraiseql.type
        ... class Contact:
        ...     name: str
        ...     phone: PhoneNumber
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "PhoneNumberField":
        """Create a new PhoneNumberField instance with validation."""
        if not _PHONE_NUMBER_REGEX.match(value):
            raise ValueError(
                f"Invalid phone number: {value}. Must be E.164 format "
                "(e.g., '+1234567890', '+447911123456')"
            )
        return super().__new__(cls, value)
