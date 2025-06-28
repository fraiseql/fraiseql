"""Test database context parameter functionality."""

import json
from unittest.mock import AsyncMock, MagicMock
from uuid import uuid4

import pytest

from fraiseql.db import FraiseQLRepository


class TestDatabaseContextParameters:
    """Test context parameter support in database layer."""

    @pytest.mark.asyncio
    async def test_execute_function_with_context_psycopg(self):
        """Test execute_function_with_context with psycopg pool."""
        # Create mock psycopg pool and connection
        mock_pool = MagicMock()
        mock_conn = AsyncMock()
        mock_cursor = AsyncMock()

        # Configure pool to have connection() method (psycopg style)
        mock_pool.connection.return_value.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_pool.connection.return_value.__aexit__ = AsyncMock(return_value=None)

        # Configure connection cursor
        mock_conn.cursor.return_value.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_conn.cursor.return_value.__aexit__ = AsyncMock(return_value=None)

        # Configure cursor to return result
        mock_result = {"success": True, "location_id": str(uuid4())}
        mock_cursor.fetchone.return_value = mock_result

        # Create repository
        repo = FraiseQLRepository(mock_pool)

        # Test data
        function_name = "app.create_location"
        context_args = ["tenant-123", "user-456"]
        input_data = {"name": "Test Location", "address": "123 Test St"}

        # Call method
        result = await repo.execute_function_with_context(
            function_name,
            context_args,
            input_data,
        )

        # Verify result
        assert result == mock_result

        # Verify SQL execution
        mock_cursor.execute.assert_called_once()
        call_args = mock_cursor.execute.call_args[0]

        # Check SQL statement
        expected_sql = "SELECT * FROM app.create_location(%s, %s, %s::jsonb)"
        assert call_args[0] == expected_sql

        # Check parameters
        expected_params = ["tenant-123", "user-456", json.dumps(input_data)]
        assert call_args[1] == expected_params

    @pytest.mark.asyncio
    async def test_execute_function_with_context_asyncpg(self):
        """Test execute_function_with_context with asyncpg pool."""
        # Create mock asyncpg pool and connection
        mock_pool = AsyncMock()
        mock_conn = AsyncMock()

        # Configure pool without connection() method (asyncpg style)
        delattr(type(mock_pool), "connection")  # Ensure no connection method
        mock_pool.acquire.return_value.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_pool.acquire.return_value.__aexit__ = AsyncMock(return_value=None)

        # Configure connection to return result
        mock_result = {"success": True, "location_id": str(uuid4())}
        mock_conn.fetchrow.return_value = mock_result

        # Create repository
        repo = FraiseQLRepository(mock_pool)

        # Test data
        function_name = "app.create_location"
        context_args = ["tenant-123", "user-456"]
        input_data = {"name": "Test Location", "address": "123 Test St"}

        # Call method
        result = await repo.execute_function_with_context(
            function_name,
            context_args,
            input_data,
        )

        # Verify result
        assert result == mock_result

        # Verify SQL execution
        mock_conn.fetchrow.assert_called_once()
        call_args = mock_conn.fetchrow.call_args[0]

        # Check SQL statement
        expected_sql = "SELECT * FROM app.create_location($1, $2, $3::jsonb)"
        assert call_args[0] == expected_sql

        # Check parameters
        expected_params = ["tenant-123", "user-456", input_data]
        assert call_args[1:] == tuple(expected_params)

    @pytest.mark.asyncio
    async def test_execute_function_with_context_empty_context(self):
        """Test execute_function_with_context with no context parameters."""
        # Create mock psycopg pool
        mock_pool = MagicMock()
        mock_conn = AsyncMock()
        mock_cursor = AsyncMock()

        mock_pool.connection.return_value.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_pool.connection.return_value.__aexit__ = AsyncMock(return_value=None)
        mock_conn.cursor.return_value.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_conn.cursor.return_value.__aexit__ = AsyncMock(return_value=None)

        mock_result = {"success": True}
        mock_cursor.fetchone.return_value = mock_result

        repo = FraiseQLRepository(mock_pool)

        # Test with empty context args
        result = await repo.execute_function_with_context(
            "app.test_function",
            [],  # No context args
            {"test": "data"},
        )

        assert result == mock_result

        # Verify SQL uses only JSONB parameter
        call_args = mock_cursor.execute.call_args[0]
        expected_sql = "SELECT * FROM app.test_function(%s::jsonb)"
        assert call_args[0] == expected_sql

    @pytest.mark.asyncio
    async def test_execute_function_with_context_invalid_function_name(self):
        """Test execute_function_with_context rejects invalid function names."""
        mock_pool = MagicMock()
        repo = FraiseQLRepository(mock_pool)

        # Test with invalid function name (SQL injection attempt)
        with pytest.raises(ValueError, match="Invalid function name"):
            await repo.execute_function_with_context(
                "app.test'; DROP TABLE users; --",
                ["tenant-123"],
                {"test": "data"},
            )

    @pytest.mark.asyncio
    async def test_execute_function_with_context_none_result(self):
        """Test execute_function_with_context handles None result."""
        # Create mock psycopg pool
        mock_pool = MagicMock()
        mock_conn = AsyncMock()
        mock_cursor = AsyncMock()

        mock_pool.connection.return_value.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_pool.connection.return_value.__aexit__ = AsyncMock(return_value=None)
        mock_conn.cursor.return_value.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_conn.cursor.return_value.__aexit__ = AsyncMock(return_value=None)

        # Configure cursor to return None
        mock_cursor.fetchone.return_value = None

        repo = FraiseQLRepository(mock_pool)

        result = await repo.execute_function_with_context(
            "app.test_function",
            ["tenant-123"],
            {"test": "data"},
        )

        # Should return empty dict when result is None
        assert result == {}
