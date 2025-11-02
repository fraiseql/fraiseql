# Extracted from: docs/production/monitoring.md
# Block number: 5
from fraiseql.caching import PostgresCache

# Initialize cache
cache = PostgresCache(db_pool)

# Basic operations
await cache.set("user:123", user_data, ttl=3600)  # 1 hour TTL
value = await cache.get("user:123")
await cache.delete("user:123")

# Pattern-based deletion
await cache.delete_pattern("user:*")  # Clear all user caches

# Batch operations
await cache.set_many(
    {"product:1": product1, "product:2": product2, "product:3": product3}, ttl=1800
)

values = await cache.get_many(["product:1", "product:2", "product:3"])
