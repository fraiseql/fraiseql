"""FraiseQL result caching functionality.

This module provides a flexible caching layer for query results with
support for multiple backends (Redis, in-memory) and automatic cache
key generation based on query parameters.
"""

from .cache_key import CacheKeyBuilder
from .redis_cache import RedisCache, RedisConnectionError
from .repository_integration import CachedRepository
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
