"""Markdown scalar type for markdown content validation."""

from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker


def serialize_markdown(value: Any) -> str | None:
    """Serialize markdown to string."""
    if value is None:
        return None

    # Accept any string as markdown (minimal validation)
    return str(value)


def parse_markdown_value(value: Any) -> str:
    """Parse markdown from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Markdown must be a string, got {type(value).__name__}")

    return value


def parse_markdown_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse markdown from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Markdown must be a string")

    return parse_markdown_value(ast.value)


MarkdownScalar = GraphQLScalarType(
    name="Markdown",
    description=(
        "Markdown formatted text content. "
        "Accepts any string content as markdown. "
        "No validation performed - stores content as-is. "
        "Examples: '# Heading\\n\\nSome **bold** text', 'Normal paragraph'. "
        "Used for rich text content that will be rendered as markdown."
    ),
    serialize=serialize_markdown,
    parse_value=parse_markdown_value,
    parse_literal=parse_markdown_literal,
)


class MarkdownField(str, ScalarMarker):
    r"""Markdown formatted text content.

    This scalar accepts any string as markdown content:
    - No validation performed
    - Content stored as-is
    - Suitable for rich text that will be rendered as markdown

    Examples:
        >>> from fraiseql.types import Markdown
        >>>
        >>> @fraiseql.type
        ... class Article:
        ...     title: str
        ...     content: Markdown  # e.g., "# My Article\\n\\nSome content"
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "MarkdownField":
        """Create a new MarkdownField instance."""
        return super().__new__(cls, value)
