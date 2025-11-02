# Extracted from: docs/performance/caching.md
# Block number: 6
from fraiseql.caching import CachedRepository

cached_repo = CachedRepository(base_repository=base_repo, cache=result_cache)

# Query with custom TTL
users = await cached_repo.find(
    "users",
    status="active",
    cache_ttl=600,  # 10 minutes for this query
)

# Skip cache for specific query
users = await cached_repo.find(
    "users",
    status="active",
    skip_cache=True,  # Bypass cache, fetch fresh data
)
