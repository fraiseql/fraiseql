# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 22
# Track CASCADE overhead
stats = await cache.get_cascade_stats()

if stats["avg_cascade_time_ms"] > 10:
    logger.warning("CASCADE is slow, review rules")
