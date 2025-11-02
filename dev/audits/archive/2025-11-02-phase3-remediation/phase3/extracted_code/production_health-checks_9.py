# Extracted from: docs/production/health-checks.md
# Block number: 9
from fraiseql.monitoring import CheckResult, HealthStatus


async def check_redis() -> CheckResult:
    """Check Redis cache connectivity."""
    try:
        redis = get_redis_client()
        await redis.ping()

        return CheckResult(
            name="redis", status=HealthStatus.HEALTHY, message="Redis connection successful"
        )

    except Exception as e:
        return CheckResult(
            name="redis", status=HealthStatus.UNHEALTHY, message=f"Redis connection failed: {e}"
        )


# Register the check
health.add_check("redis", check_redis)
