# Extracted from: docs/performance/PERFORMANCE_GUIDE.md
# Block number: 4
# Recommended settings
config = FraiseQLConfig(
    database_pool_size=20,  # 20% of max_connections
    database_max_overflow=10,  # Burst capacity
    database_pool_timeout=5.0,  # Fail fast
)
