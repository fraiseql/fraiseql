"""FastAPI endpoints for health checks.

Provides Kubernetes-compatible health check endpoints:
- /health/live - Liveness probe (always fast)
- /health/ready - Readiness probe (full checks)
- /health - Full health status
- /health/{layer} - Layer-specific health checks

Example:
    >>> from fastapi import FastAPI
    >>> from fraiseql.health import setup_health_endpoints
    >>>
    >>> app = FastAPI()
    >>> setup_health_endpoints(app)
    >>>
    >>> # Now endpoints are available at:
    >>> # GET /health/live - Returns instantly
    >>> # GET /health/ready - Returns health status
    >>> # GET /health - Full system health
"""

from typing import Any

from fastapi import APIRouter, Response

from fraiseql.health.health_check import (
    HealthCheckAggregator,
)


def create_router() -> APIRouter:
    """Create health check router.

    Returns:
        FastAPI Router with health check endpoints
    """
    router = APIRouter(prefix="/health", tags=["health"])

    # Create aggregator (shared instance)
    aggregator = HealthCheckAggregator()

    # ===== Kubernetes Probes =====

    @router.get("/live")
    async def liveness_probe() -> dict[str, str]:
        """Kubernetes liveness probe.

        Returns 200 OK immediately if service is running.
        Minimal checks - just verifies service startup.

        Used by:
        - Kubernetes: livenessProbe to restart unhealthy pods
        - Docker: Health checks for container

        Returns:
            Simple status response
        """
        return {"status": "alive"}

    @router.get("/ready")
    async def readiness_probe(response: Response) -> dict[str, Any]:
        """Kubernetes readiness probe.

        Returns 200 OK only if service is ready to handle requests.
        Runs full health checks to verify all systems operational.

        Used by:
        - Kubernetes: readinessProbe for load balancer
        - Docker Compose: Service dependencies

        Returns:
            Health status with HTTP status code
        """
        status = await aggregator.check_all()

        # Set HTTP status based on health
        if status.is_unhealthy():
            response.status_code = 503
        elif status.is_degraded():
            response.status_code = 200  # Still ready, just degraded

        return {
            "ready": status.is_healthy() or status.is_degraded(),
            "status": status.overall_status,
            "timestamp": status.timestamp.isoformat(),
            "checks": status.checks_executed,
            "duration_ms": status.check_duration_ms,
        }

    # ===== Full Health Check =====

    @router.get("")
    async def full_health_check(response: Response) -> dict[str, Any]:
        """Full system health check.

        Returns comprehensive health status for all system layers:
        - Database (query performance, connection pool)
        - Cache (hit rates, evictions)
        - GraphQL (operation success, latency)
        - Tracing (OpenTelemetry status)

        Returns:
            Complete health status across all layers
        """
        status = await aggregator.check_all()

        # Set HTTP status based on health
        if status.is_unhealthy():
            response.status_code = 503
        elif status.is_degraded():
            response.status_code = 200

        return {
            "status": status.overall_status,
            "timestamp": status.timestamp.isoformat(),
            "checks_executed": status.checks_executed,
            "check_duration_ms": status.check_duration_ms,
            "database": status.database,
            "cache": status.cache,
            "graphql": status.graphql,
            "tracing": status.tracing,
        }

    # ===== Layer-Specific Checks =====

    @router.get("/database")
    async def database_health_check(response: Response) -> dict[str, Any]:
        """Database health check only.

        Evaluates:
        - Connection pool utilization
        - Slow query rate
        - Query error rate
        - Recent performance metrics

        Returns:
            Database health status
        """
        result = await aggregator.check_database()

        if result.is_unhealthy():
            response.status_code = 503
        elif result.is_degraded():
            response.status_code = 200

        return {
            "status": result.status,
            "message": result.message,
            "response_time_ms": result.response_time_ms,
            "details": result.details,
            "warnings": result.warnings,
            "errors": result.errors,
        }

    @router.get("/cache")
    async def cache_health_check(response: Response) -> dict[str, Any]:
        """Cache health check only.

        Evaluates:
        - Cache hit rate
        - Eviction rate
        - Operation success rate

        Returns:
            Cache health status
        """
        result = await aggregator.check_cache()

        if result.is_unhealthy():
            response.status_code = 503
        elif result.is_degraded():
            response.status_code = 200

        return {
            "status": result.status,
            "message": result.message,
            "response_time_ms": result.response_time_ms,
            "details": result.details,
            "warnings": result.warnings,
            "errors": result.errors,
        }

    @router.get("/graphql")
    async def graphql_health_check(response: Response) -> dict[str, Any]:
        """GraphQL operation health check only.

        Evaluates:
        - Operation success rate
        - Query error rate
        - Mutation error rate
        - Operation latency

        Returns:
            GraphQL health status
        """
        result = await aggregator.check_graphql()

        if result.is_unhealthy():
            response.status_code = 503
        elif result.is_degraded():
            response.status_code = 200

        return {
            "status": result.status,
            "message": result.message,
            "response_time_ms": result.response_time_ms,
            "details": result.details,
            "warnings": result.warnings,
            "errors": result.errors,
        }

    @router.get("/tracing")
    async def tracing_health_check(response: Response) -> dict[str, Any]:
        """Tracing/OpenTelemetry health check only.

        Evaluates:
        - Trace context propagation
        - Span creation success
        - Telemetry provider status

        Returns:
            Tracing health status
        """
        result = await aggregator.check_tracing()

        if result.is_unhealthy():
            response.status_code = 503
        elif result.is_degraded():
            response.status_code = 200

        return {
            "status": result.status,
            "message": result.message,
            "response_time_ms": result.response_time_ms,
            "details": result.details,
            "warnings": result.warnings,
            "errors": result.errors,
        }

    return router


def setup_health_endpoints(app: Any) -> None:
    """Setup health check endpoints in FastAPI app.

    Args:
        app: FastAPI application instance

    Example:
        >>> from fastapi import FastAPI
        >>> from fraiseql.health import setup_health_endpoints
        >>>
        >>> app = FastAPI()
        >>> setup_health_endpoints(app)
    """
    router = create_router()
    app.include_router(router)


__all__ = ["create_router", "setup_health_endpoints"]
