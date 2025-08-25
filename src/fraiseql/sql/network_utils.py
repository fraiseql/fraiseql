"""Network utilities for IP address validation and SQL generation.

This module provides utilities for handling network-specific operations
in FraiseQL, including IP address validation, subnet matching, and
SQL generation for network operators.
"""

import ipaddress
from typing import Any, Tuple

from psycopg.sql import SQL, Composed


def validate_ip_address(ip_str: str) -> bool:
    """Validate if a string is a valid IP address (IPv4 or IPv6).

    Args:
        ip_str: String to validate as IP address

    Returns:
        True if valid IP address, False otherwise
    """
    if not ip_str:
        return False

    try:
        ipaddress.ip_address(ip_str.strip())
        return True
    except (ipaddress.AddressValueError, ValueError):
        return False


def is_ipv4(ip_str: str) -> bool:
    """Check if an IP address string is IPv4.

    Args:
        ip_str: IP address string

    Returns:
        True if IPv4, False if IPv6 or invalid
    """
    try:
        ip = ipaddress.ip_address(ip_str.strip())
        return isinstance(ip, ipaddress.IPv4Address)
    except (ipaddress.AddressValueError, ValueError):
        return False


def is_ipv6(ip_str: str) -> bool:
    """Check if an IP address string is IPv6.

    Args:
        ip_str: IP address string

    Returns:
        True if IPv6, False if IPv4 or invalid
    """
    try:
        ip = ipaddress.ip_address(ip_str.strip())
        return isinstance(ip, ipaddress.IPv6Address)
    except (ipaddress.AddressValueError, ValueError):
        return False


def is_private_ip(ip_str: str) -> bool:
    """Check if an IP address is in a private network range.

    Checks RFC 1918 private networks:
    - 10.0.0.0/8
    - 172.16.0.0/12
    - 192.168.0.0/16
    - Plus loopback and link-local ranges

    Args:
        ip_str: IP address string

    Returns:
        True if private, False if public or invalid
    """
    try:
        ip = ipaddress.ip_address(ip_str.strip())
        return ip.is_private
    except (ipaddress.AddressValueError, ValueError):
        return False


def ip_in_subnet(ip_str: str, subnet_str: str) -> bool:
    """Check if an IP address is in a given subnet.

    Args:
        ip_str: IP address string
        subnet_str: Subnet in CIDR notation (e.g., "192.168.1.0/24")

    Returns:
        True if IP is in subnet, False otherwise
    """
    try:
        ip = ipaddress.ip_address(ip_str.strip())
        network = ipaddress.ip_network(subnet_str.strip(), strict=False)
        return ip in network
    except (ipaddress.AddressValueError, ValueError):
        return False


def ip_in_range(ip_str: str, start_ip: str, end_ip: str) -> bool:
    """Check if an IP address is in a given range.

    Args:
        ip_str: IP address string
        start_ip: Starting IP address of range
        end_ip: Ending IP address of range

    Returns:
        True if IP is in range (inclusive), False otherwise
    """
    try:
        ip = ipaddress.ip_address(ip_str.strip())
        start = ipaddress.ip_address(start_ip.strip())
        end = ipaddress.ip_address(end_ip.strip())

        # Must be same IP version
        if not isinstance(ip, type(start)) or not isinstance(ip, type(end)):
            return False

        # Type checking: all IPs are the same type now
        return start <= ip <= end  # type: ignore[operator]
    except (ipaddress.AddressValueError, ValueError):
        return False


def validate_ip_range(start_ip: str, end_ip: str) -> bool:
    """Validate that an IP range has valid start and end addresses.

    Args:
        start_ip: Starting IP address
        end_ip: Ending IP address

    Returns:
        True if valid range, False otherwise
    """
    if not start_ip or not end_ip:
        return False

    try:
        start = ipaddress.ip_address(start_ip.strip())
        end = ipaddress.ip_address(end_ip.strip())

        # Must be same IP version
        if not isinstance(start, type(end)):
            return False

        # Start must be <= end (type checking: both IPs are same type)
        return start <= end  # type: ignore[operator]
    except (ipaddress.AddressValueError, ValueError):
        return False


def generate_subnet_sql(field_path: str, subnet: str) -> Tuple[str, list]:
    """Generate SQL for subnet matching using PostgreSQL inet operators.

    Args:
        field_path: SQL path to the IP field (e.g., "data->>'ip_address'")
        subnet: Subnet in CIDR notation

    Returns:
        Tuple of (SQL string, parameters list)
    """
    # Use PostgreSQL's inet subnet operator
    sql_template = f"({field_path})::inet <<= %s::inet"
    return sql_template, [subnet]


def generate_range_sql(field_path: str, start_ip: str, end_ip: str) -> Tuple[str, list]:
    """Generate SQL for IP range matching.

    Args:
        field_path: SQL path to the IP field
        start_ip: Starting IP address
        end_ip: Ending IP address

    Returns:
        Tuple of (SQL string, parameters list)
    """
    # Use PostgreSQL inet comparison
    sql_template = f"({field_path})::inet >= %s::inet AND ({field_path})::inet <= %s::inet"
    return sql_template, [start_ip, end_ip]


def generate_private_ip_sql(field_path: str, is_private: bool) -> Tuple[str, list]:
    """Generate SQL for private IP detection.

    Args:
        field_path: SQL path to the IP field
        is_private: True to match private IPs, False for public

    Returns:
        Tuple of (SQL string, parameters list)
    """
    if is_private:
        # Check RFC 1918 private ranges + loopback + link-local
        sql_template = f"""(
            ({field_path})::inet <<= '10.0.0.0/8'::inet OR
            ({field_path})::inet <<= '172.16.0.0/12'::inet OR
            ({field_path})::inet <<= '192.168.0.0/16'::inet OR
            ({field_path})::inet <<= '127.0.0.0/8'::inet OR
            ({field_path})::inet <<= '169.254.0.0/16'::inet
        )"""
    else:
        # Public IPs - not in private ranges
        sql_template = f"""NOT (
            ({field_path})::inet <<= '10.0.0.0/8'::inet OR
            ({field_path})::inet <<= '172.16.0.0/12'::inet OR
            ({field_path})::inet <<= '192.168.0.0/16'::inet OR
            ({field_path})::inet <<= '127.0.0.0/8'::inet OR
            ({field_path})::inet <<= '169.254.0.0/16'::inet
        )"""

    return sql_template, []


def generate_ipv4_sql(field_path: str, is_ipv4: bool) -> Tuple[str, list]:
    """Generate SQL for IPv4/IPv6 detection.

    Args:
        field_path: SQL path to the IP field
        is_ipv4: True to match IPv4, False for IPv6

    Returns:
        Tuple of (SQL string, parameters list)
    """
    if is_ipv4:
        # Check if IP is IPv4 using family function
        sql_template = f"family(({field_path})::inet) = 4"
    else:
        # IPv6
        sql_template = f"family(({field_path})::inet) = 6"

    return sql_template, []


def generate_network_filter_sql(field_path: str, operator: str, value: Any) -> Tuple[str, list]:
    """Generate SQL for network filtering operations.

    Args:
        field_path: SQL path to the IP field
        operator: Network operator name
        value: Filter value

    Returns:
        Tuple of (SQL string, parameters list)

    Raises:
        ValueError: If operator is not supported
    """
    if operator == "inSubnet":
        if not isinstance(value, str):
            raise ValueError(f"inSubnet requires string subnet, got {type(value)}")
        return generate_subnet_sql(field_path, value)

    if operator == "inRange":
        if not isinstance(value, dict) or "from" not in value or "to" not in value:
            raise ValueError(f"inRange requires dict with 'from' and 'to' keys, got {value}")
        return generate_range_sql(field_path, value["from"], value["to"])

    if operator == "isPrivate":
        if not isinstance(value, bool):
            raise ValueError(f"isPrivate requires boolean, got {type(value)}")
        return generate_private_ip_sql(field_path, value)

    if operator == "isPublic":
        if not isinstance(value, bool):
            raise ValueError(f"isPublic requires boolean, got {type(value)}")
        return generate_private_ip_sql(field_path, not value)  # Invert logic

    if operator == "isIPv4":
        if not isinstance(value, bool):
            raise ValueError(f"isIPv4 requires boolean, got {type(value)}")
        return generate_ipv4_sql(field_path, value)

    if operator == "isIPv6":
        if not isinstance(value, bool):
            raise ValueError(f"isIPv6 requires boolean, got {type(value)}")
        return generate_ipv4_sql(field_path, not value)  # Invert logic

    raise ValueError(f"Unsupported network operator: {operator}")


class NetworkOperatorStrategy:
    """Strategy for handling network-specific operators in SQL generation."""

    def can_handle(self, operator: str, field_type: type | None = None) -> bool:
        """Check if this strategy can handle the given operator."""
        from fraiseql.types import CIDR, IpAddress

        network_operators = {"inSubnet", "inRange", "isPrivate", "isPublic", "isIPv4", "isIPv6"}

        is_network_type = field_type in (IpAddress, CIDR) if field_type else False
        is_network_operator = operator in network_operators

        return is_network_type and is_network_operator

    def apply(
        self, operator: str, field_path: SQL, value: Any, field_type: type | None = None
    ) -> SQL | Composed:
        """Apply network operator to generate SQL.

        Args:
            operator: Network operator name
            field_path: SQL path to field
            value: Filter value
            field_type: Type of the field being filtered

        Returns:
            SQL expression for the network operation
        """
        try:
            sql_str, params = generate_network_filter_sql(str(field_path), operator, value)

            if params:
                # Create parameterized SQL
                return Composed(
                    [SQL(sql_str), *[SQL(" ") if i > 0 else SQL("") for i in range(len(params))]]
                )
            # Simple SQL string
            return SQL(sql_str)

        except ValueError as e:
            raise ValueError(f"Network operator {operator} failed: {e!s}") from e
