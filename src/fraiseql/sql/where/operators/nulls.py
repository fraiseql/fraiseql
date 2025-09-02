"""Null operators (isnull)."""

from psycopg.sql import SQL, Composed


def build_isnull_sql(path_sql: SQL, value: bool) -> Composed:
    """Build SQL for null checking."""
    if value:
        return Composed([path_sql, SQL(" IS NULL")])
    return Composed([path_sql, SQL(" IS NOT NULL")])
