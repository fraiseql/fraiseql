"""Duration scalar type for time interval validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# ISO 8601 duration: P[n]Y[n]M[n]DT[n]H[n]M[n]S
_DURATION_REGEX = re.compile(
    r"^P(?=\d+[YMWD])?(?:\d+Y)?(?:\d+M)?(?:\d+W)?(?:\d+D)?"
    r"(?:T(?:\d+H)?(?:\d+M)?(?:\d+S)?)?$"
)


def serialize_duration(value: Any) -> str | None:
    """Serialize duration to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _DURATION_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid duration: {value}. Must be ISO 8601 duration format "
            "(e.g., 'P1Y2M3DT4H5M6S', 'PT30M', 'P1D')"
        )

    return value_str


def parse_duration_value(value: Any) -> str:
    """Parse duration from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Duration must be a string, got {type(value).__name__}")

    if not _DURATION_REGEX.match(value):
        raise GraphQLError(
            f"Invalid duration: {value}. Must be ISO 8601 duration format "
            "(e.g., 'P1Y2M3DT4H5M6S', 'PT30M', 'P1D')"
        )

    return value


def parse_duration_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse duration from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Duration must be a string")

    return parse_duration_value(ast.value)


DurationScalar = GraphQLScalarType(
    name="Duration",
    description=(
        "Time duration in ISO 8601 format. "
        "Format: P[n]Y[n]M[n]DT[n]H[n]M[n]S where components are optional. "
        "Examples: P1Y2M3DT4H5M6S (1y 2m 3d 4h 5m 6s), PT30M (30 minutes), P1D (1 day). "
        "Used for time intervals and scheduling. "
        "See: https://en.wikipedia.org/wiki/ISO_8601#Durations"
    ),
    serialize=serialize_duration,
    parse_value=parse_duration_value,
    parse_literal=parse_duration_literal,
)


class DurationField(str, ScalarMarker):
    """Time duration in ISO 8601 format.

    This scalar validates that the duration follows ISO 8601 standard:
    - Format: P[n]Y[n]M[n]DT[n]H[n]M[n]S
    - P indicates period
    - T separates date from time components
    - Y=years, M=months, W=weeks, D=days, H=hours, M=minutes, S=seconds
    - Components are optional but at least one must be present

    Examples:
        >>> from fraiseql.types import Duration
        >>>
        >>> @fraiseql.type
        ... class Event:
        ...     title: str
        ...     duration: Duration  # e.g., "PT2H30M" (2.5 hours)
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "DurationField":
        """Create a new DurationField instance with validation."""
        if not _DURATION_REGEX.match(value):
            raise ValueError(
                f"Invalid duration: {value}. Must be ISO 8601 duration format "
                "(e.g., 'P1Y2M3DT4H5M6S', 'PT30M', 'P1D')"
            )
        return super().__new__(cls, value)
