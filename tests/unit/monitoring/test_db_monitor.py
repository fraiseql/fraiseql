"""Unit tests for database monitoring.

Tests for:
- QueryMetrics tracking
- PoolMetrics collection
- TransactionMetrics tracking
- DatabaseMonitor functionality
- Statistics calculation
- Performance reports
"""

from datetime import datetime, timedelta, UTC

import pytest

from fraiseql.monitoring.db_monitor import (
    DatabaseMonitor,
    PerformanceReport,
    PoolMetrics,
    QueryMetrics,
    QueryStatistics,
    TransactionMetrics,
)


@pytest.fixture
def sample_queries() -> list[QueryMetrics]:
    """Create sample query metrics for testing."""
    now = datetime.now(UTC)
    return [
        QueryMetrics(
            query_id="q-1",
            query_hash="hash-1",
            query_type="SELECT",
            timestamp=now - timedelta(seconds=30),
            duration_ms=45.5,
            rows_affected=100,
            is_slow=False,
        ),
        QueryMetrics(
            query_id="q-2",
            query_hash="hash-2",
            query_type="INSERT",
            timestamp=now - timedelta(seconds=20),
            duration_ms=120.3,
            rows_affected=1,
            is_slow=True,
        ),
        QueryMetrics(
            query_id="q-3",
            query_hash="hash-3",
            query_type="SELECT",
            timestamp=now - timedelta(seconds=10),
            duration_ms=50.2,
            rows_affected=50,
            is_slow=False,
        ),
        QueryMetrics(
            query_id="q-4",
            query_hash="hash-4",
            query_type="UPDATE",
            timestamp=now,
            duration_ms=5000.0,
            rows_affected=500,
            is_slow=True,
            error="Timeout",
        ),
    ]


class TestQueryMetrics:
    """Tests for QueryMetrics dataclass."""

    def test_query_metrics_creation(self):
        """QueryMetrics creates successfully."""
        now = datetime.now(UTC)
        metrics = QueryMetrics(
            query_id="q-1",
            query_hash="hash-1",
            query_type="SELECT",
            timestamp=now,
            duration_ms=50.0,
        )
        assert metrics.query_id == "q-1"
        assert metrics.query_type == "SELECT"
        assert metrics.duration_ms == 50.0

    def test_query_success_check(self):
        """is_success() works correctly."""
        metrics = QueryMetrics(
            query_id="q-1",
            query_hash="hash-1",
            query_type="SELECT",
            timestamp=datetime.now(UTC),
            duration_ms=50.0,
        )
        assert metrics.is_success() is True

    def test_query_error_check(self):
        """is_failed() works correctly."""
        metrics = QueryMetrics(
            query_id="q-1",
            query_hash="hash-1",
            query_type="SELECT",
            timestamp=datetime.now(UTC),
            duration_ms=50.0,
            error="Connection timeout",
        )
        assert metrics.is_failed() is True
        assert metrics.is_success() is False


class TestPoolMetrics:
    """Tests for PoolMetrics dataclass."""

    def test_pool_metrics_creation(self):
        """PoolMetrics creates successfully."""
        metrics = PoolMetrics(
            timestamp=datetime.now(UTC),
            total_connections=10,
            active_connections=7,
            idle_connections=3,
        )
        assert metrics.total_connections == 10
        assert metrics.active_connections == 7

    def test_pool_utilization_calculation(self):
        """get_utilization_percent() calculates correctly."""
        metrics = PoolMetrics(
            timestamp=datetime.now(UTC),
            total_connections=10,
            active_connections=7,
            idle_connections=3,
        )
        assert metrics.get_utilization_percent() == 70.0

    def test_pool_utilization_empty(self):
        """get_utilization_percent() handles empty pool."""
        metrics = PoolMetrics(
            timestamp=datetime.now(UTC),
            total_connections=0,
            active_connections=0,
        )
        assert metrics.get_utilization_percent() == 0.0


class TestTransactionMetrics:
    """Tests for TransactionMetrics dataclass."""

    def test_transaction_metrics_creation(self):
        """TransactionMetrics creates successfully."""
        now = datetime.now(UTC)
        metrics = TransactionMetrics(
            transaction_id="txn-1",
            start_time=now,
        )
        assert metrics.transaction_id == "txn-1"
        assert metrics.is_active() is True

    def test_transaction_committed(self):
        """Committed transaction status works."""
        now = datetime.now(UTC)
        metrics = TransactionMetrics(
            transaction_id="txn-1",
            start_time=now,
            end_time=now + timedelta(milliseconds=100),
            duration_ms=100.0,
            status="COMMITTED",
        )
        assert metrics.is_committed() is True
        assert metrics.is_active() is False

    def test_transaction_rolled_back(self):
        """Rolled back transaction status works."""
        now = datetime.now(UTC)
        metrics = TransactionMetrics(
            transaction_id="txn-1",
            start_time=now,
            end_time=now + timedelta(milliseconds=100),
            duration_ms=100.0,
            status="ROLLED_BACK",
        )
        assert metrics.is_rolled_back() is True
        assert metrics.is_active() is False


class TestDatabaseMonitor:
    """Tests for DatabaseMonitor class."""

    async def test_monitor_initialization(self):
        """Monitor initializes correctly."""
        monitor = DatabaseMonitor(
            max_recent_queries=1000,
            max_slow_queries=100,
            slow_query_threshold_ms=100.0,
        )
        assert await monitor.get_query_count() == 0
        assert await monitor.get_slow_query_count() == 0

    async def test_record_query(self, sample_queries):
        """record_query() stores metrics."""
        monitor = DatabaseMonitor()
        await monitor.record_query(sample_queries[0])

        count = await monitor.get_query_count()
        assert count == 1

    async def test_record_multiple_queries(self, sample_queries):
        """Multiple queries can be recorded."""
        monitor = DatabaseMonitor()
        for query in sample_queries:
            await monitor.record_query(query)

        count = await monitor.get_query_count()
        assert count == len(sample_queries)

    async def test_get_recent_queries(self, sample_queries):
        """get_recent_queries() returns recent queries."""
        monitor = DatabaseMonitor()
        for query in sample_queries:
            await monitor.record_query(query)

        recent = await monitor.get_recent_queries(limit=2)
        assert len(recent) == 2
        # Should be most recent first
        assert recent[0].query_id == "q-4"

    async def test_get_slow_queries(self, sample_queries):
        """get_slow_queries() returns only slow queries."""
        monitor = DatabaseMonitor()
        for query in sample_queries:
            await monitor.record_query(query)

        slow = await monitor.get_slow_queries()
        assert len(slow) == 2  # q-2 and q-4 are slow
        assert all(q.is_slow for q in slow)

    async def test_get_slow_queries_sorted(self, sample_queries):
        """get_slow_queries() returns sorted by duration."""
        monitor = DatabaseMonitor()
        for query in sample_queries:
            await monitor.record_query(query)

        slow = await monitor.get_slow_queries()
        durations = [q.duration_ms for q in slow]
        assert durations == sorted(durations, reverse=True)

    async def test_get_queries_by_type(self, sample_queries):
        """get_queries_by_type() groups by type."""
        monitor = DatabaseMonitor()
        for query in sample_queries:
            await monitor.record_query(query)

        by_type = await monitor.get_queries_by_type()
        assert by_type["SELECT"] == 2
        assert by_type["INSERT"] == 1
        assert by_type["UPDATE"] == 1

    async def test_record_pool_state(self):
        """record_pool_state() stores pool metrics."""
        monitor = DatabaseMonitor()
        pool = PoolMetrics(
            timestamp=datetime.now(UTC),
            total_connections=10,
            active_connections=7,
        )
        await monitor.record_pool_state(pool)

        current = await monitor.get_pool_metrics()
        assert current is not None
        assert current.active_connections == 7

    async def test_get_pool_history(self):
        """get_pool_history() returns historical pool states."""
        monitor = DatabaseMonitor()
        now = datetime.now(UTC)

        for i in range(3):
            pool = PoolMetrics(
                timestamp=now + timedelta(seconds=i),
                total_connections=10,
                active_connections=i + 1,
            )
            await monitor.record_pool_state(pool)

        history = await monitor.get_pool_history(limit=10)
        assert len(history) == 3

    async def test_transaction_tracking(self):
        """Transaction lifecycle is tracked."""
        monitor = DatabaseMonitor()
        txn_id = "txn-1"

        await monitor.start_transaction(txn_id)
        await monitor.record_transaction_query(txn_id)
        await monitor.record_transaction_query(txn_id)
        await monitor.commit_transaction(txn_id)

        # Verify transaction exists and is committed
        # (would need to expose get_transaction in real implementation)

    async def test_get_query_statistics(self, sample_queries):
        """get_query_statistics() calculates stats."""
        monitor = DatabaseMonitor()
        for query in sample_queries:
            await monitor.record_query(query)

        stats = await monitor.get_query_statistics()
        assert isinstance(stats, QueryStatistics)
        assert stats.total_count == len(sample_queries)
        assert stats.slow_count == 2
        assert stats.error_count == 1

    async def test_statistics_percentiles(self, sample_queries):
        """Statistics include percentiles."""
        monitor = DatabaseMonitor()
        for query in sample_queries:
            await monitor.record_query(query)

        stats = await monitor.get_query_statistics()
        assert stats.p50_duration_ms > 0
        assert stats.p95_duration_ms > 0
        assert stats.p99_duration_ms > 0
        # P99 should be slowest
        assert stats.p99_duration_ms >= stats.p95_duration_ms

    async def test_statistics_average(self, sample_queries):
        """Statistics average duration calculated."""
        monitor = DatabaseMonitor()
        for query in sample_queries:
            await monitor.record_query(query)

        stats = await monitor.get_query_statistics()
        expected_avg = sum(q.duration_ms for q in sample_queries) / len(
            sample_queries
        )
        assert abs(stats.avg_duration_ms - expected_avg) < 0.01

    async def test_get_performance_report(self, sample_queries):
        """get_performance_report() generates report."""
        monitor = DatabaseMonitor()
        for query in sample_queries:
            await monitor.record_query(query)

        now = datetime.now(UTC)
        report = await monitor.get_performance_report(
            start_time=now - timedelta(minutes=1),
            end_time=now + timedelta(minutes=1),
        )

        assert isinstance(report, PerformanceReport)
        assert report.query_stats.total_count == len(sample_queries)
        assert len(report.slow_queries) == 2

    async def test_performance_report_summary(self, sample_queries):
        """Performance report summary string is valid."""
        monitor = DatabaseMonitor()
        for query in sample_queries:
            await monitor.record_query(query)

        now = datetime.now(UTC)
        report = await monitor.get_performance_report(
            start_time=now - timedelta(minutes=1),
            end_time=now,
        )

        summary = report.get_summary_string()
        assert "Database Performance Report" in summary
        assert "Total Queries" in summary
        assert "Slow Queries" in summary

    async def test_clear_metrics(self, sample_queries):
        """clear() removes all metrics."""
        monitor = DatabaseMonitor()
        for query in sample_queries:
            await monitor.record_query(query)

        await monitor.clear()
        assert await monitor.get_query_count() == 0
        assert await monitor.get_slow_query_count() == 0

    async def test_monitor_thread_safety(self):
        """Monitor handles concurrent access safely."""
        monitor = DatabaseMonitor()
        now = datetime.now(UTC)

        # This is a basic test - real concurrency tests would need threading
        query1 = QueryMetrics(
            query_id="q-1",
            query_hash="h-1",
            query_type="SELECT",
            timestamp=now,
            duration_ms=50.0,
        )
        query2 = QueryMetrics(
            query_id="q-2",
            query_hash="h-2",
            query_type="SELECT",
            timestamp=now,
            duration_ms=60.0,
        )

        await monitor.record_query(query1)
        await monitor.record_query(query2)

        count = await monitor.get_query_count()
        assert count == 2


class TestEdgeCases:
    """Tests for edge cases."""

    async def test_empty_monitor_statistics(self):
        """Statistics work with no queries."""
        monitor = DatabaseMonitor()
        stats = await monitor.get_query_statistics()
        assert stats.total_count == 0
        assert stats.avg_duration_ms == 0.0

    async def test_empty_monitor_report(self):
        """Report works with no queries."""
        monitor = DatabaseMonitor()
        now = datetime.now(UTC)
        report = await monitor.get_performance_report(
            start_time=now - timedelta(minutes=1),
            end_time=now,
        )
        assert report.query_stats.total_count == 0

    async def test_single_query_statistics(self):
        """Statistics work with single query."""
        monitor = DatabaseMonitor()
        query = QueryMetrics(
            query_id="q-1",
            query_hash="h-1",
            query_type="SELECT",
            timestamp=datetime.now(UTC),
            duration_ms=50.0,
        )
        await monitor.record_query(query)

        stats = await monitor.get_query_statistics()
        assert stats.total_count == 1
        assert stats.avg_duration_ms == 50.0
        assert stats.min_duration_ms == 50.0
        assert stats.max_duration_ms == 50.0

    async def test_pool_metrics_with_zero_connections(self):
        """Pool metrics handle zero connections."""
        monitor = DatabaseMonitor()
        pool = PoolMetrics(
            timestamp=datetime.now(UTC),
            total_connections=0,
            active_connections=0,
        )
        await monitor.record_pool_state(pool)

        current = await monitor.get_pool_metrics()
        assert current is not None
        assert current.get_utilization_percent() == 0.0

    async def test_query_with_all_fields(self):
        """Query metrics can include all optional fields."""
        now = datetime.now(UTC)
        query = QueryMetrics(
            query_id="q-1",
            query_hash="hash-1",
            query_type="SELECT",
            timestamp=now,
            duration_ms=50.0,
            execution_time_ms=45.0,
            network_time_ms=5.0,
            rows_affected=100,
            parameter_count=3,
            connection_acquired_ms=1.2,
            is_slow=False,
            trace_id="trace-123",
        )
        monitor = DatabaseMonitor()
        await monitor.record_query(query)

        recent = await monitor.get_recent_queries(limit=1)
        assert recent[0].trace_id == "trace-123"
        assert recent[0].parameter_count == 3
