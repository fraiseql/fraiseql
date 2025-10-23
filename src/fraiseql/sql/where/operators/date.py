"""Date operators for building SQL WHERE conditions.

This module provides clean functions to build SQL for ISO 8601 date operations
using proper date casting for temporal comparisons.

These operators are thin wrappers around the generic base builders, specialized
for PostgreSQL date type.
"""

from psycopg.sql import SQL, Composed

from .base_builders import build_comparison_sql, build_in_list_sql


def build_date_eq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Date equality with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: ISO 8601 date string (e.g., '2023-07-15')

    Returns:
        Composed SQL: (path)::date = 'value'::date
    """
    return build_comparison_sql(path_sql, value, "=", "date")


def build_date_neq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Date inequality with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: ISO 8601 date string (e.g., '2023-07-15')

    Returns:
        Composed SQL: (path)::date != 'value'::date
    """
    return build_comparison_sql(path_sql, value, "!=", "date")


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
    return build_in_list_sql(path_sql, value, "IN", "date")


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
    return build_in_list_sql(path_sql, value, "NOT IN", "date")


def build_date_gt_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Date greater than with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: ISO 8601 date string to compare against

    Returns:
        Composed SQL: (path)::date > 'value'::date
    """
    return build_comparison_sql(path_sql, value, ">", "date")


def build_date_gte_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Date greater than or equal with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: ISO 8601 date string to compare against

    Returns:
        Composed SQL: (path)::date >= 'value'::date
    """
    return build_comparison_sql(path_sql, value, ">=", "date")


def build_date_lt_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Date less than with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: ISO 8601 date string to compare against

    Returns:
        Composed SQL: (path)::date < 'value'::date
    """
    return build_comparison_sql(path_sql, value, "<", "date")


def build_date_lte_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Date less than or equal with proper date casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: ISO 8601 date string to compare against

    Returns:
        Composed SQL: (path)::date <= 'value'::date
    """
    return build_comparison_sql(path_sql, value, "<=", "date")
