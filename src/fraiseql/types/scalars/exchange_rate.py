"""Exchange rate scalar type for currency exchange rates with high precision."""

import re
from decimal import Decimal
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Exchange rate: NUMERIC(20,8) - positive only, high precision for crypto
_EXCHANGE_RATE_REGEX = re.compile(r"^\d{1,12}(\.\d{1,8})?$")


def serialize_exchange_rate(value: Any) -> str | None:
    """Serialize exchange rate value to string."""
    if value is None:
        return None

    # Handle Decimal, float, int, and string inputs
    if isinstance(value, Decimal):
        value_str = f"{value:.8f}".rstrip("0").rstrip(".")
    elif isinstance(value, (int, float)):
        value_str = f"{Decimal(str(value)):.8f}".rstrip("0").rstrip(".")
    else:
        value_str = str(value)

    if not _EXCHANGE_RATE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid exchange rate: {value}. Must be positive numeric with up to 8 decimal places "
            "(e.g., '1.23456789', '1234.5', '0.00001234')"
        )

    return value_str


def parse_exchange_rate_value(value: Any) -> str:
    """Parse exchange rate value from variable value."""
    if isinstance(value, (int, float, Decimal)):
        # Convert to string with proper formatting
        if isinstance(value, Decimal):
            value_str = f"{value:.8f}".rstrip("0").rstrip(".")
        else:
            value_str = f"{Decimal(str(value)):.8f}".rstrip("0").rstrip(".")
    elif isinstance(value, str):
        value_str = value
    else:
        raise GraphQLError(f"Exchange rate must be a number or string, got {type(value).__name__}")

    if not _EXCHANGE_RATE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid exchange rate: {value}. Must be positive numeric with up to 8 decimal places "
            "(e.g., '1.23456789', '1234.5', '0.00001234')"
        )

    return value_str


def parse_exchange_rate_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse exchange rate value from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Exchange rate must be a string")

    return parse_exchange_rate_value(ast.value)


ExchangeRateScalar = GraphQLScalarType(
    name="ExchangeRate",
    description=(
        "Currency exchange rate with high precision. "
        "Positive values only, up to 8 decimal places for crypto precision. "
        "Format: NUMERIC(20,8) - e.g., '1.23456789', '1234.5', '0.00001234'"
    ),
    serialize=serialize_exchange_rate,
    parse_value=parse_exchange_rate_value,
    parse_literal=parse_exchange_rate_literal,
)


class ExchangeRateField(str, ScalarMarker):
    """Currency exchange rate with high precision.

    This scalar validates exchange rates:
    - Positive values only
    - Up to 12 digits before decimal point
    - Up to 8 digits after decimal point (for crypto precision)
    - Suitable for fiat and cryptocurrency exchange rates

    Example:
        >>> from fraiseql.types import ExchangeRate
        >>>
        >>> @fraiseql.type
        ... class CurrencyPair:
        ...     from_currency: CurrencyCode
        ...     to_currency: CurrencyCode
        ...     rate: ExchangeRate
        ...     timestamp: DateTime
    """

    __slots__ = ()

    def __new__(cls, value: str | float | Decimal) -> "ExchangeRateField":
        """Create a new ExchangeRateField instance with validation."""
        if isinstance(value, (int, float, Decimal)):
            if isinstance(value, Decimal):
                value_str = f"{value:.8f}".rstrip("0").rstrip(".")
            else:
                value_str = f"{Decimal(str(value)):.8f}".rstrip("0").rstrip(".")
        elif isinstance(value, str):
            value_str = value
        else:
            raise TypeError(f"Exchange rate must be a number or string, got {type(value).__name__}")

        if not _EXCHANGE_RATE_REGEX.match(value_str):
            raise ValueError(
                f"Invalid exchange rate: {value}. Must be positive numeric "
                "with up to 8 decimal places (e.g., '1.23456789', '1234.5', '0.00001234')"
            )
        return super().__new__(cls, value_str)
