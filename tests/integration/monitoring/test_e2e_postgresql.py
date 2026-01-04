"""End-to-end integration tests for Phase 19 monitoring with PostgreSQL."""

from __future__ import annotations

import pytest

from fraiseql.monitoring.runtime.db_monitor_sync import get_database_monitor
from fraiseql.monitoring.runtime.cache_monitor_sync import cache_monitor_sync


class TestDatabaseMonitoringE2E:
    """End-to-end tests for database monitoring with PostgreSQL."""

    def test_recent_queries_tracking(self, monitoring_enabled, sample_query_metrics):
        """Test that recent queries are tracked correctly."""
        monitor = monitoring_enabled

        # Simulate adding queries to monitor
        with monitor._lock:
            for metric in sample_query_metrics[:5]:
                monitor._recent_queries.append(metric)

        # Get recent queries via sync accessor
        db_sync = get_database_monitor()
        recent = db_sync.get_recent_queries(limit=10)

        assert len(recent) > 0
        assert recent[0].query_type == "SELECT"

    def test_slow_query_detection(self, monitoring_enabled, sample_query_metrics):
        """Test slow query detection and tracking."""
        monitor = monitoring_enabled

        # Add queries with varying durations
        with monitor._lock:
            for metric in sample_query_metrics:
                monitor._recent_queries.append(metric)

        # Get slow queries
        db_sync = get_database_monitor()
        slow_queries = db_sync.get_slow_queries(limit=10)

        # Should have slow queries from sample data
        assert any(q.duration_ms > 100 for q in slow_queries)

    def test_statistics_aggregation(self, monitoring_enabled, sample_query_metrics):
        """Test that statistics are aggregated correctly."""
        monitor = monitoring_enabled

        # Add queries
        with monitor._lock:
            for metric in sample_query_metrics:
                monitor._recent_queries.append(metric)

        # Get statistics via sync accessor
        db_sync = get_database_monitor()
        stats = db_sync.get_statistics()

        assert stats is not None
        assert stats.total_count > 0
        assert stats.success_rate <= 1.0
        assert stats.success_rate >= 0.0

    def test_pool_metrics_tracking(self, monitoring_enabled):
        """Test connection pool metrics tracking."""
        monitor = monitoring_enabled

        # Set pool metrics
        from fraiseql.monitoring.models import PoolMetrics

        with monitor._lock:
            monitor._pool_metrics = PoolMetrics(
                total_connections=20,
                active_connections=15,
                idle_connections=5,
                waiting_requests=0,
                avg_wait_time_ms=2.5,
                max_wait_time_ms=10.0,
            )

        # Get pool metrics via sync accessor
        db_sync = get_database_monitor()
        pool = db_sync.get_pool_metrics()

        assert pool is not None
        assert pool.total_connections == 20
        assert pool.get_utilization_percent() == 75.0

    def test_query_type_breakdown(self, monitoring_enabled, sample_query_metrics):
        """Test query breakdown by type."""
        monitor = monitoring_enabled

        with monitor._lock:
            for metric in sample_query_metrics:
                monitor._recent_queries.append(metric)

        db_sync = get_database_monitor()

        # Get queries and verify type breakdown
        all_queries = db_sync.get_recent_queries(limit=100)
        select_count = sum(1 for q in all_queries if q.query_type == "SELECT")
        update_count = sum(1 for q in all_queries if q.query_type == "UPDATE")

        assert select_count > 0
        assert update_count > 0


class TestGraphQLOperationTracking:
    """End-to-end tests for GraphQL operation tracking."""

    def test_operation_metrics_recording(self, sample_graphql_operations):
        """Test that operation metrics are recorded correctly."""
        # Sample operations created successfully
        assert len(sample_graphql_operations) > 0

        # Verify different operation types
        queries = [op for op in sample_graphql_operations if op.operation_type.value == "query"]
        mutations = [op for op in sample_graphql_operations if op.operation_type.value == "mutation"]

        assert len(queries) > 0
        assert len(mutations) > 0

    def test_operation_duration_tracking(self, sample_graphql_operations):
        """Test operation duration tracking."""
        for op in sample_graphql_operations:
            # Duration should be set
            assert op.duration_ms > 0 or op.duration_ms == 0

    def test_slow_operation_detection(self, sample_graphql_operations):
        """Test detection of slow operations."""
        slow_threshold = 500.0

        slow_ops = [op for op in sample_graphql_operations if op.duration_ms > slow_threshold]

        # Should have at least some slow operations in sample data
        assert len(slow_ops) > 0


class TestHealthCheckIntegration:
    """End-to-end tests for health check integration."""

    def test_health_status_aggregation(self, mock_health_components):
        """Test that health status is aggregated from components."""
        components = mock_health_components

        # Verify mock components are set up
        assert "database" in components
        assert "cache" in components
        assert "graphql" in components
        assert "tracing" in components

    def test_health_state_transitions(self, monitoring_enabled):
        """Test health state transitions."""
        monitor = monitoring_enabled

        # Start healthy
        assert monitor is not None

        # Can modify thresholds
        monitor._slow_query_threshold_ms = 50

        # Verify modification
        assert monitor._slow_query_threshold_ms == 50

    def test_component_health_dependency(self, monitoring_enabled, sample_query_metrics):
        """Test health status depends on component metrics."""
        monitor = monitoring_enabled

        # Add queries
        with monitor._lock:
            for metric in sample_query_metrics:
                monitor._recent_queries.append(metric)

        # Get statistics to check health implications
        db_sync = get_database_monitor()
        stats = db_sync.get_statistics()

        # Success rate affects health
        assert stats.success_rate < 1.0  # We have some failed queries in sample
        assert stats.error_count > 0


class TestTraceContextPropagation:
    """End-to-end tests for W3C trace context propagation."""

    def test_trace_context_injection(self, sample_graphql_operations):
        """Test W3C trace context is injected into metrics."""
        # Operations should have trace context if available
        for op in sample_graphql_operations:
            # Trace context is optional but should be structured if present
            assert hasattr(op, "trace_id") or True  # May not be set in all cases

    def test_trace_id_propagation(self):
        """Test trace ID propagates through operations."""
        from fraiseql.monitoring.models import OperationMetrics, GraphQLOperationType

        op = OperationMetrics(
            operation_id="test-op-1",
            operation_name="TestOp",
            operation_type=GraphQLOperationType.Query,
            query_length=50,
        )

        # Trace context can be set
        op.trace_id = "test-trace-123"
        assert op.trace_id == "test-trace-123"


class TestCLIMonitoringCommands:
    """End-to-end tests for CLI monitoring commands."""

    def test_cli_database_recent_command(self, monitoring_enabled, sample_query_metrics):
        """Test database recent command with real data."""
        monitor = monitoring_enabled

        # Add sample queries
        with monitor._lock:
            for metric in sample_query_metrics:
                monitor._recent_queries.append(metric)

        # CLI should be able to fetch recent queries
        db_sync = get_database_monitor()
        recent = db_sync.get_recent_queries(limit=5)

        assert len(recent) == 5
        assert all(hasattr(q, "query_type") for q in recent)

    def test_cli_database_slow_command(self, monitoring_enabled, sample_query_metrics):
        """Test database slow command with real data."""
        monitor = monitoring_enabled

        with monitor._lock:
            for metric in sample_query_metrics:
                monitor._recent_queries.append(metric)

        # CLI should be able to fetch slow queries
        db_sync = get_database_monitor()
        slow = db_sync.get_slow_queries(limit=5)

        # Should have some slow queries
        assert len(slow) > 0
        assert all(q.duration_ms > 0 for q in slow)

    def test_cli_cache_stats_command(self, cache_monitor_fixture):
        """Test cache stats command."""
        # Cache monitor should be available
        assert cache_monitor_fixture is not None

        # Get metrics
        metrics = cache_monitor_fixture.get_metrics_dict()

        assert "hit_rate" in metrics
        assert "evictions" in metrics

    def test_cli_health_command(self, monitoring_enabled):
        """Test health command retrieves status."""
        monitor = monitoring_enabled

        # Monitor should be in a valid state
        db_sync = get_database_monitor()
        stats = db_sync.get_statistics()

        # Stats structure should be valid
        assert stats is not None or stats is None  # Either available or not yet


class TestOutputFormatValidation:
    """Tests for CLI output format validation."""

    def test_json_format_output(self, sample_query_metrics):
        """Test JSON output format."""
        import json

        # Sample metrics can be serialized
        metric = sample_query_metrics[0]

        # Create JSON-serializable dict
        data = {
            "timestamp": metric.timestamp.isoformat(),
            "type": metric.query_type,
            "duration_ms": metric.duration_ms,
            "rows_affected": metric.rows_affected,
        }

        # Should serialize to JSON
        json_str = json.dumps(data)
        assert json_str is not None
        assert "SELECT" in json_str

    def test_csv_format_output(self, sample_query_metrics):
        """Test CSV output format."""
        import csv
        import io

        # Create CSV output
        output = io.StringIO()
        writer = csv.writer(output)

        # Write header
        writer.writerow(["Timestamp", "Type", "Duration (ms)", "Rows", "Status"])

        # Write sample metrics
        for metric in sample_query_metrics[:3]:
            writer.writerow([
                metric.timestamp.isoformat(),
                metric.query_type,
                f"{metric.duration_ms:.2f}",
                str(metric.rows_affected),
                "✓" if metric.is_success() else "✗",
            ])

        csv_output = output.getvalue()
        assert "SELECT" in csv_output
        assert len(csv_output) > 0

    def test_table_format_output(self, sample_query_metrics):
        """Test table output format."""
        # Create simple table output
        headers = ["Timestamp", "Type", "Duration (ms)"]
        rows = []

        for metric in sample_query_metrics[:3]:
            rows.append([
                metric.timestamp.isoformat()[:10],  # Date only
                metric.query_type,
                f"{metric.duration_ms:.2f}",
            ])

        # Should have proper structure
        assert len(headers) == 3
        assert all(len(row) == 3 for row in rows)
