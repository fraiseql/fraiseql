# Phase 19 Monitoring Integration - Deployment Guide

**Version**: 1.0
**Date**: January 2026
**Status**: Integration testing complete, ready for production

---

## Overview

Phase 19 introduces comprehensive monitoring integration for FraiseQL with:
- **PostgreSQL-native database monitoring** with query metrics and pool tracking
- **GraphQL operation metrics** from the Rust HTTP server
- **Health check aggregation** across all components
- **W3C Trace Context** for distributed tracing support
- **CLI monitoring commands** with table/JSON/CSV output formats
- **Audit logging** integration with Python API

---

## Quick Start

### Prerequisites

- Python 3.13+
- PostgreSQL 13+
- Rust toolchain (for compilation)

### Installation

```bash
# Install FraiseQL with monitoring support
pip install fraiseql

# Enable monitoring in your application
from fraiseql.monitoring.runtime.db_monitor_sync import get_database_monitor

monitor = get_database_monitor()
```

### Basic Usage

```python
# Get database statistics
db_sync = monitor
stats = db_sync.get_statistics()
print(f"Total queries: {stats.total_count}")
print(f"Success rate: {stats.success_rate * 100}%")

# Get recent queries
recent = db_sync.get_recent_queries(limit=10)
for query in recent:
    print(f"{query.query_type}: {query.duration_ms}ms")

# Get slow queries
slow = db_sync.get_slow_queries(limit=5)
for query in slow:
    print(f"SLOW: {query.sql} ({query.duration_ms}ms)")
```

---

## CLI Commands

Phase 19 adds monitoring commands to the FraiseQL CLI:

### Database Monitoring

```bash
# Show recent queries
fraiseql monitoring database recent [--limit 10]

# Show slow queries
fraiseql monitoring database slow [--limit 5]

# Show statistics
fraiseql monitoring database stats

# Show connection pool metrics
fraiseql monitoring database pool
```

### Cache Monitoring

```bash
# Show cache statistics
fraiseql monitoring cache stats

# Show cache health
fraiseql monitoring cache health
```

### Health Status

```bash
# Show overall health
fraiseql monitoring health status

# Show detailed component health
fraiseql monitoring health components
```

### Output Formats

All commands support multiple output formats:

```bash
# Default (table format)
fraiseql monitoring database recent

# JSON output
fraiseql monitoring database recent --format json

# CSV output
fraiseql monitoring database recent --format csv
```

---

## Component Integration

### Database Monitoring

Query metrics are automatically collected from:
- **SQL execution**: Duration, row count, success/failure status
- **Query types**: SELECT, INSERT, UPDATE, DELETE
- **Slow query detection**: Configurable threshold (default 100ms)
- **Connection pool**: Utilization, wait times, active connections

### GraphQL Operation Monitoring

Operation metrics are collected by the Rust HTTP server:
- **Operation ID**: Unique identifier per operation
- **Operation type**: Query, mutation, or subscription
- **Duration**: Total execution time in milliseconds
- **Query complexity**: Character count of GraphQL query
- **Trace context**: W3C trace context for distributed tracing

### Health Checks

Aggregate health status from:
- **Database**: Connection pool utilization, slow query rate
- **Cache**: Hit rate, eviction rate, memory usage
- **GraphQL**: Operation success rate, slow operation rate
- **Tracing**: Trace context propagation status

---

## Configuration

### Environment Variables

```bash
# PostgreSQL connection
export DATABASE_URL="postgresql://user:password@localhost:5432/fraiseql"

# Monitoring settings
export FRAISEQL_SLOW_QUERY_MS=100          # Slow query threshold
export FRAISEQL_HEALTH_CHECK_INTERVAL_S=30  # Health check interval
export FRAISEQL_TRACE_ENABLED=true          # Enable W3C trace context
```

### Programmatic Configuration

```python
from fraiseql.monitoring.models import OperationMonitorConfig

config = OperationMonitorConfig(
    sampling_rate=1.0,  # 0.0-1.0
    enable_slow_query_tracking=True,
    slow_query_threshold_ms=100.0,
)
```

---

## Performance Targets

All monitoring overhead is measured and validated:

| Component | Target | Status |
|-----------|--------|--------|
| Rust operations | < 0.15ms | ✅ Met |
| Python operations | < 1.0ms | ✅ Met |
| Health checks | < 100ms | ✅ Met |
| Database check | < 50ms | ✅ Met |
| Cache check | < 10ms | ✅ Met |
| Audit queries | < 500ms | ✅ Met |
| CLI response | < 2s worst case | ✅ Met |

See `test_performance_validation.py` for detailed benchmarks.

---

## Testing

### Run Integration Tests

```bash
# Run all Phase 19 tests
pytest tests/integration/monitoring/ -v

# Run specific test category
pytest tests/integration/monitoring/test_e2e_postgresql.py -v
pytest tests/integration/monitoring/test_concurrent_operations.py -v
pytest tests/integration/monitoring/test_component_integration.py -v
pytest tests/integration/monitoring/test_performance_validation.py -v
```

### Test Coverage

- **80+ integration tests** across 4 test files
- **E2E PostgreSQL tests**: Database monitoring, GraphQL operations, health checks, trace context, CLI commands
- **Concurrent operation tests**: Thread safety, load testing, cache behavior, connection pool stress
- **Component integration tests**: Rust↔Python data flow, error handling, configuration changes, data consistency
- **Performance validation tests**: Overhead verification, health check timing, audit query performance, CLI response time

---

## Troubleshooting

### Slow Queries Not Being Detected

1. Check the slow query threshold:
```python
from fraiseql.cli.monitoring.database_commands import _get_db_monitor
monitor = _get_db_monitor()
print(f"Threshold: {monitor._slow_query_threshold_ms}ms")
```

2. Adjust if needed:
```python
monitor._slow_query_threshold_ms = 50.0  # Lower threshold
```

### No Metrics Being Collected

1. Verify monitoring is enabled:
```python
from fraiseql.cli.monitoring.database_commands import _get_db_monitor
monitor = _get_db_monitor()
print(f"Recent queries: {len(monitor._recent_queries)}")
```

2. Check database connection:
```python
from fraiseql.monitoring.runtime.db_monitor_sync import get_database_monitor
db_monitor = get_database_monitor()
stats = db_monitor.get_statistics()
print(f"Stats available: {stats is not None}")
```

### High Memory Usage

1. Recent queries buffer might be growing unbounded. Implement rotation:
```python
monitor = _get_db_monitor()
with monitor._lock:
    # Keep only recent 10,000 queries
    if len(monitor._recent_queries) > 10000:
        monitor._recent_queries = monitor._recent_queries[-10000:]
```

### CLI Commands Timing Out

1. Reduce query limit:
```bash
fraiseql monitoring database recent --limit 5
```

2. Check database performance:
```bash
fraiseql monitoring database stats
```

---

## Migration from Phase 18

Phase 19 is **fully backward compatible** with Phase 18:

### What's New

- PostgreSQL-native query metrics collection
- GraphQL operation metrics from Rust HTTP server
- Health check aggregation system
- W3C Trace Context support
- CLI monitoring commands with multiple output formats

### What's Unchanged

- Core GraphQL type system
- Database connectivity
- Cache integration
- Authentication/authorization

### Migration Steps

1. **No code changes required** - Phase 19 is transparent
2. **Enable monitoring CLI**:
   ```bash
   # New commands available immediately
   fraiseql monitoring database recent
   ```
3. **Optional**: Use performance monitoring in your application
   ```python
   from fraiseql.cli.monitoring.database_commands import _get_db_monitor
   monitor = _get_db_monitor()
   ```

---

## Production Deployment

### Recommended Settings

```bash
# Production environment variables
export DATABASE_URL="postgresql://user:pass@db-prod:5432/fraiseql"
export FRAISEQL_SLOW_QUERY_MS=500          # Higher threshold in production
export FRAISEQL_HEALTH_CHECK_INTERVAL_S=60 # Longer check interval
export FRAISEQL_TRACE_ENABLED=true         # Enable distributed tracing
```

### Monitoring Best Practices

1. **Set appropriate slow query threshold** (100-500ms based on SLA)
2. **Monitor health check responses** (should be < 100ms)
3. **Rotate recent query buffer** (keep last 10,000-50,000 queries)
4. **Enable trace context** for distributed tracing
5. **Export metrics** to your monitoring system (Prometheus, DataDog, etc.)

### High Availability

- **Health checks are thread-safe** - safe for concurrent access
- **No external dependencies** - works with PostgreSQL alone
- **Graceful degradation** - continues to function if database is temporarily unavailable

---

## API Reference

### DatabaseMonitorSync

```python
from fraiseql.monitoring.runtime.db_monitor_sync import DatabaseMonitorSync, get_database_monitor

monitor = get_database_monitor()

# Get statistics
stats = monitor.get_statistics()
print(stats.total_count, stats.success_rate, stats.error_count)

# Get queries
recent = monitor.get_recent_queries(limit=10)
slow = monitor.get_slow_queries(limit=5)

# Get pool metrics
pool = monitor.get_pool_metrics()
print(pool.get_utilization_percent())
```

### QueryMetrics

```python
from fraiseql.monitoring.models import QueryMetrics

metric = QueryMetrics(
    query_type="SELECT",
    sql="SELECT * FROM users",
    duration_ms=10.5,
    rows_affected=100,
    error=None,  # or error message string
)

# Check if successful
is_success = metric.is_success()

# Access fields
print(metric.timestamp, metric.query_type, metric.duration_ms)
```

### OperationMetrics

```python
from fraiseql.monitoring.models import OperationMetrics, GraphQLOperationType

op = OperationMetrics(
    operation_id="op-123",
    operation_name="GetUser",
    operation_type=GraphQLOperationType.Query,
    query_length=150,
)

# Set duration
op.set_duration(25.5)

# Access
print(op.operation_id, op.duration_ms, op.operation_type.value)
```

---

## Support & Documentation

For more information:
- **Phase 19 Plan**: `docs/phases/COMMIT-8-REVISED-PLAN.md`
- **Test Details**: `tests/integration/monitoring/`
- **Architecture**: `fraiseql_rs/src/http/`

---

## Version Information

- **Phase**: 19
- **Commit**: 8 (Integration Testing)
- **Components**:
  - `fraiseql_rs/src/http/operation_metrics.rs` - GraphQL metrics
  - `src/fraiseql/monitoring/` - Python monitoring framework
  - `src/fraiseql/audit/` - Audit logging
  - `src/fraiseql/health/` - Health checks
  - `tests/integration/monitoring/` - Integration tests

---

*Last Updated: January 2026*
*Status: Production Ready*
