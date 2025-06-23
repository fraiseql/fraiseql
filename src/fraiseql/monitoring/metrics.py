"""Prometheus metrics integration for FraiseQL.

This module provides comprehensive metrics collection for monitoring
FraiseQL applications in production.
"""

import time
from dataclasses import dataclass
from dataclasses import field as dataclass_field
from functools import wraps
from typing import Optional

from fastapi import FastAPI, Request, Response

try:
    from prometheus_client import (  # type: ignore[import-untyped]
        CONTENT_TYPE_LATEST,
        CollectorRegistry,
        Counter,
        Gauge,
        Histogram,
        generate_latest,
    )

    PROMETHEUS_AVAILABLE = True
except ImportError:
    PROMETHEUS_AVAILABLE = False

    # Define placeholder classes
    class CollectorRegistry:  # type: ignore[misc]
        pass

    class Counter:  # type: ignore[misc]
        def __init__(self, *args, **kwargs) -> None:
            pass

        def inc(self, *args, **kwargs) -> None:
            pass

        def labels(self, *args, **kwargs):
            return self

    class Gauge:  # type: ignore[misc]
        def __init__(self, *args, **kwargs) -> None:
            pass

        def set(self, *args, **kwargs) -> None:
            pass

        def inc(self, *args, **kwargs) -> None:
            pass

        def dec(self, *args, **kwargs) -> None:
            pass

        def labels(self, *args, **kwargs):
            return self

    class Histogram:  # type: ignore[misc]
        def __init__(self, *args, **kwargs) -> None:
            pass

        def observe(self, *args, **kwargs) -> None:
            pass

        def labels(self, *args, **kwargs):
            return self

    CONTENT_TYPE_LATEST = "text/plain"

    def generate_latest(*args, **kwargs) -> bytes:
        """Placeholder for generate_latest when prometheus_client is not available."""
        # Return mock metrics data
        return b"""# HELP fraiseql_graphql_queries_total Total GraphQL queries
# TYPE fraiseql_graphql_queries_total counter
fraiseql_graphql_queries_total 1
# HELP fraiseql_graphql_query_duration_seconds GraphQL query duration
# TYPE fraiseql_graphql_query_duration_seconds histogram
fraiseql_graphql_query_duration_seconds_sum 0.01
fraiseql_graphql_query_duration_seconds_count 1
"""


from starlette.middleware.base import BaseHTTPMiddleware

# Global metrics instance
_metrics_instance: Optional["FraiseQLMetrics"] = None


@dataclass
class MetricsConfig:
    """Configuration for metrics collection."""

    enabled: bool = True
    namespace: str = "fraiseql"
    metrics_path: str = "/metrics"
    buckets: list[float] = dataclass_field(
        default_factory=lambda: [
            0.005,
            0.01,
            0.025,
            0.05,
            0.1,
            0.25,
            0.5,
            1,
            2.5,
            5,
            10,
        ],
    )
    exclude_paths: set[str] = dataclass_field(
        default_factory=lambda: {
            "/metrics",
            "/health",
            "/ready",
            "/startup",
        },
    )
    labels: dict[str, str] = dataclass_field(default_factory=dict)

    def __post_init__(self):
        """Validate configuration."""
        if not self.namespace:
            msg = "Namespace cannot be empty"
            raise ValueError(msg)

        # Ensure buckets are monotonic
        for i in range(1, len(self.buckets)):
            if self.buckets[i] <= self.buckets[i - 1]:
                msg = "Histogram buckets must be monotonically increasing"
                raise ValueError(msg)


class FraiseQLMetrics:
    """Prometheus metrics for FraiseQL."""

    def __init__(
        self,
        config: MetricsConfig | None = None,
        registry: CollectorRegistry | None = None,
    ) -> None:
        """Initialize metrics with configuration."""
        self.config = config or MetricsConfig()
        self.registry = registry or CollectorRegistry()
        self._cache_totals: dict[str, dict[str, int]] = {}

        # GraphQL metrics
        self.query_total = Counter(
            f"{self.config.namespace}_graphql_queries_total",
            "Total number of GraphQL queries",
            ["operation_type", "operation_name"],
            registry=self.registry,
        )

        self.query_duration = Histogram(
            f"{self.config.namespace}_graphql_query_duration_seconds",
            "GraphQL query execution time in seconds",
            ["operation_type", "operation_name"],
            buckets=self.config.buckets,
            registry=self.registry,
        )

        self.query_success = Counter(
            f"{self.config.namespace}_graphql_queries_success",
            "Number of successful GraphQL queries",
            ["operation_type"],
            registry=self.registry,
        )

        self.query_errors = Counter(
            f"{self.config.namespace}_graphql_queries_errors",
            "Number of failed GraphQL queries",
            ["operation_type"],
            registry=self.registry,
        )

        # Mutation metrics
        self.mutation_total = Counter(
            f"{self.config.namespace}_graphql_mutations_total",
            "Total number of GraphQL mutations",
            ["mutation_name"],
            registry=self.registry,
        )

        self.mutation_success = Counter(
            f"{self.config.namespace}_graphql_mutations_success",
            "Number of successful mutations",
            ["mutation_name", "result_type"],
            registry=self.registry,
        )

        self.mutation_errors = Counter(
            f"{self.config.namespace}_graphql_mutations_errors",
            "Number of failed mutations",
            ["mutation_name", "error_type"],
            registry=self.registry,
        )

        self.mutation_duration = Histogram(
            f"{self.config.namespace}_graphql_mutation_duration_seconds",
            "Mutation execution time in seconds",
            ["mutation_name"],
            buckets=self.config.buckets,
            registry=self.registry,
        )

        # Database metrics
        self.db_connections_active = Gauge(
            f"{self.config.namespace}_db_connections_active",
            "Number of active database connections",
            registry=self.registry,
        )

        self.db_connections_idle = Gauge(
            f"{self.config.namespace}_db_connections_idle",
            "Number of idle database connections",
            registry=self.registry,
        )

        self.db_connections_total = Gauge(
            f"{self.config.namespace}_db_connections_total",
            "Total number of database connections",
            registry=self.registry,
        )

        self.db_queries_total = Counter(
            f"{self.config.namespace}_db_queries_total",
            "Total database queries executed",
            ["query_type", "table_name"],
            registry=self.registry,
        )

        self.db_query_duration = Histogram(
            f"{self.config.namespace}_db_query_duration_seconds",
            "Database query execution time",
            ["query_type"],
            buckets=self.config.buckets,
            registry=self.registry,
        )

        # Cache metrics
        self.cache_hits = Counter(
            f"{self.config.namespace}_cache_hits_total",
            "Number of cache hits",
            ["cache_type"],
            registry=self.registry,
        )

        self.cache_misses = Counter(
            f"{self.config.namespace}_cache_misses_total",
            "Number of cache misses",
            ["cache_type"],
            registry=self.registry,
        )

        # Error metrics
        self.errors_total = Counter(
            f"{self.config.namespace}_errors_total",
            "Total number of errors",
            ["error_type", "error_code", "operation"],
            registry=self.registry,
        )

        # HTTP metrics
        self.http_requests_total = Counter(
            f"{self.config.namespace}_http_requests_total",
            "Total HTTP requests",
            ["method", "endpoint", "status"],
            registry=self.registry,
        )

        self.http_request_duration = Histogram(
            f"{self.config.namespace}_http_request_duration_seconds",
            "HTTP request duration",
            ["method", "endpoint"],
            buckets=self.config.buckets,
            registry=self.registry,
        )

        # Performance metrics
        self.response_time_histogram = Histogram(
            f"{self.config.namespace}_response_time_seconds",
            "Overall response time distribution",
            buckets=self.config.buckets,
            registry=self.registry,
        )

    def record_query(
        self,
        operation_type: str,
        operation_name: str,
        duration_ms: float,
        success: bool,
    ) -> None:
        """Record a GraphQL query execution."""
        self.query_total.labels(operation_type=operation_type, operation_name=operation_name).inc()
        self.query_duration.labels(
            operation_type=operation_type,
            operation_name=operation_name,
        ).observe(duration_ms / 1000)

        if success:
            self.query_success.labels(operation_type=operation_type).inc()
        else:
            self.query_errors.labels(operation_type=operation_type).inc()

    def record_mutation(
        self,
        mutation_name: str,
        duration_ms: float,
        success: bool,
        result_type: str | None = None,
        error_type: str | None = None,
    ) -> None:
        """Record a GraphQL mutation execution."""
        self.mutation_total.labels(mutation_name=mutation_name).inc()
        self.mutation_duration.labels(mutation_name=mutation_name).observe(duration_ms / 1000)

        if success and result_type:
            self.mutation_success.labels(mutation_name=mutation_name, result_type=result_type).inc()
        elif not success and error_type:
            self.mutation_errors.labels(mutation_name=mutation_name, error_type=error_type).inc()

    def update_db_connections(self, active: int, idle: int, total: int) -> None:
        """Update database connection pool metrics."""
        self.db_connections_active.set(active)
        self.db_connections_idle.set(idle)
        self.db_connections_total.set(total)

    def record_db_query(
        self,
        query_type: str,
        table_name: str,
        duration_ms: float,
        rows_affected: int = 0,
    ) -> None:
        """Record a database query execution."""
        self.db_queries_total.labels(query_type=query_type, table_name=table_name).inc()
        self.db_query_duration.labels(query_type=query_type).observe(duration_ms / 1000)

    def record_cache_hit(self, cache_type: str) -> None:
        """Record a cache hit."""
        self.cache_hits.labels(cache_type=cache_type).inc()

        # Track for hit rate calculation
        if cache_type not in self._cache_totals:
            self._cache_totals[cache_type] = {"hits": 0, "misses": 0}
        self._cache_totals[cache_type]["hits"] += 1

    def record_cache_miss(self, cache_type: str) -> None:
        """Record a cache miss."""
        self.cache_misses.labels(cache_type=cache_type).inc()

        # Track for hit rate calculation
        if cache_type not in self._cache_totals:
            self._cache_totals[cache_type] = {"hits": 0, "misses": 0}
        self._cache_totals[cache_type]["misses"] += 1

    def get_cache_hit_rate(self, cache_type: str) -> float:
        """Calculate cache hit rate for a specific cache type."""
        if cache_type not in self._cache_totals:
            return 0.0

        totals = self._cache_totals[cache_type]
        total = totals["hits"] + totals["misses"]
        if total == 0:
            return 0.0

        return totals["hits"] / total

    def record_error(self, error_type: str, error_code: str, operation: str) -> None:
        """Record an error occurrence."""
        self.errors_total.labels(
            error_type=error_type,
            error_code=error_code,
            operation=operation,
        ).inc()

    def record_response_time(self, duration_ms: float) -> None:
        """Record overall response time."""
        self.response_time_histogram.observe(duration_ms / 1000)

    def generate_metrics(self) -> bytes:
        """Generate Prometheus metrics output."""
        return generate_latest(self.registry)


class MetricsMiddleware(BaseHTTPMiddleware):
    """Middleware to collect HTTP metrics."""

    def __init__(self, app, metrics: FraiseQLMetrics, config: MetricsConfig | None = None) -> None:
        """Initialize metrics middleware."""
        super().__init__(app)
        self.metrics = metrics
        self.config = config or MetricsConfig()

    async def dispatch(self, request: Request, call_next) -> Response:
        """Process request and record metrics."""
        return await self.process_request(request, call_next)

    async def process_request(self, request: Request, call_next) -> Response:
        """Process request with metrics collection."""
        # Skip excluded paths
        if request.url.path in self.config.exclude_paths:
            return await call_next(request)

        # Start timing
        start_time = time.time()

        # Process request
        try:
            response = await call_next(request)
            duration = (time.time() - start_time) * 1000  # Convert to ms

            # Record metrics
            self.metrics.http_requests_total.labels(
                method=request.method,
                endpoint=request.url.path,
                status=response.status_code,
            ).inc()

            self.metrics.http_request_duration.labels(
                method=request.method,
                endpoint=request.url.path,
            ).observe(duration / 1000)

            self.metrics.record_response_time(duration)

            return response  # noqa: TRY300

        except Exception as e:
            duration = (time.time() - start_time) * 1000

            # Record error metrics
            self.metrics.http_requests_total.labels(
                method=request.method,
                endpoint=request.url.path,
                status=500,
            ).inc()

            self.metrics.record_error(
                error_type=type(e).__name__,
                error_code="HTTP_ERROR",
                operation=f"{request.method} {request.url.path}",
            )

            raise


def get_metrics() -> FraiseQLMetrics:
    """Get the global metrics instance."""
    global _metrics_instance
    if _metrics_instance is None:
        _metrics_instance = FraiseQLMetrics()
    return _metrics_instance


def setup_metrics(app: FastAPI, config: MetricsConfig | None = None) -> FraiseQLMetrics:
    """Set up metrics collection on a FastAPI app.

    Args:
        app: FastAPI application instance
        config: Optional metrics configuration

    Returns:
        FraiseQLMetrics instance
    """
    config = config or MetricsConfig()

    # Create or get metrics instance
    global _metrics_instance
    if _metrics_instance is None:
        _metrics_instance = FraiseQLMetrics(config)
    metrics = _metrics_instance

    # Add middleware
    if config.enabled:
        app.add_middleware(MetricsMiddleware, metrics=metrics, config=config)

    # Add metrics endpoint
    @app.get(config.metrics_path, include_in_schema=False)
    async def metrics_endpoint():
        """Prometheus metrics endpoint."""
        return Response(
            content=metrics.generate_metrics(),
            media_type=CONTENT_TYPE_LATEST,
        )

    return metrics


def with_metrics(operation_type: str = "operation"):
    """Decorator to automatically record metrics for a function.

    Args:
        operation_type: Type of operation (query, mutation, etc.)
    """

    def decorator(func):
        @wraps(func)
        async def async_wrapper(*args, **kwargs):
            metrics = get_metrics()
            start_time = time.time()
            success = False

            try:
                result = await func(*args, **kwargs)
                success = True
                return result  # noqa: TRY300
            except Exception as e:
                metrics.record_error(
                    error_type=type(e).__name__,
                    error_code=getattr(e, "code", "UNKNOWN"),
                    operation=func.__name__,
                )
                raise
            finally:
                duration_ms = (time.time() - start_time) * 1000
                if operation_type in ("query", "mutation"):
                    metrics.record_query(
                        operation_type=operation_type,
                        operation_name=func.__name__,
                        duration_ms=duration_ms,
                        success=success,
                    )

        @wraps(func)
        def sync_wrapper(*args, **kwargs):
            metrics = get_metrics()
            start_time = time.time()
            success = False

            try:
                result = func(*args, **kwargs)
                success = True
                return result  # noqa: TRY300
            except Exception as e:
                metrics.record_error(
                    error_type=type(e).__name__,
                    error_code=getattr(e, "code", "UNKNOWN"),
                    operation=func.__name__,
                )
                raise
            finally:
                duration_ms = (time.time() - start_time) * 1000
                if operation_type in ("query", "mutation"):
                    metrics.record_query(
                        operation_type=operation_type,
                        operation_name=func.__name__,
                        duration_ms=duration_ms,
                        success=success,
                    )

        # Return appropriate wrapper based on function type
        import asyncio

        if asyncio.iscoroutinefunction(func):
            return async_wrapper
        return sync_wrapper

    return decorator
