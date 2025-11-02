# Extracted from: docs/diagrams/apq-cache-flow.md
# Block number: 6
class APQCacheManager:
    async def invalidate_query(self, query_hash):
        """Remove specific query from cache"""
        await self.cache.delete(f"apq:{query_hash}")

    async def invalidate_all(self):
        """Clear entire APQ cache"""
        # Implementation depends on cache type

    async def cleanup_unused(self, days=30):
        """Remove queries not used recently"""
        cutoff = datetime.now() - timedelta(days=days)
        # Remove from database cache
        await db.execute("DELETE FROM apq_cache WHERE last_used < $1", cutoff)
