# Extracted from: docs/performance/caching.md
# Block number: 13
# Without extension (backward compatible)
cache_value = [...query results...]

# With extension
cache_value = {
    "result": [...query results...],
    "versions": {
        "user": 42,
        "post": 15,
        "product": 8
    },
    "cached_at": "2025-10-11T10:00:00Z"
}
