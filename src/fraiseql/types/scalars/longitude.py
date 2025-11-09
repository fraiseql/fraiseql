"""Longitude scalar type for geographic coordinate validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Longitude: -180 to 180, up to 8 decimal places
_LONGITUDE_REGEX = re.compile(r"^-?180(?:\.0{1,8})?$|^-?(?:1[0-7]|[1-9])?\d(?:\.\d{1,8})?$")


def serialize_longitude(value: Any) -> str | None:
    """Serialize longitude to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _LONGITUDE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid longitude: {value}. Must be between -180 and 180 "
            "(e.g., '-74.0060', '151.2093', '0.0')"
        )

    return value_str


def parse_longitude_value(value: Any) -> str:
    """Parse longitude from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Longitude must be a string, got {type(value).__name__}")

    if not _LONGITUDE_REGEX.match(value):
        raise GraphQLError(
            f"Invalid longitude: {value}. Must be between -180 and 180 "
            "(e.g., '-74.0060', '151.2093', '0.0')"
        )

    return value


def parse_longitude_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse longitude from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Longitude must be a string")

    return parse_longitude_value(ast.value)


LongitudeScalar = GraphQLScalarType(
    name="Longitude",
    description=(
        "Geographic longitude coordinate. "
        "Must be between -180 and 180 degrees. "
        "Supports up to 8 decimal places. "
        "Examples: -74.0060 (New York), 151.2093 (Sydney), 0.0 (Prime Meridian). "
        "Used for geographic positioning and mapping."
    ),
    serialize=serialize_longitude,
    parse_value=parse_longitude_value,
    parse_literal=parse_longitude_literal,
)


class LongitudeField(str, ScalarMarker):
    """Geographic longitude coordinate.

    This scalar validates that the longitude is within valid geographic bounds:
    - Range: -180.0 to 180.0 degrees
    - Supports up to 8 decimal places for precision
    - East positive, West negative

    Examples:
        >>> from fraiseql.types import Longitude
        >>>
        >>> @fraiseql.type
        ... class Location:
        ...     name: str
        ...     lat: Latitude
        ...     lng: Longitude
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "LongitudeField":
        """Create a new LongitudeField instance with validation."""
        if not _LONGITUDE_REGEX.match(value):
            raise ValueError(
                f"Invalid longitude: {value}. Must be between -180 and 180 "
                "(e.g., '-74.0060', '151.2093', '0.0')"
            )
        return super().__new__(cls, value)
