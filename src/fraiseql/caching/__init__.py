"""FraiseQL result caching functionality.

This module provides a flexible caching layer for query results with
PostgreSQL-backed caching using UNLOGGED tables for maximum performance.

Auto-CASCADE Features:
    - Automatic CASCADE rule generation from GraphQL schema relationships
    - Zero-config cache invalidation setup
    - Schema analysis and dependency tracking
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
from .schema_analyzer import (
    CascadeRule,
    SchemaAnalyzer,
    setup_auto_cascade_rules,
)

__all__ = [
    "CacheBackend",
    "CacheConfig",
    "CacheKeyBuilder",
    "CacheStats",
    "CachedRepository",
    "CascadeRule",
    "PostgresCache",
    "PostgresCacheError",
    "ResultCache",
    "SchemaAnalyzer",
    "cached_query",
    "setup_auto_cascade_rules",
]
