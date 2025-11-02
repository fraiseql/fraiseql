# Extracted from: docs/production/monitoring.md
# Block number: 20
from fraiseql.db import get_db_pool


async def collect_pool_metrics():
    """Collect database pool metrics."""
    pool = get_db_pool()
    stats = pool.get_stats()

    # Update Prometheus gauges
    db_pool_connections.labels(state="active").set(stats["pool_size"] - stats["pool_available"])
    db_pool_connections.labels(state="idle").set(stats["pool_available"])

    # Log if pool is saturated
    utilization = (stats["pool_size"] / pool.max_size) * 100
    if utilization > 90:
        logger.warning(
            "Database pool highly utilized",
            extra={
                "pool_size": stats["pool_size"],
                "max_size": pool.max_size,
                "utilization_pct": utilization,
            },
        )


# Collect metrics periodically
import asyncio


async def metrics_collector():
    while True:
        await collect_pool_metrics()
        await asyncio.sleep(15)  # Every 15 seconds


asyncio.create_task(metrics_collector())
