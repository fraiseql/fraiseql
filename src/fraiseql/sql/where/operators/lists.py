"""List operators (in, notin)."""

from psycopg.sql import SQL, Composed, Literal


def build_in_sql(path_sql: SQL, value: list) -> Composed:
    """Build SQL for IN operator."""
    if not isinstance(value, list):
        raise TypeError(f"'in' operator requires a list, got {type(value)}")

    # Apply type casting based on value types
    casted_path = _apply_type_cast_for_list(path_sql, value)

    parts = [casted_path, SQL(" IN (")]

    for i, item in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.append(Literal(item))

    parts.append(SQL(")"))
    return Composed(parts)


def build_notin_sql(path_sql: SQL, value: list) -> Composed:
    """Build SQL for NOT IN operator."""
    if not isinstance(value, list):
        raise TypeError(f"'notin' operator requires a list, got {type(value)}")

    # Apply type casting based on value types
    casted_path = _apply_type_cast_for_list(path_sql, value)

    parts = [casted_path, SQL(" NOT IN (")]

    for i, item in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.append(Literal(item))

    parts.append(SQL(")"))
    return Composed(parts)


def _apply_type_cast_for_list(path_sql: SQL, value_list: list) -> Composed | SQL:
    """Apply appropriate type casting based on the list values."""
    if not value_list:
        return path_sql

    # Check first non-None value for type
    sample_value = None
    for item in value_list:
        if item is not None:
            sample_value = item
            break

    if sample_value is None:
        return path_sql

    from decimal import Decimal

    if isinstance(sample_value, bool):
        return path_sql  # Booleans are handled as strings in JSONB
    if isinstance(sample_value, (int, float, Decimal)):
        return Composed([path_sql, SQL("::numeric")])
    return path_sql  # String comparison
