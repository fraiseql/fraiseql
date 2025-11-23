"""Example: Using HealthCheck utility in a FraiseQL application.

This example demonstrates how to create comprehensive health checks
for your application, following production best practices.

Based on the pattern from printoptim_backend but using FraiseQL's
built-in HealthCheck utility for better composability.
"""

from fastapi import APIRouter, FastAPI

from fraiseql.monitoring import (
    CheckResult,
    HealthCheck,
    HealthStatus,
    check_database,
    check_pool_stats,
)

# Create router for health endpoints
router = APIRouter(tags=["Health"])

# Initialize health check instance (singleton pattern)
health = HealthCheck()


# Register pre-built checks
health.add_check("database", check_database)
health.add_check("database_pool", check_pool_stats)


# Add custom application-specific checks
async def check_redis() -> CheckResult:
    """Example: Custom Redis connectivity check."""
    try:
        # Your Redis connection logic
        # redis_client = get_redis_client()
        # await redis_client.ping()

        # Simulated for example
        return CheckResult(
            name="redis",
            status=HealthStatus.HEALTHY,
            message="Redis connection successful",
            metadata={"version": "7.2"},
        )
    except Exception as e:
        return CheckResult(
            name="redis",
            status=HealthStatus.UNHEALTHY,
            message=f"Redis connection failed: {e!s}",
        )


async def check_external_api() -> CheckResult:
    """Example: Custom external API health check."""
    try:
        # Your external API check logic
        # response = await http_client.get("https://api.example.com/health")
        # response.raise_for_status()

        # Simulated for example
        return CheckResult(
            name="external_api",
            status=HealthStatus.HEALTHY,
            message="External API reachable",
        )
    except Exception as e:
        return CheckResult(
            name="external_api",
            status=HealthStatus.UNHEALTHY,
            message=f"External API unreachable: {e!s}",
        )


# Register custom checks (optional - only if you need them)
# health.add_check("redis", check_redis)
# health.add_check("external_api", check_external_api)


@router.get("/health")
async def health_endpoint():
    """Comprehensive health check endpoint.

    This endpoint provides detailed system information essential for:
    - Load balancer health checks (sub-100ms response times)
    - CI/CD pipeline deployment verification
    - Production monitoring with comprehensive system metrics
    - Kubernetes readiness/liveness probes

    Returns:
        Dictionary with overall status and individual check results:
        {
            "status": "healthy" | "degraded",
            "service": "my-service",
            "checks": {
                "database": {"status": "healthy", "message": "...", ...},
                "database_pool": {"status": "healthy", "message": "...", ...}
            }
        }
    """
    result = await health.run_checks()

    # Add service metadata
    result["service"] = "fraiseql-example"

    return result


@router.get("/health/simple")
async def simple_health_endpoint():
    """Simple health check for basic monitoring.

    Returns minimal health status for load balancers and basic monitors.
    This is a lightweight endpoint that doesn't check dependencies.
    """
    return {
        "status": "healthy",
        "service": "fraiseql-example",
    }


# For Kubernetes deployments
@router.get("/ready")
async def readiness_endpoint():
    """Kubernetes readiness probe endpoint.

    Checks if the application can serve traffic.
    Returns 503 if any dependency is unhealthy.
    """
    result = await health.run_checks()

    # Return 503 if degraded (some checks failing)
    if result["status"] == "degraded":
        from fastapi import status
        from fastapi.responses import JSONResponse

        return JSONResponse(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            content=result,
        )

    return result


@router.get("/health/live")
async def liveness_endpoint():
    """Kubernetes liveness probe endpoint.

    Checks if the application is alive (not checking dependencies).
    Should return 200 unless the application process is dead.
    """
    return {"status": "ok"}


# Example: Create FastAPI app and include router
def create_app() -> FastAPI:
    """Create FastAPI application with health checks."""
    app = FastAPI(title="FraiseQL Health Check Example")

    # Include health check router
    app.include_router(router)

    return app


# Example usage in main
if __name__ == "__main__":
    import uvicorn

    app = create_app()
    uvicorn.run(app, host="0.0.0.0", port=8000)

"""
Example responses:

1. All healthy:
GET /health
{
    "status": "healthy",
    "service": "fraiseql-example",
    "checks": {
        "database": {
            "status": "healthy",
            "message": "Database connection successful (PostgreSQL 16.3)",
            "metadata": {
                "database_version": "16.3",
                "full_version": "PostgreSQL 16.3 on x86_64-pc-linux-gnu"
            }
        },
        "database_pool": {
            "status": "healthy",
            "message": "Pool healthy (50.0% utilized - 10/20 active)",
            "metadata": {
                "pool_size": 10,
                "active_connections": 10,
                "idle_connections": 0,
                "max_connections": 20,
                "min_connections": 5,
                "usage_percentage": 50.0
            }
        }
    }
}

2. Database down:
GET /health
{
    "status": "degraded",
    "service": "fraiseql-example",
    "checks": {
        "database": {
            "status": "unhealthy",
            "message": "Database connection failed: Connection refused"
        },
        "database_pool": {
            "status": "unhealthy",
            "message": "Database connection pool not available"
        }
    }
}

3. Kubernetes readiness check (database down):
GET /ready
HTTP/1.1 503 Service Unavailable
{
    "status": "degraded",
    "checks": {...}
}
"""
