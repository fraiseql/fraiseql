# Extracted from: docs/production/health-checks.md
# Block number: 20
async def check_with_metadata() -> CheckResult:
    """Include diagnostic information."""
    return CheckResult(
        name="service",
        status=HealthStatus.HEALTHY,
        message="Service operational",
        metadata={
            "version": "1.2.3",
            "uptime_seconds": get_uptime(),
            "last_request": get_last_request_time(),
        },
    )
