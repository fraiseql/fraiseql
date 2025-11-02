# Extracted from: docs/production/observability.md
# Block number: 11
from apscheduler.schedulers.asyncio import AsyncIOScheduler

scheduler = AsyncIOScheduler()


@scheduler.scheduled_job("cron", hour=2, minute=0)
async def cleanup_old_observability_data():
    """Run daily at 2 AM."""
    async with db_pool.acquire() as conn:
        # Clean errors
        await conn.execute("""
            DELETE FROM monitoring.errors
            WHERE occurred_at < NOW() - INTERVAL '90 days'
        """)

        # Clean traces
        await conn.execute("""
            DELETE FROM monitoring.traces
            WHERE start_time < NOW() - INTERVAL '30 days'
        """)

        # Clean metrics
        await conn.execute("""
            DELETE FROM monitoring.metrics
            WHERE timestamp < NOW() - INTERVAL '7 days'
        """)


scheduler.start()
