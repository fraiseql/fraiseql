# Extracted from: docs/performance/caching.md
# Block number: 25
from prometheus_client import Counter, Gauge, Histogram

# Cache hit/miss counters
cache_hits = Counter("fraiseql_cache_hits_total", "Total cache hits", ["tenant_id", "view_name"])

cache_misses = Counter(
    "fraiseql_cache_misses_total", "Total cache misses", ["tenant_id", "view_name"]
)

# Cache operation duration
cache_get_duration = Histogram(
    "fraiseql_cache_get_duration_seconds",
    "Cache get operation duration",
    buckets=[0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0],
)

# Cache size
cache_size = Gauge("fraiseql_cache_entries_total", "Total cache entries")


# Instrument cache operations
@cache_get_duration.time()
async def get_cached(key: str):
    result = await cache.get(key)
    if result:
        cache_hits.labels(tenant_id, view_name).inc()
    else:
        cache_misses.labels(tenant_id, view_name).inc()
    return result
