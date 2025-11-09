"""CUSIP scalar type for Committee on Uniform Security Identification Procedures."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# CUSIP: 9 characters (8 alphanumeric + 1 check digit)
_CUSIP_REGEX = re.compile(r"^[0-9]{3}[A-Z0-9]{5}[0-9]$")


def serialize_cusip(value: Any) -> str | None:
    """Serialize CUSIP to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _CUSIP_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid CUSIP: {value}. Must be 9 characters "
            "(3 digits + 5 alphanumeric + 1 check digit) (e.g., '037833100')"
        )

    return value_str


def parse_cusip_value(value: Any) -> str:
    """Parse CUSIP from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"CUSIP must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _CUSIP_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid CUSIP: {value}. Must be 9 characters "
            "(3 digits + 5 alphanumeric + 1 check digit) (e.g., '037833100')"
        )

    return value_upper


def parse_cusip_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse CUSIP from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("CUSIP must be a string")

    return parse_cusip_value(ast.value)


CUSIPScalar = GraphQLScalarType(
    name="CUSIP",
    description=(
        "Committee on Uniform Security Identification Procedures number. "
        "Format: 9 characters (3 digits + 5 alphanumeric + 1 check digit). "
        "Used for US and Canadian securities. "
        "Example: 037833100 (Apple)"
    ),
    serialize=serialize_cusip,
    parse_value=parse_cusip_value,
    parse_literal=parse_cusip_literal,
)


class CUSIPField(str, ScalarMarker):
    """Committee on Uniform Security Identification Procedures number.

    This scalar validates CUSIP codes:
    - Exactly 9 characters
    - 3 digits + 5 alphanumeric characters + 1 check digit
    - Used for US and Canadian securities
    - Case-insensitive (normalized to uppercase)

    Example:
        >>> from fraiseql.types import CUSIP
        >>>
        >>> @fraiseql.type
        ... class Security:
        ...     cusip: CUSIP
        ...     isin: ISIN
        ...     name: str
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "CUSIPField":
        """Create a new CUSIPField instance with validation."""
        value_upper = value.upper()
        if not _CUSIP_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid CUSIP: {value}. Must be 9 characters "
                "(3 digits + 5 alphanumeric + 1 check digit) (e.g., '037833100')"
            )
        return super().__new__(cls, value_upper)
