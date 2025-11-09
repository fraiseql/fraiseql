"""Color scalar type for hex color code validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Hex color code: #RRGGBB or #RGB, case-insensitive
_COLOR_REGEX = re.compile(r"^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$")


def serialize_color(value: Any) -> str | None:
    """Serialize color to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _COLOR_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid color: {value}. Must be hex color code (e.g., '#FF0000', '#f00', '#3366CC')"
        )

    return value_str


def parse_color_value(value: Any) -> str:
    """Parse color from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Color must be a string, got {type(value).__name__}")

    if not _COLOR_REGEX.match(value):
        raise GraphQLError(
            f"Invalid color: {value}. Must be hex color code (e.g., '#FF0000', '#f00', '#3366CC')"
        )

    return value


def parse_color_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse color from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Color must be a string")

    return parse_color_value(ast.value)


ColorScalar = GraphQLScalarType(
    name="Color",
    description=(
        "Hex color code. "
        "Must start with '#' followed by 3 or 6 hexadecimal digits. "
        "Examples: #FF0000 (red), #f00 (red shorthand), #3366CC (blue). "
        "Case-insensitive. "
        "See: https://en.wikipedia.org/wiki/Web_colors"
    ),
    serialize=serialize_color,
    parse_value=parse_color_value,
    parse_literal=parse_color_literal,
)


class ColorField(str, ScalarMarker):
    """Hex color code.

    This scalar validates that the color is a valid hex color code:
    - Must start with '#'
    - Followed by 3 or 6 hexadecimal digits (0-9, A-F, case-insensitive)
    - 3-digit format: #RGB
    - 6-digit format: #RRGGBB

    Examples:
        >>> from fraiseql.types import Color
        >>>
        >>> @fraiseql.type
        ... class Theme:
        ...     name: str
        ...     primary_color: Color
        ...     secondary_color: Color
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "ColorField":
        """Create a new ColorField instance with validation."""
        if not _COLOR_REGEX.match(value):
            raise ValueError(
                f"Invalid color: {value}. Must be hex color code "
                "(e.g., '#FF0000', '#f00', '#3366CC')"
            )
        return super().__new__(cls, value)
