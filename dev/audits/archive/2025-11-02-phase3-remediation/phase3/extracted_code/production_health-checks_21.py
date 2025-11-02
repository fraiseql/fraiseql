# Extracted from: docs/production/health-checks.md
# Block number: 21
import asyncio


async def check_with_timeout() -> CheckResult:
    """Prevent health checks from hanging."""
    try:
        # Timeout after 5 seconds
        async with asyncio.timeout(5.0):
            result = await slow_external_check()

        return CheckResult(
            name="external_api", status=HealthStatus.HEALTHY, message="External API responding"
        )

    except TimeoutError:
        return CheckResult(
            name="external_api",
            status=HealthStatus.UNHEALTHY,
            message="External API timeout (> 5s)",
        )
