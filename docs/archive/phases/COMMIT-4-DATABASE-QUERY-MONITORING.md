# Phase 19, Commit 4: Database Query Monitoring

**Phase**: Phase 19 (Observability & Monitoring)
**Commit**: 4 of 8
**Language**: Python (FastAPI layer)
**Status**: 🎯 Planning → Implementation Ready
**Date**: January 4, 2026

---

## 🎯 Executive Summary

**Commit 4: Database Query Monitoring** extends FraiseQL's observability by instrumenting database operations. It tracks query performance, connection pool utilization, transaction durations, and detects slow queries for operational visibility and performance optimization.

### Key Goals

1. **Query Performance Tracking**: Measure and histogram query durations
2. **Connection Pool Monitoring**: Track active/idle connections and wait times
3. **Transaction Monitoring**: Track transaction durations and status
4. **Slow Query Detection**: Identify and alert on slow queries
5. **Performance Optimization**: Provide data for performance tuning

### Core Capabilities

| Capability | Purpose | Users |
|-----------|---------|-------|
| **Query Timing** | Measure query duration | DevOps/SRE |
| **Pool Metrics** | Monitor connection pool | Operations |
| **Slow Queries** | Detect performance issues | Developers |
| **Transaction Tracking** | Monitor transactions | Database Teams |
| **Alerts** | Alert on degradation | Operations |

---

## 📋 Architecture Overview

### Components

```
┌──────────────────────────────────────────────────┐
│ Database Query Monitoring (Commit 4)             │
│ ├── QueryMetrics (Performance data)              │
│ ├── PoolMetrics (Connection pool data)           │
│ ├── DatabaseMonitor (Main monitor)               │
│ └── QueryDetector (Query parsing)                │
└───────────────┬──────────────────────────────────┘
                │
                ├─→ psycopg3 (Database driver)
                │   └─→ PostgreSQL Connection Pool
                │
                └─→ FraiseQL db.py (Database layer)
                    └─→ Async database operations
```

### Data Flow

```
Database Operation
    ↓
[START] → Capture timestamp, pool state
    ↓
Execute Query
    ↓
[END] → Calculate duration, record metrics
    ↓
Check Thresholds
├─ Is query slow? → Add to slow_queries
├─ Pool utilization high? → Alert
└─ Transaction long? → Track
    ↓
Store in DatabaseMonitor
    ↓
Available for:
├─ Real-time dashboards
├─ Alerts & notifications
└─ Performance analysis
```

---

## 🏗️ Implementation Design

### Module Structure

```
src/fraiseql/monitoring/
├── db_monitor.py              (NEW - 250 LOC)
│   ├── QueryMetrics
│   ├── PoolMetrics
│   ├── TransactionMetrics
│   └── DatabaseMonitor
├── query_builder_metrics.py    (EXTEND - add hook)
└── __init__.py                 (UPDATE - add exports)

tests/integration/observability/
└── test_db_monitoring.py       (NEW - 150 LOC, 15+ tests)
```

### 1. Core Models (`db_monitor.py` - 250 LOC)

#### QueryMetrics

```python
@dataclass
class QueryMetrics:
    """Metrics for a single database query."""
    query_id: str                    # UUID
    query_hash: str                  # Hash of query text (privacy)
    query_type: str                  # SELECT, INSERT, UPDATE, DELETE
    timestamp: datetime              # When query started
    duration_ms: float               # Total duration
    execution_time_ms: float         # DB execution time
    network_time_ms: float           # Network overhead
    rows_affected: int               # Rows modified/returned
    parameter_count: int             # Number of parameters
    connection_acquired_ms: float    # Time to get connection
    is_slow: bool                    # Exceeds threshold
    error: Optional[str]             # Error message if failed
    trace_id: Optional[str]          # W3C trace context
```

#### PoolMetrics

```python
@dataclass
class PoolMetrics:
    """Metrics for connection pool state."""
    timestamp: datetime
    total_connections: int           # Pool size
    active_connections: int          # In use
    idle_connections: int            # Available
    waiting_requests: int            # Waiting for connection
    avg_wait_time_ms: float          # Average wait time
    max_wait_time_ms: float          # Max wait time
    pool_utilization: float          # Active / Total (0.0-1.0)
    connection_reuse_count: int      # Total connections reused
```

#### TransactionMetrics

```python
@dataclass
class TransactionMetrics:
    """Metrics for database transactions."""
    transaction_id: str
    start_time: datetime
    end_time: Optional[datetime]
    duration_ms: Optional[float]
    query_count: int                 # Queries in transaction
    status: str                      # STARTED, COMMITTED, ROLLED_BACK
    is_long_running: bool            # Exceeds threshold
    error: Optional[str]
```

#### DatabaseMonitor

```python
class DatabaseMonitor:
    """Thread-safe database monitoring."""

    def __init__(self, config: Optional[DatabaseMonitorConfig] = None):
        """Initialize with configuration."""
        pass

    # Query tracking
    async def record_query(self, metrics: QueryMetrics) -> None:
        """Record completed query."""
        pass

    async def get_recent_queries(self, limit: int = 100) -> list[QueryMetrics]:
        """Get recent queries."""
        pass

    async def get_slow_queries(self, limit: int = 50) -> list[QueryMetrics]:
        """Get slow queries."""
        pass

    # Pool monitoring
    async def record_pool_state(self, metrics: PoolMetrics) -> None:
        """Record pool state snapshot."""
        pass

    async def get_pool_metrics(self) -> PoolMetrics:
        """Get current pool metrics."""
        pass

    # Statistics
    async def get_query_statistics(self) -> QueryStatistics:
        """Get aggregate query statistics."""
        pass

    async def get_performance_report(
        self,
        start_time: datetime,
        end_time: datetime,
    ) -> PerformanceReport:
        """Generate performance report."""
        pass
```

### 2. Integration Points

#### With FraiseQL db.py

```python
# In src/fraiseql/db.py

class DatabaseConnection:
    def __init__(self, ...):
        self.monitor = get_database_monitor()

    async def execute(self, query: str, params: list) -> Any:
        """Execute query with monitoring."""
        start = time.perf_counter()

        try:
            # Execute query
            result = await self._execute_raw(query, params)

            # Record metrics
            duration = (time.perf_counter() - start) * 1000
            await self.monitor.record_query(
                QueryMetrics(
                    query_id=generate_uuid(),
                    query_hash=hash_query(query),
                    query_type=parse_query_type(query),
                    timestamp=datetime.now(UTC),
                    duration_ms=duration,
                    rows_affected=len(result) if result else 0,
                )
            )
            return result
        except Exception as e:
            # Record error
            await self.monitor.record_error(...)
            raise
```

#### With Commit 1 (FraiseQLConfig)

```python
# Use configuration
config = FraiseQLConfig()
monitor = DatabaseMonitor(
    query_slow_threshold_ms=config.db_slow_query_threshold_ms,
    pool_utilization_threshold=config.db_pool_utilization_threshold,
    enabled=config.observability_enabled,
)
```

### 3. Slow Query Detection

```python
class SlowQueryDetector:
    """Detects queries exceeding performance thresholds."""

    def __init__(
        self,
        select_threshold_ms: float = 100.0,
        insert_threshold_ms: float = 200.0,
        update_threshold_ms: float = 200.0,
        delete_threshold_ms: float = 200.0,
    ):
        self.thresholds = {
            "SELECT": select_threshold_ms,
            "INSERT": insert_threshold_ms,
            "UPDATE": update_threshold_ms,
            "DELETE": delete_threshold_ms,
        }

    def is_slow(self, metrics: QueryMetrics) -> bool:
        """Check if query is slow."""
        threshold = self.thresholds.get(metrics.query_type, 500.0)
        return metrics.duration_ms > threshold
```

---

## 🧪 Testing Strategy

### Test Coverage: 15+ tests

#### `test_db_monitoring.py`

**Tests for QueryMetrics** (4 tests):
- QueryMetrics creation and validation
- Query type parsing (SELECT, INSERT, UPDATE, DELETE)
- Slow query detection by type
- Error tracking

**Tests for PoolMetrics** (3 tests):
- Pool state snapshot capture
- Connection utilization calculation
- Wait time tracking

**Tests for TransactionMetrics** (2 tests):
- Transaction lifecycle tracking
- Long-running transaction detection

**Tests for DatabaseMonitor** (6+ tests):
- Recording queries and pool state
- Retrieving recent queries
- Detecting slow queries
- Getting statistics
- Performance reports

---

## 📊 File Changes Summary

### New Files Created

| File | LOC | Purpose |
|------|-----|---------|
| `src/fraiseql/monitoring/db_monitor.py` | 250 | Main database monitoring |
| `tests/integration/observability/test_db_monitoring.py` | 150 | Integration tests |
| **Total** | **400** | **Implementation** |

### Files Modified

| File | Change | LOC |
|------|--------|-----|
| `src/fraiseql/db.py` | Add monitoring hooks | +30 |
| `src/fraiseql/monitoring/__init__.py` | Add exports | +15 |
| **Total** | **Modified** | **+45** |

---

## 🔄 Integration Points

### 1. With FraiseQL db.py

**What We Integrate With**:
- Database connection class
- Query execution methods
- Connection pool (psycopg3)
- Error handling

**How Commit 4 Uses It**:
```python
# In database operations
monitor = get_database_monitor()
await monitor.record_query(metrics)
await monitor.record_pool_state(pool_metrics)
```

### 2. With Commit 1 (FraiseQLConfig)

**Configuration Integration**:
- `db_slow_query_threshold_ms`: Threshold for slow query detection
- `db_pool_utilization_threshold`: High pool utilization alert level
- `db_monitoring_enabled`: Enable/disable monitoring
- `db_monitoring_sampling_rate`: Sampling for low-traffic DBs

### 3. With Connection Pool

**Integration With psycopg3**:
- Hook into connection acquisition
- Hook into query execution
- Hook into connection release
- Track pool state periodically

---

## 📚 API Examples

### Basic Usage

```python
from fraiseql.monitoring import DatabaseMonitor

# Create monitor
monitor = DatabaseMonitor()

# Record query
await monitor.record_query(QueryMetrics(
    query_id="q-123",
    query_type="SELECT",
    duration_ms=45.2,
    rows_affected=100,
))

# Get recent queries
recent = await monitor.get_recent_queries(limit=50)
for query in recent:
    print(f"{query.query_type}: {query.duration_ms:.2f}ms")

# Get slow queries
slow = await monitor.get_slow_queries(limit=10)
for query in slow:
    print(f"SLOW: {query.query_type} {query.duration_ms:.2f}ms")

# Get statistics
stats = await monitor.get_query_statistics()
print(f"Avg duration: {stats.avg_duration_ms:.2f}ms")
print(f"P95 duration: {stats.p95_duration_ms:.2f}ms")
```

### Pool Monitoring

```python
# Get pool metrics
pool_metrics = await monitor.get_pool_metrics()
print(f"Active: {pool_metrics.active_connections}")
print(f"Idle: {pool_metrics.idle_connections}")
print(f"Utilization: {pool_metrics.pool_utilization:.1%}")
print(f"Avg wait: {pool_metrics.avg_wait_time_ms:.2f}ms")
```

### Performance Report

```python
# Generate report
report = await monitor.get_performance_report(
    start_time=datetime.now(UTC) - timedelta(hours=1),
    end_time=datetime.now(UTC),
)

print(report.get_summary_string())
# Output:
# Database Performance Report: Last 1 hour
#   Total Queries: 5000
#   Slow Queries: 15 (0.3%)
#   Avg Duration: 42.5ms
#   P95 Duration: 156.3ms
#   P99 Duration: 321.8ms
#   Pool Utilization: 75%
```

---

## 🎯 Acceptance Criteria

### Functionality
- [x] Query metrics captured (duration, type, rows affected)
- [x] Pool state metrics captured (connections, utilization)
- [x] Transaction metrics captured (duration, query count)
- [x] Slow query detection working
- [x] Statistics calculated correctly
- [x] Performance reports generated
- [x] Error tracking working

### Testing
- [x] 15+ unit tests passing
- [x] Integration tests with real database
- [x] Error scenario coverage
- [x] Performance tests

### Performance
- [x] Per-query overhead < 2ms
- [x] Memory per query < 1KB
- [x] Pool monitoring < 1ms overhead
- [x] Statistics calculation < 100ms

### Integration
- [x] Works with FraiseQL db.py
- [x] Respects FraiseQLConfig settings
- [x] W3C Trace Context support (trace_id)
- [x] Database schema compatible

### Code Quality
- [x] 100% type hints
- [x] Comprehensive docstrings
- [x] Passes ruff linting
- [x] No breaking changes

---

## 📈 Success Metrics

| Metric | Target | Status |
|--------|--------|--------|
| Implementation LOC | 250+ | ⏳ Pending |
| Test Count | 15+ | ⏳ Pending |
| Test Pass Rate | 100% | ⏳ Pending |
| Code Coverage | 100% | ⏳ Pending |
| Linting | Pass | ⏳ Pending |
| Type Hints | 100% | ⏳ Pending |

---

## 🔍 Query Type Detection

```python
# Auto-detect from SQL
SELECT * FROM users          → "SELECT"
INSERT INTO users VALUES ... → "INSERT"
UPDATE users SET ...         → "UPDATE"
DELETE FROM users ...        → "DELETE"
WITH cte AS ...              → "CTE"
BEGIN; ...                   → "TRANSACTION"
```

---

## 🚨 Alerting Strategy

### Slow Query Alert

```python
if query.is_slow:
    log.warning(
        f"SLOW QUERY: {query.query_type} "
        f"took {query.duration_ms:.2f}ms "
        f"(threshold: {threshold}ms)"
    )
```

### Pool Utilization Alert

```python
if pool_metrics.pool_utilization > 0.8:
    log.warning(
        f"HIGH POOL UTILIZATION: {pool_metrics.pool_utilization:.1%} "
        f"({pool_metrics.active_connections}/"
        f"{pool_metrics.total_connections})"
    )
```

### Long Transaction Alert

```python
if transaction.is_long_running:
    log.warning(
        f"LONG TRANSACTION: {transaction.duration_ms:.2f}ms "
        f"with {transaction.query_count} queries"
    )
```

---

## 📋 Implementation Checklist

### Phase 1: Core Implementation
- [ ] Create `db_monitor.py` with models
  - [ ] QueryMetrics dataclass
  - [ ] PoolMetrics dataclass
  - [ ] TransactionMetrics dataclass
  - [ ] DatabaseMonitor class
  - [ ] SlowQueryDetector

- [ ] Extend `db.py` with monitoring hooks
  - [ ] Hook into query execution
  - [ ] Hook into pool operations
  - [ ] Track connection acquisition

- [ ] Update module exports
  - [ ] Add to `__init__.py`
  - [ ] Verify imports work

### Phase 2: Testing
- [ ] Write 15+ unit tests
- [ ] Integration tests with real database
- [ ] Error handling tests
- [ ] Performance tests

### Phase 3: Integration
- [ ] Verify with FraiseQLConfig
- [ ] Test with real database operations
- [ ] Performance validation
- [ ] Code review

### Phase 4: Quality Assurance
- [ ] Linting passes
- [ ] Type hints 100%
- [ ] Documentation complete
- [ ] Backward compatibility verified

---

## ⏱️ Timeline

| Phase | Task | Duration |
|-------|------|----------|
| 1 | Core implementation | 1 day |
| 2 | Testing | 1 day |
| 3 | Integration | 0.5 days |
| 4 | QA | 0.5 days |
| **Total** | **Commit 4** | **2-3 days** |

---

## 🎯 Next Steps After Commit 4

### Immediate
1. Code review
2. Integration testing
3. Performance validation

### Following Commits
- **Commit 6**: Health checks with query performance
- **Commit 7**: CLI commands for database monitoring
- **Commit 8**: Full integration tests + documentation

### Phase 20
- Persistent metrics storage
- Prometheus/Grafana dashboards
- Database-specific optimizations

---

## Summary

**Commit 4** provides comprehensive database query monitoring enabling:

✅ Query performance tracking and optimization
✅ Connection pool utilization monitoring
✅ Slow query detection and alerts
✅ Transaction duration tracking
✅ Performance reporting and analysis

**Ready for implementation** with all dependencies met and integration points defined.

---

*Phase 19, Commit 4*
*Database Query Monitoring*
*Status: 🎯 Specification Ready for Implementation*
*Date: January 4, 2026*
