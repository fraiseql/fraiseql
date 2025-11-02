# Extracted from: docs/performance/caching.md
# Block number: 32
# Increase TTLs
result_cache.default_ttl = 600  # 10 minutes

# Check key diversity
stats = await postgres_cache.get_stats()
print(f"Total entries: {stats['total_entries']}")
# If > 100,000: Consider query normalization

# Verify tenant_id in keys
cache_key = key_builder.build_key("users", tenant_id=tenant_id, ...)
print(cache_key)  # Should include tenant_id
