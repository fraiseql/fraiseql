"""Tests for DataLoader entity resolution pattern.

Tests the automatic batching, deduplication, and caching functionality
of the EntityDataLoader for efficient federation entity resolution.
"""

import asyncio

import pytest

from fraiseql.federation import clear_entity_registry, entity
from fraiseql.federation.dataloader import EntityDataLoader


# Mock database pool for testing
class MockAsyncPool:
    """Mock async connection pool for testing."""

    def __init__(self, data=None) -> None:
        """Initialize mock pool with test data.

        Args:
            data: Dict of {(typename, key_value): entity_dict}
        """
        self.data = data or {}
        self.queries_executed = 0

    async def acquire(self) -> None:
        """Return self as context manager."""
        return self

    async def __aenter__(self) -> None:
        """Async context manager entry."""
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        """Async context manager exit."""

    async def fetch(self, sql, *params) -> None:  # noqa: ANN002
        """Mock database query execution."""
        self.queries_executed += 1

        # Parse the query to extract table and type
        # Expected format: SELECT key_field, data FROM tv_typename WHERE key_field IN (...)
        if "tv_user" in sql:
            typename = "User"
            key_field = "id"
        elif "tv_post" in sql:
            typename = "Post"
            key_field = "id"
        elif "tv_product" in sql:
            typename = "Product"
            key_field = "id"
        else:
            return []

        # Return matching rows for the given parameters
        rows = []
        for key_value in params:
            if (typename, key_value) in self.data:
                entity = self.data[(typename, key_value)]
                rows.append({key_field: key_value, "data": entity})

        return rows


class MockResolver:
    """Mock EntitiesResolver for testing."""

    def resolve(self, typename, key_values) -> None:
        """Mock resolve method."""


@pytest.fixture
def clear_entities() -> None:
    """Clear entity registry before and after each test."""
    clear_entity_registry()
    yield
    clear_entity_registry()


@pytest.fixture
def mock_pool() -> None:
    """Create mock database pool with test data."""
    data = {
        ("User", "user-1"): {"name": "Alice", "email": "alice@example.com"},
        ("User", "user-2"): {"name": "Bob", "email": "bob@example.com"},
        ("User", "user-3"): {"name": "Charlie", "email": "charlie@example.com"},
        ("Post", "post-1"): {"title": "Hello World", "content": "First post"},
        ("Post", "post-2"): {"title": "Second Post", "content": "Another post"},
        ("Product", "prod-1"): {"name": "Product 1", "price": 99.99},
    }
    return MockAsyncPool(data)


@pytest.fixture
def loader(mock_pool) -> None:
    """Create DataLoader instance for testing."""
    resolver = MockResolver()
    return EntityDataLoader(resolver, mock_pool, cache_size=100, batch_window_ms=1.0)


class TestDataLoaderBasics:
    """Test basic DataLoader functionality."""

    @pytest.mark.asyncio
    async def test_load_single_entity(self, loader, mock_pool, clear_entities) -> None:
        """Test loading a single entity."""

        @entity
        class User:
            id: str
            name: str

        result = await loader.load("User", "id", "user-1")

        assert result is not None
        assert result["name"] == "Alice"
        assert result["__typename"] == "User"
        assert mock_pool.queries_executed == 1

    @pytest.mark.asyncio
    async def test_load_missing_entity(self, loader, mock_pool, clear_entities) -> None:
        """Test loading an entity that doesn't exist."""

        @entity
        class User:
            id: str

        result = await loader.load("User", "id", "user-missing")

        assert result is None
        assert mock_pool.queries_executed == 1

    @pytest.mark.asyncio
    async def test_load_many_same_type(self, loader, mock_pool, clear_entities) -> None:
        """Test loading multiple entities of the same type."""

        @entity
        class User:
            id: str
            name: str

        results = await loader.load_many(
            [
                ("User", "id", "user-1"),
                ("User", "id", "user-2"),
                ("User", "id", "user-3"),
            ]
        )

        assert len(results) == 3
        assert results[0]["name"] == "Alice"
        assert results[1]["name"] == "Bob"
        assert results[2]["name"] == "Charlie"
        assert mock_pool.queries_executed == 1  # Single batch query

    @pytest.mark.asyncio
    async def test_load_many_different_types(self, loader, mock_pool, clear_entities) -> None:
        """Test loading entities of different types."""

        @entity
        class User:
            id: str

        @entity
        class Post:
            id: str

        results = await loader.load_many(
            [
                ("User", "id", "user-1"),
                ("Post", "id", "post-1"),
            ]
        )

        assert len(results) == 2
        assert results[0]["__typename"] == "User"
        assert results[1]["__typename"] == "Post"
        assert mock_pool.queries_executed == 2  # One query per type

    @pytest.mark.asyncio
    async def test_load_returns_future_immediately(self, loader, clear_entities) -> None:
        """Test that load returns immediately without blocking."""

        @entity
        class User:
            id: str

        # This should return immediately, not block
        future = asyncio.create_task(loader.load("User", "id", "user-1"))
        await asyncio.sleep(0.001)  # Small delay to allow request queuing

        assert not future.done()  # Should still be pending
        await loader.flush()
        assert future.done()


class TestDataLoaderDeduplication:
    """Test deduplication of identical requests."""

    @pytest.mark.asyncio
    async def test_duplicate_requests_share_future(self, loader, mock_pool, clear_entities) -> None:
        """Test that duplicate requests reuse the same Future."""

        @entity
        class User:
            id: str

        future1 = asyncio.create_task(loader.load("User", "id", "user-1"))
        future2 = asyncio.create_task(loader.load("User", "id", "user-1"))

        await asyncio.sleep(0.001)
        await loader.flush()

        result1 = await future1
        result2 = await future2

        assert result1 == result2  # Same object
        assert mock_pool.queries_executed == 1  # Only one query
        assert loader.stats.dedup_hits == 1

    @pytest.mark.asyncio
    async def test_many_duplicate_requests(self, loader, mock_pool, clear_entities) -> None:
        """Test dedup with 100 identical requests reduces to 1 query."""

        @entity
        class User:
            id: str

        futures = [asyncio.create_task(loader.load("User", "id", "user-1")) for _ in range(100)]

        await asyncio.sleep(0.001)
        await loader.flush()

        results = await asyncio.gather(*futures)

        # All results should be the same
        assert all(r == results[0] for r in results)
        # Only one query to database
        assert mock_pool.queries_executed == 1
        # 99 dedup hits (first load is cache_miss)
        assert loader.stats.dedup_hits == 99

    @pytest.mark.asyncio
    async def test_dedup_with_mixed_requests(self, loader, mock_pool, clear_entities) -> None:
        """Test dedup with mix of unique and duplicate requests."""

        @entity
        class User:
            id: str

        futures = [
            asyncio.create_task(loader.load("User", "id", "user-1")),
            asyncio.create_task(loader.load("User", "id", "user-2")),
            asyncio.create_task(loader.load("User", "id", "user-1")),  # Duplicate
            asyncio.create_task(loader.load("User", "id", "user-3")),
            asyncio.create_task(loader.load("User", "id", "user-2")),  # Duplicate
        ]

        await asyncio.sleep(0.001)
        await loader.flush()

        results = await asyncio.gather(*futures)

        assert len(results) == 5
        assert results[0] == results[2]  # Same user-1
        assert results[1] == results[4]  # Same user-2
        assert mock_pool.queries_executed == 1
        assert loader.stats.dedup_hits == 2

    @pytest.mark.asyncio
    async def test_order_preserved_with_dedup(self, loader, mock_pool, clear_entities) -> None:
        """Test that result order is preserved even with deduplication."""

        @entity
        class User:
            id: str

        futures = [
            asyncio.create_task(loader.load("User", "id", "user-2")),
            asyncio.create_task(loader.load("User", "id", "user-1")),
            asyncio.create_task(loader.load("User", "id", "user-3")),
        ]

        await asyncio.sleep(0.001)
        await loader.flush()

        results = await asyncio.gather(*futures)

        # Check order is preserved
        assert results[0]["name"] == "Bob"  # user-2
        assert results[1]["name"] == "Alice"  # user-1
        assert results[2]["name"] == "Charlie"  # user-3


class TestDataLoaderCaching:
    """Test caching of resolved entities."""

    @pytest.mark.asyncio
    async def test_cache_hit_on_second_load(self, loader, mock_pool, clear_entities) -> None:
        """Test that second load of same entity uses cache."""

        @entity
        class User:
            id: str

        # First load
        result1 = await loader.load("User", "id", "user-1")
        await loader.flush()

        # Second load should hit cache
        result2 = await loader.load("User", "id", "user-1")
        await loader.flush()

        assert result1 == result2
        assert mock_pool.queries_executed == 1  # Only first query
        assert loader.stats.cache_hits == 1

    @pytest.mark.asyncio
    async def test_cache_miss_for_different_keys(self, loader, mock_pool, clear_entities) -> None:
        """Test that different keys result in cache misses."""

        @entity
        class User:
            id: str

        await loader.load("User", "id", "user-1")
        await loader.flush()

        await loader.load("User", "id", "user-2")
        await loader.flush()

        assert mock_pool.queries_executed == 2
        assert loader.stats.cache_misses == 2

    @pytest.mark.asyncio
    async def test_clear_cache_invalidates_results(self, loader, mock_pool, clear_entities) -> None:
        """Test that clear_cache() invalidates cached results."""

        @entity
        class User:
            id: str

        # Load and cache
        result1 = await loader.load("User", "id", "user-1")
        await loader.flush()

        # Clear cache
        loader.clear_cache()

        # Reload should query database again
        result2 = await loader.load("User", "id", "user-1")
        await loader.flush()

        assert result1 == result2
        assert mock_pool.queries_executed == 2  # Two separate queries

    @pytest.mark.asyncio
    async def test_lru_cache_size_limit(self, loader, mock_pool, clear_entities) -> None:
        """Test that LRU cache respects size limit."""
        # Create loader with small cache
        small_loader = EntityDataLoader(
            MockResolver(), mock_pool, cache_size=2, batch_window_ms=1.0
        )

        @entity
        class User:
            id: str

        # Load 3 different users (exceeds cache size of 2)
        await small_loader.load("User", "id", "user-1")
        await small_loader.flush()
        await small_loader.load("User", "id", "user-2")
        await small_loader.flush()
        await small_loader.load("User", "id", "user-3")
        await small_loader.flush()

        # Cache should only have 2 entries (user-2 and user-3)
        assert len(small_loader._result_cache) <= 2


class TestDataLoaderStats:
    """Test statistics tracking."""

    @pytest.mark.asyncio
    async def test_stats_tracking(self, loader, mock_pool, clear_entities) -> None:
        """Test that stats are tracked correctly."""

        @entity
        class User:
            id: str

        await loader.load("User", "id", "user-1")
        await loader.load("User", "id", "user-1")  # Duplicate
        await loader.flush()

        stats = loader.stats

        assert stats.total_requests == 2
        assert stats.cache_misses == 1
        assert stats.cache_hits == 0
        assert stats.dedup_hits == 1
        assert stats.batch_count == 1

    @pytest.mark.asyncio
    async def test_cache_hit_rate_calculation(self, loader, mock_pool, clear_entities) -> None:
        """Test cache hit rate calculation."""

        @entity
        class User:
            id: str

        # First load: cache miss
        await loader.load("User", "id", "user-1")
        await loader.flush()

        # Second load: cache hit
        await loader.load("User", "id", "user-1")
        await loader.flush()

        stats = loader.stats
        assert stats.cache_hit_rate == 0.5  # 1 hit out of 2 requests

    @pytest.mark.asyncio
    async def test_dedup_rate_calculation(self, loader, mock_pool, clear_entities) -> None:
        """Test deduplication rate calculation."""

        @entity
        class User:
            id: str

        for _ in range(100):
            await loader.load("User", "id", "user-1")

        await loader.flush()

        stats = loader.stats
        assert stats.dedup_rate == 0.99  # 99 dedup hits out of 100 requests


class TestDataLoaderBatching:
    """Test batch window and flushing behavior."""

    @pytest.mark.asyncio
    async def test_batch_window_timeout(self, mock_pool, clear_entities) -> None:
        """Test that batch window timeout triggers flush."""

        @entity
        class User:
            id: str

        loader = EntityDataLoader(MockResolver(), mock_pool, cache_size=100, batch_window_ms=5.0)

        task = asyncio.create_task(loader.load("User", "id", "user-1"))

        # Before timeout, query should not be executed
        await asyncio.sleep(0.001)
        assert mock_pool.queries_executed == 0

        # After timeout, query should be executed
        await asyncio.sleep(0.010)
        assert mock_pool.queries_executed == 1

        result = await task
        assert result is not None

    @pytest.mark.asyncio
    async def test_explicit_flush(self, loader, mock_pool, clear_entities) -> None:
        """Test explicit flush of pending requests."""

        @entity
        class User:
            id: str

        task = asyncio.create_task(loader.load("User", "id", "user-1"))

        # No query yet
        assert mock_pool.queries_executed == 0

        # Explicit flush
        await loader.flush()

        assert mock_pool.queries_executed == 1
        result = await task
        assert result is not None


class TestDataLoaderErrorHandling:
    """Test error handling in DataLoader."""

    @pytest.mark.asyncio
    async def test_database_error_propagated(self, clear_entities) -> None:
        """Test that database errors are propagated to futures."""

        class FailingPool:
            async def acquire(self) -> None:
                return self

            async def __aenter__(self) -> None:
                return self

            async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
                pass

            async def fetch(self, sql, *params) -> None:  # noqa: ANN002
                raise RuntimeError("Database connection failed")

        @entity
        class User:
            id: str

        loader = EntityDataLoader(MockResolver(), FailingPool())
        task = asyncio.create_task(loader.load("User", "id", "user-1"))

        await loader.flush()

        with pytest.raises(RuntimeError, match="Database connection failed"):
            await task

    @pytest.mark.asyncio
    async def test_partial_batch_failure(self, clear_entities) -> None:
        """Test handling of partial failures in a batch."""

        class PartialFailPool:
            def __init__(self) -> None:
                self.call_count = 0

            async def acquire(self) -> None:
                return self

            async def __aenter__(self) -> None:
                return self

            async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
                pass

            async def fetch(self, sql, *params) -> None:  # noqa: ANN002
                self.call_count += 1
                if "user" in sql:
                    # Return partial results
                    return [{"id": "user-1", "data": {"name": "Alice"}}]
                raise RuntimeError("Post query failed")

        pool = PartialFailPool()
        loader = EntityDataLoader(MockResolver(), pool)

        @entity
        class User:
            id: str

        @entity
        class Post:
            id: str

        # Load one of each type
        user_task = asyncio.create_task(loader.load("User", "id", "user-1"))
        post_task = asyncio.create_task(loader.load("Post", "id", "post-1"))

        await loader.flush()

        # User should succeed
        user = await user_task
        assert user is not None

        # Post should fail
        with pytest.raises(RuntimeError):
            await post_task


class TestDataLoaderIntegration:
    """Integration tests for DataLoader."""

    @pytest.mark.asyncio
    async def test_concurrent_operations(self, loader, mock_pool, clear_entities) -> None:
        """Test DataLoader with concurrent requests."""

        @entity
        class User:
            id: str

        @entity
        class Post:
            id: str

        async def load_user(uid) -> None:
            return await loader.load("User", "id", uid)

        async def load_post(pid) -> None:
            return await loader.load("Post", "id", pid)

        # Create concurrent operations
        tasks = [
            asyncio.create_task(load_user("user-1")),
            asyncio.create_task(load_post("post-1")),
            asyncio.create_task(load_user("user-2")),
            asyncio.create_task(load_user("user-1")),  # Duplicate
            asyncio.create_task(load_post("post-2")),
        ]

        await asyncio.sleep(0.001)
        await loader.flush()

        results = await asyncio.gather(*tasks)

        assert len(results) == 5
        assert results[0]["__typename"] == "User"
        assert results[1]["__typename"] == "Post"
        assert mock_pool.queries_executed == 2  # One per type

    @pytest.mark.asyncio
    async def test_loader_close_flushes_pending(self, loader, mock_pool, clear_entities) -> None:
        """Test that close() flushes pending requests."""

        @entity
        class User:
            id: str

        task = asyncio.create_task(loader.load("User", "id", "user-1"))

        # Don't flush yet
        assert mock_pool.queries_executed == 0

        # Close should flush
        await loader.close()

        # Now query should be executed
        assert mock_pool.queries_executed == 1
        result = await task
        assert result is not None
