# Extracted from: docs/performance/caching.md
# Block number: 36
# Check detection
cache = PostgresCache(pool)
await cache._ensure_initialized()

print(f"Extension detected: {cache.has_domain_versioning}")
print(f"Extension version: {cache.extension_version}")
