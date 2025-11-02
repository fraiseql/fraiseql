# Extracted from: docs/performance/caching.md
# Block number: 9
# ⚠️ SECURITY ISSUE: Missing tenant_id
base_repo = FraiseQLRepository(pool, context={})

cached_repo = CachedRepository(base_repo, result_cache)
users = await cached_repo.find("users", status="active")
# Cache key: "fraiseql:users:status:active"  ← SHARED ACROSS TENANTS!
