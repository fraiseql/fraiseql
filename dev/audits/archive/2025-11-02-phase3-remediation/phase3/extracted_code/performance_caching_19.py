# Extracted from: docs/performance/caching.md
# Block number: 19
async def smart_find(view_name: str, **kwargs):
    """Cache only if query is expensive."""
    # Don't cache simple lookups by ID
    if "id" in kwargs and len(kwargs) == 1:
        return await base_repo.find_one(view_name, **kwargs)

    # Cache complex queries
    if len(kwargs) > 2 or "order_by" in kwargs:
        return await cached_repo.find(view_name, cache_ttl=300, **kwargs)

    # Default: no cache
    return await base_repo.find(view_name, **kwargs)
