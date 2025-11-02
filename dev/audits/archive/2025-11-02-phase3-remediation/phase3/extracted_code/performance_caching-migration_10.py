# Extracted from: docs/performance/caching-migration.md
# Block number: 10
# Caching automatic (no skip_cache flag)
users = await cached_repo.find("users")
products = await cached_repo.find("products", status="active")
