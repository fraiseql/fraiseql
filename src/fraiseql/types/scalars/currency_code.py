"""Currency code scalar type for ISO 4217 validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# ISO 4217: Three-letter currency codes (USD, EUR, GBP, JPY, etc.)
_CURRENCY_CODE_REGEX = re.compile(r"^[A-Z]{3}$")


def serialize_currency_code(value: Any) -> str | None:
    """Serialize currency code to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _CURRENCY_CODE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid currency code: {value}. Must be ISO 4217 three-letter code "
            "(e.g., 'USD', 'EUR', 'GBP')"
        )

    return value_str


def parse_currency_code_value(value: Any) -> str:
    """Parse currency code from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Currency code must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _CURRENCY_CODE_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid currency code: {value}. Must be ISO 4217 three-letter code "
            "(e.g., 'USD', 'EUR', 'GBP')"
        )

    return value_upper


def parse_currency_code_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse currency code from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Currency code must be a string")

    return parse_currency_code_value(ast.value)


CurrencyCodeScalar = GraphQLScalarType(
    name="CurrencyCode",
    description=(
        "ISO 4217 three-letter currency code. "
        "Valid codes: USD, EUR, GBP, JPY, CHF, CAD, AUD, etc. "
        "See: https://en.wikipedia.org/wiki/ISO_4217"
    ),
    serialize=serialize_currency_code,
    parse_value=parse_currency_code_value,
    parse_literal=parse_currency_code_literal,
)


class CurrencyCodeField(str, ScalarMarker):
    """ISO 4217 three-letter currency code.

    This scalar validates that the currency code follows ISO 4217 standard:
    - Exactly 3 uppercase letters
    - Valid codes: USD, EUR, GBP, JPY, CHF, CAD, AUD, etc.
    - Case-insensitive (normalized to uppercase)

    Example:
        >>> from fraiseql.types import CurrencyCode
        >>>
        >>> @fraiseql.input
        ... class TransactionInput:
        ...     amount: float
        ...     currency: CurrencyCode
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "CurrencyCodeField":
        """Create a new CurrencyCodeField instance with validation."""
        value_upper = value.upper()
        if not _CURRENCY_CODE_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid currency code: {value}. Must be ISO 4217 three-letter code "
                "(e.g., 'USD', 'EUR', 'GBP')"
            )
        return super().__new__(cls, value_upper)
