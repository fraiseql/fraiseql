# Extracted from: docs/performance/caching.md
# Block number: 20
from fraiseql.caching import CacheKeyBuilder

key_builder = CacheKeyBuilder(prefix="fraiseql")

cache_key = key_builder.build_key(
    query_name="users",
    tenant_id="tenant-123",  # Tenant isolation
    filters={"status": "active", "role": "admin"},
    order_by=[("created_at", "DESC")],
    limit=10,
    offset=0,
)

# Result: "fraiseql:tenant-123:users:role:admin:status:active:order:created_at:DESC:limit:10:offset:0"
