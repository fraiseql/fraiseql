"""LTree hierarchical path operators for building SQL WHERE conditions.

This module provides clean functions to build SQL for LTree hierarchical operations
using proper PostgreSQL ltree casting and specialized hierarchical operators.
"""

from psycopg.sql import SQL, Composed, Literal


def build_ltree_eq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for LTree equality with proper ltree casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'category_path')
        value: LTree path string value

    Returns:
        Composed SQL: (path)::ltree = 'value'::ltree
    """
    return Composed([SQL("("), path_sql, SQL(")::ltree = "), Literal(value), SQL("::ltree")])


def build_ltree_neq_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for LTree inequality with proper ltree casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'category_path')
        value: LTree path string value

    Returns:
        Composed SQL: (path)::ltree != 'value'::ltree
    """
    return Composed([SQL("("), path_sql, SQL(")::ltree != "), Literal(value), SQL("::ltree")])


def build_ltree_in_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for LTree IN list with proper ltree casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'category_path')
        value: List of LTree path strings

    Returns:
        Composed SQL: (path)::ltree IN ('val1'::ltree, 'val2'::ltree, ...)

    Raises:
        TypeError: If value is not a list
    """
    if not isinstance(value, list):
        raise TypeError(f"'in' operator requires a list, got {type(value)}")

    parts = [SQL("("), path_sql, SQL(")::ltree IN (")]

    for i, ltree_path in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.extend([Literal(ltree_path), SQL("::ltree")])

    parts.append(SQL(")"))
    return Composed(parts)


def build_ltree_notin_sql(path_sql: SQL, value: list[str]) -> Composed:
    """Build SQL for LTree NOT IN list with proper ltree casting.

    Args:
        path_sql: The SQL path expression (e.g., data->>'category_path')
        value: List of LTree path strings

    Returns:
        Composed SQL: (path)::ltree NOT IN ('val1'::ltree, 'val2'::ltree, ...)

    Raises:
        TypeError: If value is not a list
    """
    if not isinstance(value, list):
        raise TypeError(f"'notin' operator requires a list, got {type(value)}")

    parts = [SQL("("), path_sql, SQL(")::ltree NOT IN (")]

    for i, ltree_path in enumerate(value):
        if i > 0:
            parts.append(SQL(", "))
        parts.extend([Literal(ltree_path), SQL("::ltree")])

    parts.append(SQL(")"))
    return Composed(parts)


def build_ancestor_of_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for LTree ancestor_of (@>) relationship.

    The @> operator checks if the left path is an ancestor of the right path.

    Args:
        path_sql: The SQL path expression (e.g., data->>'category_path')
        value: LTree path string value to check as descendant

    Returns:
        Composed SQL: (path)::ltree @> 'value'::ltree
    """
    return Composed([SQL("("), path_sql, SQL(")::ltree @> "), Literal(value), SQL("::ltree")])


def build_descendant_of_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for LTree descendant_of (<@) relationship.

    The <@ operator checks if the left path is a descendant of the right path.

    Args:
        path_sql: The SQL path expression (e.g., data->>'category_path')
        value: LTree path string value to check as ancestor

    Returns:
        Composed SQL: (path)::ltree <@ 'value'::ltree
    """
    return Composed([SQL("("), path_sql, SQL(")::ltree <@ "), Literal(value), SQL("::ltree")])


def build_matches_lquery_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for LTree matches_lquery (~) pattern matching.

    The ~ operator checks if the left path matches the lquery pattern.

    Args:
        path_sql: The SQL path expression (e.g., data->>'category_path')
        value: lquery pattern string (e.g., "science.*")

    Returns:
        Composed SQL: (path)::ltree ~ 'pattern'::lquery
    """
    return Composed([SQL("("), path_sql, SQL(")::ltree ~ "), Literal(value), SQL("::lquery")])


def build_matches_ltxtquery_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for LTree matches_ltxtquery (?) text query.

    The ? operator checks if the left path matches the ltxtquery text pattern.

    Args:
        path_sql: The SQL path expression (e.g., data->>'category_path')
        value: ltxtquery text pattern string

    Returns:
        Composed SQL: (path)::ltree ? 'pattern'::ltxtquery
    """
    return Composed([SQL("("), path_sql, SQL(")::ltree ? "), Literal(value), SQL("::ltxtquery")])
