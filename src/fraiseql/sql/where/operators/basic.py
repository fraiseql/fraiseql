"""Basic comparison operators (eq, neq, gt, gte, lt, lte)."""

from psycopg.sql import SQL, Composed, Literal


def build_eq_sql(path_sql: SQL, value: any) -> Composed:
    """Build SQL for equality operator."""
    # Apply type casting if needed
    casted_path = _apply_type_cast_if_needed(path_sql, value)

    # Convert boolean to text for JSONB comparison
    if isinstance(value, bool):
        text_value = "true" if value else "false"
        return Composed([casted_path, SQL(" = "), Literal(text_value)])

    return Composed([casted_path, SQL(" = "), Literal(value)])


def build_neq_sql(path_sql: SQL, value: any) -> Composed:
    """Build SQL for inequality operator."""
    # Apply type casting if needed
    casted_path = _apply_type_cast_if_needed(path_sql, value)

    # Convert boolean to text for JSONB comparison
    if isinstance(value, bool):
        text_value = "true" if value else "false"
        return Composed([casted_path, SQL(" != "), Literal(text_value)])

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


def _apply_type_cast_if_needed(path_sql: SQL, value: any) -> Composed | SQL:
    """Apply appropriate type casting if the value needs it."""
    from datetime import date, datetime
    from decimal import Decimal
    from uuid import UUID

    # CRITICAL: Check bool BEFORE int since bool is subclass of int in Python
    if isinstance(value, bool):
        # For JSONB fields, ->> extracts values as text
        # JSONB stores booleans as "true"/"false" text when extracted with ->>
        # So we compare text-to-text rather than casting to boolean
        # The actual value conversion happens in the build_*_sql functions
        return path_sql  # No casting - will compare with text value
    if isinstance(value, UUID):
        # UUID values need to be cast when extracted from JSONB
        return Composed([path_sql, SQL("::uuid")])
    if isinstance(value, (int, float, Decimal)):
        return Composed([path_sql, SQL("::numeric")])
    if isinstance(value, datetime):
        return Composed([path_sql, SQL("::timestamp")])
    if isinstance(value, date):
        return Composed([path_sql, SQL("::date")])
    return path_sql
