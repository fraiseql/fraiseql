# Extracted from: docs/performance/apq-optimization-guide.md
# Block number: 1
from fraiseql.fastapi.config import FraiseQLConfig

# Query cache only (recommended starting point)
config = FraiseQLConfig(
    db_url="postgresql://...",
    apq_storage_backend="memory",  # or "postgresql", "redis"
    apq_cache_responses=False,  # Response caching disabled
)

# Full APQ with response caching
config = FraiseQLConfig(
    db_url="postgresql://...",
    apq_storage_backend="memory",
    apq_cache_responses=True,  # Enable response caching
    apq_backend_config={
        "response_ttl": 300,  # 5 minutes
    },
)
