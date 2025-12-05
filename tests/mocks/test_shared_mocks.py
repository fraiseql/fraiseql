"""Tests for shared mock module.

TDD RED phase: These tests define the expected interface for shared mocks.
They will fail until the shared.py implementation is complete.
"""

import pytest

from tests.mocks import (
    MockConnection,
    MockCursor,
    MockDatabase,
    MockPool,
    MockRustResponseBytes,
)


class TestMockCursor:
    """Tests for MockCursor async context manager."""

    @pytest.mark.asyncio
    async def test_cursor_is_async_context_manager(self) -> None:
        """MockCursor can be used with async with."""
        cursor = MockCursor()
        async with cursor as ctx:
            assert ctx is cursor

    @pytest.mark.asyncio
    async def test_cursor_fetchone_returns_json_tuple(self) -> None:
        """Fetchone returns a tuple with JSON string."""
        cursor = MockCursor()
        async with cursor:
            result = await cursor.fetchone()
        assert isinstance(result, tuple)
        assert len(result) == 1
        assert isinstance(result[0], str)

    @pytest.mark.asyncio
    async def test_cursor_fetchall_returns_list(self) -> None:
        """Fetchall returns a list of tuples."""
        cursor = MockCursor()
        async with cursor:
            result = await cursor.fetchall()
        assert isinstance(result, list)
        assert len(result) >= 1
        assert isinstance(result[0], tuple)

    @pytest.mark.asyncio
    async def test_cursor_with_custom_return_data(self) -> None:
        """MockCursor can be configured with custom return data."""
        custom_data = '{"custom": "data"}'
        cursor = MockCursor(return_data=custom_data)
        async with cursor:
            result = await cursor.fetchone()
        assert result[0] == custom_data


class TestMockConnection:
    """Tests for MockConnection async context manager."""

    @pytest.mark.asyncio
    async def test_connection_is_async_context_manager(self) -> None:
        """MockConnection can be used with async with."""
        conn = MockConnection()
        async with conn as ctx:
            assert ctx is conn

    @pytest.mark.asyncio
    async def test_connection_cursor_returns_mock_cursor(self) -> None:
        """connection.cursor() returns a MockCursor."""
        conn = MockConnection()
        cursor = conn.cursor()
        assert isinstance(cursor, MockCursor)

    @pytest.mark.asyncio
    async def test_connection_execute_is_async(self) -> None:
        """connection.execute() is an async method."""
        conn = MockConnection()
        async with conn:
            await conn.execute("SELECT 1")


class TestMockPool:
    """Tests for MockPool."""

    def test_pool_connection_returns_mock_connection(self) -> None:
        """pool.connection() returns a MockConnection."""
        pool = MockPool()
        conn = pool.connection()
        assert isinstance(conn, MockConnection)

    @pytest.mark.asyncio
    async def test_pool_connection_can_be_used_as_context_manager(self) -> None:
        """The connection from pool can be used as async context manager."""
        pool = MockPool()
        conn = pool.connection()
        async with conn as ctx:
            assert ctx is conn


class TestMockDatabase:
    """Tests for MockDatabase with pool and tracking support."""

    def test_database_has_pool_attribute(self) -> None:
        """MockDatabase has _pool attribute."""
        db = MockDatabase()
        assert hasattr(db, "_pool")

    def test_database_get_pool_returns_pool(self) -> None:
        """get_pool() returns a MockPool."""
        db = MockDatabase()
        pool = db.get_pool()
        assert isinstance(pool, MockPool)

    def test_database_tracks_function_calls(self) -> None:
        """MockDatabase can track the last function call."""
        db = MockDatabase()
        assert hasattr(db, "last_function_call")
        assert db.last_function_call is None

    def test_database_tracks_input_data(self) -> None:
        """MockDatabase can track the last input data."""
        db = MockDatabase()
        assert hasattr(db, "last_input_data")
        assert db.last_input_data is None

    def test_database_with_custom_return_data(self) -> None:
        """MockDatabase can be configured with custom return data."""
        custom_data = '{"status": "custom"}'
        db = MockDatabase(return_data=custom_data)
        pool = db.get_pool()
        conn = pool.connection()
        cursor = conn.cursor()
        assert isinstance(cursor, MockCursor)


class TestMockRustResponseBytes:
    """Tests for MockRustResponseBytes."""

    def test_rust_response_bytes_stores_data(self) -> None:
        """MockRustResponseBytes stores the provided data."""
        data = {"status": "success", "message": "OK"}
        response = MockRustResponseBytes(data)
        assert response._data is not None

    def test_rust_response_bytes_to_json(self) -> None:
        """to_json() returns the original dict."""
        data = {"status": "success", "message": "OK"}
        response = MockRustResponseBytes(data)
        result = response.to_json()
        assert result == data

    def test_rust_response_bytes_content_type(self) -> None:
        """content_type property returns application/json."""
        response = MockRustResponseBytes({"test": True})
        assert response.content_type == "application/json"

    def test_rust_response_bytes_from_bytes(self) -> None:
        """MockRustResponseBytes can accept bytes directly."""
        data = b'{"status": "success"}'
        response = MockRustResponseBytes(data)
        result = response.to_json()
        assert result["status"] == "success"
