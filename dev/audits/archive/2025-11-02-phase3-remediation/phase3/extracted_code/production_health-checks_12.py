# Extracted from: docs/production/health-checks.md
# Block number: 12
from fastapi import FastAPI

from fraiseql.monitoring import HealthCheck, check_database, check_pool_stats

app = FastAPI()
health = HealthCheck()

# Register checks
health.add_check("database", check_database)
health.add_check("pool", check_pool_stats)


@app.get("/health")
async def health_check():
    """Kubernetes/orchestrator health check endpoint."""
    return await health.run_checks()
