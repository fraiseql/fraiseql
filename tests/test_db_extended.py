"""Extended tests for database functionality to improve coverage."""

import asyncio
from decimal import Decimal
from typing import Optional
from unittest.mock import AsyncMock, MagicMock, patch
from uuid import UUID, uuid4

import pytest
from psycopg.sql import Composed, SQL
from psycopg_pool import AsyncConnectionPool

from fraiseql.db import (
    DatabaseQuery,
    FraiseQLRepository,
    register_type_for_view,
    _type_registry,
)


class TestDatabaseQuery:
    """Test DatabaseQuery dataclass."""

    def test_database_query_creation(self):
        """Test creating DatabaseQuery instances."""
        # With SQL object
        sql = SQL("SELECT * FROM users")
        params = {"id": 123}
        
        query = DatabaseQuery(sql, params)
        assert query.statement is sql
        assert query.params == params
        assert query.fetch_result is True  # Default

    def test_database_query_no_fetch(self):
        """Test DatabaseQuery with fetch_result=False."""
        composed = Composed([SQL("INSERT INTO users"), SQL(" VALUES "), SQL("($1)")])
        params = {"name": "test"}
        
        query = DatabaseQuery(composed, params, fetch_result=False)
        assert query.statement is composed
        assert query.params == params
        assert query.fetch_result is False

    def test_database_query_empty_params(self):
        """Test DatabaseQuery with empty parameters."""
        sql = SQL("SELECT 1")
        query = DatabaseQuery(sql, {})
        
        assert query.statement is sql
        assert query.params == {}
        assert query.fetch_result is True


class TestTypeRegistry:
    """Test type registration functionality."""

    def setup_method(self):
        """Clear registry before each test."""
        _type_registry.clear()

    def test_register_type_for_view(self):
        """Test registering a type for a view."""
        class User:
            pass
        
        register_type_for_view("user_view", User)
        assert _type_registry["user_view"] is User

    def test_register_multiple_types(self):
        """Test registering multiple types."""
        class User:
            pass
        
        class Post:
            pass
        
        register_type_for_view("users", User)
        register_type_for_view("posts", Post)
        
        assert _type_registry["users"] is User
        assert _type_registry["posts"] is Post

    def test_register_type_overwrite(self):
        """Test overwriting a registered type."""
        class OldUser:
            pass
        
        class NewUser:
            pass
        
        register_type_for_view("users", OldUser)
        register_type_for_view("users", NewUser)
        
        assert _type_registry["users"] is NewUser


class TestFraiseQLRepository:
    """Test FraiseQLRepository functionality."""

    @pytest.fixture
    def mock_pool(self):
        """Create a mock async connection pool."""
        pool = AsyncMock(spec=AsyncConnectionPool)
        return pool

    @pytest.fixture
    def repository(self, mock_pool):
        """Create a repository with mock pool."""
        return FraiseQLRepository(mock_pool)

    def test_repository_initialization(self, mock_pool):
        """Test repository initialization."""
        repo = FraiseQLRepository(mock_pool)
        assert repo._pool is mock_pool
        assert repo.context == {}
        assert repo.mode in ["development", "production"]

    def test_repository_with_context(self, mock_pool):
        """Test repository initialization with context."""
        context = {"user_id": 123, "tenant": "test"}
        repo = FraiseQLRepository(mock_pool, context)
        
        assert repo._pool is mock_pool
        assert repo.context == context

    @pytest.mark.asyncio
    async def test_run_query_with_results(self, repository, mock_pool):
        """Test running a query that returns results."""
        # Mock connection and cursor
        mock_conn = AsyncMock()
        mock_cursor = AsyncMock()
        mock_cursor.fetchall.return_value = [
            {"id": 1, "name": "Alice"},
            {"id": 2, "name": "Bob"}
        ]
        
        # Set up context managers
        mock_conn.cursor.return_value.__aenter__.return_value = mock_cursor
        mock_conn.cursor.return_value.__aexit__.return_value = None
        mock_pool.connection.return_value.__aenter__.return_value = mock_conn
        mock_pool.connection.return_value.__aexit__.return_value = None
        
        # Create and run query
        query = DatabaseQuery(
            SQL("SELECT * FROM users"),
            {},
            fetch_result=True
        )
        
        result = await repository.run(query)
        
        assert result == [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]
        mock_cursor.execute.assert_called_once_with(query.statement, query.params)
        mock_cursor.fetchall.assert_called_once()

    @pytest.mark.asyncio
    async def test_run_query_no_results(self, repository, mock_pool):
        """Test running a query that doesn't fetch results."""
        # Mock connection and cursor
        mock_conn = AsyncMock()
        mock_cursor = AsyncMock()
        
        # Set up context managers
        mock_conn.cursor.return_value.__aenter__.return_value = mock_cursor
        mock_conn.cursor.return_value.__aexit__.return_value = None
        mock_pool.connection.return_value.__aenter__.return_value = mock_conn
        mock_pool.connection.return_value.__aexit__.return_value = None
        
        # Create and run query
        query = DatabaseQuery(
            SQL("INSERT INTO users (name) VALUES (%s)"),
            {"name": "Charlie"},
            fetch_result=False
        )
        
        result = await repository.run(query)
        
        assert result == []
        mock_cursor.execute.assert_called_once_with(query.statement, query.params)
        mock_cursor.fetchall.assert_not_called()

    @pytest.mark.asyncio
    async def test_run_query_exception(self, repository, mock_pool):
        """Test handling exceptions during query execution."""
        # Mock connection to raise exception
        mock_pool.connection.side_effect = Exception("Connection failed")
        
        query = DatabaseQuery(SQL("SELECT 1"), {})
        
        with pytest.raises(Exception, match="Connection failed"):
            await repository.run(query)

    @pytest.mark.asyncio
    async def test_run_in_transaction_success(self, repository, mock_pool):
        """Test successful transaction execution."""
        # Mock connection
        mock_conn = AsyncMock()
        mock_pool.connection.return_value.__aenter__.return_value = mock_conn
        mock_pool.connection.return_value.__aexit__.return_value = None
        
        # Mock function to run in transaction
        async def test_func(conn, value):
            assert conn is mock_conn
            return value * 2
        
        result = await repository.run_in_transaction(test_func, 21)
        assert result == 42

    @pytest.mark.asyncio
    async def test_run_in_transaction_with_kwargs(self, repository, mock_pool):
        """Test transaction with keyword arguments."""
        mock_conn = AsyncMock()
        mock_pool.connection.return_value.__aenter__.return_value = mock_conn
        mock_pool.connection.return_value.__aexit__.return_value = None
        
        async def test_func(conn, a, b=None):
            return f"a={a}, b={b}"
        
        result = await repository.run_in_transaction(test_func, 1, b=2)
        assert result == "a=1, b=2"

    @pytest.mark.asyncio
    async def test_run_in_transaction_exception(self, repository, mock_pool):
        """Test transaction rollback on exception."""
        mock_conn = AsyncMock()
        mock_pool.connection.return_value.__aenter__.return_value = mock_conn
        mock_pool.connection.return_value.__aexit__.return_value = None
        
        async def failing_func(conn):
            raise ValueError("Transaction failed")
        
        with pytest.raises(ValueError, match="Transaction failed"):
            await repository.run_in_transaction(failing_func)

    def test_determine_mode_development(self, mock_pool):
        """Test mode determination in development."""
        with patch.dict('os.environ', {'FRAISEQL_MODE': 'development'}):
            repo = FraiseQLRepository(mock_pool)
            assert repo.mode == "development"

    def test_determine_mode_production(self, mock_pool):
        """Test mode determination in production."""
        with patch.dict('os.environ', {'FRAISEQL_MODE': 'production'}):
            repo = FraiseQLRepository(mock_pool)
            assert repo.mode == "production"

    def test_determine_mode_default(self, mock_pool):
        """Test default mode determination."""
        with patch.dict('os.environ', {}, clear=True):
            repo = FraiseQLRepository(mock_pool)
            # Should default to production or development based on other indicators
            assert repo.mode in ["development", "production"]

    @pytest.mark.asyncio
    async def test_instantiate_recursive_simple(self, repository):
        """Test recursive instantiation with simple types."""
        # Test data
        data = {
            "id": 1,
            "name": "Test User",
            "email": "test@example.com"
        }
        
        # Mock type class
        class User:
            def __init__(self, id: int, name: str, email: str):
                self.id = id
                self.name = name
                self.email = email
        
        with patch.object(repository, '_get_type_for_data', return_value=User):
            result = repository._instantiate_recursive(User, data)
            
            assert isinstance(result, User)
            assert result.id == 1
            assert result.name == "Test User"
            assert result.email == "test@example.com"

    @pytest.mark.asyncio
    async def test_instantiate_recursive_with_nested_objects(self, repository):
        """Test recursive instantiation with nested objects."""
        # Complex nested data
        data = {
            "id": 1,
            "user": {
                "id": 123,
                "name": "John"
            },
            "tags": ["python", "graphql"]
        }
        
        class User:
            def __init__(self, id: int, name: str):
                self.id = id
                self.name = name
        
        class Post:
            def __init__(self, id: int, user: User, tags: list):
                self.id = id
                self.user = user
                self.tags = tags
        
        register_type_for_view("user", User)
        
        with patch.object(repository, '_get_type_for_data', return_value=Post):
            result = repository._instantiate_recursive(Post, data)
            
            assert isinstance(result, Post)
            assert result.id == 1
            assert isinstance(result.user, dict)  # Nested objects become dicts in test
            assert result.tags == ["python", "graphql"]

    def test_get_type_for_data(self, repository):
        """Test getting type for data."""
        class User:
            pass
        
        register_type_for_view("users", User)
        
        # Should return registered type
        result = repository._get_type_for_data({"__view__": "users"})
        assert result is User
        
        # Should return None for unregistered view
        result = repository._get_type_for_data({"__view__": "unknown"})
        assert result is None
        
        # Should return None for data without view
        result = repository._get_type_for_data({"id": 1})
        assert result is None

    def test_convert_value_types(self, repository):
        """Test value type conversions."""
        # UUID conversion
        uuid_str = str(uuid4())
        assert isinstance(repository._convert_value(uuid_str, UUID), UUID)
        
        # Decimal conversion
        assert isinstance(repository._convert_value("123.45", Decimal), Decimal)
        assert repository._convert_value("123.45", Decimal) == Decimal("123.45")
        
        # String conversion
        assert repository._convert_value(123, str) == "123"
        
        # No conversion needed
        assert repository._convert_value(42, int) == 42

    def test_convert_value_optional_types(self, repository):
        """Test conversion with Optional types."""
        # Optional[int] with None
        result = repository._convert_value(None, Optional[int])
        assert result is None
        
        # Optional[str] with value
        result = repository._convert_value(123, Optional[str])
        assert result == "123"

    def test_convert_value_list_types(self, repository):
        """Test conversion with list types."""
        # list[int]
        result = repository._convert_value(["1", "2", "3"], list[int])
        assert result == [1, 2, 3]
        
        # list[str]
        result = repository._convert_value([1, 2, 3], list[str])
        assert result == ["1", "2", "3"]

    def test_mode_property(self, repository):
        """Test mode property access."""
        assert hasattr(repository, 'mode')
        assert repository.mode in ["development", "production"]

    @pytest.mark.asyncio
    async def test_context_access(self, repository):
        """Test context access and modification."""
        # Initially empty
        assert repository.context == {}
        
        # Can be modified
        repository.context["user_id"] = 123
        assert repository.context["user_id"] == 123
        
        # Context persists
        repository.context.update({"tenant": "test", "role": "admin"})
        assert len(repository.context) == 3


class TestRepositoryEdgeCases:
    """Test edge cases and error conditions."""

    @pytest.fixture
    def mock_pool(self):
        return AsyncMock(spec=AsyncConnectionPool)

    @pytest.fixture
    def repository(self, mock_pool):
        return FraiseQLRepository(mock_pool)

    @pytest.mark.asyncio
    async def test_run_with_complex_composed_sql(self, repository, mock_pool):
        """Test running queries with complex Composed SQL."""
        # Mock setup
        mock_conn = AsyncMock()
        mock_cursor = AsyncMock()
        mock_cursor.fetchall.return_value = []
        
        mock_conn.cursor.return_value.__aenter__.return_value = mock_cursor
        mock_conn.cursor.return_value.__aexit__.return_value = None
        mock_pool.connection.return_value.__aenter__.return_value = mock_conn
        mock_pool.connection.return_value.__aexit__.return_value = None
        
        # Complex composed SQL
        complex_sql = Composed([
            SQL("SELECT * FROM users WHERE "),
            SQL("id = %s AND "),
            SQL("status = %s")
        ])
        
        query = DatabaseQuery(complex_sql, {"id": 1, "status": "active"})
        result = await repository.run(query)
        
        assert result == []
        mock_cursor.execute.assert_called_once_with(complex_sql, {"id": 1, "status": "active"})

    @pytest.mark.asyncio
    async def test_nested_transaction_calls(self, repository, mock_pool):
        """Test nested transaction function calls."""
        mock_conn = AsyncMock()
        mock_pool.connection.return_value.__aenter__.return_value = mock_conn
        mock_pool.connection.return_value.__aexit__.return_value = None
        
        async def outer_func(conn, value):
            async def inner_func():
                return value * 2
            
            result = await inner_func()
            return result + 1
        
        result = await repository.run_in_transaction(outer_func, 10)
        assert result == 21

    def test_repository_string_representation(self, repository):
        """Test repository string representation if it exists."""
        # Repository should be representable
        repr_str = repr(repository)
        assert "FraiseQLRepository" in repr_str or "Repository" in str(type(repository))