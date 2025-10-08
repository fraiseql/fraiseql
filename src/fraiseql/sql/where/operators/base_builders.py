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
    cast_type: str | None = None,
    cast_value: bool = True,
) -> Composed:
    """Build SQL for comparison operators with flexible type casting.

    This generic builder handles all comparison operators: =, !=, >, >=, <, <=

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        value: The value to compare against
        operator: SQL comparison operator (=, !=, >, >=, <, <=)
        cast_type: PostgreSQL cast type (date, timestamptz, macaddr, integer, etc.)
                   If None, no casting is applied (for simple text comparison)
        cast_value: Whether to cast the value side. Set False for types like integer
                    where only the left side needs casting

    Returns:
        Composed SQL with appropriate casting based on parameters

    Examples:
        >>> path = SQL("data->>'created_at'")
        >>> # Both sides cast (date, datetime, mac, ltree, inet)
        >>> build_comparison_sql(path, "2023-07-15", "=", "date")
        # Produces: (data->>'created_at')::date = '2023-07-15'::date

        >>> # Left side only cast (port, integer)
        >>> build_comparison_sql(path, 8080, "=", "integer", cast_value=False)
        # Produces: (data->>'port')::integer = 8080

        >>> # No casting (email, hostname)
        >>> build_comparison_sql(path, "user@example.com", "=", None)
        # Produces: data->>'email' = 'user@example.com'
    """
    if cast_type is None:
        # No casting - simple text comparison
        return Composed([path_sql, SQL(f" {operator} "), Literal(value)])

    if not cast_value:
        # Cast left side only (e.g., integer fields)
        return Composed([SQL("("), path_sql, SQL(f")::{cast_type} {operator} "), Literal(value)])

    # Cast both sides (e.g., date, timestamptz, macaddr, inet)
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
    cast_type: str | None = None,
    cast_value: bool = True,
) -> Composed:
    """Build SQL for IN/NOT IN operators with flexible type casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'birth_date')
        values: List of values to match against
        operator: SQL list operator ("IN" or "NOT IN")
        cast_type: PostgreSQL cast type (date, timestamptz, macaddr, integer, etc.)
                   If None, no casting is applied (for simple text comparison)
        cast_value: Whether to cast the value side. Set False for types like integer
                    where only the left side needs casting

    Returns:
        Composed SQL with appropriate casting based on parameters

    Raises:
        TypeError: If values is not a list

    Examples:
        >>> path = SQL("data->>'date'")
        >>> # Both sides cast
        >>> build_in_list_sql(path, ["2023-01-01", "2023-12-31"], "IN", "date")
        # Produces: (data->>'date')::date IN ('2023-01-01'::date, '2023-12-31'::date)

        >>> # Left side only cast
        >>> build_in_list_sql(path, [80, 443, 8080], "IN", "integer", cast_value=False)
        # Produces: (data->>'port')::integer IN (80, 443, 8080)

        >>> # No casting
        >>> build_in_list_sql(path, ["user@a.com", "user@b.com"], "IN", None)
        # Produces: data->>'email' IN ('user@a.com', 'user@b.com')
    """
    if not isinstance(values, list):
        operator_name = "in" if operator == "IN" else "notin"
        raise TypeError(f"'{operator_name}' operator requires a list, got {type(values)}")

    if cast_type is None:
        # No casting - simple text comparison
        parts = [path_sql, SQL(f" {operator} (")]
        for i, val in enumerate(values):
            if i > 0:
                parts.append(SQL(", "))
            parts.append(Literal(val))
        parts.append(SQL(")"))
        return Composed(parts)

    if not cast_value:
        # Cast left side only (e.g., integer fields)
        parts = [SQL("("), path_sql, SQL(f")::{cast_type} {operator} (")]
        for i, val in enumerate(values):
            if i > 0:
                parts.append(SQL(", "))
            parts.append(Literal(val))
        parts.append(SQL(")"))
        return Composed(parts)

    # Cast both sides (e.g., date, timestamptz, macaddr, inet)
    parts = [SQL("("), path_sql, SQL(f")::{cast_type} {operator} (")]
    for i, val in enumerate(values):
        if i > 0:
            parts.append(SQL(", "))
        parts.extend([Literal(val), SQL(f"::{cast_type}")])
    parts.append(SQL(")"))
    return Composed(parts)
