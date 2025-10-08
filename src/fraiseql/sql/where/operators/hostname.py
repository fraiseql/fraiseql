"""Hostname operators for building SQL WHERE conditions.

This module provides clean functions to build SQL for hostname operations
using standard text comparison for DNS hostname fields.

These operators use no casting since hostname validation happens at the application
layer and database storage is plain text.
"""

from psycopg.sql import SQL, Composed

from .base_builders import build_comparison_sql, build_in_list_sql


def build_hostname_eq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Hostname equality with standard text comparison.

    Args:
        path_sql: The SQL path expression (e.g., data->>'hostname')
        value: Hostname string value (e.g., 'api.example.com')

    Returns:
        Composed SQL: path = 'value'
    """
    return build_comparison_sql(path_sql, value, "=", None)


def build_hostname_neq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Hostname inequality with standard text comparison.

    Args:
        path_sql: The SQL path expression (e.g., data->>'hostname')
        value: Hostname string value (e.g., 'api.example.com')

    Returns:
        Composed SQL: path != 'value'
    """
    return build_comparison_sql(path_sql, value, "!=", None)


def build_hostname_in_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for Hostname IN list with standard text comparison.

    Args:
        path_sql: The SQL path expression (e.g., data->>'hostname')
        value: List of hostname string values

    Returns:
        Composed SQL: path IN ('val1', 'val2', ...)

    Raises:
        TypeError: If value is not a list
    """
    return build_in_list_sql(path_sql, value, "IN", None)


def build_hostname_notin_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for Hostname NOT IN list with standard text comparison.

    Args:
        path_sql: The SQL path expression (e.g., data->>'hostname')
        value: List of hostname string values

    Returns:
        Composed SQL: path NOT IN ('val1', 'val2', ...)

    Raises:
        TypeError: If value is not a list
    """
    return build_in_list_sql(path_sql, value, "NOT IN", None)
