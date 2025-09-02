"""Email operators for building SQL WHERE conditions.

This module provides clean functions to build SQL for email address operations
using standard text comparison for validated email fields.
"""

from psycopg.sql import SQL, Composed, Literal


def build_email_eq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Email equality with standard text comparison.

    Args:
        path_sql: The SQL path expression (e.g., data->>'email')
        value: Email string value (e.g., 'user@example.com')

    Returns:
        Composed SQL: path = 'value'
    """
    return Composed([path_sql, SQL(" = "), Literal(value)])


def build_email_neq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Email inequality with standard text comparison.

    Args:
        path_sql: The SQL path expression (e.g., data->>'email')
        value: Email string value (e.g., 'user@example.com')

    Returns:
        Composed SQL: path != 'value'
    """
    return Composed([path_sql, SQL(" != "), Literal(value)])


def build_email_in_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for Email IN list with standard text comparison.

    Args:
        path_sql: The SQL path expression (e.g., data->>'email')
        value: List of email string values

    Returns:
        Composed SQL: path IN ('val1', 'val2', ...)

    Raises:
        TypeError: If value is not a list
    """
    if not isinstance(value, list):
        raise TypeError(f"'in' operator requires a list, got {type(value)}")

    parts = [path_sql, SQL(" IN (")]

    for i, email in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.append(Literal(email))

    parts.append(SQL(")"))
    return Composed(parts)


def build_email_notin_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for Email NOT IN list with standard text comparison.

    Args:
        path_sql: The SQL path expression (e.g., data->>'email')
        value: List of email string values

    Returns:
        Composed SQL: path NOT IN ('val1', 'val2', ...)

    Raises:
        TypeError: If value is not a list
    """
    if not isinstance(value, list):
        raise TypeError(f"'notin' operator requires a list, got {type(value)}")

    parts = [path_sql, SQL(" NOT IN (")]

    for i, email in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.append(Literal(email))

    parts.append(SQL(")"))
    return Composed(parts)
