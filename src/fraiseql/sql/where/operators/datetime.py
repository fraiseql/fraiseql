"""DateTime operators for building SQL WHERE conditions.

This module provides clean functions to build SQL for ISO 8601 datetime operations
using proper timestamptz casting for temporal comparisons with timezone support.

These operators are thin wrappers around the generic base builders, specialized
for PostgreSQL timestamptz type.
"""

from psycopg.sql import SQL, Composed

from .base_builders import build_comparison_sql, build_in_list_sql


def build_datetime_eq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateTime equality with proper timestamptz casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'created_at')
        value: ISO 8601 datetime string (e.g., '2023-07-15T14:30:00Z')

    Returns:
        Composed SQL: (path)::timestamptz = 'value'::timestamptz
    """
    return build_comparison_sql(path_sql, value, "=", "timestamptz")


def build_datetime_neq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateTime inequality with proper timestamptz casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'created_at')
        value: ISO 8601 datetime string (e.g., '2023-07-15T14:30:00Z')

    Returns:
        Composed SQL: (path)::timestamptz != 'value'::timestamptz
    """
    return build_comparison_sql(path_sql, value, "!=", "timestamptz")


def build_datetime_in_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for DateTime IN list with proper timestamptz casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'created_at')
        value: List of ISO 8601 datetime strings

    Returns:
        Composed SQL: (path)::timestamptz IN ('val1'::timestamptz, 'val2'::timestamptz, ...)

    Raises:
        TypeError: If value is not a list
    """
    return build_in_list_sql(path_sql, value, "IN", "timestamptz")


def build_datetime_notin_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for DateTime NOT IN list with proper timestamptz casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'created_at')
        value: List of ISO 8601 datetime strings

    Returns:
        Composed SQL: (path)::timestamptz NOT IN ('val1'::timestamptz, 'val2'::timestamptz, ...)

    Raises:
        TypeError: If value is not a list
    """
    return build_in_list_sql(path_sql, value, "NOT IN", "timestamptz")


def build_datetime_gt_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateTime greater than with proper timestamptz casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'created_at')
        value: ISO 8601 datetime string to compare against

    Returns:
        Composed SQL: (path)::timestamptz > 'value'::timestamptz
    """
    return build_comparison_sql(path_sql, value, ">", "timestamptz")


def build_datetime_gte_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateTime greater than or equal with proper timestamptz casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'created_at')
        value: ISO 8601 datetime string to compare against

    Returns:
        Composed SQL: (path)::timestamptz >= 'value'::timestamptz
    """
    return build_comparison_sql(path_sql, value, ">=", "timestamptz")


def build_datetime_lt_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateTime less than with proper timestamptz casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'created_at')
        value: ISO 8601 datetime string to compare against

    Returns:
        Composed SQL: (path)::timestamptz < 'value'::timestamptz
    """
    return build_comparison_sql(path_sql, value, "<", "timestamptz")


def build_datetime_lte_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateTime less than or equal with proper timestamptz casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'created_at')
        value: ISO 8601 datetime string to compare against

    Returns:
        Composed SQL: (path)::timestamptz <= 'value'::timestamptz
    """
    return build_comparison_sql(path_sql, value, "<=", "timestamptz")
