"""Image scalar type for image file validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Image file extensions (common web formats)
_IMAGE_EXTENSIONS = {".jpg", ".jpeg", ".png", ".gif", ".webp", ".svg", ".bmp", ".tiff", ".tif"}

# URL or file path with image extension
_IMAGE_REGEX = re.compile(
    r"^(?:"  # Start of non-capturing group for URL or path
    r"https?://.*\.(?:"
    + "|".join(ext[1:] for ext in _IMAGE_EXTENSIONS)
    + r")"  # URL with extension
    r"|"  # OR
    r".*\.(?:" + "|".join(ext[1:] for ext in _IMAGE_EXTENSIONS) + r")"  # Path with extension
    r")$",
    re.IGNORECASE,
)


def serialize_image(value: Any) -> str | None:
    """Serialize image URL/path to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _IMAGE_REGEX.match(value_str):
        extensions_str = ", ".join(sorted(_IMAGE_EXTENSIONS))
        raise GraphQLError(
            f"Invalid image: {value}. Must be URL or path with image extension ({extensions_str})"
        )

    return value_str


def parse_image_value(value: Any) -> str:
    """Parse image URL/path from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Image must be a string, got {type(value).__name__}")

    if not _IMAGE_REGEX.match(value):
        extensions_str = ", ".join(sorted(_IMAGE_EXTENSIONS))
        raise GraphQLError(
            f"Invalid image: {value}. Must be URL or path with image extension ({extensions_str})"
        )

    return value


def parse_image_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse image URL/path from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Image must be a string")

    return parse_image_value(ast.value)


ImageScalar = GraphQLScalarType(
    name="Image",
    description=(
        "Image file URL or path with valid image extension. "
        "Supported formats: jpg, jpeg, png, gif, webp, svg, bmp, tiff, tif. "
        "Examples: https://example.com/image.jpg, /uploads/avatar.png, image.svg. "
        "Case-insensitive extension matching."
    ),
    serialize=serialize_image,
    parse_value=parse_image_value,
    parse_literal=parse_image_literal,
)


class ImageField(str, ScalarMarker):
    """Image file URL or path.

    This scalar validates that the value is a URL or file path
    with a valid image file extension:
    - jpg, jpeg, png, gif, webp, svg, bmp, tiff, tif
    - Case-insensitive matching
    - Works with both URLs and file paths

    Example:
        >>> from fraiseql.types import Image
        >>>
        >>> @fraiseql.type
        ... class Profile:
        ...     name: str
        ...     avatar: Image
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "ImageField":
        """Create a new ImageField instance with validation."""
        if not _IMAGE_REGEX.match(value):
            extensions_str = ", ".join(sorted(_IMAGE_EXTENSIONS))
            raise ValueError(
                f"Invalid image: {value}. Must be URL or path with image extension "
                f"({extensions_str})"
            )
        return super().__new__(cls, value)
