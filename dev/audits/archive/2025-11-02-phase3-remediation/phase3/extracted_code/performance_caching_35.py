# Extracted from: docs/performance/caching.md
# Block number: 35
# Use separate pool for cache
cache_pool = DatabasePool(
    db_url,
    min_size=5,
    max_size=10,  # Smaller than main pool
)

cache = PostgresCache(cache_pool)
