"""Container number scalar type for ISO 6346 validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# ISO 6346 container number: 3 letters + U/J/Z + 6 digits + check digit
_CONTAINER_NUMBER_REGEX = re.compile(r"^[A-Z]{3}[UJZ][0-9]{6}[0-9]$")

# ISO 6346 character values (A=10, B=12, C=13, ..., Z=38)
# Numbers 0-9 have their face value
_CHAR_VALUES = {
    "0": 0,
    "1": 1,
    "2": 2,
    "3": 3,
    "4": 4,
    "5": 5,
    "6": 6,
    "7": 7,
    "8": 8,
    "9": 9,
    "A": 10,
    "B": 12,
    "C": 13,
    "D": 14,
    "E": 15,
    "F": 16,
    "G": 17,
    "H": 18,
    "I": 19,
    "J": 20,
    "K": 21,
    "L": 23,
    "M": 24,
    "N": 25,
    "O": 26,
    "P": 27,
    "Q": 28,
    "R": 29,
    "S": 30,
    "T": 31,
    "U": 32,
    "V": 34,
    "W": 35,
    "X": 36,
    "Y": 37,
    "Z": 38,
}


def _calculate_iso6346_check_digit(container: str) -> int:
    """Calculate ISO 6346 check digit for container number (first 10 characters)."""
    if len(container) != 11:
        raise ValueError("Container number must be 11 characters for check digit calculation")

    # Sum weighted values: positions 1-10 from left to right
    total = 0
    for i, char in enumerate(container[:10]):  # First 10 characters
        char_value = _CHAR_VALUES.get(char.upper())
        if char_value is None:
            raise ValueError(f"Invalid character '{char}' in container number")

        # Weight is 2^(i) since position 1 (i=0) has weight 2^0 = 1
        weight = 2**i
        total += char_value * weight

    # Check digit = (total % 11) % 10
    check_digit = (total % 11) % 10
    return check_digit


def _validate_container_check_digit(container: str) -> bool:
    """Validate that the check digit matches the calculated value."""
    if len(container) != 11:
        return False

    try:
        calculated = _calculate_iso6346_check_digit(container)
        provided = int(container[10])  # Last character is check digit
        return calculated == provided
    except (ValueError, IndexError):
        return False


def serialize_container_number(value: Any) -> str | None:
    """Serialize container number to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _CONTAINER_NUMBER_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid container number: {value}. Must be ISO 6346 format: "
            "3 letters + U/J/Z + 6 digits + check digit (e.g., 'CSQU3054383')"
        )

    if not _validate_container_check_digit(value_str):
        raise GraphQLError(
            f"Invalid container number: {value}. Check digit does not match "
            "(ISO 6346 validation failed)"
        )

    return value_str


def parse_container_number_value(value: Any) -> str:
    """Parse container number from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Container number must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _CONTAINER_NUMBER_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid container number: {value}. Must be ISO 6346 format: "
            "3 letters + U/J/Z + 6 digits + check digit (e.g., 'CSQU3054383')"
        )

    if not _validate_container_check_digit(value_upper):
        raise GraphQLError(
            f"Invalid container number: {value}. Check digit does not match "
            "(ISO 6346 validation failed)"
        )

    return value_upper


def parse_container_number_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse container number from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Container number must be a string")

    return parse_container_number_value(ast.value)


ContainerNumberScalar = GraphQLScalarType(
    name="ContainerNumber",
    description=(
        "ISO 6346 shipping container number. "
        "Format: 3 owner code letters + equipment category (U/J/Z) + "
        "6 serial digits + check digit. "
        "Examples: CSQU3054383, MSKU1234567, TCLU1234560. "
        "See: https://en.wikipedia.org/wiki/ISO_6346"
    ),
    serialize=serialize_container_number,
    parse_value=parse_container_number_value,
    parse_literal=parse_container_number_literal,
)


class ContainerNumberField(str, ScalarMarker):
    """ISO 6346 shipping container number.

    This scalar validates shipping container numbers according to ISO 6346:
    - 3 letters (owner code)
    - 1 letter (equipment category: U/J/Z)
    - 6 digits (serial number)
    - 1 digit (check digit, calculated using ISO 6346 algorithm)

    The check digit ensures data integrity and prevents transcription errors.

    Examples:
    - CSQU3054383
    - MSKU1234567
    - TCLU1234560

    Example:
        >>> from fraiseql.types import ContainerNumber
        >>>
        >>> @fraiseql.type
        ... class ShippingContainer:
        ...     container_number: ContainerNumber
        ...     contents: str
        ...     weight_kg: float
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "ContainerNumberField":
        """Create a new ContainerNumberField instance with validation."""
        value_upper = value.upper()

        if not _CONTAINER_NUMBER_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid container number: {value}. Must be ISO 6346 format: "
                "3 letters + U/J/Z + 6 digits + check digit (e.g., 'CSQU3054383')"
            )

        if not _validate_container_check_digit(value_upper):
            raise ValueError(
                f"Invalid container number: {value}. Check digit does not match "
                "(ISO 6346 validation failed)"
            )

        return super().__new__(cls, value_upper)
