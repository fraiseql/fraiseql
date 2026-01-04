"""Health check system for FraiseQL monitoring.

Provides comprehensive health status across all system layers:
- Database: Query performance and connection pool
- Cache: Hit rates and eviction rates
- GraphQL: Operation success rates and latency
- Tracing: OpenTelemetry status

Example:
    >>> from fraiseql.health import HealthCheckAggregator, setup_health_endpoints
    >>> from fastapi import FastAPI
    >>>
    >>> # Setup FastAPI endpoints
    >>> app = FastAPI()
    >>> setup_health_endpoints(app)
    >>>
    >>> # Or use aggregator directly
    >>> aggregator = HealthCheckAggregator()
    >>> status = await aggregator.check_all()
    >>> print(status.overall_status)  # "healthy", "degraded", or "unhealthy"
"""

from fraiseql.health.endpoints import (
    create_router,
    setup_health_endpoints,
)
from fraiseql.health.health_check import (
    CacheHealthCheck,
    DatabaseHealthCheck,
    GraphQLHealthCheck,
    HealthCheckAggregator,
    HealthCheckResult,
    HealthStatus,
    TracingHealthCheck,
)

__all__ = [
    "CacheHealthCheck",
    "DatabaseHealthCheck",
    "GraphQLHealthCheck",
    "HealthCheckAggregator",
    "HealthCheckResult",
    "HealthStatus",
    "TracingHealthCheck",
    "create_router",
    "setup_health_endpoints",
]
