"""Percentage scalar type for percentage values with 2 decimal precision."""

import re
from decimal import Decimal
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Percentage: NUMERIC(5,2) - 0.00 to 100.00 range
_PERCENTAGE_REGEX = re.compile(r"^(100(\.00?)?|[1-9]?\d(\.\d{1,2})?)$")


def serialize_percentage(value: Any) -> str | None:
    """Serialize percentage value to string."""
    if value is None:
        return None

    # Handle Decimal, float, int, and string inputs
    if isinstance(value, Decimal):
        value_str = f"{value:.2f}".rstrip("0").rstrip(".")
    elif isinstance(value, (int, float)):
        value_str = f"{Decimal(str(value)):.2f}".rstrip("0").rstrip(".")
    else:
        value_str = str(value)

    if not _PERCENTAGE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid percentage: {value}. Must be between 0.00 and 100.00 "
            "(e.g., '25.5', '100', '0.01')"
        )

    return value_str


def parse_percentage_value(value: Any) -> str:
    """Parse percentage value from variable value."""
    if isinstance(value, (int, float, Decimal)):
        # Convert to string with proper formatting
        if isinstance(value, Decimal):
            value_str = f"{value:.2f}".rstrip("0").rstrip(".")
        else:
            value_str = f"{Decimal(str(value)):.2f}".rstrip("0").rstrip(".")
    elif isinstance(value, str):
        value_str = value
    else:
        raise GraphQLError(f"Percentage must be a number or string, got {type(value).__name__}")

    if not _PERCENTAGE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid percentage: {value}. Must be between 0.00 and 100.00 "
            "(e.g., '25.5', '100', '0.01')"
        )

    return value_str


def parse_percentage_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse percentage value from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Percentage must be a string")

    return parse_percentage_value(ast.value)


PercentageScalar = GraphQLScalarType(
    name="Percentage",
    description=(
        "Percentage value with 2 decimal precision. "
        "Range: 0.00 to 100.00. "
        "Format: NUMERIC(5,2) - e.g., '25.5', '100.00', '0.01'"
    ),
    serialize=serialize_percentage,
    parse_value=parse_percentage_value,
    parse_literal=parse_percentage_literal,
)


class PercentageField(str, ScalarMarker):
    """Percentage value with 2 decimal precision.

    This scalar validates percentage values:
    - Range: 0.00 to 100.00
    - Up to 2 decimal places
    - Represents percentage (not decimal fraction)

    Example:
        >>> from fraiseql.types import Percentage
        >>>
        >>> @fraiseql.type
        ... class TaxRate:
        ...     rate: Percentage
        ...     description: str
    """

    __slots__ = ()

    def __new__(cls, value: str | float | Decimal) -> "PercentageField":
        """Create a new PercentageField instance with validation."""
        if isinstance(value, (int, float, Decimal)):
            if isinstance(value, Decimal):
                value_str = f"{value:.2f}".rstrip("0").rstrip(".")
            else:
                value_str = f"{Decimal(str(value)):.2f}".rstrip("0").rstrip(".")
        elif isinstance(value, str):
            value_str = value
        else:
            raise TypeError(f"Percentage must be a number or string, got {type(value).__name__}")

        if not _PERCENTAGE_REGEX.match(value_str):
            raise ValueError(
                f"Invalid percentage: {value}. Must be between 0.00 and 100.00 "
                "(e.g., '25.5', '100', '0.01')"
            )
        return super().__new__(cls, value_str)
