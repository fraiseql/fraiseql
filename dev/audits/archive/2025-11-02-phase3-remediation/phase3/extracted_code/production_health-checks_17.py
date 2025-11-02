# Extracted from: docs/production/health-checks.md
# Block number: 17
import time

from fastapi import FastAPI

from fraiseql.monitoring import HealthCheck, check_database

app = FastAPI()
health = HealthCheck()
health.add_check("database", check_database)

# Cache for high-frequency health checks
_health_cache = {"result": None, "timestamp": 0}
CACHE_TTL = 5  # seconds


@app.get("/health")
async def cached_health():
    """Health check with caching to reduce database load."""
    now = time.time()

    # Return cached result if fresh
    if _health_cache["result"] and (now - _health_cache["timestamp"]) < CACHE_TTL:
        return _health_cache["result"]

    # Run checks
    result = await health.run_checks()

    # Update cache
    _health_cache["result"] = result
    _health_cache["timestamp"] = now

    return result
