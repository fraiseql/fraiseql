"""VIN scalar type for ISO 3779/3780 vehicle identification number validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# ISO 3779/3780 VIN: 17 characters, no I, O, Q
_VIN_REGEX = re.compile(r"^[A-HJ-NPR-Z0-9]{17}$")

# VIN character values (transliteration)
_VIN_CHAR_VALUES = {
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
    "A": 1,
    "B": 2,
    "C": 3,
    "D": 4,
    "E": 5,
    "F": 6,
    "G": 7,
    "H": 8,
    "I": 9,
    "J": 1,
    "K": 2,
    "L": 3,
    "M": 4,
    "N": 5,
    "O": 0,
    "P": 7,
    "Q": 0,
    "R": 9,
    "S": 2,
    "T": 3,
    "U": 4,
    "V": 5,
    "W": 6,
    "X": 7,
    "Y": 8,
    "Z": 9,
}

# VIN position weights (1-indexed positions 1-17)
_VIN_WEIGHTS = [8, 7, 6, 5, 4, 3, 2, 10, 0, 9, 8, 7, 6, 5, 4, 3, 2]


def _calculate_vin_check_digit(vin: str) -> str:
    """Calculate VIN check digit for positions 1-17."""
    if len(vin) != 17:
        raise ValueError("VIN must be 17 characters for check digit calculation")

    total = 0
    for i in range(17):
        char = vin[i]
        value = _VIN_CHAR_VALUES.get(char.upper())
        if value is None:
            raise ValueError(f"Invalid character '{char}' in VIN")
        total += value * _VIN_WEIGHTS[i]

    remainder = total % 11
    if remainder == 10:
        return "X"
    return str(remainder)


def _validate_vin_check_digit(vin: str) -> bool:
    """Validate that the check digit at position 9 matches the calculated value."""
    if len(vin) != 17:
        return False

    try:
        calculated = _calculate_vin_check_digit(vin)
        provided = vin[8]  # Position 9 (0-indexed position 8)
        return calculated.upper() == provided.upper()
    except (ValueError, IndexError):
        return False


def serialize_vin(value: Any) -> str | None:
    """Serialize VIN to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _VIN_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid VIN: {value}. Must be 17 characters (A-H, J-N, P, R-Z, 0-9), "
            "no I, O, or Q allowed"
        )

    if not _validate_vin_check_digit(value_str):
        raise GraphQLError(
            f"Invalid VIN: {value}. Check digit does not match (ISO 3779/3780 validation failed)"
        )

    return value_str


def parse_vin_value(value: Any) -> str:
    """Parse VIN from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"VIN must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _VIN_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid VIN: {value}. Must be 17 characters (A-H, J-N, P, R-Z, 0-9), "
            "no I, O, or Q allowed"
        )

    if not _validate_vin_check_digit(value_upper):
        raise GraphQLError(
            f"Invalid VIN: {value}. Check digit does not match (ISO 3779/3780 validation failed)"
        )

    return value_upper


def parse_vin_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse VIN from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("VIN must be a string")

    return parse_vin_value(ast.value)


VINScalar = GraphQLScalarType(
    name="VIN",
    description=(
        "Vehicle Identification Number (ISO 3779/3780). "
        "Format: 17 characters (A-H, J-N, P, R-Z, 0-9), no I, O, or Q. "
        "Includes check digit validation. "
        "Examples: 1HGBH41JXMN109186, JH4KA8260MC000000"
    ),
    serialize=serialize_vin,
    parse_value=parse_vin_value,
    parse_literal=parse_vin_literal,
)


class VINField(str, ScalarMarker):
    """Vehicle Identification Number (ISO 3779/3780).

    This scalar validates vehicle identification numbers according to ISO 3779/3780:
    - Exactly 17 characters
    - Allowed characters: A-H, J-N, P, R-Z, 0-9
    - Forbidden characters: I, O, Q
    - Check digit validation at position 9

    The check digit ensures data integrity and prevents transcription errors.

    Examples:
    - 1HGBH41JXMN109186 (Honda Civic)
    - JH4KA8260MC000000 (Acura)

    Example:
        >>> from fraiseql.types import VIN
        >>>
        >>> @fraiseql.type
        ... class Vehicle:
        ...     vin: VIN
        ...     make: str
        ...     model: str
        ...     year: int
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "VINField":
        """Create a new VINField instance with validation."""
        value_upper = value.upper()

        if not _VIN_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid VIN: {value}. Must be 17 characters (A-H, J-N, P, R-Z, 0-9), "
                "no I, O, or Q allowed"
            )

        if not _validate_vin_check_digit(value_upper):
            raise ValueError(
                f"Invalid VIN: {value}. Check digit does not match "
                "(ISO 3779/3780 validation failed)"
            )

        return super().__new__(cls, value_upper)
