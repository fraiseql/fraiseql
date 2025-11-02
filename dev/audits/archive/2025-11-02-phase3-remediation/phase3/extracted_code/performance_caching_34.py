# Extracted from: docs/performance/caching.md
# Block number: 34
# Increase cleanup frequency
@scheduler.scheduled_job("interval", minutes=1)  # Every minute
async def cleanup_cache():
    await postgres_cache.cleanup_expired()


# Limit cache value size
if len(json.dumps(result)) > 100_000:  # > 100KB
    # Don't cache large results
    return result
