"""API key scalar type for access token validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# API key regex: alphanumeric with hyphens/underscores, 16-128 characters
_API_KEY_REGEX = re.compile(r"^[A-Za-z0-9_-]{16,128}$")


def serialize_api_key(value: Any) -> str | None:
    """Serialize API key to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _API_KEY_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid API key: {value}. Must be 16-128 characters containing only "
            "letters, numbers, hyphens, and underscores"
        )

    return value_str


def parse_api_key_value(value: Any) -> str:
    """Parse API key from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"API key must be a string, got {type(value).__name__}")

    if not _API_KEY_REGEX.match(value):
        raise GraphQLError(
            f"Invalid API key: {value}. Must be 16-128 characters containing only "
            "letters, numbers, hyphens, and underscores"
        )

    return value


def parse_api_key_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse API key from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("API key must be a string")

    return parse_api_key_value(ast.value)


ApiKeyScalar = GraphQLScalarType(
    name="ApiKey",
    description=(
        "API key or access token. Must be 16-128 characters containing only "
        "letters, numbers, hyphens, and underscores. "
        "Examples: test_key_4eC39HqLyjWDarjtT1zdp7dc, api_key_1234567890"
    ),
    serialize=serialize_api_key,
    parse_value=parse_api_key_value,
    parse_literal=parse_api_key_literal,
)


class ApiKeyField(str, ScalarMarker):
    """API key or access token.

    This scalar validates that the API key follows common patterns:
    - 16-128 characters in length
    - Contains only letters, numbers, hyphens, and underscores
    - No spaces or special characters

    Example:
        >>> from fraiseql.types import ApiKey
        >>>
        >>> @fraiseql.input
        ... class AuthInput:
        ...     api_key: ApiKey
        ...     endpoint: str
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "ApiKeyField":
        """Create a new ApiKeyField instance with validation."""
        if not _API_KEY_REGEX.match(value):
            raise ValueError(
                f"Invalid API key: {value}. Must be 16-128 characters containing only "
                "letters, numbers, hyphens, and underscores"
            )
        return super().__new__(cls, value)
