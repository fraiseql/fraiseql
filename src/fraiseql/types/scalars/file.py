"""File scalar type for file path/URL validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# File path or URL - accepts any extension or no extension
# Basic validation: not empty, reasonable length, no dangerous characters
_FILE_REGEX = re.compile(
    r"^(?:"  # Start of non-capturing group
    r"https?://.+"  # URL with protocol
    r"|"  # OR
    r"(?:\./|/|\.\./|\.\./\.\./)?[^\x00-\x1f\x7f-\x9f<>:\"|?*\x00]+"  # File path
    r")$",
    re.IGNORECASE,
)


def serialize_file(value: Any) -> str | None:
    """Serialize file path/URL to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _FILE_REGEX.match(value_str):
        raise GraphQLError(f"Invalid file: {value}. Must be valid file path or URL")

    return value_str


def parse_file_value(value: Any) -> str:
    """Parse file path/URL from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"File must be a string, got {type(value).__name__}")

    if not _FILE_REGEX.match(value):
        raise GraphQLError(f"Invalid file: {value}. Must be valid file path or URL")

    return value


def parse_file_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse file path/URL from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("File must be a string")

    return parse_file_value(ast.value)


FileScalar = GraphQLScalarType(
    name="File",
    description=(
        "File path or URL for any file type. "
        "Accepts local paths, relative paths, and HTTP/HTTPS URLs. "
        "Examples: /uploads/document.pdf, ./images/photo.jpg, https://example.com/file.zip. "
        "No extension restrictions - accepts any file type."
    ),
    serialize=serialize_file,
    parse_value=parse_file_value,
    parse_literal=parse_file_literal,
)


class FileField(str, ScalarMarker):
    """File path or URL.

    This scalar validates that the value is a valid file path or URL:
    - Local file paths (absolute or relative)
    - HTTP/HTTPS URLs
    - Any file extension or no extension
    - Basic safety checks for invalid characters

    Example:
        >>> from fraiseql.types import File
        >>>
        >>> @fraiseql.type
        ... class Document:
        ...     title: str
        ...     attachment: File
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "FileField":
        """Create a new FileField instance with validation."""
        if not _FILE_REGEX.match(value):
            raise ValueError(f"Invalid file: {value}. Must be valid file path or URL")
        return super().__new__(cls, value)
