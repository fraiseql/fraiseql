"""Tests for DatabaseMonitorSync (Phase 19, Commit 7).

Tests the synchronous database monitor accessor layer.
"""

from datetime import UTC, datetime

import pytest

from fraiseql.monitoring.db_monitor import (
    DatabaseMonitor,
    PoolMetrics,
    QueryMetrics,
)
from fraiseql.monitoring.runtime.db_monitor_sync import DatabaseMonitorSync


@pytest.fixture
def monitor() -> DatabaseMonitor:
    """Create a fresh DatabaseMonitor instance for testing."""
    return DatabaseMonitor()


@pytest.fixture
def sync_monitor(monitor: DatabaseMonitor) -> DatabaseMonitorSync:
    """Create a DatabaseMonitorSync instance."""
    return DatabaseMonitorSync(monitor=monitor)


@pytest.fixture
def sample_query() -> QueryMetrics:
    """Create a sample query metric."""
    return QueryMetrics(
        query_id="q1",
        query_hash="hash1",
        query_type="SELECT",
        timestamp=datetime.now(UTC),
        duration_ms=42.5,
        rows_affected=10,
    )


@pytest.fixture
def slow_query() -> QueryMetrics:
    """Create a sample slow query metric."""
    return QueryMetrics(
        query_id="q_slow",
        query_hash="hash_slow",
        query_type="SELECT",
        timestamp=datetime.now(UTC),
        duration_ms=250.0,  # Exceeds default 100ms threshold
        rows_affected=50,
        is_slow=True,
    )


class TestDatabaseMonitorSync:
    """Tests for DatabaseMonitorSync accessor."""

    def test_get_recent_queries_empty(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test getting recent queries when none exist."""
        queries = sync_monitor.get_recent_queries(limit=10)
        assert queries == []

    def test_get_recent_queries_single(
        self, sync_monitor: DatabaseMonitorSync, sample_query: QueryMetrics
    ) -> None:
        """Test getting a single recent query."""
        monitor = sync_monitor._monitor
        monitor._recent_queries.append(sample_query)

        queries = sync_monitor.get_recent_queries(limit=10)
        assert len(queries) == 1
        assert queries[0].query_id == "q1"
        assert queries[0].duration_ms == 42.5

    def test_get_recent_queries_multiple(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test getting multiple recent queries."""
        monitor = sync_monitor._monitor

        for i in range(5):
            query = QueryMetrics(
                query_id=f"q{i}",
                query_hash=f"hash{i}",
                query_type="SELECT",
                timestamp=datetime.now(UTC),
                duration_ms=float(i * 10),
            )
            monitor._recent_queries.append(query)

        queries = sync_monitor.get_recent_queries(limit=10)
        assert len(queries) == 5
        # Should be in reverse order (newest first)
        assert queries[0].query_id == "q4"
        assert queries[-1].query_id == "q0"

    def test_get_recent_queries_with_limit(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test getting recent queries respects limit."""
        monitor = sync_monitor._monitor

        for i in range(10):
            query = QueryMetrics(
                query_id=f"q{i}",
                query_hash=f"hash{i}",
                query_type="SELECT",
                timestamp=datetime.now(UTC),
                duration_ms=float(i),
            )
            monitor._recent_queries.append(query)

        queries = sync_monitor.get_recent_queries(limit=5)
        assert len(queries) == 5

    def test_get_slow_queries_empty(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test getting slow queries when none exist."""
        queries = sync_monitor.get_slow_queries(limit=10)
        assert queries == []

    def test_get_slow_queries_sorted_by_duration(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test slow queries are sorted by duration (slowest first)."""
        monitor = sync_monitor._monitor

        queries_data = [
            (100.0, "q1"),
            (250.0, "q_slow"),
            (150.0, "q2"),
        ]

        for duration, qid in queries_data:
            query = QueryMetrics(
                query_id=qid,
                query_hash=f"hash_{qid}",
                query_type="SELECT",
                timestamp=datetime.now(UTC),
                duration_ms=duration,
                is_slow=True,
            )
            monitor._slow_queries.append(query)

        slow = sync_monitor.get_slow_queries(limit=10)
        assert len(slow) == 3
        # Should be sorted slowest first
        assert slow[0].duration_ms == 250.0
        assert slow[1].duration_ms == 150.0
        assert slow[2].duration_ms == 100.0

    def test_get_queries_by_type_empty(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test getting query types when none exist."""
        types = sync_monitor.get_queries_by_type()
        assert types == {}

    def test_get_queries_by_type_breakdown(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test query breakdown by type."""
        monitor = sync_monitor._monitor

        query_types = ["SELECT", "SELECT", "INSERT", "UPDATE"]
        for i, qtype in enumerate(query_types):
            query = QueryMetrics(
                query_id=f"q{i}",
                query_hash=f"hash{i}",
                query_type=qtype,
                timestamp=datetime.now(UTC),
                duration_ms=10.0,
            )
            monitor._recent_queries.append(query)

        types = sync_monitor.get_queries_by_type()
        assert types["SELECT"] == 2
        assert types["INSERT"] == 1
        assert types["UPDATE"] == 1

    def test_get_pool_metrics_empty(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test getting pool metrics when none exist."""
        metrics = sync_monitor.get_pool_metrics()
        assert metrics is None

    def test_get_pool_metrics_latest(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test getting the latest pool metrics."""
        monitor = sync_monitor._monitor

        pool1 = PoolMetrics(
            timestamp=datetime.now(UTC),
            total_connections=10,
            active_connections=5,
        )
        pool2 = PoolMetrics(
            timestamp=datetime.now(UTC),
            total_connections=10,
            active_connections=7,
        )

        monitor._pool_states.append(pool1)
        monitor._pool_states.append(pool2)

        metrics = sync_monitor.get_pool_metrics()
        assert metrics is not None
        assert metrics.active_connections == 7

    def test_get_statistics_empty(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test statistics when no queries exist."""
        stats = sync_monitor.get_statistics()
        assert stats.total_count == 0
        assert stats.success_count == 0

    def test_get_statistics_success_rate(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test success rate calculation."""
        monitor = sync_monitor._monitor

        # Add 8 successful queries
        for i in range(8):
            query = QueryMetrics(
                query_id=f"q{i}",
                query_hash=f"hash{i}",
                query_type="SELECT",
                timestamp=datetime.now(UTC),
                duration_ms=10.0,
                error=None,
            )
            monitor._recent_queries.append(query)

        # Add 2 failed queries
        for i in range(8, 10):
            query = QueryMetrics(
                query_id=f"q{i}",
                query_hash=f"hash{i}",
                query_type="SELECT",
                timestamp=datetime.now(UTC),
                duration_ms=10.0,
                error="Connection timeout",
            )
            monitor._recent_queries.append(query)

        stats = sync_monitor.get_statistics()
        assert stats.total_count == 10
        assert stats.success_count == 8
        assert stats.error_count == 2
        assert stats.success_rate == 0.8

    def test_get_query_count(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test getting total query count."""
        monitor = sync_monitor._monitor

        for i in range(5):
            query = QueryMetrics(
                query_id=f"q{i}",
                query_hash=f"hash{i}",
                query_type="SELECT",
                timestamp=datetime.now(UTC),
                duration_ms=10.0,
            )
            monitor._recent_queries.append(query)

        count = sync_monitor.get_query_count()
        assert count == 5

    def test_get_last_query_empty(self, sync_monitor: DatabaseMonitorSync) -> None:
        """Test getting last query when none exist."""
        last = sync_monitor.get_last_query()
        assert last is None

    def test_get_last_query(
        self, sync_monitor: DatabaseMonitorSync, sample_query: QueryMetrics
    ) -> None:
        """Test getting the last recorded query."""
        monitor = sync_monitor._monitor
        monitor._recent_queries.append(sample_query)

        last = sync_monitor.get_last_query()
        assert last is not None
        assert last.query_id == "q1"

    def test_thread_safety(
        self, sync_monitor: DatabaseMonitorSync, sample_query: QueryMetrics
    ) -> None:
        """Test that operations use locks (basic verification)."""
        monitor = sync_monitor._monitor

        # This should acquire the lock
        monitor._recent_queries.append(sample_query)
        queries = sync_monitor.get_recent_queries(limit=10)

        assert len(queries) == 1
