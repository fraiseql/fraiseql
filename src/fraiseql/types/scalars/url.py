"""URL scalar type for HTTP/HTTPS validation."""

import re
from typing import Any
from urllib.parse import urlparse

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# RFC 3986 URL validation with HTTP/HTTPS requirement
_URL_REGEX = re.compile(
    r"^https?://"  # http:// or https://
    r"(?:[A-Z0-9](?:[A-Z0-9-]{0,61}[A-Z0-9])?\.)*"  # domain...
    r"[A-Z0-9](?:[A-Z0-9-]{0,61}[A-Z0-9])?"  # host
    r"(?::[0-9]{1,5})?"  # optional port
    r"(?:/?|[/?][^\s<>\"{}|\\^`]*)$",  # path without dangerous chars
    re.IGNORECASE,
)


def serialize_url(value: Any) -> str | None:
    """Serialize URL to string."""
    if value is None:
        return None

    value_str = str(value)

    # Use urlparse for proper validation
    try:
        parsed = urlparse(value_str)
        if parsed.scheme not in ("http", "https"):
            raise ValueError("Invalid scheme")
        if not parsed.netloc:
            raise ValueError("Missing netloc")
        # Check for dangerous characters
        if any(char in value_str for char in ["<", ">", '"', "{", "}", "|", "\\", "^", "`"]):
            raise ValueError("Dangerous characters")
    except (ValueError, TypeError):
        raise GraphQLError(
            f"Invalid URL: {value}. Must be valid HTTP or HTTPS URL "
            "(e.g., 'https://example.com', 'http://api.example.com/v1/users')"
        )

    return value_str


def parse_url_value(value: Any) -> str:
    """Parse URL from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"URL must be a string, got {type(value).__name__}")

    # Use urlparse for proper validation
    try:
        parsed = urlparse(value)
        if parsed.scheme not in ("http", "https"):
            raise ValueError("Invalid scheme")
        if not parsed.netloc:
            raise ValueError("Missing netloc")
        # Check for dangerous characters
        if any(char in value for char in ["<", ">", '"', "{", "}", "|", "\\", "^", "`"]):
            raise ValueError("Dangerous characters")
    except (ValueError, TypeError):
        raise GraphQLError(
            f"Invalid URL: {value}. Must be valid HTTP or HTTPS URL "
            "(e.g., 'https://example.com', 'http://api.example.com/v1/users')"
        )

    return value


def parse_url_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse URL from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("URL must be a string")

    return parse_url_value(ast.value)


URLScalar = GraphQLScalarType(
    name="URL",
    description=(
        "Valid HTTP or HTTPS URL conforming to RFC 3986. "
        "Must include protocol (http:// or https://). "
        "Examples: https://example.com, http://api.example.com/v1/users. "
        "See: https://tools.ietf.org/html/rfc3986"
    ),
    serialize=serialize_url,
    parse_value=parse_url_value,
    parse_literal=parse_url_literal,
)


class URLField(str, ScalarMarker):
    """Valid HTTP or HTTPS URL.

    This scalar validates that the URL follows RFC 3986 standard:
    - Must start with http:// or https://
    - Valid domain name or IP address
    - Optional port number
    - Optional path, query, and fragment

    Example:
        >>> from fraiseql.types import URL
        >>>
        >>> @fraiseql.type
        ... class Website:
        ...     name: str
        ...     homepage: URL
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "URLField":
        """Create a new URLField instance with validation."""
        # Use urlparse for proper validation
        try:
            parsed = urlparse(value)
            if parsed.scheme not in ("http", "https"):
                raise ValueError("Invalid scheme")
            if not parsed.netloc:
                raise ValueError("Missing netloc")
            # Check for dangerous characters
            if any(char in value for char in ["<", ">", '"', "{", "}", "|", "\\", "^", "`"]):
                raise ValueError("Dangerous characters")
        except (ValueError, TypeError):
            raise ValueError(
                f"Invalid URL: {value}. Must be valid HTTP or HTTPS URL "
                "(e.g., 'https://example.com', 'http://api.example.com/v1/users')"
            )
        return super().__new__(cls, value)
