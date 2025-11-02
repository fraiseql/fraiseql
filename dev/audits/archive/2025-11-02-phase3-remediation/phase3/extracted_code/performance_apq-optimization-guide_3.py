# Extracted from: docs/performance/apq-optimization-guide.md
# Block number: 3
config = FraiseQLConfig(
    apq_storage_backend="postgresql",
    apq_backend_config={
        "db_url": "postgresql://...",
        "table_name": "apq_cache",
        "response_ttl": 300,  # 5 minutes
    },
)
