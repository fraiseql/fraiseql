"""Postal code scalar type for international postal/ZIP code validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# International postal code: alphanumeric with spaces/hyphens, 3-10 characters
# Must start and end with alphanumeric, allows spaces and hyphens in middle
_POSTAL_CODE_REGEX = re.compile(r"^[A-Z0-9][A-Z0-9 -]{1,8}[A-Z0-9]$", re.IGNORECASE)


def serialize_postal_code(value: Any) -> str | None:
    """Serialize postal code to string."""
    if value is None:
        return None

    value_str = str(value).strip()

    if not _POSTAL_CODE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid postal code: {value}. Must be 3-10 characters, "
            "alphanumeric with optional spaces/hyphens "
            "(e.g., '90210', 'SW1A 1AA', '75001', '100-0001')"
        )

    return value_str


def parse_postal_code_value(value: Any) -> str:
    """Parse postal code from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Postal code must be a string, got {type(value).__name__}")

    value_str = value.strip()

    if not _POSTAL_CODE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid postal code: {value}. Must be 3-10 characters, "
            "alphanumeric with optional spaces/hyphens "
            "(e.g., '90210', 'SW1A 1AA', '75001', '100-0001')"
        )

    return value_str


def parse_postal_code_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse postal code from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Postal code must be a string")

    return parse_postal_code_value(ast.value)


PostalCodeScalar = GraphQLScalarType(
    name="PostalCode",
    description=(
        "International postal/ZIP code format. "
        "3-10 characters, alphanumeric with optional spaces/hyphens. "
        "Examples: 90210 (US), SW1A 1AA (UK), 75001 (France), 100-0001 (Japan). "
        "Case-insensitive, leading/trailing whitespace trimmed."
    ),
    serialize=serialize_postal_code,
    parse_value=parse_postal_code_value,
    parse_literal=parse_postal_code_literal,
)


class PostalCodeField(str, ScalarMarker):
    """International postal/ZIP code.

    This scalar validates that the postal code follows international formats:
    - 3-10 characters total
    - Must start and end with alphanumeric characters
    - Allows spaces and hyphens in the middle
    - Case-insensitive
    - Leading/trailing whitespace is trimmed

    Examples:
        >>> from fraiseql.types import PostalCode
        >>>
        >>> @fraiseql.type
        ... class Address:
        ...     street: str
        ...     postal_code: PostalCode
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "PostalCodeField":
        """Create a new PostalCodeField instance with validation."""
        value_str = value.strip()
        if not _POSTAL_CODE_REGEX.match(value_str):
            raise ValueError(
                f"Invalid postal code: {value}. Must be 3-10 characters, "
                "alphanumeric with optional spaces/hyphens "
                "(e.g., '90210', 'SW1A 1AA', '75001', '100-0001')"
            )
        return super().__new__(cls, value_str)
