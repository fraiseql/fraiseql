"""Text/string operators (contains, startswith, endswith, matches)."""

from psycopg.sql import SQL, Composed, Literal


def build_contains_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for substring containment using LIKE."""
    like_value = f"%{value}%"
    return Composed([path_sql, SQL(" LIKE "), Literal(like_value)])


def build_startswith_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for prefix matching using LIKE."""
    like_value = f"{value}%"
    return Composed([path_sql, SQL(" LIKE "), Literal(like_value)])


def build_endswith_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for suffix matching using LIKE."""
    like_value = f"%{value}"
    return Composed([path_sql, SQL(" LIKE "), Literal(like_value)])


def build_matches_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for regex matching."""
    return Composed([path_sql, SQL(" ~ "), Literal(value)])


def build_imatches_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for case-insensitive regex matching."""
    return Composed([path_sql, SQL(" ~* "), Literal(value)])


def build_not_matches_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for negative regex matching."""
    return Composed([path_sql, SQL(" !~ "), Literal(value)])
