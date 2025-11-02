# Extracted from: docs/performance/caching.md
# Block number: 29
# Admin dashboard: always fresh data
admin_stats = await cached_repo.find(
    "admin_stats",
    skip_cache=True,  # Never cache
)

# User-facing: can cache
user_stats = await cached_repo.find(
    "user_stats",
    user_id=user_id,
    cache_ttl=300,  # 5 minutes OK
)
