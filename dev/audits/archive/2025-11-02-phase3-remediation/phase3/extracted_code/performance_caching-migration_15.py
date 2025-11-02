# Extracted from: docs/performance/caching-migration.md
# Block number: 15
# Ensure auto_initialize=True
cache = PostgresCache(
    connection_pool=pool,
    auto_initialize=True,  # ← Must be True
)

# Or create manually
await cache._ensure_initialized()
