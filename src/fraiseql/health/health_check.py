"""Health check system for FraiseQL monitoring.

Provides comprehensive health status across all system layers:
- Database: Query performance and connection pool
- Cache: Hit rates and eviction rates
- GraphQL: Operation success rates and latency
- Tracing: OpenTelemetry status

Example:
    >>> from fraiseql.health import HealthCheckAggregator
    >>> aggregator = HealthCheckAggregator()
    >>> status = await aggregator.check_all()
    >>> print(status.overall_status)  # "healthy", "degraded", or "unhealthy"
"""

from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import Any

# Import monitoring classes if available from Commits 3, 4, 4.5
try:
    from fraiseql.monitoring import DatabaseMonitor
except ImportError:
    DatabaseMonitor = None  # type: ignore[misc, assignment]

try:
    from fraiseql.monitoring import CacheMonitor
except ImportError:
    CacheMonitor = None  # type: ignore[misc, assignment]

try:
    from fraiseql.monitoring import OperationMonitor
except ImportError:
    OperationMonitor = None  # type: ignore[misc, assignment]


@dataclass
class HealthCheckResult:
    """Result of a single health check.

    Attributes:
        status: "healthy", "degraded", or "unhealthy"
        message: Human-readable status message
        response_time_ms: Time taken to perform check
        details: Detailed health information
        errors: Any errors encountered during check
        warnings: Warning messages
    """

    status: str  # healthy | degraded | unhealthy
    message: str
    response_time_ms: float
    details: dict[str, Any] = field(default_factory=dict)
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    def is_healthy(self) -> bool:
        """Check if result indicates healthy status."""
        return self.status == "healthy"

    def is_degraded(self) -> bool:
        """Check if result indicates degraded status."""
        return self.status == "degraded"

    def is_unhealthy(self) -> bool:
        """Check if result indicates unhealthy status."""
        return self.status == "unhealthy"


@dataclass
class HealthStatus:
    """Overall system health status.

    Aggregates health information from all system layers.

    Attributes:
        overall_status: "healthy", "degraded", or "unhealthy"
        timestamp: When health check was performed
        database: Database health information
        cache: Cache health information
        graphql: GraphQL health information
        tracing: Tracing health information
        checks_executed: Number of health checks run
        check_duration_ms: Time to run all checks
    """

    overall_status: str  # healthy | degraded | unhealthy
    timestamp: datetime
    database: dict[str, Any] = field(default_factory=dict)
    cache: dict[str, Any] = field(default_factory=dict)
    graphql: dict[str, Any] = field(default_factory=dict)
    tracing: dict[str, Any] = field(default_factory=dict)
    checks_executed: int = 0
    check_duration_ms: float = 0.0

    def is_healthy(self) -> bool:
        """Check if system is fully healthy."""
        return self.overall_status == "healthy"

    def is_degraded(self) -> bool:
        """Check if system is degraded but functional."""
        return self.overall_status == "degraded"

    def is_unhealthy(self) -> bool:
        """Check if system is unhealthy."""
        return self.overall_status == "unhealthy"

    def get_summary_string(self) -> str:
        """Get human-readable health summary."""
        return (
            f"FraiseQL Health: {self.overall_status.upper()}\n"
            f"  Database: {self.database.get('status', 'unknown')}\n"
            f"  Cache: {self.cache.get('status', 'unknown')}\n"
            f"  GraphQL: {self.graphql.get('status', 'unknown')}\n"
            f"  Tracing: {self.tracing.get('status', 'unknown')}\n"
            f"  Check Time: {self.check_duration_ms:.1f}ms"
        )


class DatabaseHealthCheck:
    """Health check for database layer.

    Evaluates database health using metrics from DatabaseMonitor:
    - Connection pool utilization
    - Slow query rate
    - Query error rate
    - Recent query performance
    """

    def __init__(
        self,
        monitor: Any | None = None,
        pool_utilization_threshold: float = 0.8,
        slow_query_threshold: float = 0.05,
        error_rate_threshold: float = 0.01,
    ):
        """Initialize database health check.

        Args:
            monitor: DatabaseMonitor instance (uses global if None)
            pool_utilization_threshold: Warn if pool > this (0.0-1.0)
            slow_query_threshold: Warn if slow queries > this rate
            error_rate_threshold: Critical if errors > this rate
        """
        self._monitor = monitor
        self._pool_utilization_threshold = pool_utilization_threshold
        self._slow_query_threshold = slow_query_threshold
        self._error_rate_threshold = error_rate_threshold

    async def check(self) -> HealthCheckResult:
        """Run database health check.

        Returns:
            HealthCheckResult with database health status
        """
        import time

        start = time.perf_counter()

        try:
            if self._monitor is None:
                # Try to get global monitor
                from fraiseql.monitoring import get_database_monitor

                self._monitor = get_database_monitor()

            # Get statistics
            stats = await self._monitor.get_query_statistics()
            pool = await self._monitor.get_pool_metrics()

            # Check metrics
            errors = []
            warnings = []

            # Pool utilization check
            pool_util = pool.get_utilization_percent() / 100.0
            if pool_util > 0.9:
                errors.append(f"Pool utilization critical: {pool_util:.1%}")
            elif pool_util > self._pool_utilization_threshold:
                warnings.append(f"Pool utilization high: {pool_util:.1%}")

            # Slow query rate check
            if stats.total_count > 0:
                slow_rate = stats.slow_count / stats.total_count
                if slow_rate > 0.05:
                    warnings.append(f"High slow query rate: {slow_rate:.1%}")

            # Error rate check
            if stats.total_count > 0:
                error_rate = stats.error_count / stats.total_count
                if error_rate > self._error_rate_threshold:
                    if error_rate > 0.05:
                        errors.append(f"High error rate: {error_rate:.1%}")
                    else:
                        warnings.append(f"Elevated error rate: {error_rate:.1%}")

            # Determine status
            if errors:
                status = "unhealthy"
                message = f"Database unhealthy: {errors[0]}"
            elif warnings:
                status = "degraded"
                message = f"Database degraded: {warnings[0]}"
            else:
                status = "healthy"
                message = "Database healthy"

            duration = (time.perf_counter() - start) * 1000

            return HealthCheckResult(
                status=status,
                message=message,
                response_time_ms=duration,
                details={
                    "pool_utilization": pool_util,
                    "pool_active": pool.active_connections,
                    "pool_total": pool.total_connections,
                    "total_queries": stats.total_count,
                    "slow_queries": stats.slow_count,
                    "error_count": stats.error_count,
                    "avg_duration_ms": stats.avg_duration_ms,
                    "p95_duration_ms": stats.p95_duration_ms,
                    "p99_duration_ms": stats.p99_duration_ms,
                },
                errors=errors,
                warnings=warnings,
            )

        except Exception as e:
            duration = (time.perf_counter() - start) * 1000
            return HealthCheckResult(
                status="unhealthy",
                message=f"Database check failed: {e!s}",
                response_time_ms=duration,
                errors=[str(e)],
            )


class CacheHealthCheck:
    """Health check for cache layer.

    Evaluates cache health using metrics from CacheMonitor:
    - Cache hit rate
    - Eviction rate
    - Operation success rate
    """

    def __init__(
        self,
        monitor: Any | None = None,
        hit_rate_threshold: float = 0.6,
        eviction_threshold: float = 0.3,
    ):
        """Initialize cache health check.

        Args:
            monitor: CacheMonitor instance (uses global if None)
            hit_rate_threshold: Warn if hit rate below this
            eviction_threshold: Warn if eviction rate above this
        """
        self._monitor = monitor
        self._hit_rate_threshold = hit_rate_threshold
        self._eviction_threshold = eviction_threshold

    async def check(self) -> HealthCheckResult:
        """Run cache health check.

        Returns:
            HealthCheckResult with cache health status
        """
        import time

        start = time.perf_counter()

        try:
            if self._monitor is None:
                # Try to get global monitor
                from fraiseql.monitoring import get_cache_monitor

                self._monitor = get_cache_monitor()

            # Get metrics
            hit_rate = await self._monitor.get_hit_rate()
            eviction_count = await self._monitor.get_eviction_count()
            operation_count = await self._monitor.get_operation_count()

            errors = []
            warnings = []

            # Hit rate check
            if hit_rate < 0.5:
                errors.append(f"Low cache hit rate: {hit_rate:.1%}")
            elif hit_rate < self._hit_rate_threshold:
                warnings.append(f"Degraded hit rate: {hit_rate:.1%}")

            # Eviction rate check
            if operation_count > 0:
                eviction_rate = eviction_count / operation_count
                if eviction_rate > self._eviction_threshold:
                    warnings.append(f"High eviction rate: {eviction_rate:.1%}")

            # Determine status
            if errors:
                status = "unhealthy"
                message = f"Cache unhealthy: {errors[0]}"
            elif warnings:
                status = "degraded"
                message = f"Cache degraded: {warnings[0]}"
            else:
                status = "healthy"
                message = "Cache healthy"

            duration = (time.perf_counter() - start) * 1000

            return HealthCheckResult(
                status=status,
                message=message,
                response_time_ms=duration,
                details={
                    "hit_rate": hit_rate,
                    "eviction_rate": (
                        eviction_count / operation_count if operation_count > 0 else 0.0
                    ),
                    "eviction_count": eviction_count,
                    "operation_count": operation_count,
                },
                errors=errors,
                warnings=warnings,
            )

        except Exception as e:
            duration = (time.perf_counter() - start) * 1000
            return HealthCheckResult(
                status="unhealthy",
                message=f"Cache check failed: {e!s}",
                response_time_ms=duration,
                errors=[str(e)],
            )


class GraphQLHealthCheck:
    """Health check for GraphQL layer.

    Evaluates GraphQL operation health:
    - Operation success rate
    - Query/mutation error rates
    - Operation latency
    """

    def __init__(
        self,
        monitor: Any | None = None,
        success_rate_threshold: float = 0.95,
        error_rate_threshold: float = 0.01,
    ):
        """Initialize GraphQL health check.

        Args:
            monitor: OperationMonitor instance (uses global if None)
            success_rate_threshold: Warn if success rate below this
            error_rate_threshold: Critical if error rate above this
        """
        self._monitor = monitor
        self._success_rate_threshold = success_rate_threshold
        self._error_rate_threshold = error_rate_threshold

    async def check(self) -> HealthCheckResult:
        """Run GraphQL health check.

        Returns:
            HealthCheckResult with GraphQL health status
        """
        import time

        start = time.perf_counter()

        try:
            if self._monitor is None:
                # Try to get global monitor
                from fraiseql.monitoring import get_operation_monitor

                self._monitor = get_operation_monitor()

            # Get statistics
            stats = await self._monitor.get_statistics()

            errors = []
            warnings = []

            # Success rate check
            if stats.total_operations > 0:
                success_rate = stats.successful_operations / stats.total_operations
                if success_rate < 0.9:
                    errors.append(f"Low operation success rate: {success_rate:.1%}")
                elif success_rate < self._success_rate_threshold:
                    warnings.append(f"Degraded success rate: {success_rate:.1%}")

            # Error rate check
            if stats.total_operations > 0:
                error_rate = stats.failed_operations / stats.total_operations
                if error_rate > 0.05:
                    errors.append(f"High error rate: {error_rate:.1%}")
                elif error_rate > self._error_rate_threshold:
                    warnings.append(f"Elevated error rate: {error_rate:.1%}")

            # Determine status
            if errors:
                status = "unhealthy"
                message = f"GraphQL unhealthy: {errors[0]}"
            elif warnings:
                status = "degraded"
                message = f"GraphQL degraded: {warnings[0]}"
            else:
                status = "healthy"
                message = "GraphQL operations healthy"

            duration = (time.perf_counter() - start) * 1000

            success_rate = (
                stats.successful_operations / stats.total_operations
                if stats.total_operations > 0
                else 0.0
            )
            error_rate = (
                stats.failed_operations / stats.total_operations
                if stats.total_operations > 0
                else 0.0
            )

            return HealthCheckResult(
                status=status,
                message=message,
                response_time_ms=duration,
                details={
                    "total_operations": stats.total_operations,
                    "successful_operations": stats.successful_operations,
                    "failed_operations": stats.failed_operations,
                    "success_rate": success_rate,
                    "error_rate": error_rate,
                    "avg_duration_ms": stats.avg_duration_ms,
                    "p95_duration_ms": stats.p95_duration_ms,
                },
                errors=errors,
                warnings=warnings,
            )

        except Exception as e:
            duration = (time.perf_counter() - start) * 1000
            return HealthCheckResult(
                status="unhealthy",
                message=f"GraphQL check failed: {e!s}",
                response_time_ms=duration,
                errors=[str(e)],
            )


class TracingHealthCheck:
    """Health check for distributed tracing.

    Evaluates OpenTelemetry/tracing health:
    - Trace context support
    - Span creation success
    - Telemetry provider status
    """

    def __init__(self):
        """Initialize tracing health check."""

    async def check(self) -> HealthCheckResult:
        """Run tracing health check.

        Returns:
            HealthCheckResult with tracing health status
        """
        import time

        start = time.perf_counter()

        try:
            # Check if tracing is initialized
            try:
                from fraiseql.observability import get_tracer

                tracer = get_tracer()
                if tracer is None:
                    return HealthCheckResult(
                        status="degraded",
                        message="Tracing not initialized",
                        response_time_ms=(time.perf_counter() - start) * 1000,
                        warnings=["Tracing provider not configured"],
                    )

                # Try to create a test span
                with tracer.start_as_current_span("health_check"):
                    pass

                duration = (time.perf_counter() - start) * 1000

                return HealthCheckResult(
                    status="healthy",
                    message="Tracing healthy",
                    response_time_ms=duration,
                    details={"provider": "opentelemetry"},
                )

            except ImportError:
                # Tracing module not available
                duration = (time.perf_counter() - start) * 1000
                return HealthCheckResult(
                    status="degraded",
                    message="Tracing not available",
                    response_time_ms=duration,
                    warnings=["Observability module not available"],
                )

        except Exception as e:
            duration = (time.perf_counter() - start) * 1000
            return HealthCheckResult(
                status="unhealthy",
                message=f"Tracing check failed: {e!s}",
                response_time_ms=duration,
                errors=[str(e)],
            )


class HealthCheckAggregator:
    """Aggregates all health checks into unified status.

    Runs all health checks and determines overall system status:
    - healthy: All checks pass
    - degraded: Some checks report warnings
    - unhealthy: Any critical check fails
    """

    def __init__(
        self,
        database_check: Any | None = None,
        cache_check: Any | None = None,
        graphql_check: Any | None = None,
        tracing_check: Any | None = None,
    ):
        """Initialize aggregator with health checks.

        Args:
            database_check: Database health check instance
            cache_check: Cache health check instance
            graphql_check: GraphQL health check instance
            tracing_check: Tracing health check instance
        """
        self._database_check = database_check or DatabaseHealthCheck()
        self._cache_check = cache_check or CacheHealthCheck()
        self._graphql_check = graphql_check or GraphQLHealthCheck()
        self._tracing_check = tracing_check or TracingHealthCheck()

    async def check_all(self) -> HealthStatus:
        """Run all health checks and aggregate results.

        Returns:
            HealthStatus with overall system health
        """
        import time

        start = time.perf_counter()

        # Run all checks
        db_result = await self._database_check.check()
        cache_result = await self._cache_check.check()
        graphql_result = await self._graphql_check.check()
        tracing_result = await self._tracing_check.check()

        # Determine overall status
        all_results = [db_result, cache_result, graphql_result, tracing_result]

        # If any is unhealthy, overall is unhealthy
        if any(r.is_unhealthy() for r in all_results):
            overall_status = "unhealthy"
        # If any is degraded, overall is degraded
        elif any(r.is_degraded() for r in all_results):
            overall_status = "degraded"
        # Otherwise healthy
        else:
            overall_status = "healthy"

        duration = (time.perf_counter() - start) * 1000

        return HealthStatus(
            overall_status=overall_status,
            timestamp=datetime.now(UTC),
            database={
                "status": db_result.status,
                "message": db_result.message,
                **db_result.details,
            },
            cache={
                "status": cache_result.status,
                "message": cache_result.message,
                **cache_result.details,
            },
            graphql={
                "status": graphql_result.status,
                "message": graphql_result.message,
                **graphql_result.details,
            },
            tracing={
                "status": tracing_result.status,
                "message": tracing_result.message,
                **tracing_result.details,
            },
            checks_executed=4,
            check_duration_ms=duration,
        )

    async def check_database(self) -> HealthCheckResult:
        """Run only database health check."""
        return await self._database_check.check()

    async def check_cache(self) -> HealthCheckResult:
        """Run only cache health check."""
        return await self._cache_check.check()

    async def check_graphql(self) -> HealthCheckResult:
        """Run only GraphQL health check."""
        return await self._graphql_check.check()

    async def check_tracing(self) -> HealthCheckResult:
        """Run only tracing health check."""
        return await self._tracing_check.check()
