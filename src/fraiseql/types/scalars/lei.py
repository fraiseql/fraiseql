"""LEI scalar type for Legal Entity Identifier."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# LEI: 20 alphanumeric characters
_LEI_REGEX = re.compile(r"^[A-Z0-9]{18}[0-9]{2}$")


def serialize_lei(value: Any) -> str | None:
    """Serialize LEI to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _LEI_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid LEI: {value}. Must be 20 alphanumeric characters "
            "(e.g., '549300E9PC51EN656011')"
        )

    return value_str


def parse_lei_value(value: Any) -> str:
    """Parse LEI from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"LEI must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _LEI_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid LEI: {value}. Must be 20 alphanumeric characters "
            "(e.g., '549300E9PC51EN656011')"
        )

    return value_upper


def parse_lei_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse LEI from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("LEI must be a string")

    return parse_lei_value(ast.value)


LEIScalar = GraphQLScalarType(
    name="LEI",
    description=(
        "Legal Entity Identifier. "
        "Format: 20 alphanumeric characters. "
        "Globally unique identifier for legal entities. "
        "Example: 549300E9PC51EN656011"
    ),
    serialize=serialize_lei,
    parse_value=parse_lei_value,
    parse_literal=parse_lei_literal,
)


class LEIField(str, ScalarMarker):
    """Legal Entity Identifier.

    This scalar validates LEI codes:
    - Exactly 20 alphanumeric characters
    - Globally unique identifier for legal entities
    - Used in financial regulation and reporting
    - Case-insensitive (normalized to uppercase)

    Example:
        >>> from fraiseql.types import LEI
        >>>
        >>> @fraiseql.type
        ... class LegalEntity:
        ...     lei: LEI
        ...     name: str
        ...     jurisdiction: str
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "LEIField":
        """Create a new LEIField instance with validation."""
        value_upper = value.upper()
        if not _LEI_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid LEI: {value}. Must be 20 alphanumeric characters "
                "(e.g., '549300E9PC51EN656011')"
            )
        return super().__new__(cls, value_upper)
