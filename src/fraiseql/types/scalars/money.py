"""Money scalar type for financial amounts with 4 decimal precision."""

import re
from decimal import Decimal
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Money: NUMERIC(19,4) - supports up to 15 digits before decimal + 4 after
# Allows negative values, no currency info
_MONEY_REGEX = re.compile(r"^-?\d{1,15}(\.\d{1,4})?$")


def serialize_money(value: Any) -> str | None:
    """Serialize money value to string."""
    if value is None:
        return None

    # Handle Decimal, float, int, and string inputs
    if isinstance(value, Decimal):
        value_str = f"{value:.4f}".rstrip("0").rstrip(".")
    elif isinstance(value, (int, float)):
        value_str = f"{Decimal(str(value)):.4f}".rstrip("0").rstrip(".")
    else:
        value_str = str(value)

    if not _MONEY_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid money value: {value}. Must be numeric with up to 4 decimal places "
            "(e.g., '123.45', '-999.9999', '100')"
        )

    return value_str


def parse_money_value(value: Any) -> str:
    """Parse money value from variable value."""
    if isinstance(value, (int, float, Decimal)):
        # Convert to string with proper formatting
        if isinstance(value, Decimal):
            value_str = f"{value:.4f}".rstrip("0").rstrip(".")
        else:
            value_str = f"{Decimal(str(value)):.4f}".rstrip("0").rstrip(".")
    elif isinstance(value, str):
        value_str = value
    else:
        raise GraphQLError(f"Money must be a number or string, got {type(value).__name__}")

    if not _MONEY_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid money value: {value}. Must be numeric with up to 4 decimal places "
            "(e.g., '123.45', '-999.9999', '100')"
        )

    return value_str


def parse_money_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse money value from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Money must be a string")

    return parse_money_value(ast.value)


MoneyScalar = GraphQLScalarType(
    name="Money",
    description=(
        "Financial money amount with 4 decimal precision. "
        "Supports negative values, up to 15 digits before decimal. "
        "Format: NUMERIC(19,4) - e.g., '123.45', '-999.9999', '100'"
    ),
    serialize=serialize_money,
    parse_value=parse_money_value,
    parse_literal=parse_money_literal,
)


class MoneyField(str, ScalarMarker):
    """Financial money amount with 4 decimal precision.

    This scalar validates financial amounts:
    - Up to 15 digits before decimal point
    - Up to 4 digits after decimal point
    - Supports negative values
    - No currency information (use with CurrencyCode separately)

    Example:
        >>> from fraiseql.types import Money
        >>>
        >>> @fraiseql.type
        ... class Transaction:
        ...     amount: Money
        ...     currency: CurrencyCode
        ...     fee: Money | None
    """

    __slots__ = ()

    def __new__(cls, value: str | float | Decimal) -> "MoneyField":
        """Create a new MoneyField instance with validation."""
        if isinstance(value, (int, float, Decimal)):
            if isinstance(value, Decimal):
                value_str = f"{value:.4f}".rstrip("0").rstrip(".")
            else:
                value_str = f"{Decimal(str(value)):.4f}".rstrip("0").rstrip(".")
        elif isinstance(value, str):
            value_str = value
        else:
            raise TypeError(f"Money must be a number or string, got {type(value).__name__}")

        if not _MONEY_REGEX.match(value_str):
            raise ValueError(
                f"Invalid money value: {value}. Must be numeric with up to 4 decimal places "
                "(e.g., '123.45', '-999.9999', '100')"
            )
        return super().__new__(cls, value_str)
