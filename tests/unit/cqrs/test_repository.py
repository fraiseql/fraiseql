"""Tests for CQRS repository module."""

from typing import Any
from unittest.mock import AsyncMock, MagicMock
from uuid import uuid4

import pytest

from fraiseql.cqrs.repository import CQRSRepository


# Mock cursor context manager
class MockCursorContext:
    """Mock async cursor context manager."""

    def __init__(self, cursor: AsyncMock) -> None:
        self.cursor = cursor

    async def __aenter__(self) -> AsyncMock:
        return self.cursor

    async def __aexit__(self, *args: object) -> None:
        pass


@pytest.fixture
def mock_cursor() -> AsyncMock:
    """Create a mock cursor."""
    cursor = AsyncMock()
    cursor.execute = AsyncMock()
    cursor.fetchone = AsyncMock(return_value=None)
    cursor.fetchall = AsyncMock(return_value=[])
    return cursor


@pytest.fixture
def mock_connection(mock_cursor: AsyncMock) -> MagicMock:
    """Create a mock connection."""
    connection = MagicMock()
    connection.cursor = MagicMock(return_value=MockCursorContext(mock_cursor))
    connection.transaction = MagicMock()
    return connection


@pytest.fixture
def mock_executor() -> AsyncMock:
    """Create a mock executor."""
    executor = AsyncMock()
    executor.execute_function = AsyncMock(return_value={"success": True})
    executor.execute_query = AsyncMock(return_value=[])
    return executor


@pytest.fixture
def repository(mock_connection: MagicMock, mock_executor: AsyncMock) -> CQRSRepository:
    """Create a repository with mocked dependencies."""
    repo = CQRSRepository(mock_connection)
    repo.executor = mock_executor
    return repo


# Tests for CQRSRepository initialization
@pytest.mark.unit
class TestCQRSRepositoryInit:
    """Tests for CQRSRepository initialization."""

    def test_init_stores_connection(self, mock_connection: MagicMock) -> None:
        """Repository stores connection."""
        repo = CQRSRepository(mock_connection)
        assert repo.connection is mock_connection

    def test_init_creates_executor(self, mock_connection: MagicMock) -> None:
        """Repository creates CQRSExecutor."""
        repo = CQRSRepository(mock_connection)
        assert repo.executor is not None


# Tests for command methods
@pytest.mark.unit
class TestCommandMethods:
    """Tests for command methods (create, update, delete)."""

    @pytest.mark.asyncio
    async def test_create_calls_execute_function(
        self, repository: CQRSRepository, mock_executor: AsyncMock
    ) -> None:
        """Create calls execute_function with correct function name."""
        input_data = {"name": "Test", "email": "test@example.com"}
        await repository.create("user", input_data)

        mock_executor.execute_function.assert_called_once_with("fn_create_user", input_data)

    @pytest.mark.asyncio
    async def test_create_returns_result(
        self, repository: CQRSRepository, mock_executor: AsyncMock
    ) -> None:
        """Create returns result from SQL function."""
        mock_executor.execute_function.return_value = {"id": "123", "success": True}
        result = await repository.create("user", {"name": "Test"})

        assert result == {"id": "123", "success": True}

    @pytest.mark.asyncio
    async def test_update_calls_execute_function(
        self, repository: CQRSRepository, mock_executor: AsyncMock
    ) -> None:
        """Update calls execute_function with correct function name."""
        input_data = {"id": "123", "name": "Updated"}
        await repository.update("user", input_data)

        mock_executor.execute_function.assert_called_once_with("fn_update_user", input_data)

    @pytest.mark.asyncio
    async def test_delete_calls_execute_function(
        self, repository: CQRSRepository, mock_executor: AsyncMock
    ) -> None:
        """Delete calls execute_function with correct function name."""
        entity_id = uuid4()
        await repository.delete("user", entity_id)

        mock_executor.execute_function.assert_called_once_with(
            "fn_delete_user", {"id": str(entity_id)}
        )

    @pytest.mark.asyncio
    async def test_call_function_delegates_to_executor(
        self, repository: CQRSRepository, mock_executor: AsyncMock
    ) -> None:
        """call_function delegates to executor."""
        await repository.call_function("custom_function", {"arg": "value"})

        mock_executor.execute_function.assert_called_once_with("custom_function", {"arg": "value"})

    @pytest.mark.asyncio
    async def test_execute_function_is_alias_for_call_function(
        self, repository: CQRSRepository, mock_executor: AsyncMock
    ) -> None:
        """execute_function is alias for call_function."""
        # Both methods should call the same underlying function
        await repository.execute_function("test_fn", {"arg": "value"})
        await repository.call_function("test_fn2", {"arg2": "value2"})

        # Both should delegate to executor.execute_function
        assert mock_executor.execute_function.call_count == 2


# Tests for query methods
@pytest.mark.unit
class TestQueryMethods:
    """Tests for query methods."""

    @pytest.mark.asyncio
    async def test_get_by_id_returns_data(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """get_by_id returns data when found."""
        entity_id = uuid4()
        expected_data = {"id": str(entity_id), "name": "Test"}
        mock_cursor.fetchone.return_value = (expected_data,)

        result = await repository.get_by_id("v_users", entity_id)

        assert result == expected_data

    @pytest.mark.asyncio
    async def test_get_by_id_returns_none_when_not_found(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """get_by_id returns None when entity not found."""
        entity_id = uuid4()
        mock_cursor.fetchone.return_value = None

        result = await repository.get_by_id("v_users", entity_id)

        assert result is None

    @pytest.mark.asyncio
    async def test_query_returns_data_list(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """Query returns list of data."""
        mock_cursor.fetchall.return_value = [
            ({"id": "1", "name": "A"},),
            ({"id": "2", "name": "B"},),
        ]

        result = await repository.query("v_users")

        assert len(result) == 2
        assert result[0] == {"id": "1", "name": "A"}
        assert result[1] == {"id": "2", "name": "B"}

    @pytest.mark.asyncio
    async def test_query_with_filters(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """Query applies filters correctly."""
        mock_cursor.fetchall.return_value = [({"id": "1", "name": "Test"},)]

        await repository.query("v_users", filters={"status": "active"})

        # Verify execute was called - filter should be in the query
        mock_cursor.execute.assert_called_once()

    @pytest.mark.asyncio
    async def test_query_with_operator_filters(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """Query applies operator filters correctly."""
        mock_cursor.fetchall.return_value = []

        await repository.query("v_users", filters={"name": {"contains": "test"}})

        mock_cursor.execute.assert_called_once()

    @pytest.mark.asyncio
    async def test_query_with_order_by(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """Query applies ordering correctly."""
        mock_cursor.fetchall.return_value = []

        await repository.query("v_users", order_by="createdAt_desc")

        mock_cursor.execute.assert_called_once()

    @pytest.mark.asyncio
    async def test_query_with_pagination(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """Query applies limit and offset correctly."""
        mock_cursor.fetchall.return_value = []

        await repository.query("v_users", limit=10, offset=20)

        mock_cursor.execute.assert_called_once()

    @pytest.mark.asyncio
    async def test_query_raw(self, repository: CQRSRepository, mock_cursor: AsyncMock) -> None:
        """query_raw executes raw SQL."""
        mock_cursor.fetchall.return_value = [({"id": "1"},)]

        result = await repository.query_raw("SELECT * FROM users")

        assert len(result) == 1
        mock_cursor.execute.assert_called_once()


# Tests for generic methods
@pytest.mark.unit
class TestGenericMethods:
    """Tests for generic query methods."""

    @pytest.mark.asyncio
    async def test_select_from_json_view(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """select_from_json_view delegates to query."""
        mock_cursor.fetchall.return_value = [({"id": "1"},)]

        result = await repository.select_from_json_view("v_users", where={"status": "active"})

        assert len(result) == 1

    @pytest.mark.asyncio
    async def test_select_one_from_json_view_returns_first(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """select_one_from_json_view returns first result."""
        mock_cursor.fetchall.return_value = [({"id": "1"},), ({"id": "2"},)]

        result = await repository.select_one_from_json_view("v_users")

        assert result == {"id": "1"}

    @pytest.mark.asyncio
    async def test_select_one_from_json_view_returns_none_if_empty(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """select_one_from_json_view returns None if no results."""
        mock_cursor.fetchall.return_value = []

        result = await repository.select_one_from_json_view("v_users")

        assert result is None

    @pytest.mark.asyncio
    async def test_query_interface(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """query_interface queries interface view."""
        mock_cursor.fetchall.return_value = [
            ({"id": "1", "__typename": "User"},),
            ({"id": "2", "__typename": "Article"},),
        ]

        result = await repository.query_interface("v_node")

        assert len(result) == 2

    @pytest.mark.asyncio
    async def test_query_interface_with_filters(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """query_interface applies filters."""
        mock_cursor.fetchall.return_value = []

        await repository.query_interface("v_node", filters={"created_at": {"$gt": "2024-01-01"}})

        mock_cursor.execute.assert_called_once()

    @pytest.mark.asyncio
    async def test_get_polymorphic_by_id(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """get_polymorphic_by_id returns typed entity."""
        entity_id = uuid4()
        mock_cursor.fetchone.return_value = (
            {"id": str(entity_id), "name": "Test", "__typename": "User"},
        )

        class User:
            def __init__(self, **kwargs: Any) -> None:
                for k, v in kwargs.items():
                    setattr(self, k, v)

        result = await repository.get_polymorphic_by_id(
            "v_node", entity_id, type_mapping={"User": User}
        )

        assert isinstance(result, User)
        assert result.name == "Test"

    @pytest.mark.asyncio
    async def test_get_polymorphic_by_id_returns_none_if_not_found(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """get_polymorphic_by_id returns None if not found."""
        mock_cursor.fetchone.return_value = None

        result = await repository.get_polymorphic_by_id("v_node", uuid4())

        assert result is None


# Tests for batch operations
@pytest.mark.unit
class TestBatchOperations:
    """Tests for batch operations."""

    @pytest.mark.asyncio
    async def test_batch_create(self, repository: CQRSRepository, mock_executor: AsyncMock) -> None:
        """batch_create creates multiple entities."""
        inputs = [{"name": "A"}, {"name": "B"}, {"name": "C"}]
        mock_executor.execute_function.return_value = {"success": True}

        results = await repository.batch_create("user", inputs)

        assert len(results) == 3
        assert mock_executor.execute_function.call_count == 3

    @pytest.mark.asyncio
    async def test_batch_update(self, repository: CQRSRepository, mock_executor: AsyncMock) -> None:
        """batch_update updates multiple entities."""
        updates = [{"id": "1", "name": "A"}, {"id": "2", "name": "B"}]
        mock_executor.execute_function.return_value = {"success": True}

        results = await repository.batch_update("user", updates)

        assert len(results) == 2
        assert mock_executor.execute_function.call_count == 2

    @pytest.mark.asyncio
    async def test_batch_delete(self, repository: CQRSRepository, mock_executor: AsyncMock) -> None:
        """batch_delete deletes multiple entities."""
        entity_ids = [uuid4(), uuid4()]
        mock_executor.execute_function.return_value = {"success": True}

        results = await repository.batch_delete("user", entity_ids)

        assert len(results) == 2
        assert mock_executor.execute_function.call_count == 2


# Tests for utility methods
@pytest.mark.unit
class TestUtilityMethods:
    """Tests for utility methods."""

    @pytest.mark.asyncio
    async def test_count(self, repository: CQRSRepository, mock_cursor: AsyncMock) -> None:
        """Count returns entity count."""
        mock_cursor.fetchone.return_value = (42,)

        class User:
            pass

        result = await repository.count(User)

        assert result == 42

    @pytest.mark.asyncio
    async def test_count_with_where(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """Count applies where filter."""
        mock_cursor.fetchone.return_value = (10,)

        class User:
            pass

        result = await repository.count(User, where={"status": "active"})

        assert result == 10
        mock_cursor.execute.assert_called_once()

    @pytest.mark.asyncio
    async def test_count_returns_zero_on_none(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """Count returns 0 when result is None."""
        mock_cursor.fetchone.return_value = None

        class User:
            pass

        result = await repository.count(User)

        assert result == 0

    @pytest.mark.asyncio
    async def test_exists_true(self, repository: CQRSRepository, mock_cursor: AsyncMock) -> None:
        """Exists returns True when entity found."""
        entity_id = uuid4()
        mock_cursor.fetchone.return_value = ({"id": str(entity_id)},)

        class User:
            pass

        result = await repository.exists(User, entity_id)

        assert result is True

    @pytest.mark.asyncio
    async def test_exists_false(self, repository: CQRSRepository, mock_cursor: AsyncMock) -> None:
        """Exists returns False when entity not found."""
        mock_cursor.fetchone.return_value = None

        class User:
            pass

        result = await repository.exists(User, uuid4())

        assert result is False

    @pytest.mark.asyncio
    async def test_find_by_id(self, repository: CQRSRepository, mock_cursor: AsyncMock) -> None:
        """find_by_id uses entity class to determine view."""
        entity_id = uuid4()
        mock_cursor.fetchone.return_value = ({"id": str(entity_id)},)

        class User:
            pass

        result = await repository.find_by_id(User, entity_id)

        assert result == {"id": str(entity_id)}

    @pytest.mark.asyncio
    async def test_list_entities(self, repository: CQRSRepository, mock_cursor: AsyncMock) -> None:
        """list_entities returns list of entities."""
        mock_cursor.fetchall.return_value = [({"id": "1"},), ({"id": "2"},)]

        class User:
            pass

        result = await repository.list_entities(User)

        assert len(result) == 2

    @pytest.mark.asyncio
    async def test_list_entities_with_order_by(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """list_entities applies ordering."""
        mock_cursor.fetchall.return_value = []

        class User:
            pass

        await repository.list_entities(User, order_by=[("created_at", "DESC"), ("name", "ASC")])

        mock_cursor.execute.assert_called_once()

    def test_get_view_name(self, repository: CQRSRepository) -> None:
        """_get_view_name converts class name to view name."""

        class User:
            pass

        class BlogPost:
            pass

        class UserRole:
            pass

        assert repository._get_view_name(User) == "vw_user"
        assert repository._get_view_name(BlogPost) == "vw_blog_post"
        assert repository._get_view_name(UserRole) == "vw_user_role"

    def test_get_function_name(self, repository: CQRSRepository) -> None:
        """_get_function_name returns correct function name."""
        assert repository._get_function_name("create", "user") == "fn_create_user"
        assert repository._get_function_name("update", "post") == "fn_update_post"
        assert repository._get_function_name("delete", "comment") == "fn_delete_comment"

    def test_convert_order_by_to_tuples_from_tuples(self, repository: CQRSRepository) -> None:
        """_convert_order_by_to_tuples returns tuples as-is."""
        order_by = [("name", "ASC"), ("created_at", "DESC")]
        result = repository._convert_order_by_to_tuples(order_by)

        assert result == order_by

    def test_convert_order_by_to_tuples_none(self, repository: CQRSRepository) -> None:
        """_convert_order_by_to_tuples returns None for None input."""
        assert repository._convert_order_by_to_tuples(None) is None

    def test_convert_order_by_to_tuples_from_order_by_set(self, repository: CQRSRepository) -> None:
        """_convert_order_by_to_tuples converts OrderBySet."""

        class OrderByInstruction:
            def __init__(self, field: str, direction: str) -> None:
                self.field = field
                self.direction = direction

        class OrderBySet:
            def __init__(self, instructions: list[OrderByInstruction]) -> None:
                self.instructions = instructions

        order_by_set = OrderBySet(
            [OrderByInstruction("name", "ASC"), OrderByInstruction("date", "DESC")]
        )

        result = repository._convert_order_by_to_tuples(order_by_set)

        assert result == [("name", "ASC"), ("date", "DESC")]


# Tests for transaction support
@pytest.mark.unit
class TestTransactionSupport:
    """Tests for transaction support."""

    def test_transaction_returns_context_manager(
        self, repository: CQRSRepository, mock_connection: MagicMock
    ) -> None:
        """Transaction returns connection's transaction context manager."""
        _result = repository.transaction()

        mock_connection.transaction.assert_called_once()


# Tests for execute_query
@pytest.mark.unit
class TestExecuteQuery:
    """Tests for execute_query method."""

    @pytest.mark.asyncio
    async def test_execute_query_delegates_to_executor(
        self, repository: CQRSRepository, mock_executor: AsyncMock
    ) -> None:
        """execute_query delegates to executor."""
        mock_executor.execute_query.return_value = [{"id": "1"}]

        result = await repository.execute_query("SELECT * FROM users")

        mock_executor.execute_query.assert_called_once()
        assert result == [{"id": "1"}]


# Tests for paginate
@pytest.mark.unit
class TestPaginate:
    """Tests for paginate method."""

    @pytest.mark.asyncio
    async def test_paginate_returns_connection_format(
        self, repository: CQRSRepository, mock_cursor: AsyncMock
    ) -> None:
        """Paginate returns Connection format result."""
        # Mock the paginate result through the internal implementation
        mock_cursor.fetchall.return_value = [
            (uuid4(), {"id": "1", "name": "A"}),
            (uuid4(), {"id": "2", "name": "B"}),
        ]
        mock_cursor.fetchone.return_value = (2,)  # total count

        result = await repository.paginate("v_users", first=10)

        assert "edges" in result
        assert "page_info" in result


# Tests for find_by_view
@pytest.mark.unit
class TestFindByView:
    """Tests for find_by_view method."""

    @pytest.mark.asyncio
    async def test_find_by_view(self, repository: CQRSRepository, mock_cursor: AsyncMock) -> None:
        """find_by_view queries by view name."""
        mock_cursor.fetchall.return_value = [({"id": "1"},)]

        result = await repository.find_by_view("v_custom_view", where={"active": True})

        assert len(result) == 1
