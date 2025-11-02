# Extracted from: docs/performance/caching.md
# Block number: 5
from fraiseql.caching import ResultCache

result_cache = ResultCache(
    backend=postgres_cache,
    default_ttl=300,  # Default TTL in seconds (5 min)
    enable_stats=True,  # Track hit/miss statistics
)
