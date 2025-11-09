"""Latitude scalar type for geographic coordinate validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Latitude: -90 to 90, up to 8 decimal places
_LATITUDE_REGEX = re.compile(r"^-?90(?:\.0{1,8})?$|^-?[1-8]?\d(?:\.\d{1,8})?$")


def serialize_latitude(value: Any) -> str | None:
    """Serialize latitude to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _LATITUDE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid latitude: {value}. Must be between -90 and 90 "
            "(e.g., '40.7128', '-33.8688', '0.0')"
        )

    return value_str


def parse_latitude_value(value: Any) -> str:
    """Parse latitude from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Latitude must be a string, got {type(value).__name__}")

    if not _LATITUDE_REGEX.match(value):
        raise GraphQLError(
            f"Invalid latitude: {value}. Must be between -90 and 90 "
            "(e.g., '40.7128', '-33.8688', '0.0')"
        )

    return value


def parse_latitude_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse latitude from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Latitude must be a string")

    return parse_latitude_value(ast.value)


LatitudeScalar = GraphQLScalarType(
    name="Latitude",
    description=(
        "Geographic latitude coordinate. "
        "Must be between -90 and 90 degrees. "
        "Supports up to 8 decimal places. "
        "Examples: 40.7128 (New York), -33.8688 (Sydney), 0.0 (Equator). "
        "Used for geographic positioning and mapping."
    ),
    serialize=serialize_latitude,
    parse_value=parse_latitude_value,
    parse_literal=parse_latitude_literal,
)


class LatitudeField(str, ScalarMarker):
    """Geographic latitude coordinate.

    This scalar validates that the latitude is within valid geographic bounds:
    - Range: -90.0 to 90.0 degrees
    - Supports up to 8 decimal places for precision
    - North positive, South negative

    Examples:
        >>> from fraiseql.types import Latitude
        >>>
        >>> @fraiseql.type
        ... class Location:
        ...     name: str
        ...     lat: Latitude
        ...     lng: Longitude
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "LatitudeField":
        """Create a new LatitudeField instance with validation."""
        if not _LATITUDE_REGEX.match(value):
            raise ValueError(
                f"Invalid latitude: {value}. Must be between -90 and 90 "
                "(e.g., '40.7128', '-33.8688', '0.0')"
            )
        return super().__new__(cls, value)
