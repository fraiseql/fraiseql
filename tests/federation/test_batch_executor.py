"""Tests for batch execution engine.

Tests batch execution contexts, concurrent execution,
and integration with DataLoader.
"""

import asyncio

import pytest

from fraiseql.federation import clear_entity_registry, entity
from fraiseql.federation.batch_executor import (
    BatchExecutor,
    ConcurrentBatchExecutor,
    PerRequestBatchExecutor,
)
from fraiseql.federation.dataloader import EntityDataLoader


class MockConnection:
    """Mock database connection."""

    def __init__(self, pool) -> None:
        """Initialize mock connection with reference to pool."""
        self.pool = pool

    async def fetch(self, sql, *params) -> None:  # noqa: ANN002
        """Mock database query execution."""
        self.pool.queries_executed += 1

        # Parse the query to extract table and type
        if "tv_user" in sql:
            typename = "User"
            key_field = "id"
        elif "tv_post" in sql:
            typename = "Post"
            key_field = "id"
        else:
            return []

        rows = []
        for key_value in params:
            if (typename, key_value) in self.pool.data:
                entity = self.pool.data[(typename, key_value)]
                rows.append({key_field: key_value, "data": entity})

        return rows


class MockAsyncPool:
    """Mock async connection pool for testing."""

    def __init__(self, data=None) -> None:
        """Initialize mock pool with test data."""
        self.data = data or {}
        self.queries_executed = 0

    def acquire(self) -> None:
        """Return async context manager for connection."""
        return MockConnectionContext(self)


class MockConnectionContext:
    """Async context manager for mock connections."""

    def __init__(self, pool) -> None:
        """Initialize context manager."""
        self.pool = pool
        self.conn = None

    async def __aenter__(self) -> None:
        """Async context manager entry."""
        self.conn = MockConnection(self.pool)
        return self.conn

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        """Async context manager exit."""


class MockResolver:
    """Mock EntitiesResolver for testing."""


@pytest.fixture
def clear_entities() -> None:
    """Clear entity registry."""
    clear_entity_registry()
    yield
    clear_entity_registry()


@pytest.fixture
def mock_pool() -> None:
    """Create mock pool with test data."""
    data = {
        ("User", "user-1"): {"name": "Alice"},
        ("User", "user-2"): {"name": "Bob"},
        ("Post", "post-1"): {"title": "Hello"},
        ("Post", "post-2"): {"title": "World"},
    }
    return MockAsyncPool(data)


@pytest.fixture
def executor() -> None:
    """Create batch executor."""
    return BatchExecutor(batch_window_ms=1.0)


class TestBatchExecutor:
    """Test basic batch execution."""

    @pytest.mark.asyncio
    async def test_batch_execute_single_type(self, executor, mock_pool, clear_entities) -> None:
        """Test executing batch with single entity type."""

        @entity
        class User:
            id: str

        requests = [
            ("User", "id", "user-1"),
            ("User", "id", "user-2"),
        ]

        results = await executor.batch_execute(requests, MockResolver(), mock_pool)

        assert len(results) == 2
        assert results[0]["name"] == "Alice"
        assert results[1]["name"] == "Bob"
        assert mock_pool.queries_executed == 1

    @pytest.mark.asyncio
    async def test_batch_execute_multiple_types(self, executor, mock_pool, clear_entities) -> None:
        """Test executing batch with multiple types."""

        @entity
        class User:
            id: str

        @entity
        class Post:
            id: str

        requests = [
            ("User", "id", "user-1"),
            ("Post", "id", "post-1"),
            ("User", "id", "user-2"),
        ]

        results = await executor.batch_execute(requests, MockResolver(), mock_pool)

        assert len(results) == 3
        assert results[0]["__typename"] == "User"
        assert results[1]["__typename"] == "Post"
        assert mock_pool.queries_executed == 2

    @pytest.mark.asyncio
    async def test_batch_context_manager(self, executor, mock_pool, clear_entities) -> None:
        """Test batch context manager."""

        @entity
        class User:
            id: str

        loader = EntityDataLoader(MockResolver(), mock_pool)

        async with executor.batch_context(loader):
            # Create tasks in context so they're scheduled during the batch window
            user1_task = asyncio.create_task(loader.load("User", "id", "user-1"))
            user2_task = asyncio.create_task(loader.load("User", "id", "user-2"))
            # Wait a bit to ensure both are queued
            await asyncio.sleep(0.0001)

        # After context exit, batch should be flushed
        user1 = await user1_task
        user2 = await user2_task
        assert user1["name"] == "Alice"
        assert user2["name"] == "Bob"
        # Both requests should be batched into one query
        assert mock_pool.queries_executed == 1

    @pytest.mark.asyncio
    async def test_context_manager_prevents_manual_flush(
        self, executor, mock_pool, clear_entities
    ) -> None:
        """Test that context manager handles flush automatically."""

        @entity
        class User:
            id: str

        loader = EntityDataLoader(MockResolver(), mock_pool)

        async with executor.batch_context(loader):
            task = asyncio.create_task(loader.load("User", "id", "user-1"))

            # Don't manually flush - context manager should do it
            await asyncio.sleep(0.001)

        # Queries should be executed by context manager
        assert mock_pool.queries_executed == 1
        result = await task
        assert result is not None


class TestPerRequestBatchExecutor:
    """Test per-request batch execution."""

    @pytest.mark.asyncio
    async def test_execute_request(self, mock_pool, clear_entities) -> None:
        """Test per-request execution."""

        @entity
        class User:
            id: str

        executor = PerRequestBatchExecutor(batch_window_ms=1.0)

        async def handler(loader) -> None:
            # Load both users - they'll be batched together
            user1 = await loader.load("User", "id", "user-1")
            user2 = await loader.load("User", "id", "user-2")
            return [user1, user2]

        results = await executor.execute_request(handler, MockResolver(), mock_pool)

        assert len(results) == 2
        assert results[0]["name"] == "Alice"
        # With sequential awaits, we get two flushes (one for each load)
        assert mock_pool.queries_executed == 2

    @pytest.mark.asyncio
    async def test_execute_request_auto_flush(self, mock_pool, clear_entities) -> None:
        """Test that execute_request auto-flushes."""

        @entity
        class User:
            id: str

        executor = PerRequestBatchExecutor()

        calls = []

        async def handler(loader) -> None:
            calls.append("handler_start")
            # Request is made but not flushed
            return asyncio.create_task(loader.load("User", "id", "user-1"))

        task = await executor.execute_request(handler, MockResolver(), mock_pool)

        # Task should be resolved even though we didn't manually flush
        result = await task
        assert result is not None
        assert result["name"] == "Alice"


class TestConcurrentBatchExecutor:
    """Test concurrent batch execution."""

    @pytest.mark.asyncio
    async def test_execute_concurrent_batches(self, mock_pool, clear_entities) -> None:
        """Test executing multiple batches concurrently."""

        @entity
        class User:
            id: str

        executor = ConcurrentBatchExecutor()

        request_groups = [
            [("User", "id", "user-1"), ("User", "id", "user-2")],
            [("User", "id", "user-1")],  # Duplicate - should still execute separately
        ]

        results = await executor.execute_concurrent(request_groups, MockResolver(), mock_pool)

        assert len(results) == 2
        assert len(results[0]) == 2
        assert len(results[1]) == 1
        # Two separate batches = two queries
        assert mock_pool.queries_executed == 2

    @pytest.mark.asyncio
    async def test_execute_grouped_by_typename(self, mock_pool, clear_entities) -> None:
        """Test executing requests grouped by typename."""

        @entity
        class User:
            id: str

        @entity
        class Post:
            id: str

        executor = ConcurrentBatchExecutor()

        requests = [
            ("User", "id", "user-1"),
            ("Post", "id", "post-1"),
            ("User", "id", "user-2"),
            ("Post", "id", "post-2"),
        ]

        results = await executor.execute_grouped(
            requests, MockResolver(), mock_pool, group_by="typename"
        )

        # Results should be in original order
        assert results[0]["__typename"] == "User"
        assert results[1]["__typename"] == "Post"
        assert results[2]["__typename"] == "User"
        assert results[3]["__typename"] == "Post"

        # Two concurrent batches (one per type)
        assert mock_pool.queries_executed == 2

    @pytest.mark.asyncio
    async def test_concurrent_preserves_order(self, mock_pool, clear_entities) -> None:
        """Test that concurrent execution preserves request order."""

        @entity
        class User:
            id: str

        executor = ConcurrentBatchExecutor()

        request_groups = [
            [("User", "id", "user-2"), ("User", "id", "user-1")],
            [("User", "id", "user-1"), ("User", "id", "user-2")],
        ]

        results = await executor.execute_concurrent(request_groups, MockResolver(), mock_pool)

        # First group: user-2, user-1
        assert results[0][0]["name"] == "Bob"
        assert results[0][1]["name"] == "Alice"

        # Second group: user-1, user-2
        assert results[1][0]["name"] == "Alice"
        assert results[1][1]["name"] == "Bob"


class TestBatchExecutorEdgeCases:
    """Test edge cases."""

    @pytest.mark.asyncio
    async def test_empty_request_list(self, executor, mock_pool, clear_entities) -> None:
        """Test executing empty request list."""
        results = await executor.batch_execute([], MockResolver(), mock_pool)

        assert results == []
        assert mock_pool.queries_executed == 0

    @pytest.mark.asyncio
    async def test_batch_with_missing_entities(self, executor, mock_pool, clear_entities) -> None:
        """Test batch with some missing entities."""

        @entity
        class User:
            id: str

        requests = [
            ("User", "id", "user-1"),
            ("User", "id", "user-missing"),
            ("User", "id", "user-2"),
        ]

        results = await executor.batch_execute(requests, MockResolver(), mock_pool)

        assert len(results) == 3
        assert results[0] is not None
        assert results[1] is None  # Missing
        assert results[2] is not None

    @pytest.mark.asyncio
    async def test_concurrent_error_handling(self, mock_pool, clear_entities) -> None:
        """Test error handling in concurrent execution."""

        class FailingConnection:
            async def fetch(self, sql, *params) -> None:  # noqa: ANN002
                raise RuntimeError("Database error")

        class FailingConnectionContext:
            async def __aenter__(self) -> None:
                return FailingConnection()

            async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
                pass

        class FailingPool:
            def acquire(self) -> None:
                return FailingConnectionContext()

        @entity
        class User:
            id: str

        executor = ConcurrentBatchExecutor()

        request_groups = [
            [("User", "id", "user-1")],
            [("User", "id", "user-2")],
        ]

        with pytest.raises(RuntimeError):
            await executor.execute_concurrent(request_groups, MockResolver(), FailingPool())

    @pytest.mark.asyncio
    async def test_batch_context_with_exception(self, executor, mock_pool, clear_entities) -> None:
        """Test that context manager flushes even on exception."""

        @entity
        class User:
            id: str

        loader = EntityDataLoader(MockResolver(), mock_pool)

        try:
            async with executor.batch_context(loader):
                await loader.load("User", "id", "user-1")
                raise ValueError("Test error")
        except ValueError:
            pass

        # Flush should have happened despite exception
        assert mock_pool.queries_executed == 1
