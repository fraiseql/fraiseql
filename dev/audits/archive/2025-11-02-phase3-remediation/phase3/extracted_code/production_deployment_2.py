# Extracted from: docs/production/deployment.md
# Block number: 2
from fraiseql.monitoring import HealthCheck
from fraiseql.monitoring.health_checks import check_database, check_pool_stats

# Create health check
health = HealthCheck()
health.add_check("database", check_database)
health.add_check("pool", check_pool_stats)

# FastAPI endpoints
from fastapi import FastAPI, Response

app = FastAPI()


@app.get("/health")
async def health_check():
    """Simple liveness check."""
    return {"status": "healthy", "service": "fraiseql"}


@app.get("/ready")
async def readiness_check():
    """Comprehensive readiness check."""
    result = await health.run_checks()

    if result["status"] == "healthy":
        return result
    return Response(content=json.dumps(result), status_code=503, media_type="application/json")
