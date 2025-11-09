"""HTML scalar type for HTML content validation."""

from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker


def serialize_html(value: Any) -> str | None:
    """Serialize HTML to string."""
    if value is None:
        return None

    # Accept any string as HTML (minimal validation)
    # Optional: could add HTML sanitization here in the future
    return str(value)


def parse_html_value(value: Any) -> str:
    """Parse HTML from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"HTML must be a string, got {type(value).__name__}")

    return value


def parse_html_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse HTML from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("HTML must be a string")

    return parse_html_value(ast.value)


HTMLScalar = GraphQLScalarType(
    name="HTML",
    description=(
        "HTML formatted text content. "
        "Accepts any string content as HTML. "
        "No validation performed - stores content as-is. "
        "Optional HTML sanitization may be added in the future. "
        "Examples: '<p>Hello <strong>world</strong></p>', '<h1>Title</h1>'. "
        "Used for rich text content that will be rendered as HTML."
    ),
    serialize=serialize_html,
    parse_value=parse_html_value,
    parse_literal=parse_html_literal,
)


class HTMLField(str, ScalarMarker):
    """HTML formatted text content.

    This scalar accepts any string as HTML content:
    - No validation performed
    - Content stored as-is
    - Optional HTML sanitization may be added in the future
    - Suitable for rich text that will be rendered as HTML

    Examples:
        >>> from fraiseql.types import HTML
        >>>
        >>> @fraiseql.type
        ... class Page:
        ...     title: str
        ...     content: HTML  # e.g., "<h1>My Page</h1><p>Content</p>"
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "HTMLField":
        """Create a new HTMLField instance."""
        return super().__new__(cls, value)
