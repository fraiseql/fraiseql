"""Unit tests for AuditAnalyzer.

Tests the analysis helpers for audit logs, including:
- Suspicious activity detection
- User activity summarization
- Slow operation identification
- Error pattern analysis
- Time-based pattern analysis
"""

from datetime import UTC, datetime, timedelta

import pytest

from fraiseql.audit import AuditAnalyzer, AuditEvent, EventStats


@pytest.fixture
def sample_events() -> list[AuditEvent]:
    """Create sample audit events for testing."""
    now = datetime.now(UTC)
    return [
        # Successful operations
        AuditEvent(
            id="op-1",
            timestamp=now - timedelta(hours=1),
            event_type="query",
            user_id="user-1",
            result="success",
            duration_ms=50.0,
        ),
        # Failed auth attempts
        AuditEvent(
            id="auth-1",
            timestamp=now - timedelta(minutes=30),
            event_type="auth.failure",
            result="error",
            reason="Invalid credentials",
        ),
        AuditEvent(
            id="auth-2",
            timestamp=now - timedelta(minutes=25),
            event_type="auth.failure",
            result="error",
            reason="Invalid credentials",
        ),
        AuditEvent(
            id="auth-3",
            timestamp=now - timedelta(minutes=20),
            event_type="auth.failure",
            result="error",
            reason="Account locked",
        ),
        AuditEvent(
            id="auth-4",
            timestamp=now - timedelta(minutes=15),
            event_type="auth.failure",
            result="error",
            reason="Invalid credentials",
        ),
        AuditEvent(
            id="auth-5",
            timestamp=now - timedelta(minutes=10),
            event_type="auth.failure",
            result="error",
            reason="Invalid credentials",
        ),
        # Slow operation
        AuditEvent(
            id="slow-1",
            timestamp=now - timedelta(hours=2),
            event_type="mutation",
            user_id="user-2",
            result="success",
            duration_ms=5000.0,
            slow=True,
        ),
        # Access denied
        AuditEvent(
            id="denied-1",
            timestamp=now - timedelta(hours=3),
            event_type="authz.denied",
            user_id="user-3",
            result="denied",
            reason="Insufficient privileges",
        ),
        AuditEvent(
            id="denied-2",
            timestamp=now - timedelta(hours=3, minutes=5),
            event_type="authz.denied",
            user_id="user-3",
            result="denied",
            reason="Insufficient privileges",
        ),
        AuditEvent(
            id="denied-3",
            timestamp=now - timedelta(hours=3, minutes=10),
            event_type="authz.denied",
            user_id="user-3",
            result="denied",
            reason="Insufficient privileges",
        ),
        # Normal operations for user-1
        AuditEvent(
            id="op-2",
            timestamp=now - timedelta(hours=1),
            event_type="query",
            user_id="user-1",
            result="success",
            duration_ms=100.0,
        ),
        AuditEvent(
            id="op-3",
            timestamp=now - timedelta(hours=2),
            event_type="query",
            user_id="user-1",
            result="success",
            duration_ms=75.0,
        ),
    ]


class TestSuspiciousActivityDetection:
    """Tests for detecting suspicious activity."""

    async def test_detect_rapid_auth_failures(self, sample_events) -> None:
        """Detects rapid authentication failures."""
        analyzer = AuditAnalyzer()
        auth_events = [e for e in sample_events if "auth" in e.event_type]
        suspicious = analyzer.detect_suspicious_activity(auth_events)

        assert suspicious["rapid_auth_failures"] is not None
        assert suspicious["rapid_auth_failures"]["count"] >= 5

    async def test_detect_high_error_rate(self, sample_events) -> None:
        """Detects high error rate."""
        # Create high error rate scenario
        high_error_events = [
            e for e in sample_events if e.result == "error" and "auth" in e.event_type
        ]
        suspicious = AuditAnalyzer.detect_suspicious_activity(high_error_events)

        # All events are errors, so error rate is 1.0
        assert suspicious["high_error_rate"] is not None

    async def test_detect_privilege_escalation(self, sample_events) -> None:
        """Detects denied access attempts (privilege escalation)."""
        denied_events = [e for e in sample_events if e.is_denied()]
        suspicious = AuditAnalyzer.detect_suspicious_activity(denied_events)

        assert suspicious["privilege_escalation"] is not None
        assert suspicious["privilege_escalation"]["denied_count"] >= 3

    async def test_no_suspicious_activity_clean_events(self) -> None:
        """Returns clean dict for normal events."""
        normal_events = [
            AuditEvent(
                id="1",
                timestamp=datetime.now(UTC),
                event_type="query",
                result="success",
                duration_ms=50.0,
            )
        ]
        suspicious = AuditAnalyzer.detect_suspicious_activity(normal_events)

        assert suspicious["rapid_auth_failures"] is None
        assert suspicious["high_error_rate"] is None
        assert suspicious["privilege_escalation"] is None


class TestUserActivitySummary:
    """Tests for summarizing user activity."""

    async def test_summarize_user_activity(self, sample_events) -> None:
        """Summarizes user activity correctly."""
        user_events = [e for e in sample_events if e.user_id == "user-1"]
        stats = AuditAnalyzer.summarize_user_activity(user_events)

        assert isinstance(stats, EventStats)
        assert stats.total_count == len(user_events)
        assert stats.error_count == 0
        assert stats.error_rate == 0.0

    async def test_summary_calculates_percentiles(self, sample_events) -> None:
        """Summary calculates duration percentiles."""
        user_events = [e for e in sample_events if e.user_id == "user-2"]
        stats = AuditAnalyzer.summarize_user_activity(user_events)

        assert stats.p50_duration_ms > 0
        assert stats.avg_duration_ms > 0

    async def test_summary_identifies_most_common_action(self) -> None:
        """Summary identifies most common action."""
        events = [
            AuditEvent(
                id="1",
                timestamp=datetime.now(UTC),
                event_type="query",
                action="read",
                result="success",
            ),
            AuditEvent(
                id="2",
                timestamp=datetime.now(UTC),
                event_type="query",
                action="read",
                result="success",
            ),
            AuditEvent(
                id="3",
                timestamp=datetime.now(UTC),
                event_type="mutation",
                action="write",
                result="success",
            ),
        ]
        stats = AuditAnalyzer.summarize_user_activity(events)

        assert stats.most_common_action == "read"

    async def test_summary_empty_events(self) -> None:
        """Summary handles empty events list."""
        stats = AuditAnalyzer.summarize_user_activity([])
        assert stats.total_count == 0
        assert stats.error_rate == 0.0


class TestSlowOperationIdentification:
    """Tests for identifying slow operations."""

    async def test_identify_slow_operations(self, sample_events) -> None:
        """Identifies slow operations by percentile."""
        slow_ops = AuditAnalyzer.identify_slow_operations(
            sample_events,
            percentile=0.95,
        )

        # Should return slowest ~5%
        assert len(slow_ops) > 0
        assert all(isinstance(op, AuditEvent) for op in slow_ops)

    async def test_slow_operations_are_actually_slow(self, sample_events) -> None:
        """Identified slow operations are slower than threshold."""
        with_durations = [e for e in sample_events if e.duration_ms]
        slow_ops = AuditAnalyzer.identify_slow_operations(with_durations, percentile=0.5)

        if slow_ops:
            durations = sorted(e.duration_ms for e in with_durations)
            threshold = durations[len(durations) // 2]
            assert all(op.duration_ms >= threshold for op in slow_ops)

    async def test_slow_operations_empty_if_no_duration(self) -> None:
        """Returns empty list if no operations have duration."""
        events_no_duration = [
            AuditEvent(
                id="1",
                timestamp=datetime.now(UTC),
                event_type="query",
                result="success",
            )
        ]
        slow = AuditAnalyzer.identify_slow_operations(events_no_duration)
        assert slow == []


class TestErrorPatternAnalysis:
    """Tests for analyzing error patterns."""

    async def test_analyze_error_patterns(self, sample_events) -> None:
        """Analyzes error reasons and their frequency."""
        error_patterns = AuditAnalyzer.analyze_error_patterns(sample_events)

        assert isinstance(error_patterns, dict)
        # Should identify common error reasons
        assert "Invalid credentials" in error_patterns
        assert error_patterns["Invalid credentials"] >= 3

    async def test_error_patterns_counts(self, sample_events) -> None:
        """Error patterns have correct counts."""
        error_patterns = AuditAnalyzer.analyze_error_patterns(sample_events)

        # Verify counts are correct
        for reason, count in error_patterns.items():
            matching = sum(1 for e in sample_events if e.is_error() and e.reason == reason)
            assert matching == count

    async def test_error_patterns_empty(self) -> None:
        """Handles events with no errors."""
        no_error_events = [
            AuditEvent(
                id="1",
                timestamp=datetime.now(UTC),
                event_type="query",
                result="success",
            )
        ]
        patterns = AuditAnalyzer.analyze_error_patterns(no_error_events)
        assert patterns == {}


class TestMostActiveUsers:
    """Tests for identifying most active users."""

    async def test_identify_most_active_users(self, sample_events) -> None:
        """Identifies most active users."""
        top_users = AuditAnalyzer.identify_most_active_users(sample_events, top_n=5)

        assert isinstance(top_users, list)
        assert len(top_users) > 0
        # Should be tuples of (user_id, count)
        assert all(isinstance(item, tuple) and len(item) == 2 for item in top_users)

    async def test_users_ranked_correctly(self, sample_events) -> None:
        """Users are ranked by activity count."""
        top_users = AuditAnalyzer.identify_most_active_users(sample_events, top_n=10)

        # Counts should be in descending order
        counts = [count for _, count in top_users]
        assert counts == sorted(counts, reverse=True)


class TestResourceAnalysis:
    """Tests for analyzing resource access."""

    async def test_identify_most_active_resources(self, sample_events) -> None:
        """Identifies most frequently accessed resources."""
        sample_events.append(
            AuditEvent(
                id="res-1",
                timestamp=datetime.now(UTC),
                event_type="data.access",
                resource="Project:proj-123",
                result="success",
            )
        )
        top_resources = AuditAnalyzer.identify_most_active_resources(
            sample_events,
            top_n=5,
        )

        assert isinstance(top_resources, list)
        # Should have resources
        if top_resources:
            assert all(isinstance(item, tuple) and len(item) == 2 for item in top_resources)


class TestEventTypeDistribution:
    """Tests for event type distribution analysis."""

    async def test_get_event_type_distribution(self, sample_events) -> None:
        """Gets distribution of event types."""
        distribution = AuditAnalyzer.get_event_type_distribution(sample_events)

        assert isinstance(distribution, dict)
        assert "query" in distribution
        assert "auth.failure" in distribution
        # Counts should match
        assert sum(distribution.values()) == len(sample_events)

    async def test_distribution_counts_correct(self, sample_events) -> None:
        """Distribution counts are correct."""
        distribution = AuditAnalyzer.get_event_type_distribution(sample_events)

        for event_type, count in distribution.items():
            actual = sum(1 for e in sample_events if e.event_type == event_type)
            assert actual == count


class TestTimeBasedPatterns:
    """Tests for time-based pattern analysis."""

    async def test_identify_time_based_patterns(self, sample_events) -> None:
        """Analyzes time-based patterns."""
        patterns = AuditAnalyzer.identify_time_based_patterns(sample_events)

        assert isinstance(patterns, dict)
        assert "events_by_hour" in patterns
        assert "events_by_weekday" in patterns
        assert "unusual_hour_count" in patterns

    async def test_unusual_hour_detection(self) -> None:
        """Detects events at unusual hours."""
        # Create events at unusual hours
        unusual_events = [
            AuditEvent(
                id="1",
                timestamp=datetime(2026, 1, 4, 3, 0, 0, tzinfo=UTC),  # 3 AM
                event_type="query",
                result="success",
            ),
            AuditEvent(
                id="2",
                timestamp=datetime(2026, 1, 4, 23, 30, 0, tzinfo=UTC),  # 11:30 PM
                event_type="query",
                result="success",
            ),
            AuditEvent(
                id="3",
                timestamp=datetime(2026, 1, 4, 12, 0, 0, tzinfo=UTC),  # 12 PM (normal)
                event_type="query",
                result="success",
            ),
        ]
        patterns = AuditAnalyzer.identify_time_based_patterns(unusual_events)

        # Should detect 2 unusual hour events (3 AM and 11:30 PM)
        assert patterns["unusual_hour_count"] == 2


class TestUserComparison:
    """Tests for comparing users."""

    async def test_compare_users(self, sample_events) -> None:
        """Compares activity between two users."""
        user1_events = [e for e in sample_events if e.user_id == "user-1"]
        user2_events = [e for e in sample_events if e.user_id == "user-2"]

        comparison = AuditAnalyzer.compare_users(user1_events, user2_events)

        assert isinstance(comparison, dict)
        assert "event_count_diff" in comparison
        assert "error_rate_diff" in comparison
        assert "stats1" in comparison
        assert "stats2" in comparison


class TestAnomalyDetection:
    """Tests for anomaly detection."""

    async def test_identify_anomalies(self, sample_events) -> None:
        """Identifies anomalous events based on duration."""
        operations = [e for e in sample_events if e.duration_ms]
        anomalies = AuditAnalyzer.identify_anomalies(operations, std_devs=1.0)

        assert isinstance(anomalies, list)
        # Slow operation should be flagged as anomaly
        if anomalies:
            assert all(isinstance(e, AuditEvent) for e in anomalies)

    async def test_anomalies_have_high_duration(self, sample_events) -> None:
        """Anomalies have significantly higher duration."""
        operations = [e for e in sample_events if e.duration_ms]
        anomalies = AuditAnalyzer.identify_anomalies(operations, std_devs=2.0)

        if anomalies:
            normal_ops = [e for e in operations if e not in anomalies]
            if normal_ops:
                avg_normal = sum(e.duration_ms for e in normal_ops) / len(normal_ops)
                avg_anomaly = sum(e.duration_ms for e in anomalies) / len(anomalies)
                # Anomalies should have higher average duration
                assert avg_anomaly > avg_normal
