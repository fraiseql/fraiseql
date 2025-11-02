# Extracted from: docs/performance/caching.md
# Block number: 23
# Invalidate all user queries for a tenant
pattern = key_builder.build_mutation_pattern("user")
# Result: "fraiseql:user:*"

await result_cache.invalidate_pattern(pattern)
# Deletes: fraiseql:tenant-a:user:*, fraiseql:tenant-b:user:*, etc.
