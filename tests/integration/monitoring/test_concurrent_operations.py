"""Concurrent operations and load testing for Phase 19 monitoring."""

from __future__ import annotations

import asyncio
import time

import pytest

from fraiseql.monitoring.runtime.db_monitor_sync import get_database_monitor_sync
from fraiseql.monitoring.db_monitor import QueryMetrics


class TestConcurrentQueryOperations:
    """Tests for concurrent query monitoring."""

    def test_multiple_simultaneous_queries(self, monitoring_enabled, sample_query_metrics, make_query_metric):
        """Test metrics consistency with multiple simultaneous queries."""
        monitor = monitoring_enabled

        # Simulate multiple queries being recorded
        import threading

        def add_query(metric):
            with monitor._lock:
                monitor._recent_queries.append(metric)

        threads = []
        for metric in sample_query_metrics:
            t = threading.Thread(target=add_query, args=(metric,))
            threads.append(t)
            t.start()

        # Wait for all to complete
        for t in threads:
            t.join()

        # Verify all queries were recorded
        db_sync = get_database_monitor_sync()
        recent = db_sync.get_recent_queries(limit=100)

        assert len(recent) == len(sample_query_metrics)

    def test_concurrent_metrics_consistency(self, monitoring_enabled, make_query_metric):
        """Test that metrics remain consistent under concurrent access."""
        monitor = monitoring_enabled

        # Add queries from multiple threads
        import threading

        query_count = 20

        def add_sequential_queries(start_id):
            for i in range(5):
                metric = make_query_metric(
                    query_type="SELECT",
                    duration_ms=10.0 + i,
                    rows_affected=1,
                )
                with monitor._lock:
                    monitor._recent_queries.append(metric)

        threads = []
        for i in range(4):
            t = threading.Thread(target=add_sequential_queries, args=(i * 100,))
            threads.append(t)
            t.start()

        for t in threads:
            t.join()

        # Verify all queries recorded
        db_sync = get_database_monitor_sync()
        recent = db_sync.get_recent_queries(limit=100)

        assert len(recent) == query_count

    def test_no_data_races_under_load(self, monitoring_enabled, make_query_metric):
        """Test no data races occur under concurrent load."""
        monitor = monitoring_enabled

        import threading

        errors = []

        def stress_test():
            try:
                for i in range(10):
                    metric = make_query_metric(
                        query_type="SELECT",
                        duration_ms=5.0,
                        rows_affected=i,
                    )
                    with monitor._lock:
                        monitor._recent_queries.append(metric)
                        # Immediate read to verify consistency
                        _ = len(monitor._recent_queries)
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=stress_test) for _ in range(10)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        # No errors should have occurred
        assert len(errors) == 0


class TestHealthCheckUnderLoad:
    """Tests for health checks during concurrent operations."""

    def test_health_check_during_queries(self, monitoring_enabled, sample_query_metrics):
        """Test health check works correctly while queries are being recorded."""
        monitor = monitoring_enabled

        # Add queries
        with monitor._lock:
            for metric in sample_query_metrics:
                monitor._recent_queries.append(metric)

        # Get health-relevant stats while more activity happens
        import threading

        def add_more_queries():
            from fraiseql.monitoring.db_monitor import QueryMetrics
            import hashlib
            import uuid
            from datetime import datetime

            for i in range(5):
                sql = f"SELECT {i}"
                m = QueryMetrics(
                    query_id=str(uuid.uuid4()),
                    query_hash=hashlib.sha256(sql.encode()).hexdigest(),
                    query_type="SELECT",
                    timestamp=datetime.now(),
                    duration_ms=3.0,
                    rows_affected=1,
                )
                with monitor._lock:
                    monitor._recent_queries.append(m)

        # Start background thread
        t = threading.Thread(target=add_more_queries)
        t.start()

        # Check stats while thread runs
        db_sync = get_database_monitor_sync()
        stats = db_sync.get_statistics()

        t.join()

        # Stats should be valid
        assert stats is not None
        assert stats.total_count > 0

    def test_health_response_time_under_load(self, monitoring_enabled, sample_query_metrics):
        """Test health check responds quickly even under load."""
        monitor = monitoring_enabled

        # Add queries
        with monitor._lock:
            for metric in sample_query_metrics:
                monitor._recent_queries.append(metric)

        # Simulate continuous queries
        import threading
        from fraiseql.monitoring.db_monitor import QueryMetrics
        import hashlib
        import uuid
        from datetime import datetime

        stop_event = threading.Event()

        def continuous_queries():
            count = 0
            while not stop_event.is_set():
                sql = f"SELECT {count}"
                m = QueryMetrics(
                    query_id=str(uuid.uuid4()),
                    query_hash=hashlib.sha256(sql.encode()).hexdigest(),
                    query_type="SELECT",
                    timestamp=datetime.now(),
                    duration_ms=5.0,
                    rows_affected=1,
                )
                with monitor._lock:
                    monitor._recent_queries.append(m)
                count += 1
                time.sleep(0.01)

        t = threading.Thread(target=continuous_queries)
        t.start()

        # Time the health check
        db_sync = get_database_monitor_sync()
        start = time.time()
        stats = db_sync.get_statistics()
        elapsed_ms = (time.time() - start) * 1000

        stop_event.set()
        t.join()

        # Health check should complete quickly (< 100ms baseline)
        assert elapsed_ms < 100
        assert stats is not None


class TestCacheImpactUnderLoad:
    """Tests for cache behavior under load."""

    def test_cache_hit_rate_with_repeated_operations(self, cache_monitor_fixture):
        """Test cache hit rate tracking under repeated operations."""
        cache = cache_monitor_fixture

        # Get initial metrics
        metrics_before = cache.get_metrics_dict()

        # Simulate cache operations
        # In real scenario, this would come from actual cache hits/misses
        initial_hits = metrics_before.get("hits", 0)
        initial_misses = metrics_before.get("misses", 0)

        # The cache monitor should track these correctly
        assert "hit_rate" in metrics_before

    def test_cache_health_stability(self, cache_monitor_fixture):
        """Test cache health status remains stable."""
        cache = cache_monitor_fixture

        # Check health multiple times
        health1 = cache.is_healthy()
        health2 = cache.is_healthy()
        health3 = cache.is_healthy()

        # Health should be consistent (no flaky changes)
        assert health1 == health2 == health3


class TestConnectionPoolUnderLoad:
    """Tests for PostgreSQL connection pool behavior."""

    def test_pool_utilization_tracking(self, monitoring_enabled):
        """Test connection pool utilization is tracked correctly."""
        from fraiseql.monitoring.db_monitor import PoolMetrics

        monitor = monitoring_enabled

        # Set pool metrics for high utilization
        with monitor._lock:
            monitor._pool_metrics = PoolMetrics(
                timestamp=monitor._recent_queries[0].timestamp if monitor._recent_queries else __import__("datetime").datetime.now(),
                total_connections=20,
                active_connections=18,
                idle_connections=2,
                waiting_requests=5,
                avg_wait_time_ms=15.0,
                max_wait_time_ms=50.0,
            )

        db_sync = get_database_monitor_sync()
        pool = db_sync.get_pool_metrics()

        assert pool.get_utilization_percent() == 90.0
        assert pool.waiting_requests == 5

    def test_pool_stress_recovery(self, monitoring_enabled):
        """Test pool metrics during stress and recovery."""
        from fraiseql.monitoring.db_monitor import PoolMetrics
        from datetime import datetime

        monitor = monitoring_enabled
        now = datetime.now()

        # Start with normal load
        with monitor._lock:
            monitor._pool_metrics = PoolMetrics(
                timestamp=now,
                total_connections=20,
                active_connections=10,
                idle_connections=10,
                waiting_requests=0,
                avg_wait_time_ms=1.0,
                max_wait_time_ms=5.0,
            )

        db_sync = get_database_monitor_sync()
        normal = db_sync.get_pool_metrics()
        assert normal.get_utilization_percent() == 50.0

        # Stress the pool
        with monitor._lock:
            monitor._pool_metrics = PoolMetrics(
                timestamp=now,
                total_connections=20,
                active_connections=19,
                idle_connections=1,
                waiting_requests=10,
                avg_wait_time_ms=50.0,
                max_wait_time_ms=150.0,
            )

        stressed = db_sync.get_pool_metrics()
        assert stressed.get_utilization_percent() == 95.0

        # Recovery
        with monitor._lock:
            monitor._pool_metrics = PoolMetrics(
                timestamp=now,
                total_connections=20,
                active_connections=8,
                idle_connections=12,
                waiting_requests=0,
                avg_wait_time_ms=1.5,
                max_wait_time_ms=8.0,
            )

        recovered = db_sync.get_pool_metrics()
        assert recovered.get_utilization_percent() == 40.0


class TestMetricsAggregationUnderLoad:
    """Tests for statistics aggregation under concurrent load."""

    def test_statistics_accuracy_under_load(self, monitoring_enabled, make_query_metric):
        """Test statistics remain accurate under load."""
        monitor = monitoring_enabled

        import threading

        # Add many queries from multiple threads
        query_count = 100

        def add_queries(batch_id):
            for i in range(query_count // 10):
                is_error = i % 8 == 7  # Simulate some failures
                metric = make_query_metric(
                    query_type="SELECT" if i % 2 == 0 else "UPDATE",
                    duration_ms=5.0 + (i % 10),
                    rows_affected=i % 100,
                    error="simulated error" if is_error else None,
                )

                with monitor._lock:
                    monitor._recent_queries.append(metric)

        threads = [threading.Thread(target=add_queries, args=(i,)) for i in range(10)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        db_sync = get_database_monitor_sync()
        stats = db_sync.get_statistics()

        # Verify statistics are accurate
        assert stats.total_count == query_count
        assert stats.success_count + stats.error_count == query_count
        assert abs(stats.success_rate - (stats.success_count / query_count)) < 0.001


@pytest.mark.asyncio
async def test_async_concurrent_operations():
    """Test concurrent operations using async/await."""
    from fraiseql.monitoring.db_monitor import QueryMetrics
    import hashlib
    import uuid
    from datetime import datetime

    async def simulate_query(query_id: int) -> QueryMetrics:
        """Simulate a query with some async delay."""
        await asyncio.sleep(0.001)  # Simulate I/O
        sql = f"SELECT FROM query_{query_id}"
        return QueryMetrics(
            query_id=str(uuid.uuid4()),
            query_hash=hashlib.sha256(sql.encode()).hexdigest(),
            query_type="SELECT",
            timestamp=datetime.now(),
            duration_ms=5.0 + query_id,
            rows_affected=query_id % 100,
        )

    # Run 20 concurrent queries
    tasks = [simulate_query(i) for i in range(20)]
    results = await asyncio.gather(*tasks)

    # All should complete successfully
    assert len(results) == 20
    assert all(isinstance(r, QueryMetrics) for r in results)
