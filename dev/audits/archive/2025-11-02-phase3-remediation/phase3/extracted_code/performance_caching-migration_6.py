# Extracted from: docs/performance/caching-migration.md
# Block number: 6
from apscheduler.schedulers.asyncio import AsyncIOScheduler

scheduler = AsyncIOScheduler()


@scheduler.scheduled_job("interval", minutes=5)
async def cleanup_expired_cache():
    cache_backend = app.state.result_cache.backend
    cleaned = await cache_backend.cleanup_expired()
    if cleaned > 0:
        print(f"Cleaned {cleaned} expired cache entries")


@app.on_event("startup")
async def start_scheduler():
    scheduler.start()


@app.on_event("shutdown")
async def stop_scheduler():
    scheduler.shutdown()
