"""Airport code scalar type for IATA airport code validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# IATA airport code: 3 uppercase letters
_AIRPORT_CODE_REGEX = re.compile(r"^[A-Z]{3}$")


def serialize_airport_code(value: Any) -> str | None:
    """Serialize airport code to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _AIRPORT_CODE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid airport code: {value}. Must be 3 uppercase letters "
            "(e.g., 'LAX', 'JFK', 'LHR')"
        )

    return value_str


def parse_airport_code_value(value: Any) -> str:
    """Parse airport code from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Airport code must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _AIRPORT_CODE_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid airport code: {value}. Must be 3 uppercase letters "
            "(e.g., 'LAX', 'JFK', 'LHR')"
        )

    return value_upper


def parse_airport_code_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse airport code from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Airport code must be a string")

    return parse_airport_code_value(ast.value)


AirportCodeScalar = GraphQLScalarType(
    name="AirportCode",
    description=(
        "IATA airport code (3-letter code). "
        "Must be exactly 3 uppercase letters. "
        "Examples: LAX (Los Angeles), JFK (New York), LHR (London), CDG (Paris). "
        "Case-insensitive (normalized to uppercase). "
        "See: https://en.wikipedia.org/wiki/IATA_airport_code"
    ),
    serialize=serialize_airport_code,
    parse_value=parse_airport_code_value,
    parse_literal=parse_airport_code_literal,
)


class AirportCodeField(str, ScalarMarker):
    """IATA airport code (3-letter code).

    This scalar validates that the airport code follows IATA standard:
    - Exactly 3 uppercase letters
    - Case-insensitive (normalized to uppercase)
    - Used for flight and travel systems

    Examples:
        >>> from fraiseql.types import AirportCode
        >>>
        >>> @fraiseql.type
        ... class Flight:
        ...     number: str
        ...     origin: AirportCode
        ...     destination: AirportCode
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "AirportCodeField":
        """Create a new AirportCodeField instance with validation."""
        value_upper = value.upper()
        if not _AIRPORT_CODE_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid airport code: {value}. Must be 3 uppercase letters "
                "(e.g., 'LAX', 'JFK', 'LHR')"
            )
        return super().__new__(cls, value_upper)
