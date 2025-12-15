"""SQL rendering utilities for test assertions.

This module provides utilities to render psycopg.sql composed objects
into valid SQL strings for assertion testing.

Problem:
    Tests were using str(composed_sql) which returns the Python repr()
    (e.g., "Composed([SQL(...), Literal(...)])") instead of rendered SQL.

Solution:
    Use render_sql_for_testing() to properly render SQL objects to strings.

Example:
    >>> from psycopg.sql import SQL, Literal
    >>> from tests.helpers.sql_rendering import render_sql_for_testing
    >>>
    >>> field = SQL("(data ->> 'port')")
    >>> cast = SQL("::numeric")
    >>> value = Literal(443)
    >>> result = field + cast + SQL(" = ") + value
    >>>
    >>> # BROKEN: Returns repr
    >>> str(result)
    "Composed([SQL(\"(data ->> 'port')\"), SQL('::numeric'), ...])"
    >>>
    >>> # FIXED: Returns actual SQL
    >>> render_sql_for_testing(result)
    "(data ->> 'port')::numeric = 443"
"""

from typing import Any

from psycopg.sql import Composable


def render_sql_for_testing(sql_object: Any) -> str:
    """Render psycopg.sql objects to valid SQL strings for testing.

    This function handles all psycopg.sql types and renders them to
    human-readable SQL strings suitable for assertion testing.

    Args:
        sql_object: A psycopg.sql object (Composed, SQL, Literal, etc.)
                   or any other object that should be converted to string.

    Returns:
        Valid SQL string representation.

    Example:
        >>> strategy = registry.get_strategy("eq", int)
        >>> result = strategy.build_sql(jsonb_path, "eq", 443, int)
        >>> sql = render_sql_for_testing(result)
        >>> assert "::numeric" in sql  # ✅ Works!
        >>> assert "(data ->> 'port')" in sql  # ✅ Works!

    Technical Details:
        Uses as_bytes(None) which works without a database connection.
        This is safe because we're only rendering static SQL for assertions,
        not executing queries.

    See Also:
        - psycopg.sql.Composable.as_bytes()
        - psycopg.sql documentation
    """
    # Handle None
    if sql_object is None:
        return "NULL"

    # Handle psycopg.sql.Composable types (Composed, SQL, Literal, etc.)
    if isinstance(sql_object, Composable):
        try:
            # as_bytes(None) renders SQL without needing a connection
            # This is safe for test assertions
            rendered_bytes = sql_object.as_bytes(None)
            return rendered_bytes.decode("utf-8")
        except Exception as e:
            # If rendering fails, return a descriptive error
            return f"<RenderError: {type(sql_object).__name__} - {e}>"

    # Handle already-stringified SQL (shouldn't happen but be defensive)
    if isinstance(sql_object, str):
        return sql_object

    # Handle bytes
    if isinstance(sql_object, bytes):
        return sql_object.decode("utf-8")

    # Fallback: use str() for other types
    # This handles numbers, booleans, etc.
    return str(sql_object)


def render_sql_list(sql_objects: list[Any]) -> list[str]:
    """Render a list of SQL objects to strings.

    Convenience function for rendering multiple SQL objects at once.

    Args:
        sql_objects: List of psycopg.sql objects

    Returns:
        List of rendered SQL strings

    Example:
        >>> results = [result1, result2, result3]
        >>> sql_strings = render_sql_list(results)
        >>> assert all("::numeric" in s for s in sql_strings)
    """
    return [render_sql_for_testing(obj) for obj in sql_objects]


def assert_sql_contains(sql_object: Any, *expected_substrings: str) -> None:
    """Assert that rendered SQL contains all expected substrings.

    Convenience function that combines rendering and assertion.

    Args:
        sql_object: psycopg.sql object to render
        *expected_substrings: Substrings that should be in the rendered SQL

    Raises:
        AssertionError: If any expected substring is not found

    Example:
        >>> result = strategy.build_sql(path, "eq", 443, int)
        >>> assert_sql_contains(
        ...     result,
        ...     "::numeric",
        ...     "(data ->> 'port')",
        ...     "= 443"
        ... )
    """
    sql_str = render_sql_for_testing(sql_object)

    for expected in expected_substrings:
        assert expected in sql_str, (
            f"Expected substring '{expected}' not found in rendered SQL.\nRendered SQL: {sql_str}"
        )


def assert_sql_pattern(sql_object: Any, pattern: str) -> None:
    r"""Assert that rendered SQL matches a regex pattern.

    Args:
        sql_object: psycopg.sql object to render
        pattern: Regular expression pattern to match

    Raises:
        AssertionError: If pattern doesn't match

    Example:
        >>> import re
        >>> result = strategy.build_sql(path, "eq", 443, int)
        >>> assert_sql_pattern(result, r"\(data ->> 'port'\)::numeric = \d+")
    """
    import re

    sql_str = render_sql_for_testing(sql_object)
    assert re.search(pattern, sql_str), (
        f"Pattern '{pattern}' not found in rendered SQL.\nRendered SQL: {sql_str}"
    )
