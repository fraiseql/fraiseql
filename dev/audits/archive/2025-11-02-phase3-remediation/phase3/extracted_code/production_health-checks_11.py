# Extracted from: docs/production/health-checks.md
# Block number: 11
import httpx


async def check_payment_gateway() -> CheckResult:
    """Check external payment gateway availability."""
    try:
        async with httpx.AsyncClient() as client:
            response = await client.get("https://api.stripe.com/v1/health", timeout=5.0)

            if response.status_code == 200:
                return CheckResult(
                    name="stripe",
                    status=HealthStatus.HEALTHY,
                    message="Payment gateway operational",
                    metadata={
                        "latency_ms": response.elapsed.total_seconds() * 1000,
                        "status_code": response.status_code,
                    },
                )
            return CheckResult(
                name="stripe",
                status=HealthStatus.UNHEALTHY,
                message=f"Payment gateway returned {response.status_code}",
            )

    except httpx.TimeoutException:
        return CheckResult(
            name="stripe", status=HealthStatus.UNHEALTHY, message="Payment gateway timeout (> 5s)"
        )

    except Exception as e:
        return CheckResult(
            name="stripe", status=HealthStatus.UNHEALTHY, message=f"Payment gateway error: {e}"
        )
