"""Slug scalar type for URL-friendly identifier validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Slug: lowercase, hyphens, alphanumeric, no leading/trailing hyphens
_SLUG_REGEX = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")


def serialize_slug(value: Any) -> str | None:
    """Serialize slug to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _SLUG_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid slug: {value}. Must be lowercase alphanumeric with hyphens, "
            "no leading/trailing hyphens (e.g., 'hello-world', 'my-post-123')"
        )

    return value_str


def parse_slug_value(value: Any) -> str:
    """Parse slug from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Slug must be a string, got {type(value).__name__}")

    if not _SLUG_REGEX.match(value):
        raise GraphQLError(
            f"Invalid slug: {value}. Must be lowercase alphanumeric with hyphens, "
            "no leading/trailing hyphens (e.g., 'hello-world', 'my-post-123')"
        )

    return value


def parse_slug_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse slug from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Slug must be a string")

    return parse_slug_value(ast.value)


SlugScalar = GraphQLScalarType(
    name="Slug",
    description=(
        "URL-friendly identifier (slug). "
        "Lowercase alphanumeric characters with hyphens. "
        "No leading or trailing hyphens. "
        "Examples: hello-world, my-post-123, user-profile. "
        "Perfect for URLs, database keys, and identifiers."
    ),
    serialize=serialize_slug,
    parse_value=parse_slug_value,
    parse_literal=parse_slug_literal,
)


class SlugField(str, ScalarMarker):
    """URL-friendly identifier (slug).

    This scalar validates that the slug follows URL-friendly conventions:
    - Only lowercase letters, numbers, and hyphens
    - No leading or trailing hyphens
    - No consecutive hyphens
    - Perfect for URLs, database keys, and identifiers

    Examples:
        >>> from fraiseql.types import Slug
        >>>
        >>> @fraiseql.type
        ... class Article:
        ...     title: str
        ...     slug: Slug  # e.g., "my-awesome-article"
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "SlugField":
        """Create a new SlugField instance with validation."""
        if not _SLUG_REGEX.match(value):
            raise ValueError(
                f"Invalid slug: {value}. Must be lowercase alphanumeric with hyphens, "
                "no leading/trailing hyphens (e.g., 'hello-world', 'my-post-123')"
            )
        return super().__new__(cls, value)
