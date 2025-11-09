"""SEDOL scalar type for Stock Exchange Daily Official List."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# SEDOL: 7 characters (6 alphanumeric + 1 check digit)
# Excludes vowels and some letters to avoid confusion
_SEDOL_REGEX = re.compile(r"^[B-DF-HJ-NP-TV-Z0-9]{6}[0-9]$")


def serialize_sedol(value: Any) -> str | None:
    """Serialize SEDOL to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _SEDOL_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid SEDOL: {value}. Must be 7 characters (6 alphanumeric + 1 check digit) "
            "(e.g., '0263494')"
        )

    return value_str


def parse_sedol_value(value: Any) -> str:
    """Parse SEDOL from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"SEDOL must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _SEDOL_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid SEDOL: {value}. Must be 7 characters (6 alphanumeric + 1 check digit) "
            "(e.g., '0263494')"
        )

    return value_upper


def parse_sedol_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse SEDOL from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("SEDOL must be a string")

    return parse_sedol_value(ast.value)


SEDOLScalar = GraphQLScalarType(
    name="SEDOL",
    description=(
        "Stock Exchange Daily Official List number. "
        "Format: 7 characters (6 alphanumeric + 1 check digit). "
        "Used for UK securities. "
        "Example: 0263494 (BP)"
    ),
    serialize=serialize_sedol,
    parse_value=parse_sedol_value,
    parse_literal=parse_sedol_literal,
)


class SEDOLField(str, ScalarMarker):
    """Stock Exchange Daily Official List number.

    This scalar validates SEDOL codes:
    - Exactly 7 characters
    - 6 alphanumeric characters + 1 check digit
    - Excludes vowels and some letters to avoid confusion
    - Used for UK securities
    - Case-insensitive (normalized to uppercase)

    Example:
        >>> from fraiseql.types import SEDOL
        >>>
        >>> @fraiseql.type
        ... class UKSecurity:
        ...     sedol: SEDOL
        ...     isin: ISIN
        ...     name: str
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "SEDOLField":
        """Create a new SEDOLField instance with validation."""
        value_upper = value.upper()
        if not _SEDOL_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid SEDOL: {value}. Must be 7 characters (6 alphanumeric + 1 check digit) "
                "(e.g., '0263494')"
            )
        return super().__new__(cls, value_upper)
