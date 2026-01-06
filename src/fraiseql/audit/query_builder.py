"""Query builder for audit logs and security events.

This module provides a fluent query builder for accessing:
- Security events from centralized audit logging
- GraphQL operation metrics

Example:
    >>> from fraiseql.audit import AuditLogQueryBuilder
    >>> builder = AuditLogQueryBuilder(session)
    >>> ops = await builder.recent_operations(limit=50)
    >>> user_ops = await builder.by_user("user-123", hours=24)
"""

import csv
import json
from collections import Counter
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any

from fraiseql.audit.models import AuditEvent, ComplianceReport, EventStats, OperationType


class AuditLogQueryBuilder:
    """Query builder for audit logs and security events.

    Provides a chainable, type-safe API for querying:
    - Security events from audit logging
    - GraphQL operations and metrics
    - Compliance data

    Supports filtering, pagination, aggregation, and export.
    """

    def __init__(self, events: list[AuditEvent] | None = None) -> None:
        """Initialize query builder.

        Args:
            events: Optional list of events to query (for testing/in-memory).
                   In production, this would be replaced with database session.
        """
        self._events = events or []
        self._filters: dict[str, Any] = {}
        self._limit: int | None = None
        self._offset: int = 0
        self._order_by: str = "timestamp"
        self._order_desc: bool = True

    # ===== Main Query Methods =====

    async def recent_operations(
        self,
        limit: int = 100,
        operation_type: OperationType | None = None,
    ) -> list[AuditEvent]:
        """Get recent GraphQL operations.

        Returns the most recent GraphQL operations (queries, mutations, subscriptions)
        with their metrics.

        Args:
            limit: Maximum number of operations to return (default 100)
            operation_type: Filter by query/mutation/subscription (optional)

        Returns:
            List of recent AuditEvent objects with operation metrics

        Example:
            >>> ops = await builder.recent_operations(limit=50)
            >>> mutations = await builder.recent_operations(
            ...     operation_type=OperationType.MUTATION, limit=20
            ... )
        """
        results = [e for e in self._events if e.is_operational_event()]

        if operation_type:
            results = [e for e in results if e.event_type == operation_type.value]

        results = self._apply_filters(results)
        results = self._apply_ordering(results)
        return results[:limit]

    async def by_user(
        self,
        user_id: str,
        hours: int = 24,
    ) -> list[AuditEvent]:
        """Get all audit events for a specific user.

        Returns all security and operational events associated with a user,
        combining SecurityEvents and GraphQL operations.

        Args:
            user_id: User UUID or identifier
            hours: Look back this many hours (default 24)

        Returns:
            List of AuditEvent objects for the user

        Example:
            >>> ops = await builder.by_user("user-123", hours=24)
            >>> print(f"User performed {len(ops)} actions")
        """
        cutoff = datetime.now(UTC) - timedelta(hours=hours)

        results = [e for e in self._events if e.user_id == user_id and e.timestamp >= cutoff]

        results = self._apply_filters(results)
        return self._apply_ordering(results)

    async def by_entity(
        self,
        entity_type: str,
        entity_id: str,
    ) -> list[AuditEvent]:
        """Get all audit events for a specific entity/resource.

        Returns all events that affected a particular resource, identified by
        entity type and ID (e.g., Project:proj-123, User:user-456).

        Args:
            entity_type: Entity type (e.g., 'User', 'Project', 'Document')
            entity_id: Entity UUID or identifier

        Returns:
            List of AuditEvent objects for this entity

        Example:
            >>> events = await builder.by_entity("Project", "proj-123")
            >>> print(f"Project events: {len(events)}")
        """
        resource = f"{entity_type}:{entity_id}"

        results = [e for e in self._events if e.resource == resource]

        results = self._apply_filters(results)
        return self._apply_ordering(results)

    async def failed_operations(
        self,
        hours: int = 24,
        limit: int = 100,
    ) -> list[AuditEvent]:
        """Get failed operations and error events.

        Returns operations that resulted in errors, including both failed
        security checks and failed GraphQL operations.

        Args:
            hours: Look back this many hours (default 24)
            limit: Maximum results to return (default 100)

        Returns:
            List of failed AuditEvent objects

        Example:
            >>> errors = await builder.failed_operations(hours=1)
            >>> print(f"Errors: {len(errors)}")
        """
        cutoff = datetime.now(UTC) - timedelta(hours=hours)

        results = [e for e in self._events if e.is_error() and e.timestamp >= cutoff]

        results = self._apply_filters(results)
        results = self._apply_ordering(results)
        return results[:limit]

    async def by_event_type(self, event_type: str) -> list[AuditEvent]:
        """Filter by specific event type.

        Returns all events of a particular type (e.g., AUTH_FAILURE, mutation).
        Supports chaining with other filters.

        Args:
            event_type: Event type string (SecurityEventType or OperationType)

        Returns:
            List of AuditEvent objects of this type

        Example:
            >>> failures = await builder.by_event_type("auth.failure")
            >>> mutations = await builder.by_event_type("mutation")
        """
        results = [e for e in self._events if e.event_type == event_type]

        results = self._apply_filters(results)
        return self._apply_ordering(results)

    async def by_severity(self, severity: str) -> list[AuditEvent]:
        """Filter by event severity level.

        Returns all security events with a specific severity
        (info, warning, error, critical).

        Args:
            severity: Severity level string

        Returns:
            List of AuditEvent objects with this severity

        Example:
            >>> critical = await builder.by_severity("critical")
        """
        results = [e for e in self._events if e.metadata.get("severity") == severity]

        results = self._apply_filters(results)
        return self._apply_ordering(results)

    # ===== Chainable Filter Methods =====

    def filter_by_date_range(
        self,
        start: datetime,
        end: datetime,
    ) -> "AuditLogQueryBuilder":
        """Filter by date range (chainable).

        Args:
            start: Start datetime (inclusive)
            end: End datetime (inclusive)

        Returns:
            Self for method chaining
        """
        self._filters["date_range"] = (start, end)
        return self

    def filter_by_ip_address(self, ip: str) -> "AuditLogQueryBuilder":
        """Filter by IP address (chainable).

        Args:
            ip: IP address to filter by

        Returns:
            Self for method chaining
        """
        self._filters["ip_address"] = ip
        return self

    def filter_by_status(self, status: str) -> "AuditLogQueryBuilder":
        """Filter by operation status (chainable).

        Args:
            status: Status to filter by (success/error/denied)

        Returns:
            Self for method chaining
        """
        self._filters["status"] = status
        return self

    def limit(self, limit: int) -> "AuditLogQueryBuilder":
        """Set result limit (chainable).

        Args:
            limit: Maximum results to return

        Returns:
            Self for method chaining
        """
        self._limit = limit
        return self

    def offset(self, offset: int) -> "AuditLogQueryBuilder":
        """Set result offset for pagination (chainable).

        Args:
            offset: Number of results to skip

        Returns:
            Self for method chaining
        """
        self._offset = offset
        return self

    def order_by(
        self,
        field: str,
        descending: bool = True,
    ) -> "AuditLogQueryBuilder":
        """Set sort order (chainable).

        Args:
            field: Field to sort by (e.g., 'timestamp', 'duration_ms')
            descending: Sort descending (default True)

        Returns:
            Self for method chaining
        """
        self._order_by = field
        self._order_desc = descending
        return self

    # ===== Aggregation Methods =====

    async def count(self) -> int:
        """Get count of events matching current filters.

        Returns:
            Total number of events matching all applied filters

        Example:
            >>> count = await builder.filter_by_status("error").count()
        """
        results = self._apply_filters(self._events)
        return len(results)

    async def get_statistics(self) -> EventStats:
        """Get aggregate statistics for current filter set.

        Returns:
            EventStats with count, error_rate, duration percentiles, etc.
        """
        results = self._apply_filters(self._events)

        if not results:
            return EventStats()

        durations = [e.duration_ms for e in results if e.duration_ms]
        error_count = sum(1 for e in results if e.is_error())

        stats = EventStats(
            total_count=len(results),
            error_count=error_count,
            error_rate=error_count / len(results) if results else 0.0,
            avg_duration_ms=sum(durations) / len(durations) if durations else 0.0,
        )

        if durations:
            durations_sorted = sorted(durations)
            stats.p50_duration_ms = durations_sorted[len(durations_sorted) // 2]
            stats.p95_duration_ms = durations_sorted[int(len(durations_sorted) * 0.95)]
            stats.p99_duration_ms = durations_sorted[int(len(durations_sorted) * 0.99)]

        # Most common action
        actions = [e.action for e in results if e.action]
        if actions:
            stats.most_common_action = Counter(actions).most_common(1)[0][0]

        # Last event time
        if results:
            stats.last_event_time = max(e.timestamp for e in results)

        return stats

    # ===== Report Generation =====

    async def compliance_report(
        self,
        start_date: datetime,
        end_date: datetime,
        include_breakdown: bool = True,
    ) -> ComplianceReport:
        """Generate compliance audit report.

        Creates a comprehensive report of all audit events in a date range,
        including aggregate statistics and breakdowns.

        Args:
            start_date: Report start date
            end_date: Report end date
            include_breakdown: Include per-type and per-user breakdowns

        Returns:
            ComplianceReport with aggregate statistics

        Example:
            >>> report = await builder.compliance_report(
            ...     start_date=datetime(2026, 1, 1),
            ...     end_date=datetime(2026, 1, 31),
            ... )
            >>> print(f"Total: {report.total_events}")
        """
        filtered = self.filter_by_date_range(start_date, end_date)
        events = filtered._apply_filters(filtered._events)

        report = ComplianceReport(
            report_id=f"audit-{start_date.date()}-{end_date.date()}",
            start_date=start_date,
            end_date=end_date,
            generated_at=datetime.now(UTC),
        )

        for event in events:
            report.add_event(event)

        # Add breakdowns
        if include_breakdown:
            # Most active users
            user_counts = Counter(e.user_id for e in events if e.user_id)
            report.most_active_users = user_counts.most_common(10)

            # Most common events
            event_counts = Counter(e.event_type for e in events)
            report.most_common_events = event_counts.most_common(10)

        return report

    # ===== Export Methods =====

    async def export_csv(self, filepath: str) -> None:
        """Export results to CSV file.

        Args:
            filepath: Path where CSV file should be written

        Example:
            >>> await builder.export_csv("audit_report.csv")
        """
        events = self._apply_filters(self._events)

        if not events:
            return

        path = Path(filepath)
        with path.open("w", newline="") as f:
            writer = csv.DictWriter(
                f,
                fieldnames=[
                    "timestamp",
                    "event_type",
                    "user_id",
                    "resource",
                    "action",
                    "result",
                    "duration_ms",
                    "error_count",
                    "trace_id",
                ],
            )
            writer.writeheader()
            for event in events:
                writer.writerow(
                    {
                        "timestamp": event.timestamp.isoformat(),
                        "event_type": event.event_type,
                        "user_id": event.user_id,
                        "resource": event.resource,
                        "action": event.action,
                        "result": event.result,
                        "duration_ms": event.duration_ms,
                        "error_count": event.error_count,
                        "trace_id": event.trace_id,
                    },
                )

    async def export_json(self, filepath: str) -> None:
        """Export results to JSON file.

        Args:
            filepath: Path where JSON file should be written

        Example:
            >>> await builder.export_json("audit_report.json")
        """
        events = self._apply_filters(self._events)

        data = [
            {
                "timestamp": e.timestamp.isoformat(),
                "event_type": e.event_type,
                "user_id": e.user_id,
                "resource": e.resource,
                "action": e.action,
                "result": e.result,
                "duration_ms": e.duration_ms,
                "error_count": e.error_count,
                "field_count": e.field_count,
                "response_size_bytes": e.response_size_bytes,
                "trace_id": e.trace_id,
                "slow": e.slow,
                "metadata": e.metadata,
            }
            for e in events
        ]

        path = Path(filepath)
        with path.open("w") as f:
            json.dump(data, f, indent=2, default=str)

    # ===== Private Helper Methods =====

    def _apply_filters(self, events: list[AuditEvent]) -> list[AuditEvent]:
        """Apply all configured filters to events list.

        Args:
            events: List of events to filter

        Returns:
            Filtered list of events
        """
        results = events

        # Date range filter
        if "date_range" in self._filters:
            start, end = self._filters["date_range"]
            results = [e for e in results if start <= e.timestamp <= end]

        # IP address filter
        if "ip_address" in self._filters:
            ip = self._filters["ip_address"]
            results = [e for e in results if e.ip_address == ip]

        # Status filter
        if "status" in self._filters:
            status = self._filters["status"]
            results = [e for e in results if e.result == status]

        return results

    def _apply_ordering(self, events: list[AuditEvent]) -> list[AuditEvent]:
        """Apply ordering to events list.

        Args:
            events: List of events to order

        Returns:
            Ordered list of events
        """
        if self._order_by == "timestamp":
            results = sorted(
                events,
                key=lambda e: e.timestamp,
                reverse=self._order_desc,
            )
        elif self._order_by == "duration_ms":
            results = sorted(
                events,
                key=lambda e: e.duration_ms or 0,
                reverse=self._order_desc,
            )
        elif self._order_by == "error_count":
            results = sorted(
                events,
                key=lambda e: e.error_count or 0,
                reverse=self._order_desc,
            )
        else:
            results = events

        # Apply offset and limit
        if self._offset:
            results = results[self._offset :]

        if self._limit:
            results = results[: self._limit]

        return results

    def __repr__(self) -> str:
        """Return string representation."""
        """String representation of builder state."""
        return (
            f"AuditLogQueryBuilder("
            f"filters={self._filters}, "
            f"limit={self._limit}, "
            f"offset={self._offset}, "
            f"order_by={self._order_by}, "
            f"desc={self._order_desc}"
            ")"
        )
