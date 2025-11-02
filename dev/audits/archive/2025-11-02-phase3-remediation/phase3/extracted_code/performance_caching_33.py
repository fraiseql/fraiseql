# Extracted from: docs/performance/caching.md
# Block number: 33
# Check extension
if not cache.has_domain_versioning:
    print("⚠️ pg_fraiseql_cache not installed - using TTL-only")
    # Install extension or reduce TTLs

# Manual invalidation after mutation
await result_cache.invalidate_pattern(key_builder.build_mutation_pattern("user"))

# Reduce TTL for frequently changing data
cache_ttl = 30  # 30 seconds
