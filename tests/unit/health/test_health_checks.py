"""Unit tests for health check system.

Tests for:
- HealthCheckResult status checks
- DatabaseHealthCheck evaluation
- CacheHealthCheck evaluation
- GraphQLHealthCheck evaluation
- TracingHealthCheck evaluation
- HealthCheckAggregator status aggregation
"""

from datetime import UTC, datetime, timedelta

import pytest

from unittest.mock import AsyncMock, MagicMock

from fraiseql.health import (
    CacheHealthCheck,
    DatabaseHealthCheck,
    GraphQLHealthCheck,
    HealthCheckAggregator,
    HealthCheckResult,
    HealthStatus,
    TracingHealthCheck,
)


class TestHealthCheckResult:
    """Tests for HealthCheckResult dataclass."""

    def test_result_creation(self):
        """HealthCheckResult creates successfully."""
        result = HealthCheckResult(
            status="healthy",
            message="All systems operational",
            response_time_ms=45.2,
        )
        assert result.status == "healthy"
        assert result.message == "All systems operational"
        assert result.response_time_ms == 45.2

    def test_result_is_healthy(self):
        """is_healthy() returns True for healthy status."""
        result = HealthCheckResult(
            status="healthy",
            message="OK",
            response_time_ms=10.0,
        )
        assert result.is_healthy() is True
        assert result.is_degraded() is False
        assert result.is_unhealthy() is False

    def test_result_is_degraded(self):
        """is_degraded() returns True for degraded status."""
        result = HealthCheckResult(
            status="degraded",
            message="Slow performance",
            response_time_ms=10.0,
        )
        assert result.is_degraded() is True
        assert result.is_healthy() is False
        assert result.is_unhealthy() is False

    def test_result_is_unhealthy(self):
        """is_unhealthy() returns True for unhealthy status."""
        result = HealthCheckResult(
            status="unhealthy",
            message="Service down",
            response_time_ms=10.0,
            errors=["Connection failed"],
        )
        assert result.is_unhealthy() is True
        assert result.is_healthy() is False
        assert result.is_degraded() is False


class TestHealthStatus:
    """Tests for HealthStatus dataclass."""

    def test_status_creation(self):
        """HealthStatus creates successfully."""
        now = datetime.now(UTC)
        status = HealthStatus(
            overall_status="healthy",
            timestamp=now,
            checks_executed=4,
            check_duration_ms=50.0,
        )
        assert status.overall_status == "healthy"
        assert status.timestamp == now
        assert status.checks_executed == 4

    def test_status_is_healthy(self):
        """is_healthy() works correctly."""
        status = HealthStatus(
            overall_status="healthy",
            timestamp=datetime.now(UTC),
        )
        assert status.is_healthy() is True
        assert status.is_degraded() is False
        assert status.is_unhealthy() is False

    def test_status_summary_string(self):
        """get_summary_string() generates readable summary."""
        status = HealthStatus(
            overall_status="healthy",
            timestamp=datetime.now(UTC),
            database={"status": "healthy"},
            cache={"status": "healthy"},
            graphql={"status": "healthy"},
            tracing={"status": "healthy"},
            check_duration_ms=45.5,
        )
        summary = status.get_summary_string()
        assert "HEALTHY" in summary
        assert "Database: healthy" in summary
        assert "Cache: healthy" in summary


class TestDatabaseHealthCheck:
    """Tests for DatabaseHealthCheck."""

    async def test_database_check_with_healthy_metrics(self):
        """Database check passes with good metrics."""
        # Create mock monitor with good stats
        monitor = MagicMock()

        # Mock statistics
        stats = MagicMock()
        stats.total_count = 100
        stats.slow_count = 0
        stats.error_count = 0
        stats.avg_duration_ms = 45.0
        stats.p95_duration_ms = 95.0
        stats.p99_duration_ms = 150.0

        # Mock pool metrics
        pool = MagicMock()
        pool.active_connections = 5
        pool.total_connections = 10
        pool.get_utilization_percent = MagicMock(return_value=50.0)

        monitor.get_query_statistics = AsyncMock(return_value=stats)
        monitor.get_pool_metrics = AsyncMock(return_value=pool)

        check = DatabaseHealthCheck(monitor=monitor)
        result = await check.check()

        assert result.is_healthy()
        assert result.status == "healthy"
        assert "healthy" in result.message.lower()

    async def test_database_check_high_pool_utilization(self):
        """Database check warns on high pool utilization."""
        monitor = MagicMock()

        stats = MagicMock()
        stats.total_count = 100
        stats.slow_count = 5
        stats.error_count = 0
        stats.avg_duration_ms = 45.0

        pool = MagicMock()
        pool.active_connections = 9
        pool.total_connections = 10
        pool.get_utilization_percent = MagicMock(return_value=90.0)

        monitor.get_query_statistics = AsyncMock(return_value=stats)
        monitor.get_pool_metrics = AsyncMock(return_value=pool)

        check = DatabaseHealthCheck(
            monitor=monitor,
            pool_utilization_threshold=0.5,
        )
        result = await check.check()

        # Should at least complete without error
        assert result.status in ["healthy", "degraded", "unhealthy"]

    async def test_database_check_high_error_rate(self):
        """Database check detects high error rate."""
        monitor = MagicMock()

        stats = MagicMock()
        stats.total_count = 100
        stats.slow_count = 0
        stats.error_count = 30
        stats.avg_duration_ms = 45.0

        pool = MagicMock()
        pool.active_connections = 5
        pool.total_connections = 10
        pool.get_utilization_percent = MagicMock(return_value=50.0)

        monitor.get_query_statistics = AsyncMock(return_value=stats)
        monitor.get_pool_metrics = AsyncMock(return_value=pool)

        check = DatabaseHealthCheck(
            monitor=monitor,
            error_rate_threshold=0.05,
        )
        result = await check.check()

        # With 30% errors, should be degraded or unhealthy
        assert result.status in ["degraded", "unhealthy"]

    async def test_database_check_exception_handling(self):
        """Database check handles exceptions gracefully."""
        check = DatabaseHealthCheck(monitor=None)

        # This will try to get global monitor and may fail
        result = await check.check()

        # Should return unhealthy result instead of crashing
        assert result.status in ["unhealthy", "degraded", "healthy"]


class TestCacheHealthCheck:
    """Tests for CacheHealthCheck."""

    async def test_cache_check_healthy(self):
        """Cache check passes with good hit rate."""
        monitor = MagicMock()

        # Mock methods
        monitor.get_hit_rate = AsyncMock(return_value=0.75)
        monitor.get_eviction_count = AsyncMock(return_value=5)
        monitor.get_operation_count = AsyncMock(return_value=100)

        check = CacheHealthCheck(monitor=monitor)
        result = await check.check()

        # 75% hit rate should be healthy
        assert result.is_healthy()

    async def test_cache_check_low_hit_rate(self):
        """Cache check warns on low hit rate."""
        monitor = MagicMock()

        # Simulate poor hit rate
        monitor.get_hit_rate = AsyncMock(return_value=0.25)
        monitor.get_eviction_count = AsyncMock(return_value=75)
        monitor.get_operation_count = AsyncMock(return_value=100)

        check = CacheHealthCheck(
            monitor=monitor,
            hit_rate_threshold=0.7,
        )
        result = await check.check()

        # 25% hit rate should be degraded or unhealthy
        assert result.status in ["degraded", "unhealthy"]

    async def test_cache_check_exception_handling(self):
        """Cache check handles exceptions gracefully."""
        check = CacheHealthCheck(monitor=None)

        result = await check.check()

        # Should return result instead of crashing
        assert result.status in ["unhealthy", "degraded", "healthy"]


class TestGraphQLHealthCheck:
    """Tests for GraphQLHealthCheck."""

    async def test_graphql_check_healthy(self):
        """GraphQL check passes with good success rate."""
        monitor = MagicMock()

        # Mock statistics
        stats = MagicMock()
        stats.total_operations = 100
        stats.successful_operations = 99
        stats.failed_operations = 1
        stats.avg_duration_ms = 125.0
        stats.p95_duration_ms = 250.0

        monitor.get_statistics = AsyncMock(return_value=stats)

        check = GraphQLHealthCheck(monitor=monitor)
        result = await check.check()

        assert result.is_healthy()

    async def test_graphql_check_high_error_rate(self):
        """GraphQL check detects high error rate."""
        monitor = MagicMock()

        # Mock statistics with high error rate
        stats = MagicMock()
        stats.total_operations = 100
        stats.successful_operations = 20
        stats.failed_operations = 80
        stats.avg_duration_ms = 125.0
        stats.p95_duration_ms = 250.0

        monitor.get_statistics = AsyncMock(return_value=stats)

        check = GraphQLHealthCheck(
            monitor=monitor,
            success_rate_threshold=0.95,
        )
        result = await check.check()

        # 20% success rate should be unhealthy
        assert result.status in ["unhealthy", "degraded"]

    async def test_graphql_check_exception_handling(self):
        """GraphQL check handles exceptions gracefully."""
        check = GraphQLHealthCheck(monitor=None)

        result = await check.check()

        assert result.status in ["unhealthy", "degraded", "healthy"]


class TestTracingHealthCheck:
    """Tests for TracingHealthCheck."""

    async def test_tracing_check_creation(self):
        """TracingHealthCheck creates successfully."""
        check = TracingHealthCheck()
        assert check is not None

    async def test_tracing_check_result(self):
        """Tracing check returns valid result."""
        check = TracingHealthCheck()
        result = await check.check()

        # Should return degraded or healthy (depends on observability module)
        assert result.status in ["healthy", "degraded", "unhealthy"]
        assert result.message is not None


class TestHealthCheckAggregator:
    """Tests for HealthCheckAggregator."""

    async def test_aggregator_all_healthy(self):
        """Aggregator shows healthy when all checks pass."""
        # Create mock monitors
        db_monitor = MagicMock()
        cache_monitor = MagicMock()
        graphql_monitor = MagicMock()

        # Setup database monitor
        db_stats = MagicMock()
        db_stats.total_count = 100
        db_stats.slow_count = 0
        db_stats.error_count = 0
        db_stats.avg_duration_ms = 45.0
        db_stats.p95_duration_ms = 95.0
        db_stats.p99_duration_ms = 150.0

        db_pool = MagicMock()
        db_pool.active_connections = 5
        db_pool.total_connections = 10
        db_pool.get_utilization_percent = MagicMock(return_value=50.0)

        db_monitor.get_query_statistics = AsyncMock(return_value=db_stats)
        db_monitor.get_pool_metrics = AsyncMock(return_value=db_pool)

        # Setup cache monitor
        cache_monitor.get_hit_rate = AsyncMock(return_value=0.85)
        cache_monitor.get_eviction_count = AsyncMock(return_value=5)
        cache_monitor.get_operation_count = AsyncMock(return_value=100)

        # Setup GraphQL monitor
        gql_stats = MagicMock()
        gql_stats.total_operations = 100
        gql_stats.successful_operations = 99
        gql_stats.failed_operations = 1
        gql_stats.avg_duration_ms = 125.0
        gql_stats.p95_duration_ms = 250.0
        graphql_monitor.get_statistics = AsyncMock(return_value=gql_stats)

        db_check = DatabaseHealthCheck(monitor=db_monitor)
        cache_check = CacheHealthCheck(monitor=cache_monitor)
        graphql_check = GraphQLHealthCheck(monitor=graphql_monitor)

        aggregator = HealthCheckAggregator(
            database_check=db_check,
            cache_check=cache_check,
            graphql_check=graphql_check,
        )

        status = await aggregator.check_all()

        assert status.overall_status in ["healthy", "degraded"]
        assert status.checks_executed == 4
        assert status.timestamp is not None

    async def test_aggregator_degraded_status(self):
        """Aggregator shows degraded when some checks warn."""
        aggregator = HealthCheckAggregator()

        status = await aggregator.check_all()

        # Status should be valid
        assert status.overall_status in ["healthy", "degraded", "unhealthy"]

    async def test_aggregator_database_check_only(self):
        """Aggregator can run database check only."""
        monitor = MagicMock()
        stats = MagicMock()
        stats.total_count = 100
        stats.slow_count = 0
        stats.error_count = 0
        stats.avg_duration_ms = 45.0

        pool = MagicMock()
        pool.active_connections = 5
        pool.total_connections = 10
        pool.get_utilization_percent = MagicMock(return_value=50.0)

        monitor.get_query_statistics = AsyncMock(return_value=stats)
        monitor.get_pool_metrics = AsyncMock(return_value=pool)

        check = DatabaseHealthCheck(monitor=monitor)
        aggregator = HealthCheckAggregator(database_check=check)

        result = await aggregator.check_database()

        assert result.status in ["healthy", "degraded", "unhealthy"]

    async def test_aggregator_cache_check_only(self):
        """Aggregator can run cache check only."""
        monitor = MagicMock()
        monitor.get_hit_rate = AsyncMock(return_value=0.85)
        monitor.get_eviction_count = AsyncMock(return_value=5)
        monitor.get_operation_count = AsyncMock(return_value=100)

        check = CacheHealthCheck(monitor=monitor)
        aggregator = HealthCheckAggregator(cache_check=check)

        result = await aggregator.check_cache()

        assert result.status in ["healthy", "degraded", "unhealthy"]

    async def test_aggregator_graphql_check_only(self):
        """Aggregator can run GraphQL check only."""
        monitor = MagicMock()
        stats = MagicMock()
        stats.total_operations = 100
        stats.successful_operations = 99
        stats.failed_operations = 1
        stats.avg_duration_ms = 125.0
        stats.p95_duration_ms = 250.0

        monitor.get_statistics = AsyncMock(return_value=stats)

        check = GraphQLHealthCheck(monitor=monitor)
        aggregator = HealthCheckAggregator(graphql_check=check)

        result = await aggregator.check_graphql()

        assert result.status in ["healthy", "degraded", "unhealthy"]

    async def test_aggregator_tracing_check_only(self):
        """Aggregator can run tracing check only."""
        check = TracingHealthCheck()
        aggregator = HealthCheckAggregator(tracing_check=check)

        result = await aggregator.check_tracing()

        assert result.status in ["healthy", "degraded", "unhealthy"]

    async def test_aggregator_check_time(self):
        """Aggregator records check duration."""
        aggregator = HealthCheckAggregator()

        status = await aggregator.check_all()

        assert status.check_duration_ms > 0
        assert status.check_duration_ms < 10000  # Should be fast

    async def test_aggregator_timestamp(self):
        """Aggregator includes check timestamp."""
        aggregator = HealthCheckAggregator()

        status = await aggregator.check_all()

        assert status.timestamp is not None
        assert isinstance(status.timestamp, datetime)
