"""Test to replicate the query timeout bug reported in PrintOptim backend."""

from unittest.mock import AsyncMock, MagicMock

import pytest
from psycopg import errors

from fraiseql.db import FraiseQLRepository, register_type_for_view


@pytest.mark.asyncio
async def test_find_one_no_longer_uses_parameterized_set_local():
    """Verify that find_one no longer uses parameterized SET LOCAL statement_timeout."""
    # Create a mock pool that simulates the actual psycopg behavior
    mock_pool = MagicMock()
    mock_conn = AsyncMock()
    mock_cursor = AsyncMock()

    # Configure the mocks
    mock_pool_context = AsyncMock()
    mock_pool_context.__aenter__.return_value = mock_conn
    mock_pool_context.__aexit__.return_value = None
    mock_pool.connection = MagicMock(return_value=mock_pool_context)

    mock_cursor_context = MagicMock()
    mock_cursor_context.__aenter__ = AsyncMock(return_value=mock_cursor)
    mock_cursor_context.__aexit__ = AsyncMock(return_value=None)
    mock_conn.cursor = MagicMock(return_value=mock_cursor_context)

    # Track SQL calls
    executed_sql = []

    async def execute_side_effect(sql, params=None):
        executed_sql.append((str(sql), params))
        # Simulate the PostgreSQL error ONLY if parameterized SET LOCAL is used
        if "SET LOCAL statement_timeout = %s" in str(
            sql
        ) or "SET LOCAL statement_timeout = $1" in str(sql):
            raise errors.SyntaxError(
                'syntax error at or near "$1"\nLINE 1: SET LOCAL statement_timeout = $1\n'
                """                                      ^"""
            )

    mock_cursor.execute = AsyncMock(side_effect=execute_side_effect)
    mock_cursor.fetchone = AsyncMock(return_value=None)

    # Register a mock type for gateway_view
    class Gateway:
        id: str
        name: str

    register_type_for_view("gateway_view", Gateway)

    # Create repository with query timeout in development mode
    repo = FraiseQLRepository(mock_pool, context={"query_timeout": 30, "mode": "development"})

    # This should NOT raise an error anymore (bug is fixed)
    result = await repo.find_one("gateway_view", id="d3b8286c-941c-43dc-8b2c-876aa0376855")

    # Verify the SET LOCAL was called with literal value (not parameterized)
    assert len(executed_sql) == 2
    set_local_sql, _ = executed_sql[0]
    assert "SET LOCAL statement_timeout = '30000ms'" in set_local_sql
    assert result is None  # No data found


@pytest.mark.asyncio
async def test_find_one_with_fixed_timeout():
    """Test that find_one works correctly when SET LOCAL uses literal values."""
    mock_pool = MagicMock()
    mock_conn = AsyncMock()
    mock_cursor = AsyncMock()

    # Configure the mocks
    mock_pool_context = AsyncMock()
    mock_pool_context.__aenter__.return_value = mock_conn
    mock_pool_context.__aexit__.return_value = None
    mock_pool.connection = MagicMock(return_value=mock_pool_context)

    mock_cursor_context = MagicMock()
    mock_cursor_context.__aenter__ = AsyncMock(return_value=mock_cursor)
    mock_cursor_context.__aexit__ = AsyncMock(return_value=None)
    mock_conn.cursor = MagicMock(return_value=mock_cursor_context)

    # Track executed SQL statements
    executed_statements = []

    async def execute_side_effect(sql, params=None):
        executed_statements.append((str(sql), params))

    mock_cursor.execute = AsyncMock(side_effect=execute_side_effect)
    mock_cursor.fetchone = AsyncMock(
        return_value={"data": {"id": "test-id", "name": "Test Gateway"}}
    )

    # Register a mock type for gateway_view
    class Gateway:
        id: str
        name: str

    register_type_for_view("gateway_view", Gateway)

    # Create repository with query timeout in development mode
    repo = FraiseQLRepository(mock_pool, context={"query_timeout": 30, "mode": "development"})

    # Execute find_one
    await repo.find_one("gateway_view", id="test-id")

    # Verify the SET LOCAL was called with literal value
    assert len(executed_statements) == 2

    # First statement should be SET LOCAL with literal value
    set_local_sql, set_local_params = executed_statements[0]
    assert "SET LOCAL statement_timeout = '30000ms'" in set_local_sql
    assert set_local_params is None  # No parameters

    # Second statement should be the actual query
    query_sql, query_params = executed_statements[1]
    assert "SELECT" in query_sql
    assert query_params is not None
