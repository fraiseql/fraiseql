"""FraiseQL result caching functionality.

This module provides a flexible caching layer for query results with
support for multiple backends (Redis, in-memory) and automatic cache
key generation based on query parameters.
"""

from .cache_key import CacheKeyBuilder
from .repository_integration import CachedRepository

# Lazy import Redis-dependent classes
try:
    from .redis_cache import RedisCache, RedisConnectionError

    _HAS_REDIS = True
except ImportError:
    _HAS_REDIS = False

    class RedisCache:
        def __init__(self, *args, **kwargs):
            raise ImportError(
                "Redis is required for RedisCache. Install it with: pip install fraiseql[redis]"
            )

    class RedisConnectionError(Exception):
        pass


from .result_cache import (
    CacheBackend,
    CacheConfig,
    CacheStats,
    ResultCache,
    cached_query,
)

__all__ = [
    "CacheBackend",
    "CacheConfig",
    "CacheKeyBuilder",
    "CacheStats",
    "CachedRepository",
    "RedisCache",
    "RedisConnectionError",
    "ResultCache",
    "cached_query",
]
