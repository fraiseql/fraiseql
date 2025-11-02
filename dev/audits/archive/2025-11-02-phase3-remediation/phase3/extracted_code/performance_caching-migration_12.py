# Extracted from: docs/performance/caching-migration.md
# Block number: 12
# First query (cache miss)
result1 = await cached_repo.find("users", status="active")

# Second query (cache hit)
result2 = await cached_repo.find("users", status="active")

# Results should be identical
assert result1 == result2
