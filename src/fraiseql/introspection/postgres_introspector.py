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


@dataclass
class CompositeAttribute:
    """Metadata for a single attribute in a PostgreSQL composite type."""

    name: str  # Attribute name (e.g., "email")
    pg_type: str  # PostgreSQL type (e.g., "text", "uuid")
    ordinal_position: int  # Position in type (1, 2, 3, ...)
    comment: Optional[str]  # Column comment (contains @fraiseql:field metadata)


@dataclass
class CompositeTypeMetadata:
    """Metadata for a PostgreSQL composite type."""

    schema_name: str  # Schema (e.g., "app")
    type_name: str  # Type name (e.g., "type_create_contact_input")
    attributes: list[CompositeAttribute]  # List of attributes/fields
    comment: Optional[str]  # Type comment (contains @fraiseql:input metadata)


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
                WHERE schemaname = ANY(%s)
                  AND viewname LIKE %s
                ORDER BY schemaname, viewname
            """
            cursor = await conn.execute(views_query, (schemas, pattern))
            view_rows = await cursor.fetchall()

            views = []
            for row in view_rows:
                schema_name, view_name, definition = row

                # Get comment
                comment_query = """
                    SELECT obj_description(c.oid, 'pg_class') as comment
                    FROM pg_class c
                    WHERE c.relname = %s AND c.relkind = 'v'
                """
                cursor = await conn.execute(comment_query, (view_name,))
                comment_row = await cursor.fetchone()
                comment = comment_row[0] if comment_row else None

                # Get columns from pg_attribute
                columns_query = """
                    SELECT
                        a.attname as column_name,
                        t.typname as pg_type,
                        a.attnotnull as not_null,
                        col_description(a.attrelid, a.attnum) as column_comment
                    FROM pg_attribute a
                    JOIN pg_type t ON a.atttypid = t.oid
                    JOIN pg_class c ON a.attrelid = c.oid
                    WHERE c.relname = %s
                      AND c.relkind = 'v'
                      AND a.attnum > 0  -- Skip system columns
                      AND NOT a.attisdropped  -- Skip dropped columns
                    ORDER BY a.attnum
                """
                cursor = await conn.execute(columns_query, (view_name,))
                column_rows = await cursor.fetchall()

                columns = {}
                for col_row in column_rows:
                    column_name, pg_type, not_null, column_comment = col_row
                    columns[column_name] = ColumnInfo(
                        name=column_name,
                        pg_type=pg_type,
                        nullable=not not_null,  # attnotnull is True when NOT NULL
                        comment=column_comment,
                    )

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
            WHERE n.nspname = ANY(%s)
              AND p.proname LIKE %s
            ORDER BY n.nspname, p.proname
            """
            cursor = await conn.execute(query, (schemas, pattern))
            rows = await cursor.fetchall()

            functions = []
            for row in rows:
                schema_name, function_name, arguments_str, return_type, comment, language = row

                # Parse parameters from arguments string
                parameters = self._parse_function_arguments(arguments_str)

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

    async def discover_composite_type(
        self, type_name: str, schema: str = "app"
    ) -> CompositeTypeMetadata | None:
        """Introspect a PostgreSQL composite type.

        Args:
            type_name: Name of the composite type (e.g., "type_create_contact_input")
            schema: Schema name (default: "app")

        Returns:
            CompositeTypeMetadata if type exists, None otherwise

        Example:
            >>> introspector = PostgresIntrospector(pool)
            >>> metadata = await introspector.discover_composite_type(
            ...     "type_create_contact_input",
            ...     schema="app"
            ... )
            >>> print(metadata.attributes[0].name)  # "email"
        """
        async with self.pool.connection() as conn:
            # Step 1: Get type-level metadata (comment)
            type_query = """
                SELECT
                    t.typname AS type_name,
                    n.nspname AS schema_name,
                    obj_description(t.oid, 'pg_type') AS comment
                FROM pg_type t
                JOIN pg_namespace n ON n.oid = t.typnamespace
                WHERE t.typtype = 'c'         -- 'c' = composite type
                  AND n.nspname = %s          -- Schema filter
                  AND t.typname = %s          -- Type name filter
            """

            type_result = await conn.execute(type_query, (schema, type_name))
            type_row = await type_result.fetchone()

            if not type_row:
                return None  # Composite type not found

            # Step 2: Get attribute-level metadata (fields)
            attr_query = """
                SELECT
                    a.attname AS attribute_name,
                    format_type(a.atttypid, a.atttypmod) AS pg_type,
                    a.attnum AS ordinal_position,
                    col_description(c.oid, a.attnum) AS comment
                FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                JOIN pg_attribute a ON a.attrelid = c.oid
                WHERE c.relkind = 'c'         -- 'c' = composite type
                  AND n.nspname = %s          -- Schema filter
                  AND c.relname = %s          -- Type name filter
                  AND a.attnum > 0            -- Exclude system columns
                  AND NOT a.attisdropped      -- Exclude dropped columns
                ORDER BY a.attnum;
            """

            attr_result = await conn.execute(attr_query, (schema, type_name))
            attr_rows = await attr_result.fetchall()

            # Step 3: Build attribute list
            attributes = [
                CompositeAttribute(
                    name=row[0],  # attribute_name
                    pg_type=row[1],  # pg_type
                    ordinal_position=row[2],  # ordinal_position
                    comment=row[3],  # comment
                )
                for row in attr_rows
            ]

            # Step 4: Return composite type metadata
            return CompositeTypeMetadata(
                schema_name=schema,
                type_name=type_name,
                attributes=attributes,
                comment=type_row[2],  # comment is the 3rd column
            )

    def _parse_function_arguments(self, arguments_str: str) -> list[ParameterInfo]:
        """Parse function arguments string into ParameterInfo objects.

        Args:
            arguments_str: String like "p_name text, p_email text DEFAULT 'test@example.com'"

        Returns:
            List of ParameterInfo objects
        """
        if not arguments_str or arguments_str.strip() == "":
            return []

        parameters = []
        # Split on comma, but be careful with commas inside DEFAULT values
        # This is a simplified parser - for production, might need more sophisticated parsing
        arg_parts = [arg.strip() for arg in arguments_str.split(",") if arg.strip()]

        for arg_part in arg_parts:
            # Parse each argument: "p_name text DEFAULT 'value'" or "p_name text"
            parts = arg_part.split()

            if len(parts) < 2:
                continue  # Skip malformed arguments

            param_name = parts[0]
            param_type = parts[1]

            # Check for DEFAULT value
            default_value = None
            if len(parts) > 2 and parts[2].upper() == "DEFAULT":
                # Join remaining parts as default value
                default_value = " ".join(parts[3:])

            # Determine mode (simplified - assume all are IN parameters)
            mode = "IN"

            parameters.append(
                ParameterInfo(
                    name=param_name,
                    pg_type=param_type,
                    mode=mode,
                    default_value=default_value,
                )
            )

        return parameters
