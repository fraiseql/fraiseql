"""MAC address scalar type for hardware address validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# MAC address formats:
# - Standard: XX:XX:XX:XX:XX:XX (colons)
# - Cisco: XXXX.XXXX.XXXX (dots)
# - Microsoft: XX-XX-XX-XX-XX-XX (hyphens)
# - Bare: XXXXXXXXXXXX (no separators)
_MAC_PATTERNS = [
    # Colon-separated (most common)
    re.compile(r"^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$"),
    # Hyphen-separated (Windows)
    re.compile(r"^([0-9A-Fa-f]{2}-){5}[0-9A-Fa-f]{2}$"),
    # Dot-separated (Cisco)
    re.compile(r"^([0-9A-Fa-f]{4}\.){2}[0-9A-Fa-f]{4}$"),
    # No separators
    re.compile(r"^[0-9A-Fa-f]{12}$"),
]


def normalize_mac_address(mac: str) -> str:
    """Normalize MAC address to colon-separated format."""
    # Remove all separators
    mac_clean = mac.upper().replace(":", "").replace("-", "").replace(".", "")

    # Validate it's exactly 12 hex characters
    if len(mac_clean) != 12 or not all(c in "0123456789ABCDEF" for c in mac_clean):
        raise ValueError(f"Invalid MAC address format: {mac}")

    # Format as XX:XX:XX:XX:XX:XX
    return ":".join(mac_clean[i : i + 2] for i in range(0, 12, 2))


def serialize_mac_address(value: Any) -> str | None:
    """Serialize MAC address to normalized string format."""
    if value is None:
        return None

    value_str = str(value)

    # Check if it matches any valid pattern
    if not any(pattern.match(value_str) for pattern in _MAC_PATTERNS):
        raise GraphQLError(
            f"Invalid MAC address: {value}. Expected format like "
            f"'00:11:22:33:44:55' or '0011.2233.4455'"
        )

    try:
        return normalize_mac_address(value_str)
    except ValueError as e:
        raise GraphQLError(str(e))


def parse_mac_address_value(value: Any) -> str:
    """Parse MAC address from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"MAC address must be a string, got {type(value).__name__}")

    # Check if it matches any valid pattern
    if not any(pattern.match(value) for pattern in _MAC_PATTERNS):
        raise GraphQLError(
            f"Invalid MAC address: {value}. Expected format like "
            f"'00:11:22:33:44:55' or '0011.2233.4455'"
        )

    try:
        return normalize_mac_address(value)
    except ValueError as e:
        raise GraphQLError(str(e))


def parse_mac_address_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse MAC address from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("MAC address must be a string")

    return parse_mac_address_value(ast.value)


MacAddressScalar = GraphQLScalarType(
    name="MacAddress",
    description=(
        "A MAC (Media Access Control) address in any common format. "
        "Normalized to uppercase colon-separated format (XX:XX:XX:XX:XX:XX). "
        "Accepts: '00:11:22:33:44:55', '00-11-22-33-44-55', '0011.2233.4455', '001122334455'"
    ),
    serialize=serialize_mac_address,
    parse_value=parse_mac_address_value,
    parse_literal=parse_mac_address_literal,
)


class MacAddressField(str, ScalarMarker):
    """Hardware MAC address.

    This scalar validates and normalizes MAC addresses to a consistent format.

    Input formats accepted:
    - Colon-separated: 00:11:22:33:44:55
    - Hyphen-separated: 00-11-22-33-44-55
    - Dot-separated (Cisco): 0011.2233.4455
    - No separators: 001122334455

    All formats are normalized to uppercase colon-separated format.

    Example:
        >>> from fraiseql.types import MacAddress
        >>>
        >>> @fraiseql.input
        ... class NetworkInterface:
        ...     name: str
        ...     mac_address: MacAddress
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "MacAddressField":
        """Create a new MacAddressField instance with validation."""
        # Validate format
        if not any(pattern.match(value) for pattern in _MAC_PATTERNS):
            raise ValueError(
                f"Invalid MAC address: {value}. Expected format like "
                f"'00:11:22:33:44:55' or '0011.2233.4455'"
            )

        # Normalize to colon-separated format
        try:
            normalized = normalize_mac_address(value)
            return super().__new__(cls, normalized)
        except ValueError as e:
            raise ValueError(str(e))
