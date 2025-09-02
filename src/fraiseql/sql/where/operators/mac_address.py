"""MAC address operators for building SQL WHERE conditions.

This module provides clean functions to build SQL for MAC address operations
using proper PostgreSQL macaddr casting.
"""

from psycopg.sql import SQL, Composed, Literal


def build_mac_eq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for MAC address equality with proper macaddr casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'mac_address')
        value: MAC address string value

    Returns:
        Composed SQL: (path)::macaddr = 'value'::macaddr
    """
    return Composed([SQL("("), path_sql, SQL(")::macaddr = "), Literal(value), SQL("::macaddr")])


def build_mac_neq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for MAC address inequality with proper macaddr casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'mac_address')
        value: MAC address string value

    Returns:
        Composed SQL: (path)::macaddr != 'value'::macaddr
    """
    return Composed([SQL("("), path_sql, SQL(")::macaddr != "), Literal(value), SQL("::macaddr")])


def build_mac_in_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for MAC address IN list with proper macaddr casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'mac_address')
        value: List of MAC address strings

    Returns:
        Composed SQL: (path)::macaddr IN ('val1'::macaddr, 'val2'::macaddr, ...)

    Raises:
        TypeError: If value is not a list
    """
    if not isinstance(value, list):
        raise TypeError(f"'in' operator requires a list, got {type(value)}")

    parts = [SQL("("), path_sql, SQL(")::macaddr IN (")]

    for i, mac_addr in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.extend([Literal(mac_addr), SQL("::macaddr")])

    parts.append(SQL(")"))
    return Composed(parts)


def build_mac_notin_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for MAC address NOT IN list with proper macaddr casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'mac_address')
        value: List of MAC address strings

    Returns:
        Composed SQL: (path)::macaddr NOT IN ('val1'::macaddr, 'val2'::macaddr, ...)

    Raises:
        TypeError: If value is not a list
    """
    if not isinstance(value, list):
        raise TypeError(f"'notin' operator requires a list, got {type(value)}")

    parts = [SQL("("), path_sql, SQL(")::macaddr NOT IN (")]

    for i, mac_addr in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.extend([Literal(mac_addr), SQL("::macaddr")])

    parts.append(SQL(")"))
    return Composed(parts)
