"""Port operators for building SQL WHERE conditions.

This module provides clean functions to build SQL for network port operations
using proper integer casting for validated port fields (1-65535).
"""

from psycopg.sql import SQL, Composed, Literal


def build_port_eq_sql(path_sql: SQL, value: int) -> Composed:
    """Build SQL for Port equality with proper integer casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'port')
        value: Port integer value (e.g., 8080)

    Returns:
        Composed SQL: (path)::integer = value
    """
    return Composed([SQL("("), path_sql, SQL(")::integer = "), Literal(value)])


def build_port_neq_sql(path_sql: SQL, value: int) -> Composed:
    """Build SQL for Port inequality with proper integer casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'port')
        value: Port integer value (e.g., 8080)

    Returns:
        Composed SQL: (path)::integer != value
    """
    return Composed([SQL("("), path_sql, SQL(")::integer != "), Literal(value)])


def build_port_in_sql(path_sql: SQL, value: list[int]) -> Composed:
    """Build SQL for Port IN list with proper integer casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'port')
        value: List of port integer values

    Returns:
        Composed SQL: (path)::integer IN (val1, val2, ...)

    Raises:
        TypeError: If value is not a list
    """
    if not isinstance(value, list):
        raise TypeError(f"'in' operator requires a list, got {type(value)}")

    parts = [SQL("("), path_sql, SQL(")::integer IN (")]

    for i, port in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.append(Literal(port))

    parts.append(SQL(")"))
    return Composed(parts)


def build_port_notin_sql(path_sql: SQL, value: list[int]) -> Composed:
    """Build SQL for Port NOT IN list with proper integer casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'port')
        value: List of port integer values

    Returns:
        Composed SQL: (path)::integer NOT IN (val1, val2, ...)

    Raises:
        TypeError: If value is not a list
    """
    if not isinstance(value, list):
        raise TypeError(f"'notin' operator requires a list, got {type(value)}")

    parts = [SQL("("), path_sql, SQL(")::integer NOT IN (")]

    for i, port in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.append(Literal(port))

    parts.append(SQL(")"))
    return Composed(parts)


def build_port_gt_sql(path_sql: SQL, value: int) -> Composed:
    """Build SQL for Port greater than with proper integer casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'port')
        value: Port integer value to compare against

    Returns:
        Composed SQL: (path)::integer > value
    """
    return Composed([SQL("("), path_sql, SQL(")::integer > "), Literal(value)])


def build_port_gte_sql(path_sql: SQL, value: int) -> Composed:
    """Build SQL for Port greater than or equal with proper integer casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'port')
        value: Port integer value to compare against

    Returns:
        Composed SQL: (path)::integer >= value
    """
    return Composed([SQL("("), path_sql, SQL(")::integer >= "), Literal(value)])


def build_port_lt_sql(path_sql: SQL, value: int) -> Composed:
    """Build SQL for Port less than with proper integer casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'port')
        value: Port integer value to compare against

    Returns:
        Composed SQL: (path)::integer < value
    """
    return Composed([SQL("("), path_sql, SQL(")::integer < "), Literal(value)])


def build_port_lte_sql(path_sql: SQL, value: int) -> Composed:
    """Build SQL for Port less than or equal with proper integer casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'port')
        value: Port integer value to compare against

    Returns:
        Composed SQL: (path)::integer <= value
    """
    return Composed([SQL("("), path_sql, SQL(")::integer <= "), Literal(value)])
