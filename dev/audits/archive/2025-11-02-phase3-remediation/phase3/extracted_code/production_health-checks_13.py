# Extracted from: docs/production/health-checks.md
# Block number: 13
from fastapi import FastAPI, Response, status

from fraiseql.monitoring import HealthCheck, check_database

app = FastAPI()


# Liveness: Is the app running?
@app.get("/health/live")
async def liveness():
    """Liveness probe - always returns 200 if app is running."""
    return {"status": "alive"}


# Readiness: Can the app serve traffic?
readiness_checks = HealthCheck()
readiness_checks.add_check("database", check_database)


@app.get("/health/ready")
async def readiness(response: Response):
    """Readiness probe - returns 200 if dependencies are healthy."""
    result = await readiness_checks.run_checks()

    if result["status"] != "healthy":
        response.status_code = status.HTTP_503_SERVICE_UNAVAILABLE

    return result
