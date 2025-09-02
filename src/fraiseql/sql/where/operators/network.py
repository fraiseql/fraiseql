"""Network/IP address specific operators.

This module contains the core fix for the IP filtering bug described in the guide.
The key insight is to use proper PostgreSQL inet casting instead of string comparison.
"""

from psycopg.sql import SQL, Composed, Literal


def build_ip_eq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for IP address equality with proper inet casting.

    This is the core fix for the production bug. Instead of string comparison:
    (data->>'ip_address') = '192.168.1.1'

    We generate proper inet casting:
    (data->>'ip_address')::inet = '192.168.1.1'::inet
    """
    return Composed([SQL("("), path_sql, SQL(")::inet = "), Literal(value), SQL("::inet")])


def build_ip_neq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for IP address inequality with proper inet casting."""
    return Composed([SQL("("), path_sql, SQL(")::inet != "), Literal(value), SQL("::inet")])


def build_ip_in_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for IP address IN list with proper inet casting."""
    if not isinstance(value, list):
        raise TypeError(f"'in' operator requires a list, got {type(value)}")

    parts = [SQL("("), path_sql, SQL(")::inet IN (")]

    for i, ip in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.extend([Literal(ip), SQL("::inet")])

    parts.append(SQL(")"))
    return Composed(parts)


def build_ip_notin_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for IP address NOT IN list with proper inet casting."""
    if not isinstance(value, list):
        raise TypeError(f"'notin' operator requires a list, got {type(value)}")

    parts = [SQL("("), path_sql, SQL(")::inet NOT IN (")]

    for i, ip in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.extend([Literal(ip), SQL("::inet")])

    parts.append(SQL(")"))
    return Composed(parts)


def build_in_subnet_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for subnet containment using PostgreSQL <<= operator."""
    return Composed([SQL("("), path_sql, SQL(")::inet <<= "), Literal(value), SQL("::inet")])


def build_is_private_sql(path_sql: SQL, value: bool) -> Composed:
    """Build SQL for private IP detection using RFC 1918 ranges."""
    casted_path = Composed([SQL("("), path_sql, SQL(")::inet")])

    private_ranges_condition = Composed(
        [
            SQL("("),
            casted_path,
            SQL(" <<= '10.0.0.0/8'::inet OR "),
            casted_path,
            SQL(" <<= '172.16.0.0/12'::inet OR "),
            casted_path,
            SQL(" <<= '192.168.0.0/16'::inet OR "),
            casted_path,
            SQL(" <<= '127.0.0.0/8'::inet OR "),
            casted_path,
            SQL(" <<= '169.254.0.0/16'::inet"),
            SQL(")"),
        ]
    )

    if value:
        return private_ranges_condition
    return Composed([SQL("NOT "), private_ranges_condition])


def build_is_public_sql(path_sql: SQL, value: bool) -> Composed:
    """Build SQL for public IP detection (inverse of private)."""
    # Public is just NOT private
    return build_is_private_sql(path_sql, not value)
