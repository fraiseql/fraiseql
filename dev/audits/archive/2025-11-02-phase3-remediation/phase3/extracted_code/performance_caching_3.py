# Extracted from: docs/performance/caching.md
# Block number: 3
cache = PostgresCache(pool)
await cache._ensure_initialized()

if cache.has_domain_versioning:
    print(f"✓ pg_fraiseql_cache v{cache.extension_version} detected")
    print("  Domain-based invalidation enabled")
else:
    print("Using TTL-only caching (no extension)")
