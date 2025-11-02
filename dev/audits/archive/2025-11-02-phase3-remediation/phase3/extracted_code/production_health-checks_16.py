# Extracted from: docs/production/health-checks.md
# Block number: 16
from fraiseql.monitoring.sentry import capture_message

from fraiseql.monitoring import HealthCheck

health = HealthCheck()
# ... register checks


@app.get("/health")
async def health_with_alerts():
    """Health check with automatic alerting."""
    result = await health.run_checks()

    if result["status"] == "degraded":
        # Alert to Sentry
        failed_checks = {
            name: check for name, check in result["checks"].items() if check["status"] != "healthy"
        }

        capture_message(
            f"Health check degraded: {len(failed_checks)} checks failing",
            level="warning",
            extra={"failed_checks": failed_checks},
        )

    return result
