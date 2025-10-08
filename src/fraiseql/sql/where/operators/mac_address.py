"""MAC address operators for building SQL WHERE conditions.

This module provides clean functions to build SQL for MAC address operations
using proper PostgreSQL macaddr casting.

These operators are thin wrappers around the generic base builders, specialized
for PostgreSQL macaddr type.
"""

from psycopg.sql import SQL, Composed

from .base_builders import build_comparison_sql, build_in_list_sql


def build_mac_eq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for MAC address equality with proper macaddr casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'mac_address')
        value: MAC address string value

    Returns:
        Composed SQL: (path)::macaddr = 'value'::macaddr
    """
    return build_comparison_sql(path_sql, value, "=", "macaddr")


def build_mac_neq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for MAC address inequality with proper macaddr casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'mac_address')
        value: MAC address string value

    Returns:
        Composed SQL: (path)::macaddr != 'value'::macaddr
    """
    return build_comparison_sql(path_sql, value, "!=", "macaddr")


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
    return build_in_list_sql(path_sql, value, "IN", "macaddr")


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
    return build_in_list_sql(path_sql, value, "NOT IN", "macaddr")
