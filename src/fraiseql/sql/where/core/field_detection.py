"""Field type detection for where clause generation.

This module provides clean, testable functions to detect what type of field
we're dealing with based on field names, values, and type hints.
"""

import re
from enum import Enum
from typing import Any


class FieldType(Enum):
    """Enumeration of field types for where clause generation."""

    ANY = "any"
    STRING = "string"
    INTEGER = "integer"
    FLOAT = "float"
    BOOLEAN = "boolean"
    UUID = "uuid"
    DATE = "date"
    DATETIME = "datetime"
    IP_ADDRESS = "ip_address"
    MAC_ADDRESS = "mac_address"
    LTREE = "ltree"
    DATE_RANGE = "date_range"
    HOSTNAME = "hostname"
    EMAIL = "email"
    PORT = "port"

    def is_ip_address(self) -> bool:
        """Check if this field type is IP address."""
        return self == FieldType.IP_ADDRESS

    @classmethod
    def from_python_type(cls, python_type: type) -> "FieldType":
        """Convert Python type to FieldType."""
        # Try to detect FraiseQL scalar types
        try:
            from fraiseql.types.scalars.ip_address import IpAddressField

            if python_type == IpAddressField or (
                isinstance(python_type, type) and issubclass(python_type, IpAddressField)
            ):
                return cls.IP_ADDRESS
        except ImportError:
            pass

        # Try to detect other FraiseQL scalar types
        try:
            from fraiseql.types import CIDR, IpAddress, LTree, MacAddress
            from fraiseql.types.scalars.daterange import DateRangeField

            type_mapping = {
                IpAddress: cls.IP_ADDRESS,
                CIDR: cls.IP_ADDRESS,
                MacAddress: cls.MAC_ADDRESS,
                LTree: cls.LTREE,
                DateRangeField: cls.DATE_RANGE,
            }

            if python_type in type_mapping:
                return type_mapping[python_type]
        except ImportError:
            pass

        # Standard Python types
        from datetime import date, datetime
        from decimal import Decimal
        from uuid import UUID

        type_mapping = {
            str: cls.STRING,
            int: cls.INTEGER,
            float: cls.FLOAT,
            Decimal: cls.FLOAT,
            bool: cls.BOOLEAN,
            UUID: cls.UUID,
            date: cls.DATE,
            datetime: cls.DATETIME,
        }

        return type_mapping.get(python_type, cls.STRING)

    @classmethod
    def from_value(cls, value: Any) -> "FieldType":
        """Detect field type from a value."""
        if value is None:
            return cls.ANY

        if isinstance(value, bool):
            return cls.BOOLEAN

        if isinstance(value, int):
            return cls.INTEGER

        if isinstance(value, float):
            return cls.FLOAT

        if isinstance(value, str):
            # Check for IP address patterns
            if _is_ip_address_value(value):
                return cls.IP_ADDRESS

            # Check for MAC address patterns
            if _is_mac_address_value(value):
                return cls.MAC_ADDRESS

            # Check for LTree patterns
            if _is_ltree_value(value):
                return cls.LTREE

            # Check for DateRange patterns
            if _is_daterange_value(value):
                return cls.DATE_RANGE

            return cls.STRING

        if isinstance(value, list):
            # For lists, detect based on first non-None item
            for item in value:
                if item is not None:
                    return cls.from_value(item)
            return cls.ANY

        return cls.ANY


def detect_field_type(field_name: str, value: Any, field_type: type | None = None) -> FieldType:
    """Detect the type of field for where clause generation.

    Args:
        field_name: The name of the field (camelCase or snake_case)
        value: The value being filtered on
        field_type: Optional Python type hint

    Returns:
        FieldType enum indicating what type of field this is
    """
    # First priority: explicit type hint
    if field_type is not None:
        return FieldType.from_python_type(field_type)

    # Second priority: field name patterns
    field_type_from_name = _detect_field_type_from_name(field_name)
    if field_type_from_name != FieldType.ANY:
        return field_type_from_name

    # Third priority: value analysis
    return FieldType.from_value(value)


def _detect_field_type_from_name(field_name: str) -> FieldType:
    """Detect field type from field name patterns."""
    if not field_name:
        return FieldType.ANY

    field_lower = field_name.lower()

    # IP address patterns - handle both snake_case and camelCase
    ip_patterns = [
        "ip_address",
        "ipaddress",
        "server_ip",
        "gateway_ip",
        "host_ip",
        "serverip",
        "gatewayip",
        "hostip",
    ]

    # Check pattern matches
    if any(pattern in field_lower for pattern in ip_patterns):
        return FieldType.IP_ADDRESS

    # Additional IP patterns that should be whole words or at start/end
    if (
        field_lower in ["ip", "host"]
        or field_lower.endswith(("_ip", "ip"))
        or field_lower.startswith(("ip_", "ip"))
    ):
        return FieldType.IP_ADDRESS

    # MAC address patterns - handle both snake_case and camelCase
    mac_patterns = [
        "mac_address",
        "macaddress",
        "device_mac",
        "mac_addr",
        "hardware_address",
        "devicemac",
        "macaddr",
        "hardwareaddress",
    ]

    # Check MAC pattern matches
    if any(pattern in field_lower for pattern in mac_patterns):
        return FieldType.MAC_ADDRESS

    # Additional MAC patterns that should be whole words or at start/end
    if (
        field_lower in ["mac"]
        or field_lower.endswith(("_mac", "mac"))
        or field_lower.startswith(("mac_", "mac"))
    ):
        return FieldType.MAC_ADDRESS

    # LTree path patterns - handle both snake_case and camelCase
    ltree_patterns = [
        "category_path",
        "categorypath",
        "navigation_path",
        "navigationpath",
        "tree_path",
        "treepath",
        "hierarchy_path",
        "hierarchypath",
        "taxonomy_path",
        "taxonomypath",
    ]

    # Check LTree pattern matches
    if any(pattern in field_lower for pattern in ltree_patterns):
        return FieldType.LTREE

    # Additional LTree patterns that should be whole words or at start/end
    if (
        field_lower in ["path", "tree", "hierarchy"]
        or field_lower.endswith(("_path", "path", "_tree", "tree"))
        or field_lower.startswith(("path_", "path", "tree_", "tree"))
    ):
        return FieldType.LTREE

    return FieldType.ANY


def _is_ip_address_value(value: str) -> bool:
    """Check if a string value looks like an IP address."""
    try:
        import ipaddress

        # Try to parse as IP address (both IPv4 and IPv6)
        try:
            ipaddress.ip_address(value)
            return True
        except ValueError:
            # Also try as CIDR network (might be used in comparisons)
            try:
                ipaddress.ip_network(value, strict=False)
                return True
            except ValueError:
                pass

        # Additional heuristic checks for common IP patterns
        # IPv4-like pattern
        if value.count(".") == 3:
            parts = value.split(".")
            if len(parts) == 4 and all(part.isdigit() and 0 <= int(part) <= 255 for part in parts):
                return True

        # IPv6-like pattern (simplified check)
        if ":" in value and value.count(":") >= 2:
            # Basic IPv6 pattern check - contains only valid hex chars and colons
            hex_chars = "0123456789abcdefABCDEF"
            return all(c in hex_chars + ":" for c in value)

    except ImportError:
        # Fallback to basic pattern matching if ipaddress module not available
        if value.count(".") == 3:
            parts = value.split(".")
            try:
                return all(0 <= int(part) <= 255 for part in parts)
            except ValueError:
                pass

    return False


def _is_mac_address_value(value: str) -> bool:
    """Check if a string value looks like a MAC address."""
    if not value:
        return False

    # Remove common separators
    mac_clean = value.replace(":", "").replace("-", "").replace(" ", "").upper()

    # MAC address should be exactly 12 hex characters
    if len(mac_clean) != 12:
        return False

    # Check if all characters are valid hex
    try:
        int(mac_clean, 16)
        return True
    except ValueError:
        return False


def _is_ltree_value(value: str) -> bool:
    """Check if a string value looks like an LTree path."""
    if not value or value.startswith(".") or value.endswith(".") or ".." in value:
        return False

    if "." not in value:
        return False  # LTree paths should be hierarchical

    # Check for valid LTree characters and patterns
    ltree_pattern = r"^[a-zA-Z0-9_-]+(\.[a-zA-Z0-9_-]+)+$"

    if not re.match(ltree_pattern, value):
        return False

    # Additional checks to avoid domain name false positives
    domain_extensions = {
        "com",
        "net",
        "org",
        "edu",
        "gov",
        "mil",
        "int",
        "co",
        "uk",
        "ca",
        "de",
        "fr",
        "jp",
        "au",
        "ru",
        "io",
        "ai",
        "dev",
        "app",
        "api",
        "www",
    }

    # If the last part is a common domain extension, probably not an LTree
    last_part = value.split(".")[-1].lower()
    if last_part in domain_extensions:
        return False

    # If it looks like a URL, probably not an LTree
    if value.lower().startswith(("www.", "api.", "app.", "dev.", "test.")):
        return False

    return True


def _is_daterange_value(value: str) -> bool:
    """Check if a string value looks like a PostgreSQL DateRange."""
    if len(value) < 7:  # Minimum: '[a,b]'
        return False

    if not (value.startswith(("[", "(")) and value.endswith(("]", ")"))):
        return False

    # Extract the content between brackets
    content = value[1:-1]  # Remove brackets

    if "," not in content:
        return False

    # Split on comma and check each part looks like a date
    parts = content.split(",")
    if len(parts) != 2:
        return False

    # Basic date pattern check (YYYY-MM-DD)
    date_pattern = r"^\d{4}-\d{2}-\d{2}$"

    for part in parts:
        stripped_part = part.strip()
        if not stripped_part:  # Allow empty for infinite ranges
            continue
        if not re.match(date_pattern, stripped_part):
            return False

    return True
