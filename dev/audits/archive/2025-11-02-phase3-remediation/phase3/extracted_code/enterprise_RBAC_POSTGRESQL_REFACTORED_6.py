# Extracted from: docs/enterprise/RBAC_POSTGRESQL_REFACTORED.md
# Block number: 6
# Add to PermissionCache class


async def get_stats(self) -> dict:
    """Get cache statistics.

    Returns:
        Dict with cache stats (hits, misses, size, etc.)
    """
    pg_stats = await self.pg_cache.get_stats()

    # Count RBAC-specific entries
    # (would need to query fraiseql_cache table with LIKE filter)

    return {
        "request_cache_size": len(self._request_cache),
        "postgres_cache_total": pg_stats["total_entries"],
        "postgres_cache_active": pg_stats["active_entries"],
        "postgres_cache_size_bytes": pg_stats["table_size_bytes"],
        "has_domain_versioning": self.pg_cache.has_domain_versioning,
        "cache_ttl_seconds": int(self._cache_ttl.total_seconds()),
    }
