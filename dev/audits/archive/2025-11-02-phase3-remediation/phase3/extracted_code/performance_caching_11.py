# Extracted from: docs/performance/caching.md
# Block number: 11
# Cache entry valid for 5 minutes, even if data changes
users = await cached_repo.find("users", cache_ttl=300)
# ❌ If user data changes, cache remains stale until TTL expires
