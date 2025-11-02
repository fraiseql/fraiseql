# Extracted from: docs/reference/config.md
# Block number: 15
# APQ with PostgreSQL backend
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    apq_storage_backend="postgresql",
    apq_cache_responses=True,
    apq_response_cache_ttl=900,
)

# APQ with Redis backend
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    apq_storage_backend="redis",
    apq_backend_config={"redis_url": "redis://localhost:6379/0", "key_prefix": "apq:"},
)
