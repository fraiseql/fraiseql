"""Analysis helpers for audit logs and security events.

This module provides utilities for analyzing audit events, detecting patterns,
and identifying suspicious activity.
"""

from collections import Counter
from typing import Any

from fraiseql.audit.models import AuditEvent, EventStats


class AuditAnalyzer:
    """Analysis helpers for audit logs."""

    @staticmethod
    def detect_suspicious_activity(
        events: list[AuditEvent],
        window_minutes: int = 10,
    ) -> dict[str, Any]:
        """Detect suspicious patterns in audit events.

        Identifies potential security issues like:
        - Rapid failed authentication attempts
        - Privilege escalation attempts
        - Unusual data access patterns
        - Activity at unusual times
        - High error rates

        Args:
            events: List of AuditEvent objects to analyze
            window_minutes: Time window for detecting rapid activity

        Returns:
            Dict with suspicious activity findings
        """
        suspicious = {
            "rapid_auth_failures": None,
            "privilege_escalation": None,
            "data_export_spike": None,
            "unusual_times": None,
            "high_error_rate": None,
        }

        if not events:
            return suspicious

        # Detect rapid auth failures
        auth_events = [e for e in events if "auth" in e.event_type]
        if auth_events:
            failures = [e for e in auth_events if e.is_error()]
            if len(failures) >= 5:
                suspicious["rapid_auth_failures"] = {
                    "count": len(failures),
                    "timeframe_minutes": window_minutes,
                }

        # Detect high error rate
        if events:
            error_rate = sum(1 for e in events if e.is_error()) / len(events)
            if error_rate > 0.5:  # More than 50% errors
                suspicious["high_error_rate"] = {
                    "rate": f"{error_rate:.1%}",
                    "count": sum(1 for e in events if e.is_error()),
                    "threshold": "0.5",
                }

        # Detect denied access attempts
        denied = [e for e in events if e.is_denied()]
        if len(denied) >= 3:
            suspicious["privilege_escalation"] = {
                "denied_count": len(denied),
                "threshold": "3",
            }

        return suspicious

    @staticmethod
    def summarize_user_activity(
        events: list[AuditEvent],
    ) -> EventStats:
        """Summarize activity metrics for a user.

        Args:
            events: List of AuditEvent objects for a user

        Returns:
            EventStats with count, error_rate, duration percentiles
        """
        if not events:
            return EventStats()

        durations = [e.duration_ms for e in events if e.duration_ms]
        error_count = sum(1 for e in events if e.is_error())

        stats = EventStats(
            total_count=len(events),
            error_count=error_count,
            error_rate=error_count / len(events) if events else 0.0,
            avg_duration_ms=sum(durations) / len(durations) if durations else 0.0,
        )

        # Percentiles
        if durations:
            durations_sorted = sorted(durations)
            stats.p50_duration_ms = durations_sorted[len(durations_sorted) // 2]
            stats.p95_duration_ms = durations_sorted[int(len(durations_sorted) * 0.95)]
            stats.p99_duration_ms = durations_sorted[int(len(durations_sorted) * 0.99)]

        # Most common action
        actions = [e.action for e in events if e.action]
        if actions:
            stats.most_common_action = Counter(actions).most_common(1)[0][0]

        # Last event time
        if events:
            stats.last_event_time = max(e.timestamp for e in events)

        return stats

    @staticmethod
    def identify_slow_operations(
        events: list[AuditEvent],
        percentile: float = 0.95,
    ) -> list[AuditEvent]:
        """Identify slow operations by percentile.

        Returns operations that are in the slowest percentile
        (e.g., top 5% slowest for percentile=0.95).

        Args:
            events: List of AuditEvent objects with duration_ms
            percentile: Percentile threshold (0.0-1.0, default 0.95 for top 5%)

        Returns:
            Slowest operations up to the specified percentile
        """
        with_duration = [e for e in events if e.duration_ms]
        if not with_duration:
            return []

        durations = sorted(e.duration_ms for e in with_duration)
        threshold = durations[int(len(durations) * percentile)]

        return [e for e in with_duration if e.duration_ms >= threshold]

    @staticmethod
    def analyze_error_patterns(
        events: list[AuditEvent],
    ) -> dict[str, int]:
        """Analyze error types and their frequency.

        Returns the most common error reasons.

        Args:
            events: List of AuditEvent objects

        Returns:
            Dict mapping error reasons to counts (top 10)
        """
        error_reasons = [e.reason for e in events if e.is_error() and e.reason]
        return dict(Counter(error_reasons).most_common(10))

    @staticmethod
    def identify_most_active_users(
        events: list[AuditEvent],
        top_n: int = 10,
    ) -> list[tuple[str, int]]:
        """Identify the most active users.

        Args:
            events: List of AuditEvent objects
            top_n: Number of top users to return (default 10)

        Returns:
            List of (user_id, event_count) tuples sorted by count
        """
        user_counts = Counter(e.user_id for e in events if e.user_id)
        return user_counts.most_common(top_n)

    @staticmethod
    def identify_most_active_resources(
        events: list[AuditEvent],
        top_n: int = 10,
    ) -> list[tuple[str, int]]:
        """Identify the most frequently accessed resources.

        Args:
            events: List of AuditEvent objects
            top_n: Number of top resources to return

        Returns:
            List of (resource, event_count) tuples sorted by count
        """
        resource_counts = Counter(e.resource for e in events if e.resource)
        return resource_counts.most_common(top_n)

    @staticmethod
    def get_event_type_distribution(
        events: list[AuditEvent],
    ) -> dict[str, int]:
        """Get distribution of event types.

        Args:
            events: List of AuditEvent objects

        Returns:
            Dict mapping event type to count
        """
        return dict(Counter(e.event_type for e in events))

    @staticmethod
    def identify_time_based_patterns(
        events: list[AuditEvent],
    ) -> dict[str, Any]:
        """Analyze time-based patterns in events.

        Identifies unusual patterns like:
        - Events outside business hours
        - Weekday vs weekend activity
        - Peak activity times

        Args:
            events: List of AuditEvent objects

        Returns:
            Dict with time-based analysis
        """
        if not events:
            return {}

        # Hours of day
        hours = Counter(e.timestamp.hour for e in events)

        # Weekdays
        weekdays = Counter(e.timestamp.weekday() for e in events)

        # Unusual hours (before 6am or after 10pm)
        unusual_hour_events = [e for e in events if e.timestamp.hour < 6 or e.timestamp.hour > 22]

        return {
            "events_by_hour": dict(hours.most_common(24)),
            "events_by_weekday": dict(weekdays.most_common(7)),
            "unusual_hour_count": len(unusual_hour_events),
            "unusual_hour_percentage": (
                len(unusual_hour_events) / len(events) * 100 if events else 0
            ),
            "peak_hour": max(hours.keys()) if hours else None,
            "peak_hour_count": max(hours.values()) if hours else 0,
        }

    @staticmethod
    def compare_users(
        events1: list[AuditEvent],
        events2: list[AuditEvent],
    ) -> dict[str, Any]:
        """Compare activity between two sets of events (e.g., two users).

        Args:
            events1: First set of events
            events2: Second set of events

        Returns:
            Dict with comparison metrics
        """
        stats1 = AuditAnalyzer.summarize_user_activity(events1)
        stats2 = AuditAnalyzer.summarize_user_activity(events2)

        return {
            "event_count_diff": stats1.total_count - stats2.total_count,
            "error_rate_diff": stats1.error_rate - stats2.error_rate,
            "avg_duration_diff_ms": (stats1.avg_duration_ms - stats2.avg_duration_ms),
            "stats1": {
                "total": stats1.total_count,
                "error_rate": f"{stats1.error_rate:.1%}",
                "avg_duration_ms": f"{stats1.avg_duration_ms:.2f}",
            },
            "stats2": {
                "total": stats2.total_count,
                "error_rate": f"{stats2.error_rate:.1%}",
                "avg_duration_ms": f"{stats2.avg_duration_ms:.2f}",
            },
        }

    @staticmethod
    def identify_anomalies(
        events: list[AuditEvent],
        std_devs: float = 2.0,
    ) -> list[AuditEvent]:
        """Identify anomalous events based on duration.

        Events with duration > mean + (std_devs * std_dev) are flagged.

        Args:
            events: List of AuditEvent objects
            std_devs: Number of standard deviations for threshold (default 2.0)

        Returns:
            List of anomalous events
        """
        with_duration = [e for e in events if e.duration_ms is not None]
        if not with_duration:
            return []

        # Calculate mean and standard deviation
        durations = [e.duration_ms for e in with_duration]
        mean = sum(durations) / len(durations)

        if len(durations) < 2:
            return []

        variance = sum((d - mean) ** 2 for d in durations) / len(durations)
        std_dev = variance**0.5

        # Identify anomalies
        threshold = mean + (std_devs * std_dev)
        return [e for e in with_duration if e.duration_ms > threshold]
