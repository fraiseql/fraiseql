"""Date operators for building SQL WHERE conditions.

This module provides clean functions to build SQL for ISO 8601 date operations
using proper date casting for temporal comparisons.
"""

from psycopg.sql import SQL, Composed, Literal


def build_date_eq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Date equality with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: ISO 8601 date string (e.g., '2023-07-15')

    Returns:
        Composed SQL: (path)::date = 'value'::date
    """
    return Composed([SQL("("), path_sql, SQL(")::date = "), Literal(value), SQL("::date")])


def build_date_neq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Date inequality with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: ISO 8601 date string (e.g., '2023-07-15')

    Returns:
        Composed SQL: (path)::date != 'value'::date
    """
    return Composed([SQL("("), path_sql, SQL(")::date != "), Literal(value), SQL("::date")])


def build_date_in_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for Date IN list with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: List of ISO 8601 date strings

    Returns:
        Composed SQL: (path)::date IN ('val1'::date, 'val2'::date, ...)

    Raises:
        TypeError: If value is not a list
    """
    if not isinstance(value, list):
        raise TypeError(f"'in' operator requires a list, got {type(value)}")

    parts = [SQL("("), path_sql, SQL(")::date IN (")]

    for i, date_str in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.extend([Literal(date_str), SQL("::date")])

    parts.append(SQL(")"))
    return Composed(parts)


def build_date_notin_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for Date NOT IN list with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: List of ISO 8601 date strings

    Returns:
        Composed SQL: (path)::date NOT IN ('val1'::date, 'val2'::date, ...)

    Raises:
        TypeError: If value is not a list
    """
    if not isinstance(value, list):
        raise TypeError(f"'notin' operator requires a list, got {type(value)}")

    parts = [SQL("("), path_sql, SQL(")::date NOT IN (")]

    for i, date_str in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.extend([Literal(date_str), SQL("::date")])

    parts.append(SQL(")"))
    return Composed(parts)


def build_date_gt_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Date greater than with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: ISO 8601 date string to compare against

    Returns:
        Composed SQL: (path)::date > 'value'::date
    """
    return Composed([SQL("("), path_sql, SQL(")::date > "), Literal(value), SQL("::date")])


def build_date_gte_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Date greater than or equal with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: ISO 8601 date string to compare against

    Returns:
        Composed SQL: (path)::date >= 'value'::date
    """
    return Composed([SQL("("), path_sql, SQL(")::date >= "), Literal(value), SQL("::date")])


def build_date_lt_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Date less than with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: ISO 8601 date string to compare against

    Returns:
        Composed SQL: (path)::date < 'value'::date
    """
    return Composed([SQL("("), path_sql, SQL(")::date < "), Literal(value), SQL("::date")])


def build_date_lte_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Date less than or equal with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: ISO 8601 date string to compare against

    Returns:
        Composed SQL: (path)::date <= 'value'::date
    """
    return Composed([SQL("("), path_sql, SQL(")::date <= "), Literal(value), SQL("::date")])
