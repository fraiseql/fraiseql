"""Tracking number scalar type for shipping tracking validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Shipping tracking number: 8-30 alphanumeric characters
_TRACKING_NUMBER_REGEX = re.compile(r"^[A-Z0-9]{8,30}$")


def serialize_tracking_number(value: Any) -> str | None:
    """Serialize tracking number to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _TRACKING_NUMBER_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid tracking number: {value}. Must be 8-30 alphanumeric characters "
            "(e.g., '1Z999AA10123456784', '123456789012')"
        )

    return value_str


def parse_tracking_number_value(value: Any) -> str:
    """Parse tracking number from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Tracking number must be a string, got {type(value).__name__}")

    if not _TRACKING_NUMBER_REGEX.match(value):
        raise GraphQLError(
            f"Invalid tracking number: {value}. Must be 8-30 alphanumeric characters "
            "(e.g., '1Z999AA10123456784', '123456789012')"
        )

    return value


def parse_tracking_number_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse tracking number from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Tracking number must be a string")

    return parse_tracking_number_value(ast.value)


TrackingNumberScalar = GraphQLScalarType(
    name="TrackingNumber",
    description=(
        "Shipping tracking number. "
        "Valid formats: 8-30 alphanumeric characters. "
        "Examples: 1Z999AA10123456784 (UPS), 123456789012 (FedEx), etc."
    ),
    serialize=serialize_tracking_number,
    parse_value=parse_tracking_number_value,
    parse_literal=parse_tracking_number_literal,
)


class TrackingNumberField(str, ScalarMarker):
    """Shipping tracking number.

    This scalar validates that the tracking number consists of:
    - 8-30 alphanumeric characters (A-Z, 0-9)
    - No spaces or special characters

    Common formats:
    - UPS: 1Z999AA10123456784
    - FedEx: 123456789012
    - DHL: 1234567890
    - USPS: 9400111899223344

    Example:
        >>> from fraiseql.types import TrackingNumber
        >>>
        >>> @fraiseql.type
        ... class Shipment:
        ...     tracking_number: TrackingNumber
        ...     carrier: str
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "TrackingNumberField":
        """Create a new TrackingNumberField instance with validation."""
        if not _TRACKING_NUMBER_REGEX.match(value):
            raise ValueError(
                f"Invalid tracking number: {value}. Must be 8-30 alphanumeric characters "
                "(e.g., '1Z999AA10123456784', '123456789012')"
            )
        return super().__new__(cls, value)
