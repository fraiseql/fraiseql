"""Convert psycopg SQL objects to string representation.

Phase 7.1 - SQL String Conversion for Rust Query Builder
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from psycopg.sql import SQL, Composed


def sql_to_string(sql_obj: Composed | SQL | None) -> str | None:
    r"""Convert psycopg SQL object to string.

    This function renders a psycopg Composed/SQL object into its string
    representation without requiring a database connection.

    Args:
        sql_obj: psycopg Composed or SQL object

    Returns:
        SQL string, or None if input is None

    Example:
        >>> from psycopg.sql import SQL, Identifier, Literal
        >>> where = SQL('WHERE ') + Identifier('status') + SQL(' = ') + Literal('active')
        >>> sql_to_string(where)
        'WHERE "status" = \'active\''

    Note:
        psycopg's as_string(None) works without a connection - it uses
        default PostgreSQL identifier/literal quoting rules.
    """
    if sql_obj is None:
        return None

    # Both SQL and Composed support as_string(None)
    return sql_obj.as_string(None)
