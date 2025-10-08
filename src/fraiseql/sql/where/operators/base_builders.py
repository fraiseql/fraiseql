"""Generic SQL operator builders for WHERE conditions.

This module provides reusable, type-agnostic SQL operator builders that can be
specialized for different PostgreSQL types (date, timestamptz, macaddr, etc.) by
passing the appropriate cast type.

The goal is to eliminate duplication across type-specific operator modules while
maintaining type safety and clear semantics at the call site.
"""

from typing import Any

from psycopg.sql import SQL, Composed, Literal


def build_comparison_sql(
    path_sql: SQL,
    value: Any,
    operator: str,
    cast_type: str,
) -> Composed:
    """Build SQL for comparison operators with proper type casting.

    This generic builder handles all comparison operators: =, !=, >, >=, <, <=

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: The value to compare against
        operator: SQL comparison operator (=, !=, >, >=, <, <=)
        cast_type: PostgreSQL cast type (date, timestamptz, macaddr, integer, etc.)

    Returns:
        Composed SQL: (path)::cast_type operator 'value'::cast_type

    Examples:
        >>> path = SQL("data->>'created_at'")
        >>> build_comparison_sql(path, "2023-07-15", "=", "date")
        # Produces: (data->>'created_at')::date = '2023-07-15'::date

        >>> build_comparison_sql(path, "2023-07-15T10:00:00Z", ">", "timestamptz")
        # Produces: (data->>'created_at')::timestamptz > '2023-07-15T10:00:00Z'::timestamptz
    """
    return Composed(
        [
            SQL("("),
            path_sql,
            SQL(f")::{cast_type} {operator} "),
            Literal(value),
            SQL(f"::{cast_type}"),
        ]
    )


def build_in_list_sql(
    path_sql: SQL,
    values: list[Any],
    operator: str,
    cast_type: str,
) -> Composed:
    """Build SQL for IN/NOT IN operators with proper type casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        values: List of values to match against
        operator: SQL list operator ("IN" or "NOT IN")
        cast_type: PostgreSQL cast type (date, timestamptz, macaddr, etc.)

    Returns:
        Composed SQL: (path)::cast_type operator ('val1'::cast_type, 'val2'::cast_type, ...)

    Raises:
        TypeError: If values is not a list

    Examples:
        >>> path = SQL("data->>'status'")
        >>> build_in_list_sql(path, ["active", "pending"], "IN", "text")
        # Produces: (data->>'status')::text IN ('active'::text, 'pending'::text)

        >>> build_in_list_sql(path, ["2023-01-01", "2023-12-31"], "NOT IN", "date")
        # Produces: (data->>'date')::date NOT IN ('2023-01-01'::date, '2023-12-31'::date)
    """
    if not isinstance(values, list):
        operator_name = "in" if operator == "IN" else "notin"
        raise TypeError(f"'{operator_name}' operator requires a list, got {type(values)}")

    parts = [SQL("("), path_sql, SQL(f")::{cast_type} {operator} (")]

    for i, val in enumerate(values):
        if i > 0:
            parts.append(SQL(", "))
        parts.extend([Literal(val), SQL(f"::{cast_type}")])

    parts.append(SQL(")"))
    return Composed(parts)
