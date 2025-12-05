"""Test for SET LOCAL prepared statement bug fix."""

from unittest.mock import AsyncMock, MagicMock, Mock

import pytest
from psycopg.sql import SQL
from psycopg_pool import AsyncConnectionPool

from fraiseql.db import DatabaseQuery, FraiseQLRepository

pytestmark = pytest.mark.database


@pytest.mark.asyncio
async def test_set_local_with_timeout_should_not_use_prepared_statement() -> None:
    """Test that SET LOCAL statement_timeout doesn't use prepared statement parameters."""
    # Create a mock pool and connection
    mock_pool = AsyncMock(spec=AsyncConnectionPool)
    mock_conn = AsyncMock()
    mock_cursor = AsyncMock()

    # Configure the mocks to match psycopg async pattern
    # Pool returns an async context manager
    mock_pool_context = AsyncMock()
    mock_pool_context.__aenter__.return_value = mock_conn
    mock_pool_context.__aexit__.return_value = None
    mock_pool.connection.return_value = mock_pool_context

    # Connection.cursor returns an async context manager
    mock_cursor_context = Mock()
    mock_cursor_context.__aenter__ = AsyncMock(return_value=mock_cursor)
    mock_cursor_context.__aexit__ = AsyncMock(return_value=None)
    mock_conn.cursor = Mock(return_value=mock_cursor_context)

    mock_cursor.fetchall = AsyncMock(return_value=[])
    mock_cursor.execute = AsyncMock()

    # Create repository with query timeout
    repo = FraiseQLRepository(mock_pool, context={"query_timeout": 30})

    # Execute a query
    query = DatabaseQuery(statement=SQL("SELECT * FROM test_table"), params={}, fetch_result=True)

    await repo.run(query)

    # Check that SET LOCAL was called
    calls = mock_cursor.execute.call_args_list
    assert len(calls) == 2  # SET LOCAL + actual query

    # First call should be SET LOCAL without prepared statement parameters
    set_local_call = calls[0]
    sql_statement = set_local_call[0][0]
    params = set_local_call[0][1] if len(set_local_call[0]) > 1 else None

    # After fix: Should use literal value, not parameters
    assert "SET LOCAL statement_timeout = '30000ms'" in sql_statement
    assert params is None  # No parameters should be passed


@pytest.mark.asyncio
async def test_execute_function_set_local_bug() -> None:
    """Test that execute_function also has the SET LOCAL bug."""
    # Create a mock pool and connection
    mock_pool = MagicMock()  # Not AsyncMock to control attributes,
    mock_conn = AsyncMock()
    mock_cursor = AsyncMock()

    # Configure the mocks
    mock_pool_context = AsyncMock()
    mock_pool_context.__aenter__.return_value = mock_conn
    mock_pool_context.__aexit__.return_value = None

    # Set up the pool to have connection attribute (psycopg pool)
    mock_pool.connection = MagicMock(return_value=mock_pool_context)

    mock_cursor_context = Mock()
    mock_cursor_context.__aenter__ = AsyncMock(return_value=mock_cursor)
    mock_cursor_context.__aexit__ = AsyncMock(return_value=None)
    mock_conn.cursor = Mock(return_value=mock_cursor_context)

    mock_cursor.fetchone = AsyncMock(return_value={"status": "success"})
    mock_cursor.execute = AsyncMock()

    # Create repository with query timeout
    repo = FraiseQLRepository(mock_pool, context={"query_timeout": 10})

    # Execute a function
    await repo.execute_function("test_function", {"data": "test"})

    # Check that SET LOCAL was called with prepared statement (the bug)
    calls = mock_cursor.execute.call_args_list
    assert len(calls) == 2  # SET LOCAL + function call

    # First call should be SET LOCAL without parameters (fixed)
    set_local_call = calls[0]
    sql_statement = set_local_call[0][0]
    params = set_local_call[0][1] if len(set_local_call[0]) > 1 else None

    assert "SET LOCAL statement_timeout = '10000ms'" in sql_statement
    assert params is None  # No parameters should be passed


@pytest.mark.asyncio
async def test_no_timeout_skips_set_local() -> None:
    """Test that when query_timeout is None, SET LOCAL is not executed."""
    # Create a mock pool and connection
    mock_pool = AsyncMock(spec=AsyncConnectionPool)
    mock_conn = AsyncMock()
    mock_cursor = AsyncMock()

    # Configure the mocks
    mock_pool_context = AsyncMock()
    mock_pool_context.__aenter__.return_value = mock_conn
    mock_pool_context.__aexit__.return_value = None
    mock_pool.connection.return_value = mock_pool_context

    mock_cursor_context = Mock()
    mock_cursor_context.__aenter__ = AsyncMock(return_value=mock_cursor)
    mock_cursor_context.__aexit__ = AsyncMock(return_value=None)
    mock_conn.cursor = Mock(return_value=mock_cursor_context)

    mock_cursor.fetchall = AsyncMock(return_value=[])
    mock_cursor.execute = AsyncMock()

    # Create repository without query timeout
    repo = FraiseQLRepository(mock_pool, context={"query_timeout": None})

    # Execute a query
    query = DatabaseQuery(statement=SQL("SELECT * FROM test_table"), params={}, fetch_result=True)

    await repo.run(query)

    # Check that only the main query was executed, no SET LOCAL
    assert mock_cursor.execute.call_count == 1
    main_call = mock_cursor.execute.call_args_list[0]
    sql_statement = main_call[0][0]
    assert "SELECT * FROM test_table" in str(sql_statement)

    # Verify no SET LOCAL was called
    for call in mock_cursor.execute.call_args_list:
        assert "SET LOCAL" not in str(call[0][0])
