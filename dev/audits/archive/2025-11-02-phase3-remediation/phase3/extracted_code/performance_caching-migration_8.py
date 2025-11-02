# Extracted from: docs/performance/caching-migration.md
# Block number: 8
# All queries skip cache
users = await cached_repo.find("users", skip_cache=True)
