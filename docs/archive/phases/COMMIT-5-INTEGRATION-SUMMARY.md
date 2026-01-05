# Commit 5: Integration Points Summary

**Phase**: Phase 19
**Commit**: 5 of 8
**Status**: 🎯 Planning Complete - Ready for Implementation
**Date**: January 4, 2026

---

## Overview

Commit 5 (Audit Log Query Builder) integrates with **Phase 14 (Security Logging)** and **Commit 4.5 (GraphQL Operation Monitoring)** to provide a unified query interface for all audit and operational events.

---

## Integration Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ Phase 19, Commit 5: Audit Log Query Builder                    │
│ (Query Layer - Python/FastAPI)                                 │
└────────────────┬────────────────────────────────────────────────┘
                 │
         ┌───────┴────────────────────────┐
         │                                │
         ↓                                ↓
    ┌─────────────────┐         ┌──────────────────────┐
    │ Phase 14        │         │ Commit 4.5           │
    │ SecurityLogger  │         │ GraphQL Operation    │
    │ (Events)        │         │ Monitor (Metrics)    │
    └────────┬────────┘         └──────────┬───────────┘
             │                             │
             ↓                             ↓
    ┌─────────────────────────────────────────────────┐
    │ PostgreSQL Database (Audit Tables)              │
    │ ├── security_events                             │
    │ └── graphql_operations (from Phase 20)          │
    └─────────────────────────────────────────────────┘
```

---

## Integration Points Detail

### 1. Phase 14: SecurityLogger Integration ✅

**What We Integrate With**:
- `SecurityLogger` class (existing)
- `SecurityEvent` dataclass (existing)
- `SecurityEventType` enum (existing)
- `SecurityEventSeverity` enum (existing)
- `security_events` database table (existing)

**How Commit 5 Uses It**:

```python
# Query security events directly
from fraiseql.audit import AuditLogQueryBuilder, SecurityEventType

builder = AuditLogQueryBuilder(session)

# Get all auth failures
auth_failures = await builder.by_event_type(
    SecurityEventType.AUTH_FAILURE
)

# Get critical security events
critical = await builder.by_severity(
    SecurityEventSeverity.CRITICAL
)

# Generate compliance report including security events
report = await builder.compliance_report(
    start_date=datetime(2026, 1, 1),
    end_date=datetime(2026, 1, 31),
)
# Includes: total_events, critical_events, error_events, etc.
```

**Database Table Structure**:
```sql
security_events (from Phase 14):
├── id: UUID
├── event_type: VARCHAR (maps to SecurityEventType)
├── severity: VARCHAR (maps to SecurityEventSeverity)
├── timestamp: TIMESTAMPTZ
├── user_id: UUID (nullable)
├── user_email: VARCHAR
├── ip_address: INET
├── request_id: UUID
├── resource: VARCHAR
├── action: VARCHAR
├── result: VARCHAR (success/error/denied)
├── reason: TEXT
└── metadata: JSONB
```

**Key Integration**:
- Commit 5 queries the `security_events` table
- Filters by `event_type`, `severity`, `timestamp`, `user_id`
- Creates `AuditEvent` objects from SecurityEvent rows
- No modifications to Phase 14 code required

---

### 2. Commit 4.5: GraphQL Operation Monitoring ✅

**What We Integrate With**:
- `GraphQLOperationMonitor` (in-memory + persistent)
- `OperationMetrics` dataclass
- `GraphQLOperationType` enum (query/mutation/subscription)
- W3C Trace Context support (trace_id, span_id)
- Slow operation detection

**How Commit 5 Uses It**:

```python
# Query recent GraphQL operations
ops = await builder.recent_operations(limit=50)
# Returns: operations with duration_ms, error_count, slow flag, trace IDs

# Filter by operation type
mutations = await builder.recent_operations(
    operation_type=OperationType.MUTATION,
    limit=20,
)

# Get failed GraphQL operations
failed_ops = await builder.failed_operations(hours=1)
# Returns: operations where status=error or error_count > 0

# Analyze slow operations
slow_ops = AuditAnalyzer.identify_slow_operations(ops, percentile=0.95)
```

**Data Structure**:
```sql
graphql_operations (from Commit 4.5 - persisted in Phase 20):
├── id: UUID
├── operation_type: VARCHAR (query/mutation/subscription)
├── operation_name: VARCHAR
├── query_hash: VARCHAR (hashed for privacy)
├── timestamp: TIMESTAMPTZ
├── user_id: UUID (from request context)
├── trace_id: UUID (W3C Trace Context)
├── span_id: UUID
├── parent_span_id: UUID
├── duration_ms: FLOAT
├── status: VARCHAR (success/error/timeout)
├── error_count: INT
├── field_count: INT
├── response_size_bytes: INT
└── slow: BOOLEAN
```

**Key Integration**:
- Commit 5 queries `graphql_operations` table
- Integrates with W3C Trace Context for tracing
- Identifies slow mutations for operational visibility
- Supports operation type filtering (query/mutation/subscription)

---

### 3. Commit 1: FraiseQLConfig ✅

**What We Integrate With**:
- Extended `FraiseQLConfig` (from Commit 1)
- Observability settings

**How Commit 5 Uses It**:

```python
from fraiseql.fastapi.config import FraiseQLConfig

config = FraiseQLConfig()

# Respect audit retention policy
days = config.audit_retention_days

# Use configured query limits
max_results = config.audit_query_max_results

# Honor sampling rate for compliance reports
sampling_rate = config.observability_sampling_rate
```

**Configuration Integration**:
- `audit_retention_days`: How long to keep audit logs
- `audit_query_max_results`: Max results per query (default 1000)
- `observability_enabled`: Enable/disable query builder
- `observability_sampling_rate`: Sampling rate for metrics

**Key Integration**:
- Commit 5 respects retention policies
- Uses configuration defaults
- Can be disabled via config
- Honors sampling rates

---

### 4. Database Schema & Indexing ✅

**Required Indexes for Performance**:

```sql
-- Phase 14 SecurityLogger indexes
CREATE INDEX idx_security_events_timestamp ON security_events(timestamp DESC);
CREATE INDEX idx_security_events_user_id ON security_events(user_id);
CREATE INDEX idx_security_events_event_type ON security_events(event_type);
CREATE INDEX idx_security_events_severity ON security_events(severity);

-- Commit 4.5 GraphQL Operations indexes
CREATE INDEX idx_graphql_operations_timestamp ON graphql_operations(timestamp DESC);
CREATE INDEX idx_graphql_operations_user_id ON graphql_operations(user_id);
CREATE INDEX idx_graphql_operations_operation_type ON graphql_operations(operation_type);

-- Composite indexes for common queries
CREATE INDEX idx_security_events_timestamp_severity
  ON security_events(timestamp DESC, severity);
CREATE INDEX idx_graphql_operations_timestamp_status
  ON graphql_operations(timestamp DESC, status);
```

**Performance Targets**:
- Recent operations query: < 100ms for 1000 events
- User filter: < 200ms for 1000 events
- Compliance report: < 500ms for 1-month period
- Export: < 1s for 10,000 events

---

## Data Flow: Request to Query

### Scenario 1: Recent Operations Query

```
User Request
    ↓
AuditLogQueryBuilder.recent_operations(limit=50)
    ↓
Query graphql_operations table
    ├─ ORDER BY timestamp DESC
    ├─ LIMIT 50
    └─ Filter by operation_type
    ↓
Database returns rows
    ↓
Format as AuditEvent objects
    ├─ Map operation_type → event_type
    ├─ Convert duration_ms to float
    ├─ Include W3C trace IDs
    └─ Include error metrics
    ↓
Return list[AuditEvent]
```

### Scenario 2: User Activity Query

```
User Request: by_user("user-123", hours=24)
    ↓
Calculate time window
    ├─ END = now()
    ├─ START = now() - 24 hours
    └─ Filter where timestamp >= START AND timestamp <= END
    ↓
Query both tables
    ├─ SELECT * FROM security_events WHERE user_id='user-123' AND timestamp >= START
    └─ SELECT * FROM graphql_operations WHERE user_id='user-123' AND timestamp >= START
    ↓
Union results (both security events and operations)
    ↓
Format as AuditEvent objects
    ↓
Return combined list[AuditEvent]
```

### Scenario 3: Compliance Report

```
User Request: compliance_report(start, end, include_breakdown=True)
    ↓
Query date range
    ├─ SELECT * FROM security_events WHERE timestamp BETWEEN start AND end
    ├─ SELECT * FROM graphql_operations WHERE timestamp BETWEEN start AND end
    └─ UNION both results
    ↓
Aggregate statistics
    ├─ COUNT(*) → total_events
    ├─ COUNT(*) WHERE severity='critical' → critical_events
    ├─ COUNT(*) WHERE result='error' → error_events
    └─ GROUP BY event_type → events_by_type
    ↓
Build ComplianceReport object
    ├─ Timestamps and metadata
    ├─ Aggregate counts
    ├─ Event type breakdown
    ├─ User breakdown
    └─ List of failed operations
    ↓
Return ComplianceReport
```

---

## Dependencies & Prerequisites

### Required (Must Exist First)

1. **Phase 14: SecurityLogger** ✅
   - Must have: `security_events` table with indexed columns
   - Must have: SecurityEventType, SecurityEventSeverity enums
   - Must have: SecurityEvent dataclass

2. **Commit 4.5: GraphQL Operation Monitoring** ✅
   - Must have: Operation metrics collection working
   - Must have: W3C Trace Context support
   - Must have: graphql_operations table (or in-memory storage)

3. **Commit 1: FraiseQLConfig** ✅
   - Must have: Observability configuration fields
   - Must have: audit_retention_days setting

4. **PostgreSQL**
   - Must have: async_engine with proper connection pooling
   - Must have: indexed tables for performance

### Optional (Enhanced Features)

- `pandas`: For advanced analytics in export
- `reportlab`: For PDF compliance reports
- `openpyxl`: For Excel export

---

## API Integration Points

### 1. With FastAPI Dependencies

```python
from fastapi import Depends

async def get_audit_builder(session: AsyncSession = Depends(get_session)):
    """Dependency for accessing AuditLogQueryBuilder."""
    return AuditLogQueryBuilder(session)

@app.get("/api/audit/operations")
async def get_operations(
    builder: AuditLogQueryBuilder = Depends(get_audit_builder),
    limit: int = 50
):
    """API endpoint using audit builder."""
    ops = await builder.recent_operations(limit=limit)
    return [
        {
            "timestamp": op.timestamp.isoformat(),
            "type": op.event_type,
            "duration_ms": op.duration_ms,
            "result": op.result,
        }
        for op in ops
    ]
```

### 2. With CLI Commands (Commit 7)

```python
# fraiseql observability audit recent --limit 50
# fraiseql observability audit by-user user-123 --hours 24
# fraiseql observability audit compliance --month 2026-01
```

### 3. With Health Checks (Commit 6)

```python
# /health/query-builder endpoint to verify audit query functionality
# Returns 200 if queries are responsive
# Returns 503 if queries are timing out
```

---

## Testing Strategy

### Unit Tests (Commit 5)
- Test data models (AuditEvent, ComplianceReport)
- Test query builder methods
- Test analyzer functions
- Mock database responses

### Integration Tests (Commit 8)
- Real database queries
- Multi-table joins
- Compliance report generation
- Performance benchmarks

### Example Test Scenarios

```python
# Test 1: Query recent operations
async def test_recent_operations():
    # Insert test data into graphql_operations
    # Call builder.recent_operations(limit=10)
    # Assert: returns <= 10 results
    # Assert: ordered by timestamp DESC
    # Assert: all have operation_type set

# Test 2: Filter by user
async def test_by_user():
    # Insert security_events for user-123
    # Insert graphql_operations for user-123
    # Call builder.by_user("user-123", hours=24)
    # Assert: returns both security events AND operations
    # Assert: all have user_id = "user-123"

# Test 3: Compliance report
async def test_compliance_report():
    # Insert events in date range
    # Call builder.compliance_report(start, end)
    # Assert: report.total_events > 0
    # Assert: report.critical_events + report.warning_events <= total
    # Assert: sum of event_by_type breakdown = total
```

---

## Performance Characteristics

### Query Performance

| Query Type | Target Latency | Notes |
|-----------|-----------------|-------|
| Recent operations (50) | < 50ms | Simple ORDER BY timestamp DESC LIMIT |
| By user (24h) | < 150ms | User ID index + timestamp range |
| By entity | < 200ms | Resource index + timestamp |
| Failed operations (24h) | < 100ms | Status index + timestamp range |
| Compliance report (1 month) | < 500ms | Requires full table scan + aggregation |
| Export (10K events) | < 1s | Streaming to CSV/JSON |

### Storage Impact

- **Per security event**: ~500 bytes
- **Per GraphQL operation**: ~400 bytes
- **With 10K events**: ~9 MB
- **Compliance report cache**: ~50 KB

### Memory Usage

- **Builder instance**: ~1 KB
- **Results cache (1000 events)**: ~1 MB
- **Compliance report in memory**: ~100 KB

---

## Deployment Checklist

### Pre-Deployment

- [ ] All Phase 14 tables migrated to production
- [ ] All Commit 4.5 operation metrics flowing
- [ ] All indexes created on production database
- [ ] Retention policies configured in Commit 1
- [ ] Database connection pooling tuned

### Deployment

- [ ] Deploy Commit 5 code
- [ ] Verify audit module imports work
- [ ] Run smoke tests on production data
- [ ] Monitor query latencies
- [ ] Set up alerts for slow queries

### Post-Deployment

- [ ] Monitor compliance report generation times
- [ ] Track user adoption of audit queries
- [ ] Verify data accuracy (spot-check results)
- [ ] Monitor database connection pool utilization

---

## Rollback Plan

If issues are discovered:

1. **Disable audit queries**: Set `observability_enabled = false` in config
2. **Scale database**: If query latency issues, add read replicas
3. **Revert code**: Roll back Commit 5 deployment
4. **Investigate**: Check table sizes, index fragmentation, query plans

---

## Documentation & Examples

### For Users

```markdown
# Audit Log Query Builder

The Audit Log Query Builder provides easy access to:

## Get recent operations
```python
from fraiseql.audit import AuditLogQueryBuilder

builder = AuditLogQueryBuilder(session)
recent = await builder.recent_operations(limit=50)
```

## Get user activity
```python
user_events = await builder.by_user("user-123", hours=24)
```

## Generate compliance report
```python
report = await builder.compliance_report(
    start_date=datetime(2026, 1, 1),
    end_date=datetime(2026, 1, 31),
)
print(f"Total events: {report.total_events}")
```
```

### For Developers

- Integration point: Phase 14 SecurityLogger
- Integration point: Commit 4.5 GraphQL operations
- Database: PostgreSQL with indexes
- Async: Full async/await support
- Type-safe: Dataclass models

---

## Future Enhancements

### Phase 20: Persistent Operation Metrics
- Store GraphQL operations in database (currently in-memory)
- Integrate with Prometheus/Grafana dashboards
- Long-term retention of operation metrics

### Phase 21: Advanced Analysis
- Anomaly detection in operation patterns
- Machine learning-based slow operation prediction
- Automated compliance report generation

### Future: OpenTelemetry Integration
- Export audit events to OpenTelemetry collector
- Integration with Jaeger, Datadog, New Relic
- Distributed tracing across services

---

## Summary

**Commit 5** serves as a **unifying query layer** for Phase 14 security events and Commit 4.5 operation metrics, providing:

✅ **Simple API** for common queries
✅ **Chainable filters** for complex scenarios
✅ **Compliance reports** for audit requirements
✅ **Export functionality** for external systems
✅ **Analysis helpers** for operational insight
✅ **Full integration** with existing systems
✅ **Production-ready** performance and reliability

**Ready for implementation** with all dependencies met and integration points defined.

---

*Commit 5 Integration Summary*
*Phase 19, Date: January 4, 2026*
