# Extracted from: docs/performance/caching.md
# Block number: 8
# tenant_id extracted from repository context
base_repo = FraiseQLRepository(
    pool=pool,
    context={"tenant_id": "tenant-123"},  # REQUIRED for multi-tenant!
)

cached_repo = CachedRepository(base_repo, result_cache)

# Automatically generates tenant-scoped cache key
users = await cached_repo.find("users", status="active")
# Cache key: "fraiseql:tenant-123:users:status:active"
