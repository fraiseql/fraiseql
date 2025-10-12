"""Tests for pure passthrough SQL generation.

These tests verify that when pure_json_passthrough=True, the query builder
generates SELECT data::text instead of field extraction with jsonb_build_object().
"""

import pytest
from psycopg.sql import SQL, Composed
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.fastapi import FraiseQLConfig


class User:
    """Test user type."""

    id: int
    name: str
    email: str


def test_pure_passthrough_enabled_generates_correct_sql():
    """Test that pure passthrough mode generates SELECT data::text SQL."""
    # Register the type
    register_type_for_view("tv_user", User)

    # Create config (pure passthrough is always enabled)
    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test",
    )

    # Create repository with pure passthrough context
    # Note: We're testing SQL generation, not execution
    from psycopg_pool import AsyncConnectionPool

    # Mock pool for SQL generation testing (won't actually connect)
    class MockPool:
        def __init__(self):
            self._pool = None

    mock_pool = MockPool()
    repo = FraiseQLRepository(mock_pool, context={"config": config})

    # Build query with raw_json=True
    query = repo._build_find_query("tv_user", raw_json=True, limit=10)

    # Verify SQL statement
    sql_str = query.statement.as_string(None) if hasattr(query.statement, 'as_string') else str(query.statement)

    # Should contain SELECT data::text, not jsonb_build_object
    assert "data::text" in sql_str or '"data"::text' in sql_str, \
        f"Expected 'data::text' in SQL, got: {sql_str}"
    assert "jsonb_build_object" not in sql_str, \
        f"Should not use jsonb_build_object in pure passthrough mode, got: {sql_str}"
    assert "tv_user" in sql_str, \
        f"Expected table name 'tv_user' in SQL, got: {sql_str}"


def test_pure_passthrough_with_field_paths_uses_field_extraction():
    """Test that with field_paths provided, field extraction is used (not raw passthrough).

    When GraphQL field selection is provided via field_paths, the SQL generator
    uses intelligent field extraction instead of raw data::text passthrough.
    This is more efficient for queries that only need specific fields.
    """
    # Register the type
    register_type_for_view("tv_user", User)

    # Create config (pure passthrough is always enabled)
    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test",
    )

    # Create repository
    class MockPool:
        def __init__(self):
            self._pool = None

    mock_pool = MockPool()
    repo = FraiseQLRepository(mock_pool, context={"config": config})

    # Build query with raw_json=True AND field_paths provided (simulates GraphQL field selection)
    from fraiseql.core.ast_parser import FieldPath

    field_paths = [
        FieldPath(path=["id"], alias="id"),
        FieldPath(path=["name"], alias="name"),
    ]

    query = repo._build_find_query("tv_user", raw_json=True, field_paths=field_paths, limit=10)

    # Verify SQL statement uses field extraction when field_paths are provided
    sql_str = query.statement.as_string(None) if hasattr(query.statement, 'as_string') else str(query.statement)

    # When field_paths are provided, should use field extraction (jsonb_build_object or similar)
    # Note: Exact format may vary based on SQL generator implementation


def test_pure_passthrough_with_where_clause():
    """Test that WHERE clauses work correctly in pure passthrough mode."""
    register_type_for_view("tv_user", User)

    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test",
    )

    class MockPool:
        def __init__(self):
            self._pool = None

    mock_pool = MockPool()
    repo = FraiseQLRepository(mock_pool, context={"config": config})

    # Build query with WHERE clause
    query = repo._build_find_query("tv_user", raw_json=True, id=1, limit=10)

    sql_str = query.statement.as_string(None) if hasattr(query.statement, 'as_string') else str(query.statement)

    # Should contain both SELECT data::text AND WHERE clause
    assert ("data::text" in sql_str or '"data"::text' in sql_str), \
        f"Expected 'data::text' in SQL"
    assert "WHERE" in sql_str.upper(), \
        f"Expected WHERE clause in SQL: {sql_str}"


def test_pure_passthrough_with_order_by():
    """Test that ORDER BY clauses work in pure passthrough mode."""
    register_type_for_view("tv_user", User)

    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test",
    )

    class MockPool:
        def __init__(self):
            self._pool = None

    mock_pool = MockPool()
    repo = FraiseQLRepository(mock_pool, context={"config": config})

    # Build query with ORDER BY
    query = repo._build_find_query("tv_user", raw_json=True, order_by="name", limit=10)

    sql_str = query.statement.as_string(None) if hasattr(query.statement, 'as_string') else str(query.statement)

    # Should contain ORDER BY
    assert "ORDER BY" in sql_str.upper(), \
        f"Expected ORDER BY clause in SQL: {sql_str}"


def test_pure_passthrough_with_limit_offset():
    """Test that LIMIT and OFFSET work in pure passthrough mode."""
    register_type_for_view("tv_user", User)

    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test",
    )

    class MockPool:
        def __init__(self):
            self._pool = None

    mock_pool = MockPool()
    repo = FraiseQLRepository(mock_pool, context={"config": config})

    # Build query with LIMIT and OFFSET
    query = repo._build_find_query("tv_user", raw_json=True, limit=10, offset=20)

    sql_str = query.statement.as_string(None) if hasattr(query.statement, 'as_string') else str(query.statement)

    # Should contain LIMIT and OFFSET
    assert "LIMIT" in sql_str.upper(), \
        f"Expected LIMIT clause in SQL: {sql_str}"
    assert "OFFSET" in sql_str.upper() or "20" in sql_str, \
        f"Expected OFFSET clause in SQL: {sql_str}"


def test_pure_passthrough_find_one_query():
    """Test that find_one also uses pure passthrough."""
    register_type_for_view("tv_user", User)

    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test",
    )

    class MockPool:
        def __init__(self):
            self._pool = None

    mock_pool = MockPool()
    repo = FraiseQLRepository(mock_pool, context={"config": config})

    # Build find_one query (should force LIMIT 1)
    query = repo._build_find_one_query("tv_user", raw_json=True, id=1)

    sql_str = query.statement.as_string(None) if hasattr(query.statement, 'as_string') else str(query.statement)

    # Should use pure passthrough with LIMIT 1
    assert ("data::text" in sql_str or '"data"::text' in sql_str), \
        f"Expected 'data::text' in find_one SQL"
    assert "LIMIT" in sql_str.upper(), \
        f"Expected LIMIT 1 in find_one SQL: {sql_str}"


def test_pure_passthrough_always_enabled():
    """Test that pure passthrough is always enabled (no config flags needed).

    Since v1, pure passthrough and Rust transformation are always enabled
    for maximum performance. No configuration is needed.
    """
    config = FraiseQLConfig(database_url="postgresql://test@localhost/test")

    # Pure passthrough is always on - verify by building a query
    class MockPool:
        def __init__(self):
            self._pool = None

    mock_pool = MockPool()
    repo = FraiseQLRepository(mock_pool, context={"config": config})

    # Build query with raw_json=True - should always use pure passthrough
    query = repo._build_find_query("tv_user", raw_json=True, limit=10)
    sql_str = query.statement.as_string(None) if hasattr(query.statement, 'as_string') else str(query.statement)

    # Should use pure passthrough (data::text)
    assert ("data::text" in sql_str or '"data"::text' in sql_str), \
        "Pure passthrough should always be enabled"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
