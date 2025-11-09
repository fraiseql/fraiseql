"""Mime type scalar type for media type validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# MIME type: type/subtype format
_MIME_TYPE_REGEX = re.compile(
    r"^[a-zA-Z][a-zA-Z0-9][a-zA-Z0-9!#$&^_-]*\/[a-zA-Z][a-zA-Z0-9][a-zA-Z0-9!#$&^_-]*(?:\+[a-zA-Z][a-zA-Z0-9][a-zA-Z0-9!#$&^_-]*)?$"
)


def serialize_mime_type(value: Any) -> str | None:
    """Serialize MIME type to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _MIME_TYPE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid MIME type: {value}. Must be type/subtype format "
            "(e.g., 'application/json', 'image/png', 'text/html')"
        )

    return value_str


def parse_mime_type_value(value: Any) -> str:
    """Parse MIME type from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"MIME type must be a string, got {type(value).__name__}")

    if not _MIME_TYPE_REGEX.match(value):
        raise GraphQLError(
            f"Invalid MIME type: {value}. Must be type/subtype format "
            "(e.g., 'application/json', 'image/png', 'text/html')"
        )

    return value


def parse_mime_type_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse MIME type from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("MIME type must be a string")

    return parse_mime_type_value(ast.value)


MimeTypeScalar = GraphQLScalarType(
    name="MimeType",
    description=(
        "MIME media type (content type). "
        "Must be in type/subtype format. "
        "Examples: application/json, image/png, text/html, audio/mpeg. "
        "Supports optional suffix with '+' (e.g., application/json+ld). "
        "See: https://en.wikipedia.org/wiki/Media_type"
    ),
    serialize=serialize_mime_type,
    parse_value=parse_mime_type_value,
    parse_literal=parse_mime_type_literal,
)


class MimeTypeField(str, ScalarMarker):
    """MIME media type (content type).

    This scalar validates that the MIME type follows RFC 6838 standard:
    - Format: type/subtype
    - Type and subtype must start with alphanumeric
    - May contain: alphanumeric, !, #, $, &, ^, _, -
    - Optional suffix with '+' for structured types

    Examples:
        >>> from fraiseql.types import MimeType
        >>>
        >>> @fraiseql.type
        ... class File:
        ...     name: str
        ...     content_type: MimeType
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "MimeTypeField":
        """Create a new MimeTypeField instance with validation."""
        if not _MIME_TYPE_REGEX.match(value):
            raise ValueError(
                f"Invalid MIME type: {value}. Must be type/subtype format "
                "(e.g., 'application/json', 'image/png', 'text/html')"
            )
        return super().__new__(cls, value)
