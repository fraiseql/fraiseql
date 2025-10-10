"""FraiseQL result caching functionality.

This module provides a flexible caching layer for query results with
PostgreSQL-backed caching using UNLOGGED tables for maximum performance.
"""

from .cache_key import CacheKeyBuilder
from .postgres_cache import PostgresCache, PostgresCacheError
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
    "PostgresCache",
    "PostgresCacheError",
    "ResultCache",
    "cached_query",
]
