"""Exchange code scalar type for stock exchange identifiers."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Exchange code: 2-6 uppercase letters
_EXCHANGE_CODE_REGEX = re.compile(r"^[A-Z]{2,6}$")


def serialize_exchange_code(value: Any) -> str | None:
    """Serialize exchange code to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _EXCHANGE_CODE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid exchange code: {value}. Must be 2-6 uppercase letters "
            "(e.g., 'NYSE', 'NASDAQ', 'LSE', 'TSE')"
        )

    return value_str


def parse_exchange_code_value(value: Any) -> str:
    """Parse exchange code from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Exchange code must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _EXCHANGE_CODE_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid exchange code: {value}. Must be 2-6 uppercase letters "
            "(e.g., 'NYSE', 'NASDAQ', 'LSE', 'TSE')"
        )

    return value_upper


def parse_exchange_code_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse exchange code from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Exchange code must be a string")

    return parse_exchange_code_value(ast.value)


ExchangeCodeScalar = GraphQLScalarType(
    name="ExchangeCode",
    description=(
        "Stock exchange code. Format: 2-6 uppercase letters. Examples: NYSE, NASDAQ, LSE, TSE, HKEX"
    ),
    serialize=serialize_exchange_code,
    parse_value=parse_exchange_code_value,
    parse_literal=parse_exchange_code_literal,
)


class ExchangeCodeField(str, ScalarMarker):
    """Stock exchange code.

    This scalar validates stock exchange identifiers:
    - 2-6 uppercase letters
    - Identifies stock exchanges globally
    - Case-insensitive (normalized to uppercase)

    Example:
        >>> from fraiseql.types import ExchangeCode
        >>>
        >>> @fraiseql.type
        ... class Exchange:
        ...     code: ExchangeCode
        ...     name: str
        ...     country: str
        ...     mic: MIC
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "ExchangeCodeField":
        """Create a new ExchangeCodeField instance with validation."""
        value_upper = value.upper()
        if not _EXCHANGE_CODE_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid exchange code: {value}. Must be 2-6 uppercase letters "
                "(e.g., 'NYSE', 'NASDAQ', 'LSE', 'TSE')"
            )
        return super().__new__(cls, value_upper)
