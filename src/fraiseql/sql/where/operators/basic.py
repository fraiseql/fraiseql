"""Basic comparison operators (eq, neq, gt, gte, lt, lte)."""

from psycopg.sql import SQL, Composed, Literal


def build_eq_sql(path_sql: SQL, value: any) -> Composed:
    """Build SQL for equality operator."""
    # Apply type casting if needed
    casted_path = _apply_type_cast_if_needed(path_sql, value)
    return Composed([casted_path, SQL(" = "), Literal(value)])


def build_neq_sql(path_sql: SQL, value: any) -> Composed:
    """Build SQL for inequality operator."""
    # Apply type casting if needed
    casted_path = _apply_type_cast_if_needed(path_sql, value)
    return Composed([casted_path, SQL(" != "), Literal(value)])


def build_gt_sql(path_sql: SQL, value: any) -> Composed:
    """Build SQL for greater than operator."""
    # Apply type casting for comparison
    casted_path = _apply_type_cast_if_needed(path_sql, value)
    return Composed([casted_path, SQL(" > "), Literal(value)])


def build_gte_sql(path_sql: SQL, value: any) -> Composed:
    """Build SQL for greater than or equal operator."""
    casted_path = _apply_type_cast_if_needed(path_sql, value)
    return Composed([casted_path, SQL(" >= "), Literal(value)])


def build_lt_sql(path_sql: SQL, value: any) -> Composed:
    """Build SQL for less than operator."""
    casted_path = _apply_type_cast_if_needed(path_sql, value)
    return Composed([casted_path, SQL(" < "), Literal(value)])


def build_lte_sql(path_sql: SQL, value: any) -> Composed:
    """Build SQL for less than or equal operator."""
    casted_path = _apply_type_cast_if_needed(path_sql, value)
    return Composed([casted_path, SQL(" <= "), Literal(value)])


def _apply_type_cast_if_needed(path_sql: SQL, value: any) -> Composed:
    """Apply appropriate type casting if the value needs it."""
    from datetime import date, datetime
    from decimal import Decimal

    if isinstance(value, bool):
        return Composed([path_sql, SQL("::boolean")])
    if isinstance(value, (int, float, Decimal)):
        return Composed([path_sql, SQL("::numeric")])
    if isinstance(value, datetime):
        return Composed([path_sql, SQL("::timestamp")])
    if isinstance(value, date):
        return Composed([path_sql, SQL("::date")])
    return path_sql
