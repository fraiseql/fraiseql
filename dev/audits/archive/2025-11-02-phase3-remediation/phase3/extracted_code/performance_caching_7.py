# Extracted from: docs/performance/caching.md
# Block number: 7
from apscheduler.schedulers.asyncio import AsyncIOScheduler

scheduler = AsyncIOScheduler()


# Clean expired entries every 5 minutes
@scheduler.scheduled_job("interval", minutes=5)
async def cleanup_cache():
    cleaned = await postgres_cache.cleanup_expired()
    print(f"Cleaned {cleaned} expired cache entries")


scheduler.start()
