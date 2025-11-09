"""Stock symbol scalar type for stock ticker symbols."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Stock symbol: 1-5 uppercase letters + optional class suffix (.A, .B)
_STOCK_SYMBOL_REGEX = re.compile(r"^[A-Z]{1,5}(\.[A-Z])?$")


def serialize_stock_symbol(value: Any) -> str | None:
    """Serialize stock symbol to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _STOCK_SYMBOL_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid stock symbol: {value}. Must be 1-5 uppercase letters "
            "with optional class suffix (e.g., 'AAPL', 'MSFT', 'BRK.A', 'BRK.B')"
        )

    return value_str


def parse_stock_symbol_value(value: Any) -> str:
    """Parse stock symbol from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Stock symbol must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _STOCK_SYMBOL_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid stock symbol: {value}. Must be 1-5 uppercase letters "
            "with optional class suffix (e.g., 'AAPL', 'MSFT', 'BRK.A', 'BRK.B')"
        )

    return value_upper


def parse_stock_symbol_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse stock symbol from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Stock symbol must be a string")

    return parse_stock_symbol_value(ast.value)


StockSymbolScalar = GraphQLScalarType(
    name="StockSymbol",
    description=(
        "Stock ticker symbol. "
        "Format: 1-5 uppercase letters with optional class suffix. "
        "Examples: AAPL, MSFT, GOOGL, BRK.A, BRK.B"
    ),
    serialize=serialize_stock_symbol,
    parse_value=parse_stock_symbol_value,
    parse_literal=parse_stock_symbol_literal,
)


class StockSymbolField(str, ScalarMarker):
    """Stock ticker symbol.

    This scalar validates stock ticker symbols:
    - 1-5 uppercase letters
    - Optional class suffix (.A, .B, etc.)
    - Case-insensitive (normalized to uppercase)

    Example:
        >>> from fraiseql.types import StockSymbol
        >>>
        >>> @fraiseql.type
        ... class Stock:
        ...     symbol: StockSymbol
        ...     name: str
        ...     price: Money
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "StockSymbolField":
        """Create a new StockSymbolField instance with validation."""
        value_upper = value.upper()
        if not _STOCK_SYMBOL_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid stock symbol: {value}. Must be 1-5 uppercase letters "
                "with optional class suffix (e.g., 'AAPL', 'MSFT', 'BRK.A', 'BRK.B')"
            )
        return super().__new__(cls, value_upper)
