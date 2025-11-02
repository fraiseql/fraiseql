# Extracted from: docs/performance/caching.md
# Block number: 28
# Frequently changing data (short TTL)
recent_orders = await cached_repo.find(
    "orders",
    created_at__gte=today,
    cache_ttl=60,  # 1 minute
)

# Rarely changing data (long TTL)
categories = await cached_repo.find(
    "categories",
    status="active",
    cache_ttl=3600,  # 1 hour
)

# Static data (very long TTL)
countries = await cached_repo.find(
    "countries",
    cache_ttl=86400,  # 24 hours
)
