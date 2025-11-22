"""ISIN scalar type for International Securities Identification Numbers."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# ISIN: 12 characters (2 country + 9 security + 1 check digit)
_ISIN_REGEX = re.compile(r"^[A-Z]{2}[A-Z0-9]{9}[0-9]$")


def _validate_isin_check_digit(isin: str) -> bool:
    """Validate ISIN check digit using Luhn algorithm."""
    # Convert letters to numbers (A=10, B=11, ..., Z=35)
    digits = []
    for char in isin[:-1]:  # Exclude check digit
        if char.isdigit():
            digits.append(int(char))
        else:
            # Convert letter to number: A=10, B=11, ..., Z=35
            num = ord(char) - ord("A") + 10
            # For double-digit numbers, add both digits
            digits.extend([num // 10, num % 10])

    # Apply Luhn algorithm
    total = 0
    for i, digit in enumerate(reversed(digits)):
        if i % 2 == 0:  # Even positions (from right, 0-based)
            doubled = digit * 2
            total += doubled if doubled < 10 else doubled - 9
        else:
            total += digit

    check_digit = (10 - (total % 10)) % 10
    return check_digit == int(isin[-1])


def serialize_isin(value: Any) -> str | None:
    """Serialize ISIN to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _ISIN_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid ISIN: {value}. Must be 12 characters "
            "(2 country + 9 security + 1 check digit) (e.g., 'US0378331005', 'GB0002374006')"
        )

    if not _validate_isin_check_digit(value_str):
        raise GraphQLError(f"Invalid ISIN check digit: {value}")

    return value_str


def parse_isin_value(value: Any) -> str:
    """Parse ISIN from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"ISIN must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _ISIN_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid ISIN: {value}. Must be 12 characters "
            "(2 country + 9 security + 1 check digit) (e.g., 'US0378331005', 'GB0002374006')"
        )

    if not _validate_isin_check_digit(value_upper):
        raise GraphQLError(f"Invalid ISIN check digit: {value}")

    return value_upper


def parse_isin_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse ISIN from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("ISIN must be a string")

    return parse_isin_value(ast.value)


ISINScalar = GraphQLScalarType(
    name="ISIN",
    description=(
        "International Securities Identification Number. "
        "Format: 12 characters (2 country code + 9 security identifier + 1 check digit). "
        "Validates check digit using Luhn algorithm. "
        "Examples: US0378331005 (Apple), GB0002374006 (BP)"
    ),
    serialize=serialize_isin,
    parse_value=parse_isin_value,
    parse_literal=parse_isin_literal,
)


class ISINField(str, ScalarMarker):
    """International Securities Identification Number.

    This scalar validates ISIN codes:
    - Exactly 12 characters
    - 2-letter country code + 9-character security identifier + 1 check digit
    - Check digit validated using Luhn algorithm
    - Case-insensitive (normalized to uppercase)

    Example:
        >>> from fraiseql.types import ISIN
        >>>
        >>> @fraiseql.type
        ... class Security:
        ...     isin: ISIN
        ...     symbol: StockSymbol
        ...     name: str
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "ISINField":
        """Create a new ISINField instance with validation."""
        value_upper = value.upper()
        if not _ISIN_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid ISIN: {value}. Must be 12 characters "
                "(2 country + 9 security + 1 check digit) (e.g., 'US0378331005', 'GB0002374006')"
            )

        if not _validate_isin_check_digit(value_upper):
            raise ValueError(f"Invalid ISIN check digit: {value}")

        return super().__new__(cls, value_upper)
