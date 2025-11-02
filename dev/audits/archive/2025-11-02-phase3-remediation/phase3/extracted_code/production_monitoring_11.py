# Extracted from: docs/production/monitoring.md
# Block number: 11
from fraiseql.caching import PostgresCache

cache = PostgresCache(db_pool)

await cache.set("key", "value", ttl=3600)
value = await cache.get("key")
