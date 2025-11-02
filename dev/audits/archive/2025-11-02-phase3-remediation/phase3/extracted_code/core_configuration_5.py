# Extracted from: docs/core/configuration.md
# Block number: 5
# High-performance configuration
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    enable_query_caching=True,
    cache_ttl=600,  # 10 minutes
    enable_turbo_router=True,
    turbo_router_cache_size=5000,
    turbo_max_complexity=200,
)
