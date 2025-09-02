"""DateRange operators for building SQL WHERE conditions.

This module provides clean functions to build SQL for PostgreSQL daterange operations
using proper daterange casting for temporal range operations.
"""

from psycopg.sql import SQL, Composed, Literal


def build_daterange_eq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateRange equality with proper daterange casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'period')
        value: DateRange string value (e.g., '[2023-01-01,2023-12-31]')

    Returns:
        Composed SQL: (path)::daterange = 'value'::daterange
    """
    return Composed(
        [SQL("("), path_sql, SQL(")::daterange = "), Literal(value), SQL("::daterange")]
    )


def build_daterange_neq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateRange inequality with proper daterange casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'period')
        value: DateRange string value (e.g., '[2023-01-01,2023-12-31]')

    Returns:
        Composed SQL: (path)::daterange != 'value'::daterange
    """
    return Composed(
        [SQL("("), path_sql, SQL(")::daterange != "), Literal(value), SQL("::daterange")]
    )


def build_daterange_in_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for DateRange IN list with proper daterange casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'period')
        value: List of DateRange string values

    Returns:
        Composed SQL: (path)::daterange IN ('val1'::daterange, 'val2'::daterange, ...)

    Raises:
        TypeError: If value is not a list
    """
    if not isinstance(value, list):
        raise TypeError(f"'in' operator requires a list, got {type(value)}")

    parts = [SQL("("), path_sql, SQL(")::daterange IN (")]

    for i, date_range in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.extend([Literal(date_range), SQL("::daterange")])

    parts.append(SQL(")"))
    return Composed(parts)


def build_daterange_notin_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for DateRange NOT IN list with proper daterange casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'period')
        value: List of DateRange string values

    Returns:
        Composed SQL: (path)::daterange NOT IN ('val1'::daterange, 'val2'::daterange, ...)

    Raises:
        TypeError: If value is not a list
    """
    if not isinstance(value, list):
        raise TypeError(f"'notin' operator requires a list, got {type(value)}")

    parts = [SQL("("), path_sql, SQL(")::daterange NOT IN (")]

    for i, date_range in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.extend([Literal(date_range), SQL("::daterange")])

    parts.append(SQL(")"))
    return Composed(parts)


def build_contains_date_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateRange contains_date (@>) operator.

    The @> operator checks if the daterange contains the given date or range.

    Args:
        path_sql: The SQL path expression (e.g., data->>'period')
        value: Date or DateRange string value to check for containment

    Returns:
        Composed SQL: (path)::daterange @> 'value'
    """
    return Composed([SQL("("), path_sql, SQL(")::daterange @> "), Literal(value)])


def build_overlaps_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateRange overlaps (&&) operator.

    The && operator checks if two dateranges overlap.

    Args:
        path_sql: The SQL path expression (e.g., data->>'period')
        value: DateRange string value to check for overlap

    Returns:
        Composed SQL: (path)::daterange && 'value'::daterange
    """
    return Composed(
        [SQL("("), path_sql, SQL(")::daterange && "), Literal(value), SQL("::daterange")]
    )


def build_adjacent_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateRange adjacent (-|-) operator.

    The -|- operator checks if two dateranges are adjacent (touching but not overlapping).

    Args:
        path_sql: The SQL path expression (e.g., data->>'period')
        value: DateRange string value to check for adjacency

    Returns:
        Composed SQL: (path)::daterange -|- 'value'::daterange
    """
    return Composed(
        [SQL("("), path_sql, SQL(")::daterange -|- "), Literal(value), SQL("::daterange")]
    )


def build_strictly_left_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateRange strictly_left (<<) operator.

    The << operator checks if the left daterange is strictly left of the right daterange.

    Args:
        path_sql: The SQL path expression (e.g., data->>'period')
        value: DateRange string value to compare against

    Returns:
        Composed SQL: (path)::daterange << 'value'::daterange
    """
    return Composed(
        [SQL("("), path_sql, SQL(")::daterange << "), Literal(value), SQL("::daterange")]
    )


def build_strictly_right_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateRange strictly_right (>>) operator.

    The >> operator checks if the left daterange is strictly right of the right daterange.

    Args:
        path_sql: The SQL path expression (e.g., data->>'period')
        value: DateRange string value to compare against

    Returns:
        Composed SQL: (path)::daterange >> 'value'::daterange
    """
    return Composed(
        [SQL("("), path_sql, SQL(")::daterange >> "), Literal(value), SQL("::daterange")]
    )


def build_not_left_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateRange not_left (&>) operator.

    The &> operator checks if the left daterange does not extend to the left of the right daterange.

    Args:
        path_sql: The SQL path expression (e.g., data->>'period')
        value: DateRange string value to compare against

    Returns:
        Composed SQL: (path)::daterange &> 'value'::daterange
    """
    return Composed(
        [SQL("("), path_sql, SQL(")::daterange &> "), Literal(value), SQL("::daterange")]
    )


def build_not_right_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for DateRange not_right (&<) operator.

    The &< operator checks if the left daterange does not extend to the right of the right.

    Args:
        path_sql: The SQL path expression (e.g., data->>'period')
        value: DateRange string value to compare against

    Returns:
        Composed SQL: (path)::daterange &< 'value'::daterange
    """
    return Composed(
        [SQL("("), path_sql, SQL(")::daterange &< "), Literal(value), SQL("::daterange")]
    )
