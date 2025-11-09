"""Utility functions for schema-qualified database queries in tests.

This module provides helper functions for working with schema-qualified queries
in database tests, particularly useful when using the committed data fixture pattern.
"""

from typing import Optional

from psycopg import AsyncConnection
from psycopg.sql import SQL, Composed, Identifier


async def get_current_schema(connection: AsyncConnection) -> str:
    """Get the current schema name from a database connection.

    Args:
        connection: The database connection to query
    Returns:
        The current schema name, or "public" as fallback
    """
    try:
        result = await connection.execute("SHOW search_path")
        row = await result.fetchone()
        if row and row[0]:
            # First item in search_path is our test schema
            search_path = row[0].strip().strip('"')
            return search_path.split(",")[0].strip().strip('"')
    except Exception:
        pass
    return "public"  # Fallback


def schema_qualified_sql(sql_template: str, schema: str, *identifiers: str) -> SQL:
    """Create a schema-qualified SQL query.

    Args:
        sql_template: SQL template with {} placeholders for schema and identifiers
        schema: The schema name to use
        *identifiers: Additional identifiers (table names, column names, etc.)

    Returns:
        A SQL object with proper schema qualification
    Examples:
        >>> schema_qualified_sql("SELECT * FROM {}.{}", "test_schema", "users")
        SQL('SELECT * FROM "test_schema"."users"')

        >>> schema_qualified_sql("UPDATE {}.{} SET {} = %s", "test_schema", "users", "name")
        SQL('UPDATE "test_schema"."users" SET "name" = %s')
    """
    format_args = [Identifier(schema)] + [Identifier(ident) for ident in identifiers]
    return SQL(sql_template).format(*format_args)


def schema_qualified_composed(
    schema: str,
    table: str,
    operation: str = "SELECT",
    columns: Optional[list[str]] = None,
    where_clause: Optional[str] = None,
) -> Composed:
    """Create a schema-qualified composed SQL query.

    Args:
        schema: The schema name,
        table: The table name
        operation: SQL operation (SELECT, INSERT, UPDATE, DELETE)
        columns: List of column names for SELECT or INSERT,
        where_clause: WHERE clause (without the WHERE keyword)

    Returns:
        A Composed SQL object with proper schema qualification
    Examples:
        >>> schema_qualified_composed("test_schema", "users")
        Composed([SQL('SELECT * FROM '), Identifier('test_schema', 'users')])

        >>> schema_qualified_composed("test_schema", "users", columns=["id", "name"])
        Composed([SQL('SELECT '), Identifier('id'), SQL(', '), Identifier('name')
                 SQL(' FROM '), Identifier('test_schema', 'users')])
    """
    parts = []

    if operation.upper() == "SELECT":
        parts.append(SQL("SELECT "))
        if columns:
            for i, col in enumerate(columns):
                if i > 0:
                    parts.append(SQL(", "))
                parts.append(Identifier(col))
        else:
            parts.append(SQL("*"))
        parts.append(SQL(" FROM "))
        parts.append(Identifier(schema, table))

    elif operation.upper() in ("INSERT", "UPDATE", "DELETE"):
        parts.append(SQL(f"{operation.upper()} "))
        if operation.upper() != "DELETE":
            parts.append(SQL("INTO " if operation.upper() == "INSERT" else ""))
        parts.append(Identifier(schema, table))

    else:
        # Generic operation
        parts.append(SQL(f"{operation} "))
        parts.append(Identifier(schema, table))

    if where_clause:
        parts.append(SQL(f" WHERE {where_clause}"))

    return Composed(parts)


async def create_test_schema_context(connection: AsyncConnection) -> str:
    """Create a unique test schema and set it as the current search path.

    Args:
        connection: Database connection to use
    Returns:
        The name of the created test schema

    Note:
        This is typically used in test fixtures. The schema should be cleaned up
        after the test using drop_test_schema_context().
    """
    import uuid

    test_schema = f"test_{uuid.uuid4().hex[:8]}"

    # Create and use test schema
    await connection.execute(f"CREATE SCHEMA {test_schema}")
    await connection.execute(f"SET search_path TO {test_schema}, public")

    return test_schema


async def drop_test_schema_context(connection: AsyncConnection, schema: str) -> None:
    """Drop a test schema and all its contents.

    Args:
        connection: Database connection to use
        schema: Name of the schema to drop
    """
    await connection.execute(f"DROP SCHEMA {schema} CASCADE")
    await connection.commit()


class SchemaQualifiedQueryBuilder:
    """Helper class for building schema-qualified queries.

    This class provides a fluent interface for building complex schema-qualified
    SQL queries in tests.
    """

    def __init__(self, schema: str) -> None:
        """Initialize the query builder with a schema name.

        Args:
            schema: The schema name to use for all queries
        """
        self.schema = schema
        self._parts = []

    def select(self, *columns: str) -> "SchemaQualifiedQueryBuilder":
        """Add a SELECT clause."""
        self._parts.append(SQL("SELECT "))
        if columns:
            for i, col in enumerate(columns):
                if i > 0:
                    self._parts.append(SQL(", "))
                # Check if this is a simple column name or an expression
                if (
                    " " in col
                    or "(" in col
                    or ">" in col
                    or "-" in col
                    or "'" in col
                    or ":" in col
                    or "as " in col.lower()
                ):
                    # This is an expression, use SQL instead of Identifier
                    self._parts.append(SQL(col))
                else:
                    # This is a simple column name, use Identifier
                    self._parts.append(Identifier(col))
        else:
            self._parts.append(SQL("*"))
        return self

    def from_table(self, table: str) -> "SchemaQualifiedQueryBuilder":
        """Add a FROM clause with schema-qualified table."""
        self._parts.append(SQL(" FROM "))
        self._parts.append(Identifier(self.schema, table))
        return self

    def where(self, condition: str) -> "SchemaQualifiedQueryBuilder":
        """Add a WHERE clause."""
        self._parts.append(SQL(f" WHERE {condition}"))
        return self

    def order_by(self, *columns: str) -> "SchemaQualifiedQueryBuilder":
        """Add an ORDER BY clause."""
        self._parts.append(SQL(" ORDER BY "))
        for i, col in enumerate(columns):
            if i > 0:
                self._parts.append(SQL(", "))
            self._parts.append(Identifier(col))
        return self

    def group_by(self, *columns: str) -> "SchemaQualifiedQueryBuilder":
        """Add a GROUP BY clause."""
        self._parts.append(SQL(" GROUP BY "))
        for i, col in enumerate(columns):
            if i > 0:
                self._parts.append(SQL(", "))
            # GROUP BY expressions should not be quoted
            self._parts.append(SQL(col))
        return self

    def limit(self, count: int) -> "SchemaQualifiedQueryBuilder":
        """Add a LIMIT clause."""
        self._parts.append(SQL(f" LIMIT {count}"))
        return self

    def insert_into(self, table: str) -> "SchemaQualifiedQueryBuilder":
        """Add an INSERT INTO clause."""
        self._parts.append(SQL("INSERT INTO "))
        self._parts.append(Identifier(self.schema, table))
        return self

    def values(self, columns: list[str]) -> "SchemaQualifiedQueryBuilder":
        """Add a VALUES clause for INSERT."""
        self._parts.append(SQL(" ("))
        for i, col in enumerate(columns):
            if i > 0:
                self._parts.append(SQL(", "))
            self._parts.append(Identifier(col))
        self._parts.append(SQL(") VALUES ("))
        for i in range(len(columns)):
            if i > 0:
                self._parts.append(SQL(", "))
            self._parts.append(SQL(f"%({columns[i]})s"))
        self._parts.append(SQL(")"))
        return self

    def update_table(self, table: str) -> "SchemaQualifiedQueryBuilder":
        """Add an UPDATE clause."""
        self._parts.append(SQL("UPDATE "))
        self._parts.append(Identifier(self.schema, table))
        return self

    def set_columns(self, **kwargs) -> "SchemaQualifiedQueryBuilder":
        """Add a SET clause for UPDATE."""
        self._parts.append(SQL(" SET "))
        for i, (col, _) in enumerate(kwargs.items()):
            if i > 0:
                self._parts.append(SQL(", "))
            self._parts.append(Identifier(col))
            self._parts.append(SQL(f" = %({col})s"))
        return self

    def delete_from(self, table: str) -> "SchemaQualifiedQueryBuilder":
        """Add a DELETE FROM clause."""
        self._parts.append(SQL("DELETE FROM "))
        self._parts.append(Identifier(self.schema, table))
        return self

    def returning(self, *columns: str) -> "SchemaQualifiedQueryBuilder":
        """Add a RETURNING clause."""
        self._parts.append(SQL(" RETURNING "))
        if columns:
            for i, col in enumerate(columns):
                if i > 0:
                    self._parts.append(SQL(", "))
                self._parts.append(Identifier(col))
        else:
            self._parts.append(SQL("*"))
        return self

    def extend_query(self, additional_sql: str) -> "SchemaQualifiedQueryBuilder":
        """Add custom SQL to the query."""
        self._parts.append(SQL(additional_sql))
        return self

    def build(self) -> Composed:
        """Build the final Composed SQL query."""
        return Composed(self._parts)

    def reset(self) -> "SchemaQualifiedQueryBuilder":
        """Reset the builder to start a new query."""
        self._parts = []
        return self


# Convenience functions for common query patterns
def build_select_query(
    schema: str,
    table: str,
    columns: Optional[list[str]] = None,
    where: Optional[str] = None,
    group_by: Optional[list[str]] = None,
    order_by: Optional[list[str]] = None,
    limit: Optional[int] = None,
) -> Composed:
    """Build a SELECT query with schema qualification.

    Args:
        schema: Schema name
        table: Table name
        columns: Column names to select (None for *)
        where: WHERE clause condition
        group_by: Columns to group by
        order_by: Columns to order by
        limit: LIMIT value

    Returns:
        Composed SQL query
    """
    builder = SchemaQualifiedQueryBuilder(schema).select(*(columns or [])).from_table(table)

    if where:
        builder.where(where)
    if group_by:
        builder.group_by(*group_by)
    if order_by:
        builder.order_by(*order_by)
    if limit:
        builder.limit(limit)

    return builder.build()


def build_insert_query(
    schema: str, table: str, columns: list[str], returning: Optional[list[str]] = None
) -> Composed:
    """Build an INSERT query with schema qualification.

    Args:
        schema: Schema name
        table: Table name
        columns: Column names for the insert
        returning: Columns to return (None for no RETURNING clause)

    Returns:
        Composed SQL query
    """
    builder = SchemaQualifiedQueryBuilder(schema).insert_into(table).values(columns)

    if returning:
        builder.returning(*returning)

    return builder.build()


def build_update_query(
    schema: str,
    table: str,
    set_columns: dict[str, any],
    where: str,
    returning: Optional[list[str]] = None,
) -> Composed:
    """Build an UPDATE query with schema qualification.

    Args:
        schema: Schema name
        table: Table name
        set_columns: Dictionary of column names to update
        where: WHERE clause condition
        returning: Columns to return (None for no RETURNING clause)

    Returns:
        Composed SQL query
    """
    builder = (
        SchemaQualifiedQueryBuilder(schema)
        .update_table(table)
        .set_columns(**set_columns)
        .where(where)
    )

    if returning:
        builder.returning(*returning)

    return builder.build()


def build_delete_query(
    schema: str, table: str, where: str, returning: Optional[list[str]] = None
) -> Composed:
    """Build a DELETE query with schema qualification.

    Args:
        schema: Schema name
        table: Table name
        where: WHERE clause condition
        returning: Columns to return (None for no RETURNING clause)

    Returns:
        Composed SQL query
    """
    builder = SchemaQualifiedQueryBuilder(schema).delete_from(table).where(where)

    if returning:
        builder.returning(*returning)

    return builder.build()
