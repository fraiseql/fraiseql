"""MIC scalar type for Market Identifier Code (ISO 10383)."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# MIC: 4 uppercase letters
_MIC_REGEX = re.compile(r"^[A-Z]{4}$")


def serialize_mic(value: Any) -> str | None:
    """Serialize MIC to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _MIC_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid MIC: {value}. Must be 4 uppercase letters (e.g., 'XNYS', 'XNAS', 'XLON')"
        )

    return value_str


def parse_mic_value(value: Any) -> str:
    """Parse MIC from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"MIC must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _MIC_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid MIC: {value}. Must be 4 uppercase letters (e.g., 'XNYS', 'XNAS', 'XLON')"
        )

    return value_upper


def parse_mic_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse MIC from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("MIC must be a string")

    return parse_mic_value(ast.value)


MICScalar = GraphQLScalarType(
    name="MIC",
    description=(
        "Market Identifier Code (ISO 10383). "
        "Format: 4 uppercase letters. "
        "Uniquely identifies regulated financial market. "
        "Examples: XNYS (NYSE), XNAS (NASDAQ), XLON (London)"
    ),
    serialize=serialize_mic,
    parse_value=parse_mic_value,
    parse_literal=parse_mic_literal,
)


class MICField(str, ScalarMarker):
    """Market Identifier Code (ISO 10383).

    This scalar validates MIC codes:
    - Exactly 4 uppercase letters
    - Uniquely identifies regulated financial markets
    - Defined by ISO 10383 standard
    - Case-insensitive (normalized to uppercase)

    Example:
        >>> from fraiseql.types import MIC
        >>>
        >>> @fraiseql.type
        ... class Market:
        ...     mic: MIC
        ...     name: str
        ...     country: str
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "MICField":
        """Create a new MICField instance with validation."""
        value_upper = value.upper()
        if not _MIC_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid MIC: {value}. Must be 4 uppercase letters (e.g., 'XNYS', 'XNAS', 'XLON')"
            )
        return super().__new__(cls, value_upper)
