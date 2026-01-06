"""Unit tests for AuditLogQueryBuilder.

Tests the query builder for audit logs, including:
- Basic query methods (recent_operations, by_user, by_entity, etc.)
- Chainable filters
- Pagination
- Aggregations
- Report generation
- Export functionality
"""

import tempfile
from datetime import UTC, datetime, timedelta

import pytest

from fraiseql.audit import (
    AuditEvent,
    AuditLogQueryBuilder,
    ComplianceReport,
    EventStats,
    OperationType,
)


@pytest.fixture
def sample_events() -> list[AuditEvent]:
    """Create sample audit events for testing."""
    now = datetime.now(UTC)
    return [
        # GraphQL operations
        AuditEvent(
            id="op-1",
            timestamp=now - timedelta(hours=1),
            event_type="query",
            user_id="user-1",
            result="success",
            duration_ms=50.0,
            error_count=0,
            field_count=5,
        ),
        AuditEvent(
            id="op-2",
            timestamp=now - timedelta(hours=2),
            event_type="mutation",
            user_id="user-1",
            result="success",
            duration_ms=200.0,
            error_count=0,
            field_count=10,
        ),
        AuditEvent(
            id="op-3",
            timestamp=now - timedelta(hours=3),
            event_type="query",
            user_id="user-2",
            result="error",
            duration_ms=5000.0,
            error_count=1,
            slow=True,
        ),
        AuditEvent(
            id="op-4",
            timestamp=now - timedelta(hours=24),
            event_type="mutation",
            user_id="user-1",
            result="error",
            duration_ms=300.0,
            error_count=2,
        ),
        # Security events
        AuditEvent(
            id="sec-1",
            timestamp=now - timedelta(minutes=30),
            event_type="auth.success",
            user_id="user-1",
            user_email="user1@example.com",
            ip_address="192.168.1.100",
            result="success",
            metadata={"severity": "info"},
        ),
        AuditEvent(
            id="sec-2",
            timestamp=now - timedelta(minutes=20),
            event_type="auth.failure",
            ip_address="10.0.0.50",
            result="error",
            reason="Invalid credentials",
            metadata={"severity": "warning"},
        ),
        AuditEvent(
            id="sec-3",
            timestamp=now - timedelta(minutes=10),
            event_type="authz.denied",
            user_id="user-3",
            resource="Project:proj-123",
            action="delete",
            result="denied",
            metadata={"severity": "warning"},
        ),
        AuditEvent(
            id="res-1",
            timestamp=now - timedelta(hours=5),
            event_type="data.access",
            user_id="user-2",
            resource="Project:proj-123",
            action="read",
            result="success",
            metadata={"severity": "info"},
        ),
    ]


class TestAuditLogQueryBuilderBasics:
    """Tests for basic builder functionality."""

    async def test_builder_initialization(self, sample_events) -> None:
        """Builder initializes with clean state."""
        builder = AuditLogQueryBuilder(sample_events)
        assert builder._filters == {}
        assert builder._limit is None
        assert builder._offset == 0
        assert builder._order_by == "timestamp"
        assert builder._order_desc is True

    async def test_recent_operations_basic(self, sample_events) -> None:
        """recent_operations() returns recent ops."""
        builder = AuditLogQueryBuilder(sample_events)
        ops = await builder.recent_operations(limit=10)
        assert len(ops) <= 10
        assert all(isinstance(op, AuditEvent) for op in ops)
        assert all(op.is_operational_event() for op in ops)

    async def test_recent_operations_respects_limit(self, sample_events) -> None:
        """recent_operations() respects limit parameter."""
        builder = AuditLogQueryBuilder(sample_events)
        ops = await builder.recent_operations(limit=2)
        assert len(ops) == 2

    async def test_recent_operations_with_type_filter(self, sample_events) -> None:
        """recent_operations() filters by operation type."""
        builder = AuditLogQueryBuilder(sample_events)
        mutations = await builder.recent_operations(
            operation_type=OperationType.MUTATION,
            limit=20,
        )
        assert all(op.event_type == "mutation" for op in mutations)

    async def test_by_user_returns_user_events(self, sample_events) -> None:
        """by_user() returns all events for user."""
        builder = AuditLogQueryBuilder(sample_events)
        ops = await builder.by_user("user-1", hours=48)
        assert all(op.user_id == "user-1" for op in ops)
        assert len(ops) >= 3  # user-1 has 3 operational + 1 security event

    async def test_by_user_respects_time_window(self, sample_events) -> None:
        """by_user() respects hours parameter."""
        builder = AuditLogQueryBuilder(sample_events)
        ops = await builder.by_user("user-1", hours=1)
        # Should only get events from last hour
        now = datetime.now(UTC)
        cutoff = now - timedelta(hours=1)
        assert all(op.timestamp >= cutoff for op in ops)

    async def test_by_entity_filters_correctly(self, sample_events) -> None:
        """by_entity() filters by resource."""
        builder = AuditLogQueryBuilder(sample_events)
        ops = await builder.by_entity("Project", "proj-123")
        assert all("proj-123" in op.resource for op in ops if op.resource)
        assert len(ops) >= 2  # proj-123 has at least 2 events

    async def test_failed_operations_returns_errors(self, sample_events) -> None:
        """failed_operations() returns error events."""
        builder = AuditLogQueryBuilder(sample_events)
        ops = await builder.failed_operations(hours=24)
        assert all(op.is_error() for op in ops)
        assert len(ops) >= 2  # Should have error events

    async def test_by_event_type_filters(self, sample_events) -> None:
        """by_event_type() filters by security event type."""
        builder = AuditLogQueryBuilder(sample_events)
        ops = await builder.by_event_type("auth.failure")
        assert all(op.event_type == "auth.failure" for op in ops)

    async def test_by_severity_filters(self, sample_events) -> None:
        """by_severity() filters by severity level."""
        builder = AuditLogQueryBuilder(sample_events)
        ops = await builder.by_severity("warning")
        assert all(op.metadata.get("severity") == "warning" for op in ops)


class TestChaining:
    """Tests for chainable filter methods."""

    async def test_filter_by_date_range_chaining(self, sample_events) -> None:
        """filter_by_date_range() returns self."""
        builder = AuditLogQueryBuilder(sample_events)
        now = datetime.now(UTC)
        result = builder.filter_by_date_range(now - timedelta(days=1), now)
        assert result is builder

    async def test_limit_returns_self(self, sample_events) -> None:
        """limit() returns self for chaining."""
        builder = AuditLogQueryBuilder(sample_events)
        result = builder.limit(10)
        assert result is builder

    async def test_offset_returns_self(self, sample_events) -> None:
        """offset() returns self for chaining."""
        builder = AuditLogQueryBuilder(sample_events)
        result = builder.offset(5)
        assert result is builder

    async def test_order_by_returns_self(self, sample_events) -> None:
        """order_by() returns self for chaining."""
        builder = AuditLogQueryBuilder(sample_events)
        result = builder.order_by("timestamp")
        assert result is builder

    async def test_filter_by_status_returns_self(self, sample_events) -> None:
        """filter_by_status() returns self for chaining."""
        builder = AuditLogQueryBuilder(sample_events)
        result = builder.filter_by_status("error")
        assert result is builder

    async def test_chaining_multiple_filters(self, sample_events) -> None:
        """Multiple filters can be chained."""
        builder = AuditLogQueryBuilder(sample_events)
        now = datetime.now(UTC)
        ops = await (
            builder.filter_by_date_range(now - timedelta(days=1), now)
            .filter_by_status("error")
            .limit(50)
            .recent_operations()
        )
        assert len(ops) <= 50
        assert all(op.is_error() for op in ops)


class TestPagination:
    """Tests for pagination."""

    async def test_limit_constrains_results(self, sample_events) -> None:
        """limit() constrains results."""
        builder = AuditLogQueryBuilder(sample_events)
        ops = await builder.limit(2).recent_operations()
        assert len(ops) <= 2

    async def test_offset_skips_results(self, sample_events) -> None:
        """offset() skips results."""
        builder1 = AuditLogQueryBuilder(sample_events)
        builder2 = AuditLogQueryBuilder(sample_events)

        all_ops = await builder1.recent_operations()
        offset_ops = await builder2.offset(2).recent_operations()

        # Should have fewer results with offset
        assert len(offset_ops) <= len(all_ops)

    async def test_limit_and_offset_together(self, sample_events) -> None:
        """limit() and offset() work together."""
        builder = AuditLogQueryBuilder(sample_events)
        ops = await builder.offset(1).limit(2).recent_operations()
        assert len(ops) <= 2


class TestAggregations:
    """Tests for aggregation methods."""

    async def test_count_returns_total(self, sample_events) -> None:
        """count() returns total matching events."""
        builder = AuditLogQueryBuilder(sample_events)
        count = await builder.count()
        assert count == len(sample_events)

    async def test_count_with_filter(self, sample_events) -> None:
        """count() respects filters."""
        builder = AuditLogQueryBuilder(sample_events)
        count = await builder.filter_by_status("error").count()
        assert count == sum(1 for e in sample_events if e.is_error())

    async def test_get_statistics_returns_stats(self, sample_events) -> None:
        """get_statistics() returns EventStats."""
        builder = AuditLogQueryBuilder(sample_events)
        stats = await builder.get_statistics()
        assert isinstance(stats, EventStats)
        assert stats.total_count == len(sample_events)
        assert stats.error_count > 0
        assert 0.0 <= stats.error_rate <= 1.0

    async def test_statistics_percentiles(self, sample_events) -> None:
        """get_statistics() calculates percentiles correctly."""
        builder = AuditLogQueryBuilder(sample_events)
        stats = await builder.get_statistics()
        # P50 should be between min and max
        operations = [e for e in sample_events if e.duration_ms]
        if operations:
            durations = sorted(e.duration_ms for e in operations)
            assert min(durations) <= stats.p50_duration_ms <= max(durations)


class TestReports:
    """Tests for report generation."""

    async def test_compliance_report_generation(self, sample_events) -> None:
        """compliance_report() generates valid report."""
        builder = AuditLogQueryBuilder(sample_events)
        now = datetime.now(UTC)
        report = await builder.compliance_report(
            start_date=now - timedelta(days=1),
            end_date=now,
        )
        assert isinstance(report, ComplianceReport)
        assert report.total_events > 0
        assert report.critical_events <= report.total_events

    async def test_compliance_report_aggregates_correctly(self, sample_events) -> None:
        """compliance_report() aggregates statistics."""
        builder = AuditLogQueryBuilder(sample_events)
        now = datetime.now(UTC)
        report = await builder.compliance_report(
            start_date=now - timedelta(days=2),
            end_date=now,
        )
        # Check that breakdown matches total
        assert sum(report.events_by_type.values()) == report.total_events

    async def test_compliance_report_includes_failed_ops(self, sample_events) -> None:
        """compliance_report() includes failed operations."""
        builder = AuditLogQueryBuilder(sample_events)
        now = datetime.now(UTC)
        report = await builder.compliance_report(
            start_date=now - timedelta(days=2),
            end_date=now,
        )
        assert len(report.failed_operations) > 0
        assert all(op.is_error() for op in report.failed_operations)


class TestExport:
    """Tests for export functionality."""

    async def test_export_csv(self, sample_events) -> None:
        """export_csv() creates valid CSV file."""
        builder = AuditLogQueryBuilder(sample_events)

        with tempfile.NamedTemporaryFile(mode="w", suffix=".csv", delete=False) as f:
            filepath = f.name

        await builder.export_csv(filepath)

        # Verify file was created and has content
        with open(filepath) as f:  # noqa: ASYNC230, PTH123
            content = f.read()
            assert "timestamp" in content
            assert "event_type" in content

    async def test_export_json(self, sample_events) -> None:
        """export_json() creates valid JSON file."""
        builder = AuditLogQueryBuilder(sample_events)

        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            filepath = f.name

        await builder.export_json(filepath)

        # Verify file was created and has valid JSON
        import json

        with open(filepath) as f:  # noqa: ASYNC230, PTH123
            data = json.load(f)
            assert isinstance(data, list)
            assert len(data) > 0


class TestEdgeCases:
    """Tests for edge cases."""

    async def test_empty_events_list(self) -> None:
        """Builder handles empty events list."""
        builder = AuditLogQueryBuilder([])
        ops = await builder.recent_operations()
        assert ops == []

    async def test_recent_operations_empty_result(self) -> None:
        """recent_operations() handles no matches."""
        # Create events that are all security events (not operations)
        security_only = [
            AuditEvent(
                id="sec-1",
                timestamp=datetime.now(UTC),
                event_type="auth.success",
                result="success",
            )
        ]
        builder = AuditLogQueryBuilder(security_only)
        ops = await builder.recent_operations()
        assert ops == []

    async def test_by_user_no_match(self, sample_events) -> None:
        """by_user() handles non-existent user."""
        builder = AuditLogQueryBuilder(sample_events)
        ops = await builder.by_user("user-nonexistent")
        assert ops == []

    async def test_statistics_with_no_duration(self, sample_events) -> None:
        """get_statistics() handles events without duration."""
        events = [e for e in sample_events if not e.duration_ms]
        builder = AuditLogQueryBuilder(events)
        stats = await builder.get_statistics()
        assert stats.total_count > 0
        assert stats.avg_duration_ms == 0.0

    async def test_compliance_report_summary_string(self, sample_events) -> None:
        """ComplianceReport.get_summary_string() produces valid string."""
        builder = AuditLogQueryBuilder(sample_events)
        now = datetime.now(UTC)
        report = await builder.compliance_report(
            start_date=now - timedelta(days=1),
            end_date=now,
        )
        summary = report.get_summary_string()
        assert "Total Events" in summary
        assert "Error Rate" in summary
