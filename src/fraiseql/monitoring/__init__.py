"""FraiseQL monitoring module.

Provides utilities for application monitoring including:
- Prometheus metrics integration
- Health check patterns
- Pre-built health checks for common services
- OpenTelemetry tracing

Example:
    >>> from fraiseql.monitoring import HealthCheck, check_database, check_pool_stats
    >>> from fraiseql.monitoring import setup_metrics, MetricsConfig
    >>>
    >>> # Set up metrics
    >>> setup_metrics(MetricsConfig(enabled=True))
    >>>
    >>> # Create health checks with pre-built functions
    >>> health = HealthCheck()
    >>> health.add_check("database", check_database)
    >>> health.add_check("pool", check_pool_stats)
    >>>
    >>> # Run checks
    >>> result = await health.run_checks()
"""

from .health import (
    CheckFunction,
    CheckResult,
    HealthCheck,
    HealthStatus,
)
from .health_checks import (
    check_database,
    check_pool_stats,
)
from .metrics import (
    FraiseQLMetrics,
    MetricsConfig,
    MetricsMiddleware,
    get_metrics,
    setup_metrics,
    with_metrics,
)

__all__ = [
    "CheckFunction",
    "CheckResult",
    "FraiseQLMetrics",
    "HealthCheck",
    "HealthStatus",
    "MetricsConfig",
    "MetricsMiddleware",
    "check_database",
    "check_pool_stats",
    "get_metrics",
    "setup_metrics",
    "with_metrics",
]
