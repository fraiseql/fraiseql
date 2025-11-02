# Extracted from: docs/production/health-checks.md
# Block number: 14
import os

from fastapi import FastAPI

from fraiseql.monitoring import HealthCheck, check_database, check_pool_stats

app = FastAPI()

# Different check sets for different purposes
liveness = HealthCheck()  # Minimal checks

readiness = HealthCheck()  # Critical dependencies
readiness.add_check("database", check_database)

comprehensive = HealthCheck()  # All dependencies
comprehensive.add_check("database", check_database)
comprehensive.add_check("pool", check_pool_stats)
# ... add custom checks


@app.get("/health")
async def health():
    """Comprehensive health check with version info."""
    result = await comprehensive.run_checks()

    # Add application metadata
    result["version"] = os.getenv("APP_VERSION", "unknown")
    result["environment"] = os.getenv("ENV", "development")

    return result


@app.get("/health/live")
async def live():
    """Liveness - minimal check."""
    return await liveness.run_checks()


@app.get("/health/ready")
async def ready(response: Response):
    """Readiness - critical dependencies."""
    result = await readiness.run_checks()

    if result["status"] != "healthy":
        response.status_code = 503

    return result
