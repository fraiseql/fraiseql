# Extracted from: docs/production/health-checks.md
# Block number: 15
import logging

from fraiseql.monitoring import HealthCheck, check_database, check_pool_stats

logger = logging.getLogger(__name__)

health = HealthCheck()
health.add_check("database", check_database)
health.add_check("pool", check_pool_stats)


@app.get("/health")
async def health_endpoint():
    """Health check with monitoring integration."""
    result = await health.run_checks()

    # Log degraded status for alerting
    if result["status"] == "degraded":
        failed_checks = [
            name for name, check in result["checks"].items() if check["status"] != "healthy"
        ]
        logger.warning(
            f"Health check degraded: {', '.join(failed_checks)}",
            extra={"failed_checks": failed_checks, "health_status": result},
        )

    return result
