"""Tests for result cache module."""

from typing import Any

import pytest

from fraiseql.caching.result_cache import (
    CacheConfig,
    CacheStats,
    ResultCache,
    cached_query,
)


# Mock cache backend for testing
class MockCacheBackend:
    """Mock cache backend for testing."""

    def __init__(self) -> None:
        self._cache: dict[str, Any] = {}
        self.get_called = 0
        self.set_called = 0
        self.delete_called = 0
        self.delete_pattern_called = 0

    async def get(self, key: str) -> Any | None:
        self.get_called += 1
        return self._cache.get(key)

    async def set(self, key: str, value: Any, ttl: int) -> None:
        self.set_called += 1
        self._cache[key] = value

    async def delete(self, key: str) -> bool:
        self.delete_called += 1
        if key in self._cache:
            del self._cache[key]
            return True
        return False

    async def delete_pattern(self, pattern: str) -> int:
        self.delete_pattern_called += 1
        # Simple pattern matching (just count matches)
        prefix = pattern.replace("*", "")
        to_delete = [k for k in self._cache if k.startswith(prefix)]
        for k in to_delete:
            del self._cache[k]
        return len(to_delete)


# Tests for CacheConfig
@pytest.mark.unit
class TestCacheConfig:
    """Tests for CacheConfig dataclass."""

    def test_cache_config_defaults(self) -> None:
        """CacheConfig has sensible defaults."""
        config = CacheConfig()

        assert config.enabled is True
        assert config.default_ttl == 300  # 5 minutes
        assert config.max_ttl == 3600  # 1 hour
        assert config.cache_errors is False
        assert config.key_prefix == "fraiseql"

    def test_cache_config_custom_values(self) -> None:
        """CacheConfig accepts custom values."""
        config = CacheConfig(
            enabled=False,
            default_ttl=600,
            max_ttl=7200,
            cache_errors=True,
            key_prefix="myapp",
        )

        assert config.enabled is False
        assert config.default_ttl == 600
        assert config.max_ttl == 7200
        assert config.cache_errors is True
        assert config.key_prefix == "myapp"


# Tests for CacheStats
@pytest.mark.unit
class TestCacheStats:
    """Tests for CacheStats dataclass."""

    def test_cache_stats_defaults(self) -> None:
        """CacheStats starts with zero counts."""
        stats = CacheStats()

        assert stats.hits == 0
        assert stats.misses == 0
        assert stats.errors == 0

    def test_cache_stats_total(self) -> None:
        """CacheStats.total returns hits + misses."""
        stats = CacheStats(hits=10, misses=5)

        assert stats.total == 15

    def test_cache_stats_hit_rate(self) -> None:
        """CacheStats.hit_rate calculates percentage correctly."""
        stats = CacheStats(hits=75, misses=25)

        assert stats.hit_rate == 75.0

    def test_cache_stats_hit_rate_zero_total(self) -> None:
        """CacheStats.hit_rate returns 0.0 when no operations."""
        stats = CacheStats()

        assert stats.hit_rate == 0.0

    def test_cache_stats_hit_rate_all_hits(self) -> None:
        """CacheStats.hit_rate is 100% when all hits."""
        stats = CacheStats(hits=100, misses=0)

        assert stats.hit_rate == 100.0

    def test_cache_stats_hit_rate_all_misses(self) -> None:
        """CacheStats.hit_rate is 0% when all misses."""
        stats = CacheStats(hits=0, misses=100)

        assert stats.hit_rate == 0.0


# Tests for ResultCache
@pytest.mark.unit
class TestResultCache:
    """Tests for ResultCache class."""

    @pytest.fixture
    def backend(self) -> MockCacheBackend:
        """Create mock backend."""
        return MockCacheBackend()

    @pytest.fixture
    def config(self) -> CacheConfig:
        """Create cache config."""
        return CacheConfig()

    @pytest.fixture
    def cache(self, backend: MockCacheBackend, config: CacheConfig) -> ResultCache:
        """Create result cache."""
        return ResultCache(backend, config)

    def test_init_stores_backend_and_config(self, backend: MockCacheBackend) -> None:
        """ResultCache stores backend and config."""
        config = CacheConfig(default_ttl=600)
        cache = ResultCache(backend, config)

        assert cache.backend is backend
        assert cache.config is config
        assert cache.config.default_ttl == 600

    def test_init_default_config(self, backend: MockCacheBackend) -> None:
        """ResultCache uses default config when not provided."""
        cache = ResultCache(backend)

        assert cache.config.enabled is True
        assert cache.config.default_ttl == 300

    @pytest.mark.asyncio
    async def test_get_or_set_cache_hit(
        self, cache: ResultCache, backend: MockCacheBackend
    ) -> None:
        """get_or_set returns cached value on hit."""
        backend._cache["test_key"] = {"data": "cached"}

        async def fetch_data() -> dict[str, str]:
            return {"data": "fresh"}

        result = await cache.get_or_set("test_key", fetch_data)

        assert result == {"data": "cached"}
        assert cache.stats.hits == 1
        assert cache.stats.misses == 0

    @pytest.mark.asyncio
    async def test_get_or_set_cache_miss(
        self, cache: ResultCache, backend: MockCacheBackend
    ) -> None:
        """get_or_set executes function on miss and caches result."""

        async def fetch_data() -> dict[str, str]:
            return {"data": "fresh"}

        result = await cache.get_or_set("test_key", fetch_data)

        assert result == {"data": "fresh"}
        assert cache.stats.hits == 0
        assert cache.stats.misses == 1
        assert backend._cache["test_key"] == {"data": "fresh"}

    @pytest.mark.asyncio
    async def test_get_or_set_disabled(self, backend: MockCacheBackend) -> None:
        """get_or_set bypasses cache when disabled."""
        config = CacheConfig(enabled=False)
        cache = ResultCache(backend, config)
        call_count = 0

        async def fetch_data() -> dict[str, str]:
            nonlocal call_count
            call_count += 1
            return {"data": "fresh"}

        result = await cache.get_or_set("test_key", fetch_data)

        assert result == {"data": "fresh"}
        assert call_count == 1
        assert backend.get_called == 0  # Cache not consulted

    @pytest.mark.asyncio
    async def test_get_or_set_cache_error(self, backend: MockCacheBackend) -> None:
        """get_or_set handles cache errors gracefully."""
        config = CacheConfig(cache_errors=False)
        cache = ResultCache(backend, config)

        # Make get() raise an error
        async def failing_get(key: str) -> Any:
            raise RuntimeError("Cache failure")

        backend.get = failing_get  # type: ignore[method-assign]

        async def fetch_data() -> dict[str, str]:
            return {"data": "fresh"}

        result = await cache.get_or_set("test_key", fetch_data)

        assert result == {"data": "fresh"}
        assert cache.stats.errors == 1

    @pytest.mark.asyncio
    async def test_get_or_set_respects_max_ttl(
        self, cache: ResultCache, backend: MockCacheBackend
    ) -> None:
        """get_or_set caps TTL at max_ttl."""
        cache.config.max_ttl = 300
        set_ttls: list[int] = []

        original_set = backend.set

        async def track_set(key: str, value: Any, ttl: int) -> None:
            set_ttls.append(ttl)
            await original_set(key, value, ttl)

        backend.set = track_set  # type: ignore[method-assign]

        async def fetch_data() -> dict[str, str]:
            return {"data": "fresh"}

        await cache.get_or_set("test_key", fetch_data, ttl=600)  # Request 600s

        assert set_ttls[0] == 300  # Should be capped at max_ttl

    @pytest.mark.asyncio
    async def test_invalidate_key(self, cache: ResultCache, backend: MockCacheBackend) -> None:
        """invalidate() removes specific key from cache."""
        backend._cache["test_key"] = {"data": "cached"}

        await cache.invalidate("test_key")

        assert "test_key" not in backend._cache
        assert backend.delete_called == 1

    @pytest.mark.asyncio
    async def test_invalidate_pattern(self, cache: ResultCache, backend: MockCacheBackend) -> None:
        """invalidate_pattern() removes matching keys."""
        backend._cache["users:1"] = {"id": 1}
        backend._cache["users:2"] = {"id": 2}
        backend._cache["orders:1"] = {"id": 1}

        await cache.invalidate_pattern("users:*")

        assert "users:1" not in backend._cache
        assert "users:2" not in backend._cache
        assert "orders:1" in backend._cache

    def test_get_stats(self, cache: ResultCache) -> None:
        """get_stats() returns current statistics."""
        cache.stats.hits = 10
        cache.stats.misses = 5
        cache.stats.errors = 1

        stats = cache.get_stats()

        assert stats.hits == 10
        assert stats.misses == 5
        assert stats.errors == 1

    def test_reset_stats(self, cache: ResultCache) -> None:
        """reset_stats() resets all statistics."""
        cache.stats.hits = 10
        cache.stats.misses = 5
        cache.stats.errors = 1

        cache.reset_stats()

        assert cache.stats.hits == 0
        assert cache.stats.misses == 0
        assert cache.stats.errors == 0

    @pytest.mark.asyncio
    async def test_warm_cache(self, cache: ResultCache, backend: MockCacheBackend) -> None:
        """warm_cache() pre-populates cache with query results."""
        queries = [
            ("users", {"status": "active"}),
            ("orders", {"status": "pending"}),
        ]

        async def query_func(name: str, filters: dict[str, Any]) -> dict[str, Any]:
            return {"query": name, "filters": filters}

        await cache.warm_cache(queries, query_func)

        assert backend.set_called == 2


# Tests for cached_query decorator
@pytest.mark.unit
class TestCachedQueryDecorator:
    """Tests for cached_query decorator."""

    @pytest.fixture
    def backend(self) -> MockCacheBackend:
        """Create mock backend."""
        return MockCacheBackend()

    @pytest.fixture
    def cache(self, backend: MockCacheBackend) -> ResultCache:
        """Create result cache."""
        return ResultCache(backend)

    @pytest.mark.asyncio
    async def test_cached_query_decorator(
        self, cache: ResultCache, backend: MockCacheBackend
    ) -> None:
        """cached_query decorator caches function results."""
        call_count = 0

        @cached_query(cache, ttl=300)
        async def get_user(user_id: int) -> dict[str, Any]:
            nonlocal call_count
            call_count += 1
            return {"id": user_id, "name": "Test"}

        # First call - cache miss
        result1 = await get_user(user_id=123)
        assert result1 == {"id": 123, "name": "Test"}
        assert call_count == 1

        # Second call - cache hit
        result2 = await get_user(user_id=123)
        assert result2 == {"id": 123, "name": "Test"}
        assert call_count == 1  # Function not called again

    @pytest.mark.asyncio
    async def test_cached_query_skip_cache(
        self, cache: ResultCache, backend: MockCacheBackend
    ) -> None:
        """cached_query respects skip_cache parameter."""
        call_count = 0

        @cached_query(cache, ttl=300)
        async def get_user(user_id: int) -> dict[str, Any]:
            nonlocal call_count
            call_count += 1
            return {"id": user_id, "name": "Test"}

        # First call
        await get_user(user_id=123)
        assert call_count == 1

        # Second call with skip_cache=True
        await get_user(user_id=123, skip_cache=True)
        assert call_count == 2  # Function called again

    @pytest.mark.asyncio
    async def test_cached_query_custom_key_func(
        self, cache: ResultCache, backend: MockCacheBackend
    ) -> None:
        """cached_query uses custom key function if provided."""

        def custom_key(user_id: int, **kwargs: Any) -> str:
            return f"custom:user:{user_id}"

        @cached_query(cache, ttl=300, key_func=custom_key)
        async def get_user(user_id: int) -> dict[str, Any]:
            return {"id": user_id}

        await get_user(user_id=123)

        assert "custom:user:123" in backend._cache

    @pytest.mark.asyncio
    async def test_cached_query_auto_key_generation(
        self, cache: ResultCache, backend: MockCacheBackend
    ) -> None:
        """cached_query auto-generates key from function name and args."""

        @cached_query(cache, ttl=300)
        async def fetch_data(param1: str, param2: int) -> dict[str, Any]:
            return {"param1": param1, "param2": param2}

        await fetch_data(param1="test", param2=42)

        # Key should contain function name and params
        keys = list(backend._cache.keys())
        assert len(keys) == 1
        assert "fetch_data" in keys[0]
        assert "param1:test" in keys[0]
        assert "param2:42" in keys[0]
