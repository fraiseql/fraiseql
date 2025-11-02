# Extracted from: docs/performance/caching.md
# Block number: 17
from fraiseql.caching import CacheKeyBuilder

key_builder = CacheKeyBuilder()

# Build cache key
cache_key = key_builder.build_key(
    query_name="active_users", tenant_id=tenant_id, filters={"status": "active"}, limit=10
)

# Check cache
cached_result = await result_cache.get(cache_key)
if cached_result:
    return cached_result

# Fetch from database
result = await base_repo.find("users", status="active", limit=10)

# Cache result
await result_cache.set(cache_key, result, ttl=300)
