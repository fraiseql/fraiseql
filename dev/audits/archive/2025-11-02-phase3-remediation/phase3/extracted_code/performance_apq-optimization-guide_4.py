# Extracted from: docs/performance/apq-optimization-guide.md
# Block number: 4
config = FraiseQLConfig(
    apq_storage_backend="redis",
    apq_backend_config={
        "redis_url": "redis://localhost:6379/0",
        "key_prefix": "fraiseql:apq:",
        "response_ttl": 300,
    },
)
