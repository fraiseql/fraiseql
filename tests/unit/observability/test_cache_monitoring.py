"""Unit tests for cache monitoring (Phase 19, Commit 3).

Tests for cache metrics collection, monitoring integration, and per-cache tracking.
"""

import pytest

from fraiseql.monitoring.cache_monitoring import (
    CacheMetrics,
    CacheMonitor,
    CacheMonitoringIntegration,
    get_cache_monitoring,
    set_cache_monitoring,
)


class TestCacheMetrics:
    """Tests for CacheMetrics dataclass."""

    def test_cache_metrics_creation(self) -> None:
        """Test creating CacheMetrics with defaults."""
        metrics = CacheMetrics()
        assert metrics.hits == 0
        assert metrics.misses == 0
        assert metrics.errors == 0
        assert metrics.evictions == 0
        assert metrics.memory_bytes == 0

    def test_cache_metrics_custom_values(self) -> None:
        """Test creating CacheMetrics with custom values."""
        metrics = CacheMetrics(
            hits=100, misses=50, errors=5, evictions=10, memory_bytes=1024
        )
        assert metrics.hits == 100
        assert metrics.misses == 50
        assert metrics.errors == 5
        assert metrics.evictions == 10
        assert metrics.memory_bytes == 1024

    def test_total_operations(self) -> None:
        """Test total operations calculation."""
        metrics = CacheMetrics(hits=75, misses=25)
        assert metrics.total_operations == 100

    def test_total_operations_zero(self) -> None:
        """Test total operations when no operations."""
        metrics = CacheMetrics()
        assert metrics.total_operations == 0

    def test_hit_rate_calculation(self) -> None:
        """Test hit rate percentage calculation."""
        metrics = CacheMetrics(hits=80, misses=20)
        assert metrics.hit_rate == 80.0

    def test_hit_rate_no_operations(self) -> None:
        """Test hit rate when no operations."""
        metrics = CacheMetrics()
        assert metrics.hit_rate == 0.0

    def test_error_rate_calculation(self) -> None:
        """Test error rate calculation."""
        metrics = CacheMetrics(hits=95, misses=4, errors=1)
        assert metrics.error_rate == pytest.approx(1.01, abs=0.1)

    def test_error_rate_no_operations(self) -> None:
        """Test error rate when no operations."""
        metrics = CacheMetrics()
        assert metrics.error_rate == 0.0

    def test_bytes_per_entry(self) -> None:
        """Test bytes per entry calculation."""
        metrics = CacheMetrics(memory_bytes=1000, effective_entries=10)
        assert metrics.bytes_per_entry == 100.0

    def test_bytes_per_entry_no_entries(self) -> None:
        """Test bytes per entry when no entries."""
        metrics = CacheMetrics(memory_bytes=1000)
        assert metrics.bytes_per_entry == 0.0

    def test_to_dict(self) -> None:
        """Test conversion to dictionary."""
        metrics = CacheMetrics(
            hits=100, misses=50, errors=5, memory_bytes=5000, effective_entries=20
        )
        d = metrics.to_dict()

        assert d["hits"] == 100
        assert d["misses"] == 50
        assert d["errors"] == 5
        assert d["memory_bytes"] == 5000
        assert d["effective_entries"] == 20
        assert d["hit_rate_percent"] == pytest.approx(66.67, abs=0.1)
        assert d["error_rate_percent"] == pytest.approx(3.33, abs=0.1)
        assert d["bytes_per_entry"] == 250.0
        assert d["total_operations"] == 150


class TestCacheMonitor:
    """Tests for CacheMonitor class."""

    def test_cache_monitor_creation(self) -> None:
        """Test creating a cache monitor."""
        monitor = CacheMonitor("test_cache")
        assert monitor.cache_name == "test_cache"
        assert monitor.metrics.hits == 0
        assert monitor.metrics.misses == 0

    def test_record_hit(self) -> None:
        """Test recording cache hits."""
        monitor = CacheMonitor("test")
        monitor.record_hit()
        assert monitor.metrics.hits == 1
        assert monitor.metrics.misses == 0

    def test_record_hit_with_latency(self) -> None:
        """Test recording cache hits with latency."""
        monitor = CacheMonitor("test")
        monitor.record_hit(10.5)
        monitor.record_hit(12.5)

        assert monitor.metrics.hits == 2
        assert monitor.metrics.avg_hit_latency_ms == pytest.approx(11.5)

    def test_record_miss(self) -> None:
        """Test recording cache misses."""
        monitor = CacheMonitor("test")
        monitor.record_miss()
        assert monitor.metrics.misses == 1
        assert monitor.metrics.hits == 0

    def test_record_miss_with_latency(self) -> None:
        """Test recording cache misses with latency."""
        monitor = CacheMonitor("test")
        monitor.record_miss(50.0)
        monitor.record_miss(60.0)

        assert monitor.metrics.misses == 2
        assert monitor.metrics.avg_miss_latency_ms == pytest.approx(55.0)

    def test_record_error(self) -> None:
        """Test recording cache errors."""
        monitor = CacheMonitor("test")
        monitor.record_error()
        monitor.record_error()

        assert monitor.metrics.errors == 2

    def test_record_eviction(self) -> None:
        """Test recording cache evictions."""
        monitor = CacheMonitor("test")
        monitor.record_eviction()
        monitor.record_eviction(5)

        assert monitor.metrics.evictions == 6

    def test_record_ttl_expiration(self) -> None:
        """Test recording TTL expirations."""
        monitor = CacheMonitor("test")
        monitor.record_ttl_expiration()
        monitor.record_ttl_expiration(3)

        assert monitor.metrics.ttl_expirations == 4

    def test_set_memory_usage(self) -> None:
        """Test setting memory usage."""
        monitor = CacheMonitor("test")
        monitor.set_memory_usage(5000)
        assert monitor.metrics.memory_bytes == 5000

    def test_set_effective_entries(self) -> None:
        """Test setting effective entries count."""
        monitor = CacheMonitor("test")
        monitor.set_effective_entries(100)
        assert monitor.metrics.effective_entries == 100

    def test_get_metrics(self) -> None:
        """Test getting metrics from monitor."""
        monitor = CacheMonitor("test")
        monitor.record_hit()
        monitor.record_miss()

        metrics = monitor.get_metrics()
        assert metrics.hits == 1
        assert metrics.misses == 1

    def test_reset(self) -> None:
        """Test resetting monitor."""
        monitor = CacheMonitor("test")
        monitor.record_hit(10.0)
        monitor.record_miss(20.0)
        monitor.set_memory_usage(5000)

        monitor.reset()

        assert monitor.metrics.hits == 0
        assert monitor.metrics.misses == 0
        assert monitor.metrics.memory_bytes == 0
        assert monitor.metrics.avg_hit_latency_ms == 0.0


class TestCacheMonitoringIntegration:
    """Tests for CacheMonitoringIntegration class."""

    def test_get_monitor_creates_new(self) -> None:
        """Test getting a monitor creates new one."""
        integration = CacheMonitoringIntegration()
        monitor = integration.get_monitor("cache1")

        assert monitor is not None
        assert monitor.cache_name == "cache1"

    def test_get_monitor_returns_existing(self) -> None:
        """Test getting monitor returns same instance."""
        integration = CacheMonitoringIntegration()
        monitor1 = integration.get_monitor("cache1")
        monitor1.record_hit()

        monitor2 = integration.get_monitor("cache1")
        assert monitor2 is monitor1
        assert monitor2.metrics.hits == 1

    def test_record_cache_operation_hit(self) -> None:
        """Test recording hit operation."""
        integration = CacheMonitoringIntegration()
        integration.record_cache_operation("cache1", "hit", latency_ms=5.0)

        monitor = integration.get_monitor("cache1")
        assert monitor.metrics.hits == 1
        assert monitor.metrics.avg_hit_latency_ms == 5.0

    def test_record_cache_operation_miss(self) -> None:
        """Test recording miss operation."""
        integration = CacheMonitoringIntegration()
        integration.record_cache_operation("cache1", "miss", latency_ms=20.0)

        monitor = integration.get_monitor("cache1")
        assert monitor.metrics.misses == 1
        assert monitor.metrics.avg_miss_latency_ms == 20.0

    def test_record_cache_operation_error(self) -> None:
        """Test recording error operation."""
        integration = CacheMonitoringIntegration()
        integration.record_cache_operation("cache1", "error", success=False)

        monitor = integration.get_monitor("cache1")
        assert monitor.metrics.errors == 1

    def test_get_all_metrics(self) -> None:
        """Test getting all metrics from all caches."""
        integration = CacheMonitoringIntegration()
        integration.record_cache_operation("cache1", "hit")
        integration.record_cache_operation("cache2", "miss")

        all_metrics = integration.get_all_metrics()

        assert len(all_metrics) == 2
        assert all_metrics["cache1"].hits == 1
        assert all_metrics["cache2"].misses == 1

    def test_get_metrics_dict(self) -> None:
        """Test getting metrics as dictionaries."""
        integration = CacheMonitoringIntegration()
        integration.record_cache_operation("cache1", "hit")
        integration.record_cache_operation("cache1", "miss")

        metrics_dict = integration.get_metrics_dict()

        assert "cache1" in metrics_dict
        assert metrics_dict["cache1"]["hits"] == 1
        assert metrics_dict["cache1"]["misses"] == 1
        assert metrics_dict["cache1"]["hit_rate_percent"] == 50.0

    def test_reset_all(self) -> None:
        """Test resetting all monitors."""
        integration = CacheMonitoringIntegration()
        integration.record_cache_operation("cache1", "hit")
        integration.record_cache_operation("cache2", "miss")

        integration.reset_all()

        all_metrics = integration.get_all_metrics()
        assert all_metrics["cache1"].hits == 0
        assert all_metrics["cache1"].misses == 0
        assert all_metrics["cache2"].hits == 0
        assert all_metrics["cache2"].misses == 0


class TestGlobalMonitoring:
    """Tests for global cache monitoring instance."""

    def test_get_cache_monitoring(self) -> None:
        """Test getting global cache monitoring instance."""
        # Reset to ensure clean state
        set_cache_monitoring(CacheMonitoringIntegration())

        monitoring = get_cache_monitoring()
        assert monitoring is not None
        assert isinstance(monitoring, CacheMonitoringIntegration)

    def test_set_cache_monitoring(self) -> None:
        """Test setting global cache monitoring instance."""
        custom_monitoring = CacheMonitoringIntegration()
        custom_monitoring.record_cache_operation("test", "hit")

        set_cache_monitoring(custom_monitoring)

        monitoring = get_cache_monitoring()
        assert monitoring is custom_monitoring
        assert monitoring.get_monitor("test").metrics.hits == 1

    def test_get_cache_monitoring_returns_same_instance(self) -> None:
        """Test that get_cache_monitoring returns same instance."""
        set_cache_monitoring(CacheMonitoringIntegration())

        monitoring1 = get_cache_monitoring()
        monitoring1.record_cache_operation("cache1", "hit")

        monitoring2 = get_cache_monitoring()
        assert monitoring2 is monitoring1
        assert monitoring2.get_monitor("cache1").metrics.hits == 1


class TestCacheMonitoringScenarios:
    """Integration tests for realistic cache monitoring scenarios."""

    def test_typical_cache_workflow(self) -> None:
        """Test typical cache workflow with hits, misses, and errors."""
        monitor = CacheMonitor("query_cache")

        # Simulate cache operations
        monitor.record_hit(5.0)  # Hit
        monitor.record_hit(4.5)  # Hit
        monitor.record_miss(25.0)  # Miss
        monitor.record_error()  # Error
        monitor.record_hit(5.5)  # Hit
        monitor.record_miss(30.0)  # Miss

        metrics = monitor.get_metrics()

        assert metrics.hits == 3
        assert metrics.misses == 2
        assert metrics.errors == 1
        assert metrics.hit_rate == pytest.approx(60.0, abs=0.1)
        assert metrics.avg_hit_latency_ms == pytest.approx(5.0, abs=0.1)

    def test_multi_cache_monitoring(self) -> None:
        """Test monitoring multiple caches simultaneously."""
        integration = CacheMonitoringIntegration()

        # Result cache operations
        integration.record_cache_operation("result_cache", "hit", latency_ms=2.0)
        integration.record_cache_operation("result_cache", "hit", latency_ms=2.5)
        integration.record_cache_operation("result_cache", "miss", latency_ms=100.0)

        # Query plan cache operations
        integration.record_cache_operation("plan_cache", "hit", latency_ms=0.5)
        integration.record_cache_operation("plan_cache", "miss", latency_ms=5.0)

        metrics = integration.get_all_metrics()

        result_metrics = metrics["result_cache"]
        assert result_metrics.hits == 2
        assert result_metrics.misses == 1
        assert result_metrics.avg_hit_latency_ms == pytest.approx(2.25)

        plan_metrics = metrics["plan_cache"]
        assert plan_metrics.hits == 1
        assert plan_metrics.misses == 1
        assert plan_metrics.avg_hit_latency_ms == 0.5

    def test_cache_memory_tracking(self) -> None:
        """Test tracking cache memory usage."""
        monitor = CacheMonitor("memory_cache")

        # Simulate cache growth
        monitor.set_effective_entries(10)
        monitor.set_memory_usage(10000)

        monitor.set_effective_entries(20)
        monitor.set_memory_usage(20000)

        metrics = monitor.get_metrics()

        assert metrics.effective_entries == 20
        assert metrics.memory_bytes == 20000
        assert metrics.bytes_per_entry == 1000.0

    def test_cache_eviction_tracking(self) -> None:
        """Test tracking cache evictions and TTL expirations."""
        monitor = CacheMonitor("eviction_cache")

        # Simulate capacity evictions
        monitor.record_eviction(5)
        monitor.set_effective_entries(95)

        # Simulate TTL expirations
        monitor.record_ttl_expiration(10)
        monitor.set_effective_entries(85)

        metrics = monitor.get_metrics()

        assert metrics.evictions == 5
        assert metrics.ttl_expirations == 10
        assert metrics.effective_entries == 85

    def test_latency_history_limit(self) -> None:
        """Test that latency history respects max size limit."""
        monitor = CacheMonitor("latency_cache")

        # Record more hits than max history
        for i in range(1500):
            monitor.record_hit(float(i))

        metrics = monitor.get_metrics()

        # Should still calculate average correctly
        # Last 1000 values: 500-1499, average = 999.5
        assert len(monitor._hit_latencies) <= 1000
        assert metrics.hits == 1500

    def test_monitoring_integration_with_dict(self) -> None:
        """Test getting metrics as dict for JSON serialization."""
        integration = CacheMonitoringIntegration()
        integration.record_cache_operation("cache", "hit", latency_ms=5.0)
        integration.record_cache_operation("cache", "hit", latency_ms=5.0)
        integration.record_cache_operation("cache", "miss", latency_ms=50.0)

        metrics_dict = integration.get_metrics_dict()["cache"]

        # Verify it's a proper dict with expected keys
        assert isinstance(metrics_dict, dict)
        assert metrics_dict["hits"] == 2
        assert metrics_dict["misses"] == 1
        assert metrics_dict["hit_rate_percent"] == pytest.approx(66.67, abs=0.1)
        assert "total_operations" in metrics_dict
        assert "error_rate_percent" in metrics_dict
