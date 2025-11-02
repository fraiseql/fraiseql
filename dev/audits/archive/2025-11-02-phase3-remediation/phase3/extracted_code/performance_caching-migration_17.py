# Extracted from: docs/performance/caching-migration.md
# Block number: 17
# Ensure mutations use cached_repo (auto-invalidates)
await cached_repo.execute_function("update_user", {"id": user_id, ...})

# Or manually invalidate
from fraiseql.caching import CacheKeyBuilder
key_builder = CacheKeyBuilder()
pattern = key_builder.build_mutation_pattern("user")
await result_cache.invalidate_pattern(pattern)
