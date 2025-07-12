"""Test Redis optional dependency functionality."""

import sys
from unittest.mock import patch
import pytest


class TestRedisOptionalDependency:
    """Test that FraiseQL works without Redis installed."""

    def test_basic_imports_work_without_redis(self):
        """Test that basic FraiseQL imports work without Redis."""
        # Mock Redis import failure
        with patch.dict("sys.modules", {"redis": None, "redis.asyncio": None}):
            # These imports should work without Redis
            from fraiseql.fastapi import create_fraiseql_app, FraiseQLConfig
            from fraiseql.db import FraiseQLRepository
            import fraiseql

            # Basic decorators should work
            @fraiseql.type
            class TestType:
                id: str
                name: str

            @fraiseql.query
            async def test_query(info) -> TestType:
                return TestType(id="1", name="test")

            # Should be able to create config
            config = FraiseQLConfig(
                database_url="postgresql://test/test",
                environment="development",
            )

            assert config.environment == "development"

    def test_redis_cache_fails_gracefully_without_redis(self):
        """Test that Redis-dependent classes fail with helpful errors."""
        # Mock Redis import failure
        with patch.dict("sys.modules", {"redis": None, "redis.asyncio": None}):
            from fraiseql.caching import RedisCache

            # Should raise helpful error when trying to instantiate
            with pytest.raises(ImportError) as exc_info:
                RedisCache("fake_client")

            assert "Redis is required for RedisCache" in str(exc_info.value)
            assert "pip install fraiseql[redis]" in str(exc_info.value)

    def test_redis_rate_limiter_fails_gracefully_without_redis(self):
        """Test that RedisRateLimiter fails with helpful error."""
        with patch.dict("sys.modules", {"redis": None, "redis.asyncio": None}):
            from fraiseql.middleware import RedisRateLimiter, RateLimitConfig

            config = RateLimitConfig(requests_per_minute=60)

            with pytest.raises(ImportError) as exc_info:
                RedisRateLimiter("fake_client", config)

            assert "Redis is required for RedisRateLimiter" in str(exc_info.value)
            assert "pip install fraiseql[redis]" in str(exc_info.value)

    def test_redis_revocation_store_fails_gracefully_without_redis(self):
        """Test that RedisRevocationStore fails with helpful error."""
        with patch.dict("sys.modules", {"redis": None, "redis.asyncio": None}):
            from fraiseql.auth import RedisRevocationStore

            with pytest.raises(ImportError) as exc_info:
                RedisRevocationStore("fake_client")

            assert "Redis is required for RedisRevocationStore" in str(exc_info.value)
            assert "pip install fraiseql[redis]" in str(exc_info.value)

    def test_non_redis_classes_work_without_redis(self):
        """Test that non-Redis classes work fine without Redis installed."""
        with patch.dict("sys.modules", {"redis": None, "redis.asyncio": None}):
            from fraiseql.caching import CacheKeyBuilder, CacheConfig
            from fraiseql.auth import InMemoryRevocationStore, RevocationConfig
            from fraiseql.middleware import InMemoryRateLimiter, RateLimitConfig

            # These should work fine
            cache_builder = CacheKeyBuilder()
            cache_config = CacheConfig()

            revocation_store = InMemoryRevocationStore()
            revocation_config = RevocationConfig()

            rate_config = RateLimitConfig(requests_per_minute=60)
            rate_limiter = InMemoryRateLimiter(rate_config)

            # Basic functionality should work
            assert cache_config.enabled is True
            assert revocation_config.enabled is True
            assert rate_config.requests_per_minute == 60

    def test_subscription_decorator_works_without_redis(self):
        """Test that subscription decorator is available without Redis."""
        with patch.dict("sys.modules", {"redis": None, "redis.asyncio": None}):
            import fraiseql

            # Should be able to import and use subscription decorator
            assert hasattr(fraiseql, "subscription")

            # Should be able to use it (though won't work without Redis backend)
            @fraiseql.subscription
            async def test_subscription(info):
                yield {"data": "test"}

            assert hasattr(test_subscription, "__fraiseql_subscription__")

    def test_lazy_import_caching_module(self):
        """Test that caching module imports work with and without Redis."""
        # First test without Redis
        with patch.dict("sys.modules", {"redis": None, "redis.asyncio": None}):
            from fraiseql.caching import RedisCache, RedisConnectionError

            # Classes should be available but fail on instantiation
            assert RedisCache is not None
            assert RedisConnectionError is not None

            with pytest.raises(ImportError):
                RedisCache("fake_client")

    def test_lazy_import_auth_module(self):
        """Test that auth module imports work with and without Redis."""
        with patch.dict("sys.modules", {"redis": None, "redis.asyncio": None}):
            from fraiseql.auth import (
                RedisRevocationStore,
                InMemoryRevocationStore,
                RevocationConfig,
            )

            # Non-Redis classes should work
            store = InMemoryRevocationStore()
            config = RevocationConfig()

            assert store is not None
            assert config.enabled is True

            # Redis class should fail on instantiation
            with pytest.raises(ImportError):
                RedisRevocationStore("fake_client")

    def test_lazy_import_middleware_module(self):
        """Test that middleware module imports work with and without Redis."""
        with patch.dict("sys.modules", {"redis": None, "redis.asyncio": None}):
            from fraiseql.middleware import RedisRateLimiter, InMemoryRateLimiter, RateLimitConfig

            # Non-Redis classes should work
            config = RateLimitConfig(requests_per_minute=60)
            limiter = InMemoryRateLimiter(config)

            assert config.requests_per_minute == 60
            assert limiter is not None

            # Redis class should fail on instantiation
            with pytest.raises(ImportError):
                RedisRateLimiter("fake_client", config)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
