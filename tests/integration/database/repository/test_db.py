from typing import Never
from unittest.mock import AsyncMock, MagicMock, Mock

import pytest
from psycopg.sql import SQL
from psycopg_pool import AsyncConnectionPool

from fraiseql.db import DatabaseQuery, FraiseQLRepository

pytestmark = [pytest.mark.integration, pytest.mark.database]


@pytest.mark.integration
@pytest.mark.database
class TestFraiseQLRepository:
    """Test suite for FraiseQLRepository class."""

    @pytest.fixture
    def mock_pool(self) -> None:
        """Create a mock connection pool."""
        return AsyncMock(spec=AsyncConnectionPool)

    @pytest.fixture
    def repository(self, mock_pool) -> None:
        """Create a repository instance with mocked pool."""
        # Disable query timeout for tests to avoid SET LOCAL calls
        return FraiseQLRepository(pool=mock_pool, context={"query_timeout": None})

    def _setup_mocks(self, mock_pool, mock_cursor) -> None:
        """Helper to set up the mock connection and cursor properly."""
        # Mock cursor context manager
        mock_cursor_cm = Mock()
        mock_cursor_cm.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor_cm.__aexit__ = AsyncMock(return_value=None)

        # Mock connection
        mock_connection = Mock()
        mock_connection.cursor = Mock(return_value=mock_cursor_cm)

        # Mock connection context manager
        mock_connection_cm = Mock()
        mock_connection_cm.__aenter__ = AsyncMock(return_value=mock_connection)
        mock_connection_cm.__aexit__ = AsyncMock(return_value=None)

        # Set up pool to return connection context manager
        mock_pool.connection.return_value = mock_connection_cm

        return mock_connection

    @pytest.mark.asyncio
    async def test_run_simple_query(self, repository, mock_pool) -> None:
        """Test running a simple SQL query."""
        # Setup mock cursor
        mock_cursor = AsyncMock()
        mock_cursor.fetchall.return_value = [
            {"id": 1, "name": "Test 1"},
            {"id": 2, "name": "Test 2"},
        ]

        self._setup_mocks(mock_pool, mock_cursor)

        # Run query
        query = DatabaseQuery(statement=SQL("SELECT * FROM users"), params={}, fetch_result=True)
        result = await repository.run(query)

        # Assertions
        assert len(result) == 2
        assert result[0]["id"] == 1
        assert result[1]["name"] == "Test 2"
        mock_cursor.execute.assert_called_once_with(query.statement)

    @pytest.mark.asyncio
    async def test_run_query_with_params(self, repository, mock_pool) -> None:
        """Test running a query with parameters."""
        mock_cursor = AsyncMock()
        mock_cursor.fetchall.return_value = [{"id": 1, "email": "test@example.com"}]

        self._setup_mocks(mock_pool, mock_cursor)

        # Run query with params
        query = DatabaseQuery(
            statement=SQL("SELECT * FROM users WHERE email = %(email)s"),
            params={"email": "test@example.com"},
            fetch_result=True,
        )
        result = await repository.run(query)

        # Assertions
        assert len(result) == 1
        assert result[0]["email"] == "test@example.com"
        mock_cursor.execute.assert_called_once_with(query.statement, query.params)

    @pytest.mark.asyncio
    async def test_run_composed_query(self, repository, mock_pool) -> None:
        """Test running a Composed SQL query."""
        mock_cursor = AsyncMock()
        mock_cursor.fetchall.return_value = [{"count": 5}]

        self._setup_mocks(mock_pool, mock_cursor)

        # Create a Composed query
        query = DatabaseQuery(
            statement=SQL("SELECT COUNT(*) FROM {}").format(SQL("users")),
            params={},
            fetch_result=True,
        )
        result = await repository.run(query)

        # Assertions
        assert len(result) == 1
        assert result[0]["count"] == 5
        mock_cursor.execute.assert_called_once()

    @pytest.mark.asyncio
    async def test_run_empty_result(self, repository, mock_pool) -> None:
        """Test running a query that returns no results."""
        mock_cursor = AsyncMock()
        mock_cursor.fetchall.return_value = []

        self._setup_mocks(mock_pool, mock_cursor)

        query = DatabaseQuery(
            statement=SQL("SELECT * FROM users WHERE id = %(id)s"),
            params={"id": 999},
            fetch_result=True,
        )
        result = await repository.run(query)

        # Assertions
        assert result == []
        mock_cursor.execute.assert_called_once_with(query.statement, query.params)

    @pytest.mark.asyncio
    async def test_connection_error_handling(self, repository, mock_pool) -> None:
        """Test handling of connection errors."""
        # Make the connection raise an error
        mock_pool.connection.side_effect = (Exception("Connection pool error"),)

        query = DatabaseQuery(statement=SQL("SELECT * FROM users"), params={}, fetch_result=True)

        with pytest.raises(Exception, match="Connection pool error"):
            await repository.run(query)

    @pytest.mark.asyncio
    async def test_cursor_error_handling(self, repository, mock_pool) -> None:
        """Test handling of cursor execution errors."""
        mock_cursor = AsyncMock()
        mock_cursor.execute.side_effect = Exception("Query execution error")

        self._setup_mocks(mock_pool, mock_cursor)

        query = DatabaseQuery(
            statement=SQL("SELECT * FROM invalid_table"), params={}, fetch_result=True
        )

        with pytest.raises(Exception, match="Query execution error"):
            await repository.run(query)

    def test_repository_initialization(self) -> None:
        """Test repository initialization with pool."""
        mock_pool = MagicMock(spec=AsyncConnectionPool)
        repo = FraiseQLRepository(pool=mock_pool)

        assert repo._pool is mock_pool

    @pytest.mark.asyncio
    async def test_dict_row_factory(self, repository, mock_pool) -> None:
        """Test that dict_row factory is used for cursor."""
        mock_cursor = AsyncMock()
        mock_cursor.fetchall.return_value = [{"id": 1}]

        mock_connection = self._setup_mocks(mock_pool, mock_cursor)

        query = DatabaseQuery(statement=SQL("SELECT id FROM users"), params={}, fetch_result=True)
        await repository.run(query)

        # Verify dict_row factory was used
        from psycopg.rows import dict_row

        mock_connection.cursor.assert_called_once_with(row_factory=dict_row)

    @pytest.mark.asyncio
    async def test_sql_with_jsonb(self, repository, mock_pool) -> None:
        """Test running query with JSONB operations."""
        mock_cursor = AsyncMock()
        mock_cursor.fetchall.return_value = [{"data": {"name": "John", "age": 30}}]

        self._setup_mocks(mock_pool, mock_cursor)

        # Query with JSONB operations
        query = DatabaseQuery(
            statement=SQL("SELECT data->>'name' as name FROM users WHERE data @> %(filter)s"),
            params={"filter": '{"active": true}'},
            fetch_result=True,
        )
        result = await repository.run(query)

        # Assertions
        assert len(result) == 1
        assert result[0]["data"]["name"] == "John"
        mock_cursor.execute.assert_called_once_with(query.statement, query.params)

    @pytest.mark.asyncio
    async def test_run_in_transaction(self, repository, mock_pool) -> None:
        """Test running a function inside a transaction."""
        # Mock transaction context manager
        mock_transaction_cm = Mock()
        mock_transaction_cm.__aenter__ = AsyncMock(return_value=None)
        mock_transaction_cm.__aexit__ = AsyncMock(return_value=None)

        # Mock connection
        mock_connection = Mock()
        mock_connection.transaction = Mock(return_value=mock_transaction_cm)

        # Mock connection context manager
        mock_connection_cm = Mock()
        mock_connection_cm.__aenter__ = AsyncMock(return_value=mock_connection)
        mock_connection_cm.__aexit__ = AsyncMock(return_value=None)

        mock_pool.connection.return_value = mock_connection_cm

        # Define a test function to run in transaction
        @pytest.mark.asyncio
        async def test_func(conn, value) -> str:
            return f"Result: {value}"

        result = await repository.run_in_transaction(test_func, "test_value")

        # Assertions
        assert result == "Result: test_value"
        mock_pool.connection.assert_called_once()
        mock_connection.transaction.assert_called_once()

    @pytest.mark.asyncio
    async def test_run_in_transaction_with_error(self, repository, mock_pool) -> None:
        """Test transaction rollback on error."""
        # Mock transaction context manager
        mock_transaction_cm = Mock()
        mock_transaction_cm.__aenter__ = AsyncMock(return_value=None)
        mock_transaction_cm.__aexit__ = AsyncMock(return_value=None)

        # Mock connection
        mock_connection = Mock()
        mock_connection.transaction = Mock(return_value=mock_transaction_cm)

        # Mock connection context manager
        mock_connection_cm = Mock()
        mock_connection_cm.__aenter__ = AsyncMock(return_value=mock_connection)
        mock_connection_cm.__aexit__ = AsyncMock(return_value=None)

        mock_pool.connection.return_value = mock_connection_cm

        # Define a test function that raises an error
        async def failing_func(conn) -> Never:
            msg = "Transaction error"
            raise ValueError(msg)

        with pytest.raises(ValueError, match="Transaction error"):
            await repository.run_in_transaction(failing_func)

    def test_get_pool(self, repository, mock_pool) -> None:
        """Test getting the underlying connection pool."""
        pool = repository.get_pool()
        assert pool is mock_pool

    @pytest.mark.asyncio
    async def test_no_fetch_result(self, repository, mock_pool) -> None:
        """Test running a query with fetch_result=False."""
        mock_cursor = AsyncMock()

        self._setup_mocks(mock_pool, mock_cursor)

        # Run an INSERT query with no fetch
        query = DatabaseQuery(
            statement=SQL("INSERT INTO users (name) VALUES (%(name)s)"),
            params={"name": "New User"},
            fetch_result=False,
        )
        result = await repository.run(query)

        # Assertions
        assert result == []
        mock_cursor.execute.assert_called_once_with(query.statement, query.params)
        mock_cursor.fetchall.assert_not_called()
