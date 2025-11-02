# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 14
# Immediate: Invalidate now (default)
await cache.invalidate("user:123")

# Lazy: Queue for later invalidation
await cache.invalidate_lazy("user:123", delay=5.0)

# Useful for:
# - Non-critical caches
# - Batch processing
# - Reducing mutation latency
