# Extracted from: docs/database/DATABASE_LEVEL_CACHING.md
# Block number: 4
from fraiseql import query


class DatabaseCache:
    """Database-level result cache with metrics"""

    async def get_or_compute(self, cache_key: str, query_type: str, ttl: int, compute_fn) -> Any:
        # 1. Try cache
        cached = await self.db.fetchrow(
            """
            SELECT result, hit_count
            FROM result_cache
            WHERE cache_key = $1 AND valid_until > NOW()
            """,
            cache_key,
        )

        if cached:
            # Cache hit - increment counter
            await self.db.execute("SELECT increment_cache_hit($1)", cache_key)
            return json.loads(cached["result"])

        # 2. Cache miss - compute
        start = time.perf_counter()
        result = await compute_fn()
        duration_ms = (time.perf_counter() - start) * 1000

        # 3. Store with metrics
        await self.db.execute(
            """
            INSERT INTO result_cache (cache_key, query_type, result, valid_until, computation_time_ms)
            VALUES ($1, $2, $3, NOW() + $4 * INTERVAL '1 second', $5)
            ON CONFLICT (cache_key) DO UPDATE
            SET result = EXCLUDED.result,
                valid_until = EXCLUDED.valid_until,
                computation_time_ms = EXCLUDED.computation_time_ms,
                computed_at = NOW()
            """,
            cache_key,
            query_type,
            json.dumps(result),
            ttl,
            int(duration_ms),
        )

        return result

    async def get_cache_stats(self, query_type: str) -> dict:
        """Analyze cache effectiveness"""
        stats = await self.db.fetchrow(
            """
            SELECT
                COUNT(*) as total_entries,
                SUM(hit_count) as total_hits,
                AVG(computation_time_ms) as avg_computation_ms,
                SUM(CASE WHEN hit_count > 0 THEN 1 ELSE 0 END) as entries_with_hits
            FROM result_cache
            WHERE query_type = $1
            """,
            query_type,
        )
        return dict(stats)


# Usage
@query
async def dashboard(info) -> Dashboard:
    cache = DatabaseCache(info.context["db"])

    return await cache.get_or_compute(
        cache_key="dashboard:main",
        query_type="dashboard",
        ttl=300,
        compute_fn=lambda: compute_expensive_dashboard(),
    )


# Monitoring
async def analyze_cache_performance():
    stats = await cache.get_cache_stats("dashboard")
    print(
        f"Dashboard cache: {stats['total_hits']} hits, "
        f"avg computation: {stats['avg_computation_ms']}ms"
    )
