"""Time scalar type for time of day validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Time: HH:MM:SS or HH:MM format, 00:00:00 to 23:59:59
_TIME_REGEX = re.compile(r"^(?:[01]\d|2[0-3]):[0-5]\d(?::[0-5]\d)?$")


def serialize_time(value: Any) -> str | None:
    """Serialize time to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _TIME_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid time: {value}. Must be HH:MM or HH:MM:SS format "
            "(e.g., '14:30', '09:15:30', '23:59')"
        )

    return value_str


def parse_time_value(value: Any) -> str:
    """Parse time from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Time must be a string, got {type(value).__name__}")

    if not _TIME_REGEX.match(value):
        raise GraphQLError(
            f"Invalid time: {value}. Must be HH:MM or HH:MM:SS format "
            "(e.g., '14:30', '09:15:30', '23:59')"
        )

    return value


def parse_time_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse time from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Time must be a string")

    return parse_time_value(ast.value)


TimeScalar = GraphQLScalarType(
    name="Time",
    description=(
        "Time of day in 24-hour format. "
        "Must be HH:MM or HH:MM:SS format. "
        "Hours: 00-23, Minutes: 00-59, Seconds: 00-59. "
        "Examples: 14:30, 09:15:30, 23:59, 00:00:00. "
        "Used for scheduling and time-based operations."
    ),
    serialize=serialize_time,
    parse_value=parse_time_value,
    parse_literal=parse_time_literal,
)


class TimeField(str, ScalarMarker):
    """Time of day in 24-hour format.

    This scalar validates that the time follows 24-hour format:
    - HH:MM or HH:MM:SS format
    - Hours: 00-23
    - Minutes: 00-59
    - Seconds: 00-59 (optional)

    Examples:
        >>> from fraiseql.types import Time
        >>>
        >>> @fraiseql.type
        ... class Schedule:
        ...     event: str
        ...     start_time: Time
        ...     end_time: Time
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "TimeField":
        """Create a new TimeField instance with validation."""
        if not _TIME_REGEX.match(value):
            raise ValueError(
                f"Invalid time: {value}. Must be HH:MM or HH:MM:SS format "
                "(e.g., '14:30', '09:15:30', '23:59')"
            )
        return super().__new__(cls, value)
