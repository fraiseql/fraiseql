# Extracted from: docs/production/health-checks.md
# Block number: 1
from fastapi import FastAPI

from fraiseql.monitoring import HealthCheck, check_database, check_pool_stats

app = FastAPI()

# Create health check instance
health = HealthCheck()

# Register pre-built checks
health.add_check("database", check_database)
health.add_check("pool", check_pool_stats)


@app.get("/health")
async def health_endpoint():
    """Health check endpoint for monitoring and orchestration."""
    return await health.run_checks()
