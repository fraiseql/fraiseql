# Extracted from: docs/performance/caching-migration.md
# Block number: 16
# Option 1: Increase pool size
pool = DatabasePool(db_url, min_size=20, max_size=40)

# Option 2: Use separate pool for cache
cache_pool = DatabasePool(db_url, min_size=5, max_size=10)
cache = PostgresCache(cache_pool)
