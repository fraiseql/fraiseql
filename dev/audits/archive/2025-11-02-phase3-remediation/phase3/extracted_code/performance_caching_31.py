# Extracted from: docs/performance/caching.md
# Block number: 31
# Scheduled health check
async def check_cache_health():
    stats = await postgres_cache.get_stats()

    # Alert if too many expired entries (cleanup not working)
    if stats["expired_entries"] > 10000:
        logger.warning(f"High expired entry count: {stats['expired_entries']}")

    # Alert if cache table too large (increase cleanup frequency)
    if stats["table_size_bytes"] > 1_000_000_000:  # 1GB
        logger.warning(f"Cache table large: {stats['table_size_bytes']} bytes")

    # Alert if hit rate too low (TTLs too short or invalidation too aggressive)
    hit_rate = stats["hits"] / (stats["hits"] + stats["misses"])
    if hit_rate < 0.5:
        logger.warning(f"Low cache hit rate: {hit_rate:.1%}")
