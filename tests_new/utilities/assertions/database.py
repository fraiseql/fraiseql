"""Database-specific assertion helpers for FraiseQL testing.

This module provides specialized assertion utilities for database state
validation, including row counts, data integrity, constraint validation,
and JSONB field assertions.
"""

from typing import Any, Dict, List, Optional, Union

import psycopg
from psycopg.sql import SQL, Identifier


async def assert_table_exists(
    connection: psycopg.AsyncConnection, table_name: str, schema: str = "public"
) -> None:
    """Assert table exists in database.

    Args:
        connection: Database connection
        table_name: Name of table to check
        schema: Schema name (default: public)

    Raises:
        AssertionError: If table doesn't exist
    """
    query = """
        SELECT EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = %s AND table_name = %s
        )
    """

    result = await connection.execute(query, (schema, table_name))
    row = await result.fetchone()

    exists = row[0] if row else False
    assert exists, f"Table '{schema}.{table_name}' does not exist"


async def assert_table_empty(
    connection: psycopg.AsyncConnection, table_name: str, schema: str = "public"
) -> None:
    """Assert table is empty.

    Args:
        connection: Database connection
        table_name: Name of table to check
        schema: Schema name

    Raises:
        AssertionError: If table is not empty
    """
    await assert_table_exists(connection, table_name, schema)

    query = SQL("SELECT COUNT(*) FROM {}.{}").format(Identifier(schema), Identifier(table_name))

    result = await connection.execute(query)
    row = await result.fetchone()
    count = row[0] if row else 0

    assert count == 0, f"Table '{schema}.{table_name}' is not empty (count: {count})"


async def assert_table_not_empty(
    connection: psycopg.AsyncConnection, table_name: str, schema: str = "public"
) -> None:
    """Assert table is not empty.

    Args:
        connection: Database connection
        table_name: Name of table to check
        schema: Schema name

    Raises:
        AssertionError: If table is empty
    """
    await assert_table_exists(connection, table_name, schema)

    query = SQL("SELECT COUNT(*) FROM {}.{}").format(Identifier(schema), Identifier(table_name))

    result = await connection.execute(query)
    row = await result.fetchone()
    count = row[0] if row else 0

    assert count > 0, f"Table '{schema}.{table_name}' is empty"


async def assert_row_count(
    connection: psycopg.AsyncConnection,
    table_name: str,
    expected_count: int,
    schema: str = "public",
    where_clause: Optional[str] = None,
    where_params: Optional[tuple] = None,
) -> None:
    """Assert table has expected row count.

    Args:
        connection: Database connection
        table_name: Name of table to check
        expected_count: Expected number of rows
        schema: Schema name
        where_clause: Optional WHERE clause (without WHERE keyword)
        where_params: Parameters for WHERE clause

    Raises:
        AssertionError: If row count doesn't match
    """
    await assert_table_exists(connection, table_name, schema)

    query_parts = [
        SQL("SELECT COUNT(*) FROM {}.{}").format(Identifier(schema), Identifier(table_name))
    ]

    params = []
    if where_clause:
        query_parts.append(SQL(f" WHERE {where_clause}"))
        if where_params:
            params.extend(where_params)

    query = SQL("").join(query_parts)

    result = await connection.execute(query, params)
    row = await result.fetchone()
    actual_count = row[0] if row else 0

    assert actual_count == expected_count, (
        f"Row count mismatch in '{schema}.{table_name}': expected {expected_count}, got {actual_count}"
    )


async def assert_row_exists(
    connection: psycopg.AsyncConnection,
    table_name: str,
    where_clause: str,
    where_params: tuple,
    schema: str = "public",
) -> None:
    """Assert row exists matching criteria.

    Args:
        connection: Database connection
        table_name: Name of table to check
        where_clause: WHERE clause (without WHERE keyword)
        where_params: Parameters for WHERE clause
        schema: Schema name

    Raises:
        AssertionError: If row doesn't exist
    """
    await assert_table_exists(connection, table_name, schema)

    query = SQL("SELECT EXISTS(SELECT 1 FROM {}.{} WHERE {})").format(
        Identifier(schema), Identifier(table_name), SQL(where_clause)
    )

    result = await connection.execute(query, where_params)
    row = await result.fetchone()
    exists = row[0] if row else False

    assert exists, (
        f"Row does not exist in '{schema}.{table_name}' matching: {where_clause} with params {where_params}"
    )


async def assert_row_not_exists(
    connection: psycopg.AsyncConnection,
    table_name: str,
    where_clause: str,
    where_params: tuple,
    schema: str = "public",
) -> None:
    """Assert row does not exist matching criteria.

    Args:
        connection: Database connection
        table_name: Name of table to check
        where_clause: WHERE clause (without WHERE keyword)
        where_params: Parameters for WHERE clause
        schema: Schema name

    Raises:
        AssertionError: If row exists
    """
    await assert_table_exists(connection, table_name, schema)

    query = SQL("SELECT EXISTS(SELECT 1 FROM {}.{} WHERE {})").format(
        Identifier(schema), Identifier(table_name), SQL(where_clause)
    )

    result = await connection.execute(query, where_params)
    row = await result.fetchone()
    exists = row[0] if row else False

    assert not exists, (
        f"Row should not exist in '{schema}.{table_name}' matching: {where_clause} with params {where_params}"
    )


async def assert_field_equals(
    connection: psycopg.AsyncConnection,
    table_name: str,
    field_name: str,
    expected_value: Any,
    where_clause: str,
    where_params: tuple,
    schema: str = "public",
) -> None:
    """Assert field has expected value.

    Args:
        connection: Database connection
        table_name: Name of table
        field_name: Name of field to check
        expected_value: Expected field value
        where_clause: WHERE clause to identify row
        where_params: Parameters for WHERE clause
        schema: Schema name

    Raises:
        AssertionError: If field value doesn't match
    """
    await assert_row_exists(connection, table_name, where_clause, where_params, schema)

    query = SQL("SELECT {} FROM {}.{} WHERE {}").format(
        Identifier(field_name), Identifier(schema), Identifier(table_name), SQL(where_clause)
    )

    result = await connection.execute(query, where_params)
    row = await result.fetchone()

    assert row, f"No row found in '{schema}.{table_name}' matching: {where_clause}"

    actual_value = row[0]

    # Handle UUID comparison by converting both to string
    if hasattr(actual_value, "hex") or hasattr(expected_value, "hex"):
        actual_str = str(actual_value)
        expected_str = str(expected_value)
        assert actual_str == expected_str, (
            f"Field '{field_name}' value mismatch: expected {expected_str}, got {actual_str}"
        )
    else:
        assert actual_value == expected_value, (
            f"Field '{field_name}' value mismatch: expected {expected_value}, got {actual_value}"
        )


async def assert_jsonb_field_equals(
    connection: psycopg.AsyncConnection,
    table_name: str,
    field_name: str,
    jsonb_path: str,
    expected_value: Any,
    where_clause: str,
    where_params: tuple,
    schema: str = "public",
) -> None:
    """Assert JSONB field path has expected value.

    Args:
        connection: Database connection
        table_name: Name of table
        field_name: Name of JSONB field
        jsonb_path: JSONB path (e.g., 'profile.email' or 'tags.0')
        expected_value: Expected value
        where_clause: WHERE clause to identify row
        where_params: Parameters for WHERE clause
        schema: Schema name

    Raises:
        AssertionError: If JSONB value doesn't match
    """
    await assert_row_exists(connection, table_name, where_clause, where_params, schema)

    # Convert dot notation to PostgreSQL JSONB path
    path_parts = jsonb_path.split(".")
    if len(path_parts) == 1:
        # Simple key access
        jsonb_expr = SQL("{}->%s").format(Identifier(field_name))
        jsonb_params = [path_parts[0]]
    else:
        # Nested path access
        jsonb_expr = SQL("{}#>%s").format(Identifier(field_name))
        jsonb_params = [path_parts]

    query = SQL("SELECT {} FROM {}.{} WHERE {}").format(
        jsonb_expr, Identifier(schema), Identifier(table_name), SQL(where_clause)
    )

    all_params = jsonb_params + list(where_params)
    result = await connection.execute(query, all_params)
    row = await result.fetchone()

    assert row, f"No row found in '{schema}.{table_name}' matching: {where_clause}"

    actual_value = row[0]

    # Handle JSON string comparison
    if isinstance(expected_value, str) and isinstance(actual_value, str):
        # Remove quotes from JSON string values
        if actual_value.startswith('"') and actual_value.endswith('"'):
            actual_value = actual_value[1:-1]

    assert actual_value == expected_value, (
        f"JSONB field '{field_name}.{jsonb_path}' value mismatch: expected {expected_value}, got {actual_value}"
    )


async def assert_jsonb_field_contains(
    connection: psycopg.AsyncConnection,
    table_name: str,
    field_name: str,
    expected_subset: Dict[str, Any],
    where_clause: str,
    where_params: tuple,
    schema: str = "public",
) -> None:
    """Assert JSONB field contains expected subset.

    Args:
        connection: Database connection
        table_name: Name of table
        field_name: Name of JSONB field
        expected_subset: Expected JSONB subset
        where_clause: WHERE clause to identify row
        where_params: Parameters for WHERE clause
        schema: Schema name

    Raises:
        AssertionError: If JSONB doesn't contain subset
    """
    await assert_row_exists(connection, table_name, where_clause, where_params, schema)

    query = SQL("SELECT {} @> %s FROM {}.{} WHERE {}").format(
        Identifier(field_name), Identifier(schema), Identifier(table_name), SQL(where_clause)
    )

    import json

    params = [json.dumps(expected_subset)] + list(where_params)
    result = await connection.execute(query, params)
    row = await result.fetchone()

    assert row, f"No row found in '{schema}.{table_name}' matching: {where_clause}"

    contains = row[0]
    assert contains, (
        f"JSONB field '{field_name}' does not contain expected subset: {expected_subset}"
    )


async def assert_foreign_key_constraint(
    connection: psycopg.AsyncConnection,
    child_table: str,
    child_column: str,
    parent_table: str,
    parent_column: str = "id",
    child_schema: str = "public",
    parent_schema: str = "public",
) -> None:
    """Assert foreign key constraint exists.

    Args:
        connection: Database connection
        child_table: Child table name
        child_column: Child column name
        parent_table: Parent table name
        parent_column: Parent column name (default: id)
        child_schema: Child table schema
        parent_schema: Parent table schema

    Raises:
        AssertionError: If foreign key constraint doesn't exist
    """
    query = """
        SELECT EXISTS (
            SELECT 1 FROM information_schema.key_column_usage kcu
            JOIN information_schema.referential_constraints rc ON kcu.constraint_name = rc.constraint_name
            JOIN information_schema.key_column_usage referenced_kcu ON rc.unique_constraint_name = referenced_kcu.constraint_name
            WHERE kcu.table_schema = %s
                AND kcu.table_name = %s
                AND kcu.column_name = %s
                AND referenced_kcu.table_schema = %s
                AND referenced_kcu.table_name = %s
                AND referenced_kcu.column_name = %s
        )
    """

    result = await connection.execute(
        query, (child_schema, child_table, child_column, parent_schema, parent_table, parent_column)
    )
    row = await result.fetchone()

    exists = row[0] if row else False
    assert exists, (
        f"Foreign key constraint not found: {child_schema}.{child_table}.{child_column} -> {parent_schema}.{parent_table}.{parent_column}"
    )


async def assert_unique_constraint(
    connection: psycopg.AsyncConnection,
    table_name: str,
    column_names: Union[str, List[str]],
    schema: str = "public",
) -> None:
    """Assert unique constraint exists on column(s).

    Args:
        connection: Database connection
        table_name: Table name
        column_names: Column name(s) in the constraint
        schema: Schema name

    Raises:
        AssertionError: If unique constraint doesn't exist
    """
    if isinstance(column_names, str):
        column_names = [column_names]

    # Check for unique constraint
    query = """
        SELECT COUNT(*) FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name
        WHERE tc.table_schema = %s
            AND tc.table_name = %s
            AND tc.constraint_type = 'UNIQUE'
            AND kcu.column_name = ANY(%s)
        GROUP BY tc.constraint_name
        HAVING COUNT(*) = %s
    """

    result = await connection.execute(query, (schema, table_name, column_names, len(column_names)))
    row = await result.fetchone()

    exists = row is not None
    assert exists, (
        f"Unique constraint not found on {schema}.{table_name}({', '.join(column_names)})"
    )


async def assert_index_exists(
    connection: psycopg.AsyncConnection, index_name: str, table_name: str, schema: str = "public"
) -> None:
    """Assert index exists on table.

    Args:
        connection: Database connection
        index_name: Index name
        table_name: Table name
        schema: Schema name

    Raises:
        AssertionError: If index doesn't exist
    """
    query = """
        SELECT EXISTS (
            SELECT 1 FROM pg_indexes
            WHERE schemaname = %s
                AND tablename = %s
                AND indexname = %s
        )
    """

    result = await connection.execute(query, (schema, table_name, index_name))
    row = await result.fetchone()

    exists = row[0] if row else False
    assert exists, f"Index '{index_name}' not found on table '{schema}.{table_name}'"


async def assert_function_exists(
    connection: psycopg.AsyncConnection, function_name: str, schema: str = "public"
) -> None:
    """Assert function exists in database.

    Args:
        connection: Database connection
        function_name: Function name
        schema: Schema name

    Raises:
        AssertionError: If function doesn't exist
    """
    query = """
        SELECT EXISTS (
            SELECT 1 FROM information_schema.routines
            WHERE routine_schema = %s
                AND routine_name = %s
        )
    """

    result = await connection.execute(query, (schema, function_name))
    row = await result.fetchone()

    exists = row[0] if row else False
    assert exists, f"Function '{schema}.{function_name}' does not exist"


async def assert_trigger_exists(
    connection: psycopg.AsyncConnection, trigger_name: str, table_name: str, schema: str = "public"
) -> None:
    """Assert trigger exists on table.

    Args:
        connection: Database connection
        trigger_name: Trigger name
        table_name: Table name
        schema: Schema name

    Raises:
        AssertionError: If trigger doesn't exist
    """
    query = """
        SELECT EXISTS (
            SELECT 1 FROM information_schema.triggers
            WHERE event_object_schema = %s
                AND event_object_table = %s
                AND trigger_name = %s
        )
    """

    result = await connection.execute(query, (schema, table_name, trigger_name))
    row = await result.fetchone()

    exists = row[0] if row else False
    assert exists, f"Trigger '{trigger_name}' not found on table '{schema}.{table_name}'"


async def get_row_data(
    connection: psycopg.AsyncConnection,
    table_name: str,
    where_clause: str,
    where_params: tuple,
    schema: str = "public",
) -> Optional[Dict[str, Any]]:
    """Get row data as dictionary.

    Args:
        connection: Database connection
        table_name: Table name
        where_clause: WHERE clause
        where_params: Parameters for WHERE clause
        schema: Schema name

    Returns:
        Optional[Dict]: Row data or None if not found
    """
    query = SQL("SELECT * FROM {}.{} WHERE {}").format(
        Identifier(schema), Identifier(table_name), SQL(where_clause)
    )

    result = await connection.execute(query, where_params)
    row = await result.fetchone()

    if not row:
        return None

    # Get column names
    columns = [desc.name for desc in result.description]

    # Return as dictionary
    return dict(zip(columns, row, strict=False))


async def debug_table_contents(
    connection: psycopg.AsyncConnection, table_name: str, schema: str = "public", limit: int = 10
) -> List[Dict[str, Any]]:
    """Get table contents for debugging (use in test failures).

    Args:
        connection: Database connection
        table_name: Table name
        schema: Schema name
        limit: Maximum rows to return

    Returns:
        List[Dict]: Table rows as dictionaries
    """
    query = SQL("SELECT * FROM {}.{} LIMIT %s").format(Identifier(schema), Identifier(table_name))

    result = await connection.execute(query, (limit,))
    rows = await result.fetchall()

    if not rows:
        return []

    # Get column names
    columns = [desc.name for desc in result.description]

    # Return as list of dictionaries
    return [dict(zip(columns, row, strict=False)) for row in rows]
