"""Flight number scalar type for IATA flight number validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# IATA flight number: 2 letters + 1-4 digits + optional letter
_FLIGHT_NUMBER_REGEX = re.compile(r"^[A-Z]{2}[0-9]{1,4}[A-Z]?$")


def serialize_flight_number(value: Any) -> str | None:
    """Serialize flight number to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _FLIGHT_NUMBER_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid flight number: {value}. Must be IATA format: "
            "2 letters + 1-4 digits + optional letter (e.g., 'AA100', 'BA2276', 'DL1234A')"
        )

    return value_str


def parse_flight_number_value(value: Any) -> str:
    """Parse flight number from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Flight number must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _FLIGHT_NUMBER_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid flight number: {value}. Must be IATA format: "
            "2 letters + 1-4 digits + optional letter (e.g., 'AA100', 'BA2276', 'DL1234A')"
        )

    return value_upper


def parse_flight_number_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse flight number from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Flight number must be a string")

    return parse_flight_number_value(ast.value)


FlightNumberScalar = GraphQLScalarType(
    name="FlightNumber",
    description=(
        "IATA flight number. "
        "Format: 2 airline letters + 1-4 flight digits + optional suffix letter. "
        "Examples: AA100, BA2276, LH400, DL1234A. "
        "See: https://en.wikipedia.org/wiki/Flight_number"
    ),
    serialize=serialize_flight_number,
    parse_value=parse_flight_number_value,
    parse_literal=parse_flight_number_literal,
)


class FlightNumberField(str, ScalarMarker):
    """IATA flight number.

    This scalar validates flight numbers according to IATA standards:
    - 2 uppercase letters (airline code)
    - 1-4 digits (flight number)
    - Optional uppercase letter suffix

    Common formats:
    - AA100 (American Airlines flight 100)
    - BA2276 (British Airways flight 2276)
    - LH400 (Lufthansa flight 400)
    - DL1234A (Delta flight 1234A)

    Example:
        >>> from fraiseql.types import FlightNumber
        >>>
        >>> @fraiseql.type
        ... class Flight:
        ...     flight_number: FlightNumber
        ...     airline: str
        ...     departure_time: DateTime
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "FlightNumberField":
        """Create a new FlightNumberField instance with validation."""
        value_upper = value.upper()

        if not _FLIGHT_NUMBER_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid flight number: {value}. Must be IATA format: "
                "2 letters + 1-4 digits + optional letter (e.g., 'AA100', 'BA2276', 'DL1234A')"
            )

        return super().__new__(cls, value_upper)
