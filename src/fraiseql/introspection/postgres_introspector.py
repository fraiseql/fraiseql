"""PostgreSQL database introspection for AutoFraiseQL.

This module provides the core introspection engine that discovers database views,
functions, and their metadata from PostgreSQL catalog tables.
"""

from dataclasses import dataclass
from typing import Optional

import psycopg_pool


@dataclass
class ViewMetadata:
    """Metadata for a database view."""

    schema_name: str
    view_name: str
    definition: str
    comment: Optional[str]
    columns: dict[str, "ColumnInfo"]


@dataclass
class ColumnInfo:
    """Column metadata."""

    name: str
    pg_type: str
    nullable: bool
    comment: Optional[str]


@dataclass
class FunctionMetadata:
    """Metadata for a database function."""

    schema_name: str
    function_name: str
    parameters: list["ParameterInfo"]
    return_type: str
    comment: Optional[str]
    language: str


@dataclass
class ParameterInfo:
    """Function parameter metadata."""

    name: str
    pg_type: str
    mode: str  # IN, OUT, INOUT
    default_value: Optional[str]


class PostgresIntrospector:
    """Introspect PostgreSQL database for FraiseQL metadata."""

    def __init__(self, connection_pool: psycopg_pool.AsyncConnectionPool):
        self.pool = connection_pool

    async def discover_views(
        self, pattern: str = "v_%", schemas: list[str] | None = None
    ) -> list[ViewMetadata]:
        """Discover database views matching the given pattern."""
        if schemas is None:
            schemas = ["public"]
        """Discover views matching pattern.

        Implementation:
        1. Query pg_views for view definitions
        2. Query pg_class for comments
        3. Query pg_attribute for column info
        4. Combine into ViewMetadata objects
        """
        async with self.pool.connection() as conn:
            # Get views
            views_query = """
                SELECT schemaname, viewname, definition
                FROM pg_views
                WHERE schemaname = ANY($1)
                  AND viewname LIKE $2
                ORDER BY schemaname, viewname
            """
            cursor = await conn.execute(views_query, [schemas, pattern])
            view_rows = await cursor.fetchall()

            views = []
            for row in view_rows:
                schema_name, view_name, definition = row

                # Get comment
                comment_query = """
                    SELECT obj_description(c.oid, 'pg_class') as comment
                    FROM pg_class c
                    WHERE c.relname = $1 AND c.relkind = 'v'
                """
                cursor = await conn.execute(comment_query, [view_name])
                comment_row = await cursor.fetchone()
                comment = comment_row[0] if comment_row else None

                # Get columns (simplified for now)
                columns = {}  # TODO(@lionel): Implement column introspection - https://github.com/fraiseql/fraiseql/issues/AUTOFRAISEQL-2

                view_metadata = ViewMetadata(
                    schema_name=schema_name,
                    view_name=view_name,
                    definition=definition,
                    comment=comment,
                    columns=columns,
                )
                views.append(view_metadata)

            return views

    async def discover_functions(
        self, pattern: str = "fn_%", schemas: list[str] | None = None
    ) -> list[FunctionMetadata]:
        """Discover database functions matching the given pattern."""
        if schemas is None:
            schemas = ["public"]
        """Discover functions matching pattern."""
        async with self.pool.connection() as conn:
            query = """
            SELECT
                n.nspname as schema_name,
                p.proname as function_name,
                pg_get_function_arguments(p.oid) as arguments,
                pg_get_function_result(p.oid) as return_type,
                obj_description(p.oid, 'pg_proc') as comment,
                l.lanname as language
            FROM pg_proc p
            JOIN pg_namespace n ON n.oid = p.pronamespace
            JOIN pg_language l ON l.oid = p.prolang
            WHERE n.nspname = ANY($1)
              AND p.proname LIKE $2
            ORDER BY n.nspname, p.proname
            """
            cursor = await conn.execute(query, [schemas, pattern])
            rows = await cursor.fetchall()

            functions = []
            for row in rows:
                schema_name, function_name, _arguments, return_type, comment, language = row

                # Parse parameters (simplified)
                parameters = []  # TODO(@lionel): Implement parameter parsing - https://github.com/fraiseql/fraiseql/issues/AUTOFRAISEQL-3

                function_metadata = FunctionMetadata(
                    schema_name=schema_name,
                    function_name=function_name,
                    parameters=parameters,
                    return_type=return_type,
                    comment=comment,
                    language=language,
                )
                functions.append(function_metadata)

            return functions
