"""Data models for audit log queries.

This module provides dataclasses for representing audit events and compliance
reports, combining security events from Phase 14 and GraphQL operations from
Commit 4.5.
"""

from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Any


class AuditFilterType(str, Enum):
    """Types of audit query filters."""

    USER = "user"
    ENTITY = "entity"
    EVENT_TYPE = "event_type"
    SEVERITY = "severity"
    DATE_RANGE = "date_range"
    STATUS = "status"
    IP_ADDRESS = "ip_address"
    OPERATION_TYPE = "operation_type"


class OperationType(str, Enum):
    """GraphQL operation types."""

    QUERY = "query"
    MUTATION = "mutation"
    SUBSCRIPTION = "subscription"
    UNKNOWN = "unknown"


@dataclass
class AuditEvent:
    """Represents a single audit event (security or operational).

    This can be either:
    - A SecurityEvent from Phase 14 audit logging
    - A GraphQL operation metric from Commit 4.5

    Attributes:
        id: Unique identifier for the event
        timestamp: When the event occurred
        event_type: Type of event (SecurityEventType or OperationType)
        user_id: User who triggered the event (nullable)
        user_email: User email address (nullable)
        ip_address: IP address of the request (nullable)
        resource: Resource/entity affected (nullable)
        action: Action performed (nullable)
        result: Result of the action (success, error, denied, etc.)
        reason: Reason for the result (error message, denial reason, etc.)
        duration_ms: Duration in milliseconds (for operations)
        error_count: Number of errors in response
        field_count: Number of fields in GraphQL query/response
        response_size_bytes: Size of response in bytes
        trace_id: W3C Trace Context trace ID (nullable)
        slow: Whether event took longer than expected
        metadata: Additional metadata as dictionary
    """

    id: str
    timestamp: datetime
    event_type: str
    user_id: str | None = None
    user_email: str | None = None
    ip_address: str | None = None
    resource: str | None = None
    action: str | None = None
    result: str = "unknown"
    reason: str | None = None
    duration_ms: float | None = None
    error_count: int | None = None
    field_count: int | None = None
    response_size_bytes: int | None = None
    trace_id: str | None = None
    slow: bool = False
    metadata: dict[str, Any] = field(default_factory=dict)

    def is_security_event(self) -> bool:
        """Check if this is a security event.

        Returns:
            True if event is a security event from Phase 14
        """
        return (
            self.event_type.startswith("auth.")
            or self.event_type.startswith("authz.")
            or self.event_type.startswith("query.")
            or self.event_type.startswith("config.")
            or self.event_type.startswith("rate_limit.")
            or self.event_type.startswith("csrf.")
            or self.event_type.startswith("system.")
            or self.event_type.startswith("data.")
        )

    def is_operational_event(self) -> bool:
        """Check if this is an operational event (GraphQL operation).

        Returns:
            True if event is a GraphQL operation from Commit 4.5
        """
        return self.event_type in ("query", "mutation", "subscription")

    def is_slow(self) -> bool:
        """Check if event took longer than expected.

        Returns:
            True if event is flagged as slow or duration exceeds 1 second
        """
        return self.slow or (self.duration_ms and self.duration_ms > 1000)

    def is_error(self) -> bool:
        """Check if event resulted in error.

        Returns:
            True if result is "error" or error_count > 0
        """
        return self.result == "error" or (self.error_count and self.error_count > 0)

    def is_denied(self) -> bool:
        """Check if event was denied (authorization failure).

        Returns:
            True if result is "denied"
        """
        return self.result == "denied"

    def is_success(self) -> bool:
        """Check if event succeeded.

        Returns:
            True if result is "success"
        """
        return self.result == "success"


@dataclass
class EventStats:
    """Statistics for a set of events.

    Attributes:
        total_count: Total number of events
        error_count: Number of events with errors
        error_rate: Percentage of events that errored (0.0-1.0)
        avg_duration_ms: Average duration in milliseconds
        p50_duration_ms: 50th percentile (median) duration
        p95_duration_ms: 95th percentile duration
        p99_duration_ms: 99th percentile duration
        most_common_action: Most frequently performed action
        last_event_time: Timestamp of most recent event
    """

    total_count: int = 0
    error_count: int = 0
    error_rate: float = 0.0
    avg_duration_ms: float = 0.0
    p50_duration_ms: float = 0.0
    p95_duration_ms: float = 0.0
    p99_duration_ms: float = 0.0
    most_common_action: str | None = None
    last_event_time: datetime | None = None

    def success_count(self) -> int:
        """Get count of successful events.

        Returns:
            total_count - error_count
        """
        return self.total_count - self.error_count

    def success_rate(self) -> float:
        """Get success rate as percentage.

        Returns:
            Percentage of successful events (0.0-1.0)
        """
        if self.total_count == 0:
            return 0.0
        return self.success_count() / self.total_count


@dataclass
class ComplianceReport:
    """Audit compliance report for a time period.

    Includes aggregated statistics for compliance and audit purposes.

    Attributes:
        report_id: Unique identifier for this report
        start_date: Report period start date
        end_date: Report period end date
        generated_at: When the report was generated
        total_events: Total number of events in period
        critical_events: Number of critical severity events
        error_events: Number of error severity events
        warning_events: Number of warning severity events
        info_events: Number of info severity events
        successful_events: Number of successful operations
        failed_events: Number of failed operations
        denied_events: Number of denied access events
        events_by_type: Breakdown of events by type
        events_by_user: Breakdown of events by user
        events_by_severity: Breakdown of events by severity
        most_active_users: Top users by event count
        most_common_events: Most frequently occurring event types
        failed_operations: List of all failed operations
        suspicious_activities: List of suspicious patterns detected
    """

    report_id: str
    start_date: datetime
    end_date: datetime
    generated_at: datetime
    total_events: int = 0
    critical_events: int = 0
    error_events: int = 0
    warning_events: int = 0
    info_events: int = 0
    successful_events: int = 0
    failed_events: int = 0
    denied_events: int = 0
    events_by_type: dict[str, int] = field(default_factory=dict)
    events_by_user: dict[str, int] = field(default_factory=dict)
    events_by_severity: dict[str, int] = field(default_factory=dict)
    most_active_users: list[tuple[str, int]] = field(default_factory=list)
    most_common_events: list[tuple[str, int]] = field(default_factory=list)
    failed_operations: list[AuditEvent] = field(default_factory=list)
    suspicious_activities: list[str] = field(default_factory=list)

    def add_event(self, event: AuditEvent) -> None:
        """Add event to report statistics.

        Increments appropriate counters based on event properties.

        Args:
            event: AuditEvent to add to report
        """
        self.total_events += 1

        # Update by type
        self.events_by_type[event.event_type] = self.events_by_type.get(event.event_type, 0) + 1

        # Update by user
        if event.user_id:
            self.events_by_user[event.user_id] = self.events_by_user.get(event.user_id, 0) + 1

        # Update by result
        if event.result == "error":
            self.failed_events += 1
            self.error_events += 1
            self.failed_operations.append(event)
        elif event.result == "success":
            self.successful_events += 1
        elif event.result == "denied":
            self.denied_events += 1

    def get_period_days(self) -> int:
        """Get number of days covered by report.

        Returns:
            Number of days between start and end dates
        """
        delta = self.end_date - self.start_date
        return delta.days

    def get_events_per_day(self) -> float:
        """Get average events per day.

        Returns:
            Total events divided by number of days
        """
        days = self.get_period_days()
        if days == 0:
            return float(self.total_events)
        return self.total_events / days

    def get_error_rate(self) -> float:
        """Get error rate as percentage.

        Returns:
            Percentage of events that resulted in errors (0.0-1.0)
        """
        if self.total_events == 0:
            return 0.0
        return self.failed_events / self.total_events

    def get_denial_rate(self) -> float:
        """Get denial rate as percentage.

        Returns:
            Percentage of access denied events (0.0-1.0)
        """
        if self.total_events == 0:
            return 0.0
        return self.denied_events / self.total_events

    def get_top_user(self) -> tuple[str, int] | None:
        """Get most active user.

        Returns:
            Tuple of (user_id, event_count) or None if no users
        """
        return self.most_active_users[0] if self.most_active_users else None

    def get_summary_string(self) -> str:
        """Get human-readable summary of report.

        Returns:
            Formatted string with key statistics
        """
        days = self.get_period_days()
        return (
            f"Audit Report: {self.start_date.date()} to {self.end_date.date()} "
            f"({days} days)\n"
            f"  Total Events: {self.total_events}\n"
            f"  Critical: {self.critical_events}, "
            f"Error: {self.error_events}, "
            f"Warning: {self.warning_events}\n"
            f"  Success: {self.successful_events}, "
            f"Failed: {self.failed_events}, "
            f"Denied: {self.denied_events}\n"
            f"  Error Rate: {self.get_error_rate():.1%}\n"
            f"  Events/Day: {self.get_events_per_day():.1f}"
        )
