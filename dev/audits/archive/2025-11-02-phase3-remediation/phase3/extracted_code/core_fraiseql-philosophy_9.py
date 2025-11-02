# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 9
from fraiseql.caching import PostgresCache

cache = PostgresCache(db_pool)
await cache.set("user:123", user_data, ttl=3600)

# Features:
# - UNLOGGED tables for Redis-level performance
# - No WAL overhead = fast writes
# - Shared across app instances
# - TTL-based automatic expiration
# - Pattern-based deletion
