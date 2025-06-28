"""Comprehensive tests for db module to improve coverage."""

import os
from dataclasses import dataclass
from typing import Optional
from unittest.mock import AsyncMock, patch
from uuid import UUID

import pytest
from psycopg.sql import SQL
from psycopg_pool import AsyncConnectionPool

from fraiseql.db import (
    DatabaseQuery,
    FraiseQLRepository,
    _type_registry,
    register_type_for_view,
)
from fraiseql.types import fraise_type


@fraise_type
@dataclass
class SimpleType:
    """Simple type for testing."""
    id: UUID
    name: str
    is_active: bool = True


@fraise_type
@dataclass
class NestedType:
    """Type with nested object."""
    id: UUID
    title: str
    author: SimpleType


@fraise_type
@dataclass
class TypeWithOptional:
    """Type with optional fields."""
    id: UUID
    name: str
    description: Optional[str] = None
    nested: Optional[SimpleType] = None


@fraise_type
@dataclass
class TypeWithList:
    """Type with list fields."""
    id: UUID
    tags: list[str]
    items: list[SimpleType]


@pytest.fixture
def mock_pool():
    """Create a mock async connection pool."""
    pool = AsyncMock(spec=AsyncConnectionPool)
    return pool


@pytest.fixture
def mock_connection():
    """Create a mock database connection."""
    conn = AsyncMock()
    return conn


@pytest.fixture
def mock_cursor():
    """Create a mock database cursor."""
    cursor = AsyncMock()
    cursor.fetchall = AsyncMock(return_value=[])
    cursor.fetchone = AsyncMock(return_value=None)
    return cursor


@pytest.fixture
async def repository(mock_pool):
    """Create a repository instance with mock pool."""
    return FraiseQLRepository(mock_pool)


class TestDatabaseQuery:
    """Test DatabaseQuery dataclass."""

    def test_database_query_creation(self):
        """Test creating a DatabaseQuery instance."""
        statement = SQL("SELECT * FROM users")
        params = {"id": 123}

        query = DatabaseQuery(statement=statement, params=params)

        assert query.statement == statement
        assert query.params == params
        assert query.fetch_result is True

    def test_database_query_no_fetch(self):
        """Test creating a DatabaseQuery with fetch_result=False."""
        statement = SQL("INSERT INTO users (name) VALUES (%s)")
        params = {"name": "Test"}

        query = DatabaseQuery(statement=statement, params=params, fetch_result=False)

        assert query.fetch_result is False


class TestTypeRegistry:
    """Test type registry functions."""

    def test_register_type_for_view(self):
        """Test registering a type for a view."""
        # Clear registry first
        _type_registry.clear()

        register_type_for_view("user_view", SimpleType)

        assert "user_view" in _type_registry
        assert _type_registry["user_view"] == SimpleType

    def test_register_multiple_types(self):
        """Test registering multiple types."""
        _type_registry.clear()

        register_type_for_view("user_view", SimpleType)
        register_type_for_view("post_view", NestedType)

        assert len(_type_registry) == 2
        assert _type_registry["user_view"] == SimpleType
        assert _type_registry["post_view"] == NestedType

    def test_override_existing_registration(self):
        """Test overriding an existing type registration."""
        _type_registry.clear()

        register_type_for_view("user_view", SimpleType)
        register_type_for_view("user_view", NestedType)

        assert _type_registry["user_view"] == NestedType


class TestFraiseQLRepository:
    """Test FraiseQLRepository class."""

    async def test_repository_initialization(self, mock_pool):
        """Test repository initialization."""
        context = {"user": "test", "tenant_id": 123}
        repo = FraiseQLRepository(mock_pool, context)

        assert repo._pool == mock_pool
        assert repo.context == context

    async def test_repository_initialization_no_context(self, mock_pool):
        """Test repository initialization without context."""
        repo = FraiseQLRepository(mock_pool)

        assert repo.context == {}

    async def test_run_query_with_results(self, repository, mock_pool, mock_connection, mock_cursor):
        """Test running a query that returns results."""
        # Setup mocks
        expected_results = [
            {"id": "1", "name": "User 1"},
            {"id": "2", "name": "User 2"},
        ]
        mock_cursor.fetchall.return_value = expected_results
        mock_connection.cursor.return_value.__aenter__.return_value = mock_cursor
        mock_pool.connection.return_value.__aenter__.return_value = mock_connection

        # Create query
        query = DatabaseQuery(
            statement=SQL("SELECT * FROM users"),
            params={},
        )

        # Execute
        results = await repository.run(query)

        # Verify
        assert results == expected_results
        mock_cursor.execute.assert_called_once_with(query.statement, query.params)
        mock_cursor.fetchall.assert_called_once()

    async def test_run_query_without_fetch(self, repository, mock_pool, mock_connection, mock_cursor):
        """Test running a query without fetching results."""
        # Setup mocks
        mock_connection.cursor.return_value.__aenter__.return_value = mock_cursor
        mock_pool.connection.return_value.__aenter__.return_value = mock_connection

        # Create query
        query = DatabaseQuery(
            statement=SQL("INSERT INTO users (name) VALUES (%s)"),
            params={"name": "Test"},
            fetch_result=False,
        )

        # Execute
        results = await repository.run(query)

        # Verify
        assert results == []
        mock_cursor.execute.assert_called_once()
        mock_cursor.fetchall.assert_not_called()

    async def test_run_query_with_exception(self, repository, mock_pool, mock_connection, mock_cursor):
        """Test running a query that raises an exception."""
        # Setup mocks
        mock_cursor.execute.side_effect = Exception("Database error")
        mock_connection.cursor.return_value.__aenter__.return_value = mock_cursor
        mock_pool.connection.return_value.__aenter__.return_value = mock_connection

        # Create query
        query = DatabaseQuery(
            statement=SQL("SELECT * FROM invalid_table"),
            params={},
        )

        # Execute and expect exception
        with pytest.raises(Exception, match="Database error"):
            await repository.run(query)

    async def test_run_in_transaction_success(self, repository, mock_pool, mock_connection):
        """Test running a function in a transaction successfully."""
        # Setup mocks
        mock_pool.connection.return_value.__aenter__.return_value = mock_connection

        # Define test function
        async def test_func(conn):
            await conn.execute("SELECT 1")
            return "success"

        # Execute
        result = await repository.run_in_transaction(test_func)

        # Verify
        assert result == "success"
        mock_connection.commit.assert_called_once()

    async def test_run_in_transaction_rollback(self, repository, mock_pool, mock_connection):
        """Test transaction rollback on error."""
        # Setup mocks
        mock_pool.connection.return_value.__aenter__.return_value = mock_connection

        # Define test function that raises error
        async def test_func(conn):
            raise ValueError("Test error")

        # Execute and expect exception
        with pytest.raises(ValueError, match="Test error"):
            await repository.run_in_transaction(test_func)

        # Verify rollback was called
        mock_connection.rollback.assert_called_once()

    async def test_execute_function(self, repository, mock_pool, mock_connection, mock_cursor):
        """Test executing a PostgreSQL function."""
        # Setup mocks
        expected_result = {"id": "123", "status": "success"}
        mock_cursor.fetchone.return_value = {"result": expected_result}
        mock_connection.cursor.return_value.__aenter__.return_value = mock_cursor
        mock_pool.connection.return_value.__aenter__.return_value = mock_connection

        # Execute
        result = await repository.execute_function("test_function", {"param": "value"})

        # Verify
        assert result == expected_result
        # Should call the function with proper SQL
        call_args = mock_cursor.execute.call_args[0]
        assert "test_function" in str(call_args[0])

    async def test_execute_function_no_result(self, repository, mock_pool, mock_connection, mock_cursor):
        """Test executing a function that returns no result."""
        # Setup mocks
        mock_cursor.fetchone.return_value = None
        mock_connection.cursor.return_value.__aenter__.return_value = mock_cursor
        mock_pool.connection.return_value.__aenter__.return_value = mock_connection

        # Execute
        result = await repository.execute_function("void_function", {})

        # Verify
        assert result == {}

    async def test_find_with_simple_data(self, repository, mock_pool, mock_connection, mock_cursor):
        """Test find method with simple data."""
        # Setup mocks
        expected_data = [
            {"data": {"id": "1", "name": "User 1"}},
            {"data": {"id": "2", "name": "User 2"}},
        ]
        mock_cursor.fetchall.return_value = expected_data
        mock_connection.cursor.return_value.__aenter__.return_value = mock_cursor
        mock_pool.connection.return_value.__aenter__.return_value = mock_connection

        # Execute
        results = await repository.find("users", name="User%")

        # Verify
        assert len(results) == 2
        assert results[0]["id"] == "1"
        assert results[1]["name"] == "User 2"

    async def test_find_one(self, repository, mock_pool, mock_connection, mock_cursor):
        """Test find_one method."""
        # Setup mocks
        expected_data = [{"data": {"id": "1", "name": "User 1"}}]
        mock_cursor.fetchall.return_value = expected_data
        mock_connection.cursor.return_value.__aenter__.return_value = mock_cursor
        mock_pool.connection.return_value.__aenter__.return_value = mock_connection

        # Execute
        result = await repository.find_one("users", id="1")

        # Verify
        assert result is not None
        assert result["id"] == "1"
        assert result["name"] == "User 1"

    async def test_find_one_no_result(self, repository, mock_pool, mock_connection, mock_cursor):
        """Test find_one when no result found."""
        # Setup mocks
        mock_cursor.fetchall.return_value = []
        mock_connection.cursor.return_value.__aenter__.return_value = mock_cursor
        mock_pool.connection.return_value.__aenter__.return_value = mock_connection

        # Execute
        result = await repository.find_one("users", id="999")

        # Verify
        assert result is None




class TestRepositoryModes:
    """Test repository mode handling."""

    async def test_development_mode(self, mock_pool):
        """Test repository in development mode."""
        with patch.dict(os.environ, {"FRAISEQL_MODE": "development"}):
            repo = FraiseQLRepository(mock_pool)
            assert repo.mode == "development"

    async def test_production_mode(self, mock_pool):
        """Test repository in production mode (default)."""
        with patch.dict(os.environ, {}, clear=True):
            repo = FraiseQLRepository(mock_pool)
            assert repo.mode == "production"

    async def test_mode_from_context(self, mock_pool):
        """Test mode determination from context."""
        context = {"mode": "development"}
        repo = FraiseQLRepository(mock_pool, context)
        assert repo.mode == "development"


class TestRepositoryHelpers:
    """Test repository helper methods."""

    async def test_to_snake_case_table_name(self, repository):
        """Test table name conversion to snake case."""
        # This would be used internally when auto_camel_case is enabled
        from fraiseql.utils.casing import to_snake_case

        assert to_snake_case("UserProfile") == "user_profile"
        assert to_snake_case("APIKey") == "api_key"
        assert to_snake_case("HTTPSConnection") == "https_connection"
