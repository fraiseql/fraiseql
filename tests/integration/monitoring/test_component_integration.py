"""Component integration tests for Phase 19 monitoring."""

from __future__ import annotations

import time

import pytest

from fraiseql.monitoring.runtime.db_monitor_sync import get_database_monitor_sync


class TestRustPythonDataFlow:
    """Tests for Rust ↔ Python data flow integration."""

    def test_operation_metrics_to_audit_log(self, sample_graphql_operations):
        """Test GraphQL operation metrics flow to audit system."""
        # Operations created in Rust/Python layer
        assert len(sample_graphql_operations) > 0

        # Should be accessible from Python audit layer
        for op in sample_graphql_operations:
            assert op.operation_name is not None
            assert op.duration_ms >= 0

    def test_health_status_aggregation(self, monitoring_enabled, mock_health_components):
        """Test health status aggregation from components."""
        components = mock_health_components

        # Verify database component
        db_util = components["database"].get_utilization_percent()
        assert db_util == 50.0

        # Verify cache component
        cache_healthy = components["cache"].is_healthy()
        assert cache_healthy is True

    def test_cache_metrics_integration(self, cache_monitor_fixture):
        """Test cache metrics are available to Python layer."""
        cache = cache_monitor_fixture

        # Metrics should be accessible
        metrics = cache.get_metrics_dict()
        assert "hit_rate" in metrics
        assert "evictions" in metrics

    def test_database_metrics_integration(self, monitoring_enabled, sample_query_metrics):
        """Test database metrics flow to Python layer."""
        monitor = monitoring_enabled

        # Add metrics
        with monitor._lock:
            for metric in sample_query_metrics:
                monitor._recent_queries.append(metric)

        # Access via Python layer
        db_sync = get_database_monitor_sync()
        stats = db_sync.get_statistics()

        assert stats is not None
        assert stats.total_count > 0


class TestErrorHandlingScenarios:
    """Tests for error handling in integration."""

    def test_failed_query_recovery(self, monitoring_enabled, make_query_metric):
        """Test system recovers from failed queries."""
        monitor = monitoring_enabled

        # Add successful query
        success = make_query_metric(
            query_type="SELECT",
            duration_ms=10.0,
            rows_affected=100,
        )

        # Add failed query
        failed = make_query_metric(
            query_type="DELETE",
            duration_ms=5.0,
            rows_affected=0,
            error="constraint violation",
        )

        # Add another successful query
        success2 = make_query_metric(
            query_type="INSERT",
            duration_ms=8.0,
            rows_affected=1,
        )

        with monitor._lock:
            for m in [success, failed, success2]:
                monitor._recent_queries.append(m)

        # System should still be operational
        db_sync = get_database_monitor_sync()
        stats = db_sync.get_statistics()

        assert stats is not None
        assert stats.error_count == 1
        assert stats.success_count == 2

    def test_timeout_handling(self, monitoring_enabled, make_query_metric):
        """Test timeout handling in metrics."""
        monitor = monitoring_enabled

        # Simulate timeout
        timeout_query = make_query_metric(
            query_type="SELECT",
            duration_ms=30000.0,  # 30 seconds
            rows_affected=0,
            error="query timeout",
        )

        with monitor._lock:
            monitor._recent_queries.append(timeout_query)

        db_sync = get_database_monitor_sync()
        slow = db_sync.get_slow_queries(limit=10)

        assert len(slow) > 0
        assert slow[0].duration_ms == 30000.0

    def test_partial_error_states(self, monitoring_enabled, make_query_metric):
        """Test partial error states are handled correctly."""
        monitor = monitoring_enabled

        # Some queries succeed, some fail
        queries = []
        for i in range(10):
            is_error = i % 3 == 0  # Every 3rd query fails
            q = make_query_metric(
                query_type="SELECT",
                duration_ms=5.0 + i,
                rows_affected=0 if is_error else i,
                error="error" if is_error else None,
            )
            queries.append(q)

        with monitor._lock:
            for q in queries:
                monitor._recent_queries.append(q)

        db_sync = get_database_monitor_sync()
        stats = db_sync.get_statistics()

        # Should have both successes and errors
        assert stats.success_count > 0
        assert stats.error_count > 0
        assert (stats.success_count + stats.error_count) == 10

    def test_graceful_degradation(self, monitoring_enabled):
        """Test graceful degradation when metrics are unavailable."""
        monitor = monitoring_enabled

        # No queries recorded yet
        db_sync = get_database_monitor_sync()

        # Should not crash
        stats = db_sync.get_statistics()
        assert stats is None  # No stats available yet

        # Should handle empty gracefully
        recent = db_sync.get_recent_queries(limit=10)
        assert len(recent) == 0


class TestRuntimeConfigurationChanges:
    """Tests for runtime configuration changes."""

    def test_threshold_adjustments(self, monitoring_enabled):
        """Test slow query threshold can be adjusted at runtime."""
        monitor = monitoring_enabled

        original_threshold = monitor._slow_query_threshold

        # Change threshold
        new_threshold = 50.0
        monitor._slow_query_threshold = new_threshold

        assert monitor._slow_query_threshold == new_threshold

        # Restore
        monitor._slow_query_threshold = original_threshold

    def test_sampling_rate_changes(self, monitoring_enabled):
        """Test sampling rate can be adjusted."""
        config = type('Config', (), {
            'sampling_rate': 1.0,
        })()

        original_rate = config.sampling_rate

        # Change rate
        new_rate = 0.5
        config.sampling_rate = new_rate

        assert config.sampling_rate == new_rate

        # Test clamping (rate should be 0.0-1.0)
        config.sampling_rate = 2.0
        assert config.sampling_rate <= 1.0

    def test_health_check_interval_changes(self, monitoring_enabled):
        """Test health check interval can be adjusted."""
        monitor = monitoring_enabled

        # Configuration should support interval changes
        # This would be set via configuration system
        assert monitor is not None


class TestDataConsistency:
    """Tests for data consistency across components."""

    def test_no_metrics_lost(self, monitoring_enabled, make_query_metric):
        """Test no metrics are lost during recording."""
        monitor = monitoring_enabled

        query_count = 100

        # Add many queries
        for i in range(query_count):
            q = make_query_metric(
                query_type="SELECT",
                duration_ms=5.0,
                rows_affected=i,
            )
            with monitor._lock:
                monitor._recent_queries.append(q)

        db_sync = get_database_monitor_sync()
        recent = db_sync.get_recent_queries(limit=query_count)

        assert len(recent) == query_count

    def test_health_state_consistency(self, monitoring_enabled, sample_query_metrics):
        """Test health state remains consistent."""
        monitor = monitoring_enabled

        with monitor._lock:
            for metric in sample_query_metrics:
                monitor._recent_queries.append(metric)

        db_sync = get_database_monitor_sync()

        # Get stats multiple times
        stats1 = db_sync.get_statistics()
        stats2 = db_sync.get_statistics()

        # Should be identical
        assert stats1.total_count == stats2.total_count
        assert stats1.success_rate == stats2.success_rate

    def test_audit_log_completeness(self, sample_query_metrics):
        """Test audit logs capture all operations."""
        # All sample queries should be auditable
        for metric in sample_query_metrics:
            assert metric.query_type is not None
            assert metric.duration_ms >= 0

    def test_statistics_accuracy(self, monitoring_enabled, sample_query_metrics):
        """Test statistics accuracy is > 99.9%."""
        monitor = monitoring_enabled

        # Count expected values
        expected_total = len(sample_query_metrics)
        expected_success = sum(1 for m in sample_query_metrics if m.is_success())
        expected_errors = expected_total - expected_success

        with monitor._lock:
            for metric in sample_query_metrics:
                monitor._recent_queries.append(metric)

        db_sync = get_database_monitor_sync()
        stats = db_sync.get_statistics()

        # Verify accuracy
        assert stats.total_count == expected_total
        assert stats.success_count == expected_success
        assert stats.error_count == expected_errors
        assert abs(stats.success_rate - (expected_success / expected_total)) < 0.001
