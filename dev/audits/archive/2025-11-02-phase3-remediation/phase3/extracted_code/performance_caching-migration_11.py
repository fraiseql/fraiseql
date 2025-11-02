# Extracted from: docs/performance/caching-migration.md
# Block number: 11
from fraiseql.caching import CacheKeyBuilder

key_builder = CacheKeyBuilder()
cache_key = key_builder.build_key(
    query_name="users", tenant_id=repo.context.get("tenant_id"), filters={"status": "active"}
)

print(cache_key)
# Should include tenant_id: "fraiseql:tenant-123:users:status:active"
