# Extracted from: docs/performance/caching.md
# Block number: 12
# Cache automatically invalidated when 'user' domain data changes
users = await cached_repo.find("users", cache_ttl=300)
# ✅ If user data changes, cache immediately invalidated (via triggers)
