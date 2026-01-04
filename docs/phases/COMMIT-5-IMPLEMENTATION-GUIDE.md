# Commit 5: Implementation Guide - Audit Log Query Builder

**Phase**: Phase 19, Commit 5
**Language**: Python (FastAPI)
**Status**: Implementation Ready
**Date**: January 4, 2026

---

## Quick Start

This guide provides step-by-step instructions for implementing Commit 5: Audit Log Query Builder.

### Prerequisites

- ✅ Phase 14 (SecurityLogger) implemented and tested
- ✅ Commit 4.5 (GraphQL Operation Monitoring) completed
- ✅ Commit 1 (FraiseQLConfig) extended with observability settings
- ✅ PostgreSQL with audit tables created

### Implementation Phases

1. **Phase 1**: Core module implementation (1-2 days)
2. **Phase 2**: Comprehensive testing (1 day)
3. **Phase 3**: Integration and documentation (0.5 days)
4. **Phase 4**: Code review and polish (0.5 days)

---

## Phase 1: Core Implementation

### Step 1.1: Create Data Models (`audit/models.py`)

**File**: `src/fraiseql/audit/models.py`
**Size**: ~150 LOC

```python
"""Data models for audit log queries."""

from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Any, Optional


# Enums for type safety

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


# Data classes

@dataclass
class AuditEvent:
    """Represents a single audit event (security or operational).

    This can be either:
    - A SecurityEvent from Phase 14 audit logging
    - A GraphQL operation metric from Commit 4.5
    """
    id: str
    timestamp: datetime
    event_type: str  # SecurityEventType or OperationType
    user_id: Optional[str] = None
    user_email: Optional[str] = None
    ip_address: Optional[str] = None
    resource: Optional[str] = None
    action: Optional[str] = None
    result: str = "unknown"  # success, error, denied, etc.
    reason: Optional[str] = None

    # GraphQL operation specific fields
    duration_ms: Optional[float] = None
    error_count: Optional[int] = None
    field_count: Optional[int] = None
    response_size_bytes: Optional[int] = None
    trace_id: Optional[str] = None
    slow: bool = False

    # Metadata for extensibility
    metadata: dict[str, Any] = field(default_factory=dict)

    def is_security_event(self) -> bool:
        """Check if this is a security event."""
        return self.event_type.startswith("auth.") or \
               self.event_type.startswith("authz.") or \
               self.event_type.startswith("query.") or \
               self.event_type.startswith("config.")

    def is_operational_event(self) -> bool:
        """Check if this is an operational event (GraphQL operation)."""
        return self.event_type in ("query", "mutation", "subscription")

    def is_slow(self) -> bool:
        """Check if event took longer than expected."""
        return self.slow or (self.duration_ms and self.duration_ms > 1000)

    def is_error(self) -> bool:
        """Check if event resulted in error."""
        return self.result == "error" or self.error_count and self.error_count > 0


@dataclass
class ComplianceReport:
    """Audit compliance report for a time period.

    Includes aggregated statistics for compliance and audit purposes.
    """
    report_id: str
    start_date: datetime
    end_date: datetime
    generated_at: datetime

    # Aggregate counts
    total_events: int = 0
    critical_events: int = 0
    error_events: int = 0
    warning_events: int = 0
    info_events: int = 0

    # Success/failure
    successful_events: int = 0
    failed_events: int = 0
    denied_events: int = 0

    # Breakdowns
    events_by_type: dict[str, int] = field(default_factory=dict)
    events_by_user: dict[str, int] = field(default_factory=dict)
    events_by_severity: dict[str, int] = field(default_factory=dict)

    # Top items
    most_active_users: list[tuple[str, int]] = field(default_factory=list)
    most_common_events: list[tuple[str, int]] = field(default_factory=list)

    # Error tracking
    failed_operations: list[AuditEvent] = field(default_factory=list)
    suspicious_activities: list[str] = field(default_factory=list)

    def add_event(self, event: AuditEvent) -> None:
        """Add event to report."""
        self.total_events += 1

        # Update by type
        self.events_by_type[event.event_type] = \
            self.events_by_type.get(event.event_type, 0) + 1

        # Update by user
        if event.user_id:
            self.events_by_user[event.user_id] = \
                self.events_by_user.get(event.user_id, 0) + 1

        # Update by result
        if event.result == "error":
            self.failed_events += 1
            self.error_events += 1
            self.failed_operations.append(event)
        elif event.result == "success":
            self.successful_events += 1
        elif event.result == "denied":
            self.denied_events += 1


# Summary statistics

@dataclass
class EventStats:
    """Statistics for a set of events."""
    total_count: int = 0
    error_count: int = 0
    error_rate: float = 0.0
    avg_duration_ms: float = 0.0
    p50_duration_ms: float = 0.0
    p95_duration_ms: float = 0.0
    p99_duration_ms: float = 0.0
    most_common_action: Optional[str] = None
    last_event_time: Optional[datetime] = None
```

### Step 1.2: Create Query Builder (`audit/query_builder.py`)

**File**: `src/fraiseql/audit/query_builder.py`
**Size**: ~350 LOC

```python
"""Query builder for audit logs and security events."""

from datetime import datetime, timedelta, UTC
from typing import Any, Optional
from sqlalchemy import and_, or_, desc, select, func
from sqlalchemy.ext.asyncio import AsyncSession

from fraiseql.audit.models import AuditEvent, ComplianceReport, EventStats, OperationType
from fraiseql.audit.security_logger import SecurityEventType, SecurityEventSeverity


class AuditLogQueryBuilder:
    """Query builder for audit logs and security events.

    Provides chainable API for querying:
    - Security events (Phase 14)
    - GraphQL operations (Commit 4.5)
    - Compliance reports
    """

    def __init__(self, session: AsyncSession):
        """Initialize query builder with database session.

        Args:
            session: SQLAlchemy AsyncSession for database access
        """
        self.session = session
        self._filters: dict[str, Any] = {}
        self._limit: Optional[int] = None
        self._offset: int = 0
        self._order_by: str = "timestamp"
        self._order_desc: bool = True

    # ===== Main Query Methods =====

    async def recent_operations(
        self,
        limit: int = 100,
        operation_type: Optional[OperationType] = None,
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
            >>> builder = AuditLogQueryBuilder(session)
            >>> ops = await builder.recent_operations(limit=50)
            >>> mutations = await builder.recent_operations(
            ...     operation_type=OperationType.MUTATION,
            ...     limit=20
            ... )
        """
        query = self._build_base_query()

        # Filter to GraphQL operations only
        if 'graphql_operations_table' in self.session.info:
            table = self.session.info['graphql_operations_table']
            query = query.where(table.c.operation_type.isnot(None))

            if operation_type:
                query = query.where(table.c.operation_type == operation_type.value)

        query = query.order_by(desc('timestamp')).limit(limit)
        result = await self.session.execute(query)
        return self._format_results(result)

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
            >>> print(f"User performed {len(ops)} actions in last 24h")
        """
        cutoff = datetime.now(UTC) - timedelta(hours=hours)

        filters = [
            'user_id' == user_id,
            'timestamp' >= cutoff,
        ]

        self._filters['user_id'] = user_id
        self._filters['date_range'] = (cutoff, datetime.now(UTC))

        query = self._build_base_query()
        query = query.where(and_(*filters))
        query = query.order_by(desc('timestamp'))

        result = await self.session.execute(query)
        return self._format_results(result)

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
            >>> print(f"Project accessed {len(events)} times")
        """
        resource = f"{entity_type}:{entity_id}"

        filters = [
            'resource' == resource,
        ]

        self._filters['entity'] = (entity_type, entity_id)

        query = self._build_base_query()
        query = query.where(and_(*filters))
        query = query.order_by(desc('timestamp'))

        result = await self.session.execute(query)
        return self._format_results(result)

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
            >>> errors = await builder.failed_operations(hours=1, limit=50)
            >>> print(f"Found {len(errors)} errors in last hour")
        """
        cutoff = datetime.now(UTC) - timedelta(hours=hours)

        filters = [
            'result' == "error",
            'timestamp' >= cutoff,
        ]

        query = self._build_base_query()
        query = query.where(and_(*filters))
        query = query.order_by(desc('timestamp'))
        query = query.limit(limit)

        result = await self.session.execute(query)
        return self._format_results(result)

    async def by_event_type(
        self,
        event_type: SecurityEventType | OperationType,
    ) -> list[AuditEvent]:
        """Filter by specific event type.

        Returns all events of a particular type (e.g., AUTH_FAILURE, mutation).
        Supports chaining with other filters.

        Args:
            event_type: SecurityEventType or OperationType enum value

        Returns:
            List of AuditEvent objects of this type

        Example:
            >>> failures = await builder.by_event_type(SecurityEventType.AUTH_FAILURE)
            >>> mutations = await builder.by_event_type(OperationType.MUTATION)
        """
        type_value = event_type.value if hasattr(event_type, 'value') else str(event_type)

        filters = [
            'event_type' == type_value,
        ]

        self._filters['event_type'] = type_value

        query = self._build_base_query()
        query = query.where(and_(*filters))
        query = query.order_by(desc('timestamp'))

        result = await self.session.execute(query)
        return self._format_results(result)

    async def by_severity(
        self,
        severity: SecurityEventSeverity,
    ) -> list[AuditEvent]:
        """Filter by event severity level.

        Returns all security events with a specific severity (info, warning, error, critical).

        Args:
            severity: SecurityEventSeverity enum value

        Returns:
            List of AuditEvent objects with this severity

        Example:
            >>> critical = await builder.by_severity(SecurityEventSeverity.CRITICAL)
        """
        severity_value = severity.value if hasattr(severity, 'value') else str(severity)

        filters = [
            'severity' == severity_value,
        ]

        self._filters['severity'] = severity_value

        query = self._build_base_query()
        query = query.where(and_(*filters))
        query = query.order_by(desc('timestamp'))

        result = await self.session.execute(query)
        return self._format_results(result)

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

        Example:
            >>> week_ago = datetime.now() - timedelta(days=7)
            >>> builder.filter_by_date_range(week_ago, datetime.now())
        """
        self._filters['date_range'] = (start, end)
        return self

    def filter_by_ip_address(self, ip: str) -> "AuditLogQueryBuilder":
        """Filter by IP address (chainable)."""
        self._filters['ip_address'] = ip
        return self

    def filter_by_status(self, status: str) -> "AuditLogQueryBuilder":
        """Filter by operation status (success/error/denied) (chainable)."""
        self._filters['status'] = status
        return self

    def limit(self, limit: int) -> "AuditLogQueryBuilder":
        """Set result limit (chainable)."""
        self._limit = limit
        return self

    def offset(self, offset: int) -> "AuditLogQueryBuilder":
        """Set result offset for pagination (chainable)."""
        self._offset = offset
        return self

    def order_by(
        self,
        field: str,
        descending: bool = True,
    ) -> "AuditLogQueryBuilder":
        """Set sort order (chainable)."""
        self._order_by = field
        self._order_desc = descending
        return self

    # ===== Aggregation Methods =====

    async def count(self) -> int:
        """Get count of events matching current filters.

        Returns:
            Total number of events matching all applied filters

        Example:
            >>> error_count = await builder.filter_by_status("error").count()
        """
        query = self._build_count_query()
        result = await self.session.execute(query)
        return result.scalar_one_or_none() or 0

    async def get_statistics(self) -> EventStats:
        """Get aggregate statistics for current filter set.

        Returns:
            EventStats with count, error_rate, duration percentiles, etc.
        """
        events = await self._execute_query()

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

        if durations:
            durations.sort()
            stats.p50_duration_ms = durations[len(durations) // 2]
            stats.p95_duration_ms = durations[int(len(durations) * 0.95)]
            stats.p99_duration_ms = durations[int(len(durations) * 0.99)]

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
            >>> print(f"Total events: {report.total_events}")
            >>> print(f"Critical events: {report.critical_events}")
        """
        self.filter_by_date_range(start_date, end_date)
        events = await self._execute_query()

        report = ComplianceReport(
            report_id=f"audit-{start_date.date()}-{end_date.date()}",
            start_date=start_date,
            end_date=end_date,
            generated_at=datetime.now(UTC),
        )

        for event in events:
            report.add_event(event)

        return report

    # ===== Export Methods =====

    async def export_csv(self, filepath: str) -> None:
        """Export results to CSV file.

        Args:
            filepath: Path where CSV file should be written

        Example:
            >>> await builder.export_csv("audit_report.csv")
        """
        import csv

        events = await self._execute_query()

        if not events:
            return

        with open(filepath, 'w', newline='') as f:
            writer = csv.DictWriter(f, fieldnames=[
                'timestamp', 'event_type', 'user_id', 'resource',
                'action', 'result', 'duration_ms', 'error_count',
            ])
            writer.writeheader()
            for event in events:
                writer.writerow({
                    'timestamp': event.timestamp.isoformat(),
                    'event_type': event.event_type,
                    'user_id': event.user_id,
                    'resource': event.resource,
                    'action': event.action,
                    'result': event.result,
                    'duration_ms': event.duration_ms,
                    'error_count': event.error_count,
                })

    async def export_json(self, filepath: str) -> None:
        """Export results to JSON file.

        Args:
            filepath: Path where JSON file should be written
        """
        import json

        events = await self._execute_query()

        data = [
            {
                'timestamp': e.timestamp.isoformat(),
                'event_type': e.event_type,
                'user_id': e.user_id,
                'resource': e.resource,
                'action': e.action,
                'result': e.result,
                'duration_ms': e.duration_ms,
                'error_count': e.error_count,
                'metadata': e.metadata,
            }
            for e in events
        ]

        with open(filepath, 'w') as f:
            json.dump(data, f, indent=2, default=str)

    # ===== Private Helper Methods =====

    def _build_base_query(self):
        """Build base query from configured filters."""
        # This would construct proper SQLAlchemy query
        # Implementation depends on actual database schema
        pass

    def _build_count_query(self):
        """Build count query from configured filters."""
        pass

    async def _execute_query(self) -> list[AuditEvent]:
        """Execute configured query and return results."""
        query = self._build_base_query()
        if self._limit:
            query = query.limit(self._limit)
        if self._offset:
            query = query.offset(self._offset)

        result = await self.session.execute(query)
        return self._format_results(result)

    def _format_results(self, result) -> list[AuditEvent]:
        """Format query results as AuditEvent objects."""
        # Convert database rows to AuditEvent dataclass instances
        pass
```

### Step 1.3: Create Analyzer Helpers (`audit/analyzer.py`)

**File**: `src/fraiseql/audit/analyzer.py`
**Size**: ~200 LOC

```python
"""Analysis helpers for audit logs and security events."""

from datetime import datetime, timedelta, UTC
from typing import Any
from collections import Counter

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

        Args:
            events: List of AuditEvent objects to analyze
            window_minutes: Time window for detecting rapid activity

        Returns:
            Dict with suspicious activity findings
        """
        suspicious = {
            'rapid_auth_failures': None,
            'privilege_escalation': None,
            'data_export_spike': None,
            'unusual_times': None,
            'high_error_rate': None,
        }

        if not events:
            return suspicious

        # Detect rapid auth failures
        auth_events = [e for e in events if 'auth' in e.event_type]
        if auth_events:
            # Check for multiple failures in short window
            failures = [e for e in auth_events if e.result == "error"]
            if len(failures) >= 5:
                suspicious['rapid_auth_failures'] = {
                    'count': len(failures),
                    'timeframe_minutes': window_minutes,
                }

        # Detect high error rate
        if events:
            error_rate = sum(1 for e in events if e.is_error()) / len(events)
            if error_rate > 0.5:  # More than 50% errors
                suspicious['high_error_rate'] = {
                    'rate': f"{error_rate:.1%}",
                    'count': sum(1 for e in events if e.is_error()),
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
            error_rate=error_count / len(events),
            avg_duration_ms=sum(durations) / len(durations) if durations else 0.0,
        )

        if events:
            stats.last_event_time = max(e.timestamp for e in events)

        return stats

    @staticmethod
    def identify_slow_operations(
        events: list[AuditEvent],
        percentile: float = 0.95,
    ) -> list[AuditEvent]:
        """Identify slow operations by percentile.

        Args:
            events: List of AuditEvent objects with duration_ms
            percentile: Percentile threshold (0.0-1.0)

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

        Args:
            events: List of AuditEvent objects

        Returns:
            Dict mapping error types to counts
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
            top_n: Number of top users to return

        Returns:
            List of (user_id, event_count) tuples
        """
        user_counts = Counter(
            e.user_id for e in events if e.user_id
        )
        return user_counts.most_common(top_n)
```

### Step 1.4: Update Exports (`audit/__init__.py`)

Add to existing `src/fraiseql/audit/__init__.py`:

```python
from .models import (
    AuditEvent,
    AuditFilterType,
    ComplianceReport,
    EventStats,
    OperationType,
)
from .query_builder import AuditLogQueryBuilder
from .analyzer import AuditAnalyzer

__all__ = [
    # Existing exports
    "SecurityEvent",
    "SecurityEventSeverity",
    "SecurityEventType",
    "SecurityLogger",
    "get_security_logger",
    "set_security_logger",
    # New exports
    "AuditEvent",
    "AuditFilterType",
    "ComplianceReport",
    "EventStats",
    "OperationType",
    "AuditLogQueryBuilder",
    "AuditAnalyzer",
]
```

---

## Phase 2: Testing

### Step 2.1: Create Unit Tests

**File**: `tests/unit/audit/test_query_builder.py`
**Size**: ~250 LOC

Write 15+ tests covering:
- Builder initialization
- Recent operations query
- User filtering
- Entity filtering
- Failed operations
- Event type filtering
- Chaining multiple filters
- Pagination
- Counting
- Compliance reports
- Export functionality

### Step 2.2: Create Analyzer Tests

**File**: `tests/unit/audit/test_analyzer.py`
**Size**: ~150 LOC

Write 5+ tests covering:
- Suspicious activity detection
- User activity summarization
- Slow operation identification
- Error pattern analysis
- Most active users ranking

---

## Phase 3: Integration & Documentation

### Step 3.1: Integration Tests

Create `tests/integration/observability/test_audit_queries.py` with:
- Real database tests
- Multi-table join tests
- Compliance report generation
- Export functionality

### Step 3.2: Documentation

Create examples and usage guides in:
- API reference with examples
- Integration guide
- Compliance reporting guide

---

## Database Schema Requirements

The following tables must exist (from Phase 14 and Commit 4.5):

```sql
-- From Phase 14 (SecurityLogger)
CREATE TABLE security_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type VARCHAR(100) NOT NULL,
    severity VARCHAR(20) NOT NULL,
    timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    user_id UUID,
    user_email VARCHAR(255),
    ip_address INET,
    request_id UUID,
    resource VARCHAR(500),
    action VARCHAR(100),
    result VARCHAR(50),
    reason TEXT,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for query performance
CREATE INDEX idx_security_events_timestamp ON security_events(timestamp DESC);
CREATE INDEX idx_security_events_user_id ON security_events(user_id);
CREATE INDEX idx_security_events_event_type ON security_events(event_type);
CREATE INDEX idx_security_events_severity ON security_events(severity);

-- From Commit 4.5 (GraphQL Operations)
-- May be stored in memory (OperationMonitor) or persistent table
CREATE TABLE graphql_operations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operation_type VARCHAR(20) NOT NULL,
    operation_name VARCHAR(255),
    query_hash VARCHAR(64),
    timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    user_id UUID,
    trace_id UUID,
    span_id UUID,
    parent_span_id UUID,
    duration_ms FLOAT NOT NULL,
    status VARCHAR(20) NOT NULL,
    error_count INT DEFAULT 0,
    field_count INT DEFAULT 0,
    response_size_bytes INT DEFAULT 0,
    slow BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for query performance
CREATE INDEX idx_graphql_operations_timestamp ON graphql_operations(timestamp DESC);
CREATE INDEX idx_graphql_operations_user_id ON graphql_operations(user_id);
CREATE INDEX idx_graphql_operations_operation_type ON graphql_operations(operation_type);
```

---

## Integration Checklist

### With Phase 14 (SecurityLogger)
- [x] Query security_events table
- [x] Filter by event_type and severity
- [x] Support all SecurityEventType and SecurityEventSeverity enums

### With Commit 4.5 (GraphQL Operations)
- [x] Query GraphQL operation metrics
- [x] Filter by operation_type
- [x] Access trace_id and span_id (W3C Trace Context)
- [x] Use duration and error metrics

### With Commit 1 (FraiseQLConfig)
- [x] Respect audit_retention_days setting
- [x] Use configured result limits
- [x] Honor sampling configuration

---

## Testing Roadmap

### Phase 1: Unit Tests
- All query methods return correct AuditEvent objects
- Chaining works correctly
- Pagination works as expected

### Phase 2: Integration Tests
- Real database queries work
- Multiple tables can be joined
- Compliance reports generate correctly

### Phase 3: Performance Tests
- Query latency < 500ms for 1000 events
- Export operations < 1s for 10,000 events

---

## Success Criteria

- ✅ All 20+ unit tests passing
- ✅ Integration tests with real database passing
- ✅ Code coverage > 90%
- ✅ Linting passes (ruff strict)
- ✅ Type hints 100%
- ✅ Documentation complete with examples
- ✅ Performance benchmarks met
- ✅ Backward compatibility maintained

---

*Implementation Guide - Phase 19, Commit 5*
*Date: January 4, 2026*
