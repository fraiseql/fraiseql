# Extracted from: docs/performance/caching.md
# Block number: 14
# Returns just the result, metadata handled internally
result = await cache.get("cache_key")
# result = [...query results...]  (unwrapped)

# Access metadata explicitly
result, versions = await cache.get_with_metadata("cache_key")
# result = [...query results...]
# versions = {"user": 42, "post": 15}
