# Extracted from: docs/database/DATABASE_LEVEL_CACHING.md
# Block number: 2
from fraiseql import query


async def cached_query(cache_key: str, ttl: int, query_fn):
    """Query with database-level caching"""
    # 1. Check cache
    result = await db.fetchrow(
        "SELECT result FROM query_cache WHERE cache_key = $1 AND expires_at > NOW()", cache_key
    )

    if result:
        # Cache hit (0.1ms)
        return result["result"]

    # 2. Execute query
    data = await query_fn()

    # 3. Store in cache
    await db.execute(
        """
        INSERT INTO query_cache (cache_key, result, expires_at)
        VALUES ($1, $2, NOW() + $3 * INTERVAL '1 second')
        ON CONFLICT (cache_key) DO UPDATE
        SET result = EXCLUDED.result, expires_at = EXCLUDED.expires_at
        """,
        cache_key,
        json.dumps(data),
        ttl,
    )

    return data


# Usage
@query
async def expensive_query(info) -> DashboardStats:
    return await cached_query(
        cache_key="dashboard:main",
        ttl=300,  # 5 minutes
        query_fn=lambda: execute_expensive_query(),
    )
