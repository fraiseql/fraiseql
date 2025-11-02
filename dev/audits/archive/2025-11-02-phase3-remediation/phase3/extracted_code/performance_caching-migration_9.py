# Extracted from: docs/performance/caching-migration.md
# Block number: 9
# Cache rarely-changing data
countries = await cached_repo.find("countries", cache_ttl=3600)

# Skip cache for frequently-changing data
orders = await cached_repo.find("orders", skip_cache=True)
