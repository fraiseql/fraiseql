"""Hostname operators for building SQL WHERE conditions.

This module provides clean functions to build SQL for hostname operations
using standard text comparison for DNS hostname fields.
"""

from psycopg.sql import SQL, Composed, Literal


def build_hostname_eq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Hostname equality with standard text comparison.

    Args:
        path_sql: The SQL path expression (e.g., data->>'hostname')
        value: Hostname string value (e.g., 'api.example.com')

    Returns:
        Composed SQL: path = 'value'
    """
    return Composed([path_sql, SQL(" = "), Literal(value)])


def build_hostname_neq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Hostname inequality with standard text comparison.

    Args:
        path_sql: The SQL path expression (e.g., data->>'hostname')
        value: Hostname string value (e.g., 'api.example.com')

    Returns:
        Composed SQL: path != 'value'
    """
    return Composed([path_sql, SQL(" != "), Literal(value)])


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
    if not isinstance(value, list):
        raise TypeError(f"'in' operator requires a list, got {type(value)}")

    parts = [path_sql, SQL(" IN (")]

    for i, hostname in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.append(Literal(hostname))

    parts.append(SQL(")"))
    return Composed(parts)


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
    if not isinstance(value, list):
        raise TypeError(f"'notin' operator requires a list, got {type(value)}")

    parts = [path_sql, SQL(" NOT IN (")]

    for i, hostname in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.append(Literal(hostname))

    parts.append(SQL(")"))
    return Composed(parts)
