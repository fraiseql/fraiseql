"""Tests for result caching layer with Redis support.

Following TDD principles, these tests are written before the implementation.
"""

import json
from datetime import UTC, datetime
from unittest.mock import AsyncMock
from uuid import UUID, uuid4

import pytest

from fraiseql.caching.cache_key import CacheKeyBuilder
from fraiseql.caching.redis_cache import RedisCache, RedisConnectionError
from fraiseql.caching.result_cache import CacheConfig, CacheStats, ResultCache, cached_query


class TestCacheKeyBuilder:
    """Test cache key generation."""

    def test_simple_query_key(self):
        """Test cache key generation for simple queries."""
        builder = CacheKeyBuilder()

        key = builder.build_key(query_name="users", filters={"status": "active"}, limit=10)

        assert key.startswith("fraiseql:users:")
        assert "status:active" in key
        assert "limit:10" in key

    def test_complex_filter_key(self):
        """Test cache key with complex filters."""
        builder = CacheKeyBuilder()

        key = builder.build_key(
            query_name="products",
            filters={
                "category": {"in": ["electronics", "books"]},
                "price": {"gte": 10, "lte": 100},
            },
            order_by=[("created_at", "DESC")],
            limit=20,
            offset=40,
        )

        assert key.startswith("fraiseql:products:")
        assert "category:in:" in key
        assert "price:gte:10" in key
        assert "lte:100" in key  # The lte is part of the price filter
        assert "order:created_at:DESC" in key
        assert "limit:20" in key
        assert "offset:40" in key

    def test_uuid_handling(self):
        """Test proper UUID serialization in cache keys."""
        builder = CacheKeyBuilder()
        user_id = uuid4()

        key = builder.build_key(query_name="user_posts", filters={"user_id": user_id})

        assert str(user_id) in key

    def test_datetime_handling(self):
        """Test proper datetime serialization in cache keys."""
        builder = CacheKeyBuilder()

        now = datetime.now(UTC)

        key = builder.build_key(query_name="events", filters={"start_date": {"gte": now}})

        assert now.isoformat() in key

    def test_consistent_key_generation(self):
        """Test that same inputs produce same keys."""
        builder = CacheKeyBuilder()

        # Order of filters shouldn't matter
        key1 = builder.build_key(query_name="users", filters={"name": "Alice", "age": 30})

        key2 = builder.build_key(query_name="users", filters={"age": 30, "name": "Alice"})

        assert key1 == key2


class TestRedisCache:
    """Test Redis cache backend."""

    @pytest.fixture
    def mock_redis(self):
        """Create mock Redis client."""
        mock = AsyncMock()
        mock.get = AsyncMock(return_value=None)
        mock.setex = AsyncMock(return_value=True)
        mock.delete = AsyncMock(return_value=1)
        mock.ping = AsyncMock(return_value=True)
        return mock

    @pytest.mark.asyncio
    async def test_get_miss(self, mock_redis):
        """Test cache miss returns None."""
        cache = RedisCache(mock_redis)

        result = await cache.get("nonexistent")
        assert result is None
        mock_redis.get.assert_called_once_with("nonexistent")

    @pytest.mark.asyncio
    async def test_get_hit(self, mock_redis):
        """Test cache hit returns deserialized data."""
        test_data = {"users": [{"id": 1, "name": "Alice"}]}
        mock_redis.get.return_value = json.dumps(test_data)

        cache = RedisCache(mock_redis)
        result = await cache.get("test_key")

        assert result == test_data
        mock_redis.get.assert_called_once_with("test_key")

    @pytest.mark.asyncio
    async def test_set_with_ttl(self, mock_redis):
        """Test setting cache with TTL."""
        cache = RedisCache(mock_redis)
        test_data = {"count": 42}

        await cache.set("test_key", test_data, ttl=300)

        mock_redis.setex.assert_called_once_with("test_key", 300, json.dumps(test_data))

    @pytest.mark.asyncio
    async def test_delete(self, mock_redis):
        """Test cache deletion."""
        cache = RedisCache(mock_redis)

        result = await cache.delete("test_key")

        assert result is True
        mock_redis.delete.assert_called_once_with("test_key")

    @pytest.mark.asyncio
    async def test_delete_pattern(self, mock_redis):
        """Test pattern-based cache deletion."""

        # Create an async generator for scan_iter
        async def async_scan_iter(match=None):
            for key in ["key1", "key2", "key3"]:
                yield key

        mock_redis.scan_iter = async_scan_iter
        mock_redis.delete = AsyncMock(return_value=3)
        cache = RedisCache(mock_redis)

        count = await cache.delete_pattern("fraiseql:users:*")

        assert count == 3
        mock_redis.delete.assert_called_once_with("key1", "key2", "key3")

    @pytest.mark.asyncio
    async def test_connection_error_handling(self, mock_redis):
        """Test proper error handling for connection issues."""
        from redis.exceptions import ConnectionError as RedisConnectionErrorBase

        mock_redis.get.side_effect = RedisConnectionErrorBase("Redis unavailable")
        cache = RedisCache(mock_redis)

        with pytest.raises(RedisConnectionError):
            await cache.get("test_key")

    @pytest.mark.asyncio
    async def test_serialization_error_handling(self, mock_redis):
        """Test handling of non-serializable data."""
        cache = RedisCache(mock_redis)

        # Object that can't be JSON serialized
        non_serializable = {"func": lambda x: x}

        with pytest.raises(ValueError, match="Failed to serialize"):
            await cache.set("test_key", non_serializable, ttl=300)


class TestResultCache:
    """Test the main result cache functionality."""

    @pytest.fixture
    def mock_backend(self):
        """Create mock cache backend."""
        return AsyncMock()

    @pytest.fixture
    def cache_config(self):
        """Create test cache configuration."""
        return CacheConfig(default_ttl=300, max_ttl=3600, cache_errors=False, key_prefix="test")

    @pytest.mark.asyncio
    async def test_cache_hit(self, mock_backend, cache_config):
        """Test returning cached result on hit."""
        cached_data = {"users": [{"id": 1, "name": "Alice"}]}
        mock_backend.get.return_value = cached_data

        cache = ResultCache(backend=mock_backend, config=cache_config)

        # Define a query function
        query_func = AsyncMock(return_value={"users": []})

        # Execute with cache
        result = await cache.get_or_set(key="test_key", func=query_func, ttl=300)

        assert result == cached_data
        query_func.assert_not_called()  # Should not execute query
        assert cache.stats.hits == 1
        assert cache.stats.misses == 0

    @pytest.mark.asyncio
    async def test_cache_miss(self, mock_backend, cache_config):
        """Test executing query and caching on miss."""
        mock_backend.get.return_value = None
        query_result = {"users": [{"id": 2, "name": "Bob"}]}

        cache = ResultCache(backend=mock_backend, config=cache_config)
        query_func = AsyncMock(return_value=query_result)

        result = await cache.get_or_set(key="test_key", func=query_func, ttl=300)

        assert result == query_result
        query_func.assert_called_once()
        mock_backend.set.assert_called_once_with("test_key", query_result, ttl=300)
        assert cache.stats.hits == 0
        assert cache.stats.misses == 1

    @pytest.mark.asyncio
    async def test_cache_disabled(self, mock_backend):
        """Test bypass when cache is disabled."""
        config = CacheConfig(enabled=False)
        cache = ResultCache(backend=mock_backend, config=config)

        query_result = {"data": "fresh"}
        query_func = AsyncMock(return_value=query_result)

        result = await cache.get_or_set(key="test_key", func=query_func, ttl=300)

        assert result == query_result
        query_func.assert_called_once()
        mock_backend.get.assert_not_called()
        mock_backend.set.assert_not_called()

    @pytest.mark.asyncio
    async def test_cache_error_fallback(self, mock_backend, cache_config):
        """Test fallback to query execution on cache error."""
        mock_backend.get.side_effect = Exception("Cache error")
        query_result = {"data": "fresh"}

        cache = ResultCache(backend=mock_backend, config=cache_config)
        query_func = AsyncMock(return_value=query_result)

        result = await cache.get_or_set(key="test_key", func=query_func, ttl=300)

        assert result == query_result
        query_func.assert_called_once()
        assert cache.stats.errors == 1

    @pytest.mark.asyncio
    async def test_ttl_limits(self, mock_backend, cache_config):
        """Test TTL is limited by max_ttl config."""
        cache = ResultCache(backend=mock_backend, config=cache_config)
        mock_backend.get.return_value = None

        await cache.get_or_set(
            key="test_key",
            func=AsyncMock(return_value={}),
            ttl=7200,  # Exceeds max_ttl
        )

        # Should be capped at max_ttl
        mock_backend.set.assert_called_once()
        call_args = mock_backend.set.call_args
        assert call_args[1]["ttl"] == 3600

    @pytest.mark.asyncio
    async def test_invalidation(self, mock_backend, cache_config):
        """Test cache invalidation."""
        cache = ResultCache(backend=mock_backend, config=cache_config)

        # Single key invalidation
        await cache.invalidate("test_key")
        mock_backend.delete.assert_called_once_with("test_key")

        # Pattern invalidation
        await cache.invalidate_pattern("users:*")
        mock_backend.delete_pattern.assert_called_once_with("users:*")

    def test_stats_tracking(self, mock_backend, cache_config):
        """Test cache statistics tracking."""
        cache = ResultCache(backend=mock_backend, config=cache_config)

        stats = cache.get_stats()
        assert isinstance(stats, CacheStats)
        assert stats.hits == 0
        assert stats.misses == 0
        assert stats.errors == 0
        assert stats.hit_rate == 0.0

    @pytest.mark.asyncio
    async def test_cache_warming(self, mock_backend, cache_config):
        """Test cache warming functionality."""
        cache = ResultCache(backend=mock_backend, config=cache_config)

        # Warm cache with specific queries
        queries = [("users", {"status": "active"}), ("products", {"category": "electronics"})]

        query_func = AsyncMock(side_effect=[{"users": [1, 2, 3]}, {"products": [4, 5, 6]}])

        await cache.warm_cache(queries, query_func)

        assert query_func.call_count == 2
        assert mock_backend.set.call_count == 2


class TestCachedQueryDecorator:
    """Test the @cached_query decorator."""

    @pytest.mark.asyncio
    async def test_decorator_basic(self):
        """Test basic decorator functionality."""
        mock_cache = AsyncMock()
        mock_cache.get_or_set.return_value = {"result": "cached"}

        @cached_query(cache=mock_cache, ttl=300)
        async def get_users(status: str = "active"):
            return {"users": ["user1", "user2"]}

        result = await get_users(status="active")

        assert result == {"result": "cached"}
        mock_cache.get_or_set.assert_called_once()

    @pytest.mark.asyncio
    async def test_decorator_key_generation(self):
        """Test automatic cache key generation from function args."""
        mock_cache = AsyncMock()

        @cached_query(cache=mock_cache, ttl=300)
        async def get_user_posts(user_id: UUID, limit: int = 10):
            return {"posts": []}

        user_id = uuid4()
        await get_user_posts(user_id=user_id, limit=20)

        call_args = mock_cache.get_or_set.call_args
        cache_key = call_args[1]["key"]

        assert "get_user_posts" in cache_key
        assert str(user_id) in cache_key
        assert "limit:20" in cache_key

    @pytest.mark.asyncio
    async def test_decorator_skip_cache(self):
        """Test skipping cache with skip_cache parameter."""
        mock_cache = AsyncMock()

        @cached_query(cache=mock_cache, ttl=300)
        async def get_data():
            return {"data": "fresh"}

        # Normal call - uses cache
        await get_data()
        assert mock_cache.get_or_set.called

        # Skip cache
        mock_cache.reset_mock()
        result = await get_data(skip_cache=True)
        assert result == {"data": "fresh"}
        mock_cache.get_or_set.assert_not_called()

    @pytest.mark.asyncio
    async def test_decorator_custom_key_func(self):
        """Test custom cache key function."""
        mock_cache = AsyncMock()

        def custom_key_func(**kwargs):
            return f"custom:{kwargs.get('user_id', 'unknown')}"

        @cached_query(cache=mock_cache, ttl=300, key_func=custom_key_func)
        async def get_user_data(user_id: str):
            return {"user": user_id}

        await get_user_data(user_id="123")

        call_args = mock_cache.get_or_set.call_args
        assert call_args[1]["key"] == "custom:123"


class TestCacheIntegration:
    """Integration tests for caching with FraiseQL."""

    @pytest.mark.asyncio
    async def test_repository_caching(self):
        """Test caching integration with FraiseQLRepository."""
        # This would test actual integration with the repository
        # For now, it's a placeholder showing the intended behavior

    @pytest.mark.asyncio
    async def test_graphql_query_caching(self):
        """Test caching of GraphQL query results."""
        # This would test caching at the GraphQL layer
        # For now, it's a placeholder showing the intended behavior

    @pytest.mark.asyncio
    async def test_cache_invalidation_on_mutations(self):
        """Test automatic cache invalidation on data mutations."""
        # This would test that mutations properly invalidate related caches
        # For now, it's a placeholder showing the intended behavior
