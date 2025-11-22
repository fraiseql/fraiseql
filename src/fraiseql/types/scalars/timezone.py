"""Timezone scalar type for IANA timezone identifier validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# IANA timezone database format: Region/City or Region/City/Locality
# Examples: America/New_York, Europe/Paris, Asia/Tokyo, Pacific/Auckland
_TIMEZONE_REGEX = re.compile(r"^[A-Z][a-z]+[a-zA-Z_]*(/[A-Z][a-z]+[a-zA-Z_]*){1,2}$")


def serialize_timezone(value: Any) -> str | None:
    """Serialize timezone to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _TIMEZONE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid timezone: {value}. Must be IANA timezone identifier "
            "(e.g., 'America/New_York', 'Europe/Paris')"
        )

    return value_str


def parse_timezone_value(value: Any) -> str:
    """Parse timezone from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Timezone must be a string, got {type(value).__name__}")

    if not _TIMEZONE_REGEX.match(value):
        raise GraphQLError(
            f"Invalid timezone: {value}. Must be IANA timezone identifier "
            "(e.g., 'America/New_York', 'Europe/Paris')"
        )

    return value


def parse_timezone_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse timezone from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Timezone must be a string")

    return parse_timezone_value(ast.value)


TimezoneScalar = GraphQLScalarType(
    name="Timezone",
    description=(
        "IANA timezone database identifier. "
        "Format: Region/City or Region/City/Locality. "
        "Examples: America/New_York, Europe/Paris, Asia/Tokyo, Pacific/Auckland. "
        "See: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones"
    ),
    serialize=serialize_timezone,
    parse_value=parse_timezone_value,
    parse_literal=parse_timezone_literal,
)


class TimezoneField(str, ScalarMarker):
    """IANA timezone identifier for timezone-aware applications.

    This scalar validates timezone identifiers from the IANA timezone database:
    - Format: Region/City (e.g., America/New_York, Europe/Paris)
    - Case-sensitive (standard capitalization required)
    - Handles daylight saving time transitions correctly
    - Better than UTC offsets (which don't handle DST)

    Example:
        >>> from fraiseql.types import Timezone
        >>>
        >>> @fraiseql.type
        ... class User:
        ...     timezone: Timezone
        ...
        >>> @fraiseql.input
        ... class ScheduleEvent:
        ...     start_time: DateTime
        ...     timezone: Timezone  # for display in user's local time
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "TimezoneField":
        """Create a new TimezoneField instance with validation."""
        if not _TIMEZONE_REGEX.match(value):
            raise ValueError(
                f"Invalid timezone: {value}. Must be IANA timezone identifier "
                "(e.g., 'America/New_York', 'Europe/Paris')"
            )
        return super().__new__(cls, value)
