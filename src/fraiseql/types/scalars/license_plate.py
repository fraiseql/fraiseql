"""License plate scalar type for vehicle license plate validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# International license plate: 2-12 alphanumeric characters with spaces/hyphens
_LICENSE_PLATE_REGEX = re.compile(r"^[A-Z0-9 -]{2,12}$")


def serialize_license_plate(value: Any) -> str | None:
    """Serialize license plate to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _LICENSE_PLATE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid license plate: {value}. Must be 2-12 alphanumeric characters "
            "(A-Z, 0-9, spaces, hyphens only)"
        )

    return value_str


def parse_license_plate_value(value: Any) -> str:
    """Parse license plate from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"License plate must be a string, got {type(value).__name__}")

    if not _LICENSE_PLATE_REGEX.match(value):
        raise GraphQLError(
            f"Invalid license plate: {value}. Must be 2-12 alphanumeric characters "
            "(A-Z, 0-9, spaces, hyphens only)"
        )

    return value


def parse_license_plate_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse license plate from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("License plate must be a string")

    return parse_license_plate_value(ast.value)


LicensePlateScalar = GraphQLScalarType(
    name="LicensePlate",
    description=(
        "Vehicle license plate number. "
        "Valid formats: 2-12 alphanumeric characters (A-Z, 0-9) with optional spaces and hyphens. "
        "Examples: ABC123, NY 1234 AB, ABC-1234"
    ),
    serialize=serialize_license_plate,
    parse_value=parse_license_plate_value,
    parse_literal=parse_license_plate_literal,
)


class LicensePlateField(str, ScalarMarker):
    """Vehicle license plate number.

    This scalar validates international vehicle license plate formats:
    - 2-12 characters total
    - Alphanumeric characters (A-Z, 0-9)
    - Spaces and hyphens allowed
    - No other special characters

    Common formats:
    - US: ABC123, ABC 123
    - EU: AB-123-CD, AB 123 CD
    - International variations

    Example:
        >>> from fraiseql.types import LicensePlate
        >>>
        >>> @fraiseql.type
        ... class Vehicle:
        ...     license_plate: LicensePlate
        ...     make: str
        ...     model: str
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "LicensePlateField":
        """Create a new LicensePlateField instance with validation."""
        if not _LICENSE_PLATE_REGEX.match(value):
            raise ValueError(
                f"Invalid license plate: {value}. Must be 2-12 alphanumeric characters "
                "(A-Z, 0-9, spaces, hyphens only)"
            )
        return super().__new__(cls, value)
