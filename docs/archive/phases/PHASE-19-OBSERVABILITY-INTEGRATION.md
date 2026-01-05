# Phase 19: Observability Integration Layer

**Status**: Planning
**Target Version**: v2.0.0-rc1
**Duration**: 2-3 weeks
**Priority**: CRITICAL (blocks v2.0.0 release)

---

## 🎯 Objective

Create a unified observability integration layer that connects FraiseQL's existing components (Audit Logging, HTTP Server, Redis Cache, Metrics) into a cohesive observability story. This phase bridges the gap between production-grade components and production-grade monitoring.

**Current State**: Components exist independently
- ✅ Phase 14: Audit Logging (Rust-based, 100x faster)
- ✅ Phase 17A: Redis Cache with coherency validation
- ✅ Phase 18: HTTP/2 native server
- ✅ Prometheus metrics (partial implementation)
- ❌ Unified observability narrative

**Target State**: Integrated observability platform
- ✅ Automatic metrics collection from HTTP requests
- ✅ Audit log query interface with common patterns
- ✅ Cache hit/miss tracking and reporting
- ✅ Request tracing middleware
- ✅ GraphQL operation complexity tracking
- ✅ Database query performance monitoring
- ✅ Health check framework

---

## 📊 Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    FraiseQL Application                     │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│             Phase 19 Observability Layer                    │
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐   │
│  │   Metrics   │  │   Request   │  │  Audit Log      │   │
│  │ Middleware  │  │   Tracing   │  │  Query Builder  │   │
│  └─────────────┘  └─────────────┘  └─────────────────┘   │
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐   │
│  │   Cache     │  │ Database    │  │  Health Check   │   │
│  │   Tracking  │  │   Monitoring│  │   Framework     │   │
│  └─────────────┘  └─────────────┘  └─────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│            Existing FraiseQL Components                     │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────────┐    │
│  │  Audit   │  │  Redis   │  │  Prometheus Metrics  │    │
│  │  Logger  │  │  Cache   │  │  (existing)          │    │
│  │(Phase 14)│  │(Phase17A)│  │                      │    │
│  └──────────┘  └──────────┘  └──────────────────────┘    │
│                                                             │
│  ┌──────────────────────────────────────────────────┐    │
│  │        HTTP/2 Server (Phase 18)                  │    │
│  └──────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│              PostgreSQL + Redis                            │
│            (Audit Logs + Cache Data)                       │
└─────────────────────────────────────────────────────────────┘
```

---

## 📋 Implementation Breakdown

### Commit 1: Metrics Collection Framework

**Files to Create/Modify**:
- `src/fraiseql/observability/__init__.py` (new module)
- `src/fraiseql/observability/metrics_collector.py` (new - unified metrics aggregator)
- `src/fraiseql/observability/middleware.py` (new - HTTP middleware)
- `src/fraiseql/fastapi/middleware.py` (modify - add hooks)

**Scope**:
1. Create unified metrics collection framework
   - Central metrics aggregator
   - Decorator-based metric registration
   - Automatic middleware integration

2. Implement metrics middleware
   - Request lifecycle tracking (start → execution → response)
   - Automatic Prometheus metric recording
   - Minimal overhead (<1ms per request)

3. Metrics to collect:
   ```
   - fraiseql_http_requests_total (counter: method, status, endpoint)
   - fraiseql_http_request_duration_ms (histogram: operation, mode)
   - fraiseql_cache_hits_total (counter: cache_type, hit/miss)
   - fraiseql_cache_duration_ms (histogram: operation, cache_type)
   - fraiseql_graphql_query_complexity (gauge: operation)
   - fraiseql_graphql_query_depth (gauge: operation)
   - fraiseql_db_pool_active_connections (gauge)
   - fraiseql_db_pool_idle_connections (gauge)
   - fraiseql_rust_pipeline_duration_us (histogram)
   - fraiseql_audit_events_logged (counter: event_type, severity)
   ```

**Tests**:
- Unit tests for metrics aggregation
- Middleware integration tests
- Performance tests (verify <1ms overhead)
- Edge cases (null values, missing context)

**Deliverable**: ~400 lines of code + tests
- Metrics collector class
- Middleware integration
- Prometheus exporter updates

---

### Commit 2: Request Tracing & Context Propagation

**Files to Create/Modify**:
- `src/fraiseql/observability/tracing.py` (new - request tracing)
- `src/fraiseql/observability/context.py` (new - context management)
- `src/fraiseql/fastapi/routers.py` (modify - add tracing hooks)

**Scope**:
1. Request context tracking
   - Generate `request_id` and `trace_id` for every request
   - Propagate through entire request lifecycle
   - Include in all logging/monitoring

2. Operation tracking
   - Track GraphQL operation start → parsing → execution → serialization
   - Record execution mode (Rust pipeline vs Python vs APQ cache)
   - Capture field selection details
   - Record error details (if any)

3. Distributed tracing support
   - Support `X-Trace-ID`, `X-Request-ID` headers
   - Support W3C Trace Context standard
   - Propagate to downstream calls

4. Context object
   ```python
   @dataclass
   class RequestContext:
       request_id: str
       trace_id: str
       operation_name: str | None
       operation_type: str  # query, mutation, subscription
       start_time: float
       graphql_mode: str  # rust, python, apq, passthrough
       user_id: str | None
       complexity_score: int | None
       field_count: int
       depth: int
       cache_hit: bool | None
       duration_ms: float | None
       error: Exception | None
   ```

**Tests**:
- Context propagation tests
- Header parsing tests
- Multi-operation query tests
- Error handling tests

**Deliverable**: ~300 lines of code + tests
- Tracing middleware
- Context management
- Header integration

---

### Commit 3: Cache Monitoring

**Files to Create/Modify**:
- `src/fraiseql/observability/cache_monitor.py` (new - cache metrics)
- `src/fraiseql/caching/redis.py` (modify - add hooks)
- `src/fraiseql/enterprise/security/audit.py` (modify - add hook)

**Scope**:
1. Cache hit/miss tracking
   - Track per-cache-type (query, field, result)
   - Calculate hit rates
   - Measure access patterns

2. Cache coherency monitoring
   - Track invalidation events
   - Measure cache coherency percentage
   - Alert on cascading invalidations

3. Cache performance metrics
   - Measure cache access latency
   - Compare cached vs uncached performance
   - Track memory usage (if applicable)

4. APQ cache metrics (for Phase 15A APQ)
   - Track persisted query hits
   - Measure bandwidth savings
   - Cache size and growth

**Metrics to add**:
```
- fraiseql_cache_operation_duration_ms (get, set, delete)
- fraiseql_cache_hit_rate (by cache type)
- fraiseql_cache_coherency_pct (Phase 17A validation)
- fraiseql_cache_size_bytes
- fraiseql_apq_hit_rate
```

**Tests**:
- Cache monitoring integration tests
- Hit/miss calculation tests
- Coherency tracking tests

**Deliverable**: ~250 lines of code + tests
- Cache monitor class
- Hooks in cache implementation
- Metric recording

---

### Commit 4: Database Query Monitoring

**Files to Create/Modify**:
- `src/fraiseql/observability/db_monitor.py` (new - database metrics)
- `src/fraiseql/core/rust_pipeline.py` (modify - add timing)
- `src/fraiseql/db.py` (modify - add pool monitoring)

**Scope**:
1. Query performance monitoring
   - Measure database query duration
   - Track query complexity
   - Identify slow queries (P95, P99)

2. Connection pool monitoring
   - Track active connections
   - Track idle connections
   - Alert on pool saturation
   - Measure connection wait time

3. Transaction monitoring
   - Track transaction duration
   - Track rollback rate
   - Measure contention

4. Slow query detection
   - Identify queries >100ms
   - Log slow query details
   - Alert on threshold breach

**Metrics to add**:
```
- fraiseql_db_query_duration_ms (operation)
- fraiseql_db_pool_utilization_pct
- fraiseql_db_transaction_duration_ms
- fraiseql_db_slow_queries_total
- fraiseql_db_connection_wait_ms
```

**Tests**:
- Query timing tests
- Pool utilization tests
- Slow query detection tests

**Deliverable**: ~300 lines of code + tests
- Database monitor class
- Query timing instrumentation
- Pool monitoring

---

### Commit 5: Audit Log Query Builder

**Files to Create/Modify**:
- `src/fraiseql/observability/audit_queries.py` (new - query builder)
- `src/fraiseql/observability/audit_analyzer.py` (new - analysis helpers)
- Tests for query builder

**Scope**:
1. Pre-built audit log queries
   - Most recent operations
   - Operations by user
   - Operations by entity
   - Errors and failures
   - Permission changes
   - Data access patterns

2. Query builder helper
   ```python
   class AuditLogQueryBuilder:
       @staticmethod
       async def recent_operations(pool, limit=100) -> list[dict]
       @staticmethod
       async def by_user(pool, user_id: str, limit=100) -> list[dict]
       @staticmethod
       async def by_entity(pool, entity_type: str, entity_id: str) -> list[dict]
       @staticmethod
       async def failed_operations(pool, hours=24) -> list[dict]
       @staticmethod
       async def permission_changes(pool, hours=24) -> list[dict]
       @staticmethod
       async def data_access(pool, user_id: str, entity_type: str) -> list[dict]
       @staticmethod
       async def compliance_report(pool, start_date: date, end_date: date) -> dict
   ```

3. Analysis helpers
   - Calculate operation statistics
   - Identify access patterns
   - Detect anomalies
   - Generate compliance reports

**Tests**:
- Query builder tests
- Edge case tests
- Empty result tests
- Large dataset tests

**Deliverable**: ~400 lines of code + tests
- Audit query builder
- Analysis helpers
- Documentation with examples

---

### Commit 6: Health Check Framework

**Files to Create/Modify**:
- `src/fraiseql/observability/health.py` (new - health framework)
- `src/fraiseql/fastapi/app.py` (modify - add health endpoints)

**Scope**:
1. Health check framework
   - Composable health checks
   - Liveness, readiness, startup probes
   - Kubernetes-compatible

2. Built-in health checks
   - Database connectivity
   - Database pool health
   - Cache connectivity (Redis)
   - Required services
   - Application startup status

3. Custom health checks
   - Allow applications to register custom checks
   - Support async checks
   - Timeout handling

4. Health endpoints
   - `/health/live` - Liveness (always success, checks crash)
   - `/health/ready` - Readiness (checks dependencies)
   - `/health/startup` - Startup (checks initialization)

**Response format**:
```json
{
  "status": "healthy|degraded|unhealthy",
  "timestamp": "2025-01-04T...",
  "checks": {
    "database": {
      "status": "healthy",
      "latency_ms": 5.2,
      "pool": {
        "active": 12,
        "idle": 3,
        "max": 20
      }
    },
    "cache": {
      "status": "healthy",
      "latency_ms": 0.8,
      "hit_rate": 0.92
    }
  }
}
```

**Tests**:
- Health check lifecycle tests
- Kubernetes probe tests
- Custom check registration tests
- Timeout handling tests

**Deliverable**: ~350 lines of code + tests
- Health check framework
- Built-in checks
- Kubernetes integration

---

### Commit 7: Observability CLI & Configuration

**Files to Create/Modify**:
- `src/fraiseql/observability/config.py` (new - configuration)
- `src/fraiseql/observability/cli.py` (new - CLI tools)
- `docs/observability/` (new - documentation)

**Scope**:
1. Configuration management
   - Enable/disable specific metrics
   - Set sampling rates
   - Configure thresholds
   - Configure alerts

2. CLI tools for common queries
   ```bash
   fraiseql-observe audit recent-operations --limit 50
   fraiseql-observe audit by-user --user-id <id> --limit 100
   fraiseql-observe audit entity-history --entity-type users --entity-id <id>
   fraiseql-observe cache stats
   fraiseql-observe db pool-status
   fraiseql-observe metrics export prometheus
   fraiseql-observe health check
   ```

3. Configuration options
   ```python
   @dataclass
   class ObservabilityConfig:
       metrics_enabled: bool = True
       tracing_enabled: bool = True
       audit_logging_enabled: bool = True
       cache_monitoring_enabled: bool = True
       slow_query_threshold_ms: int = 100
       sample_rate: float = 1.0
       include_query_bodies: bool = False  # for privacy
       audit_retention_days: int = 90
   ```

**Tests**:
- Configuration tests
- CLI integration tests

**Deliverable**: ~300 lines of code + tests
- Configuration system
- CLI tools
- Documentation

---

### Commit 8: Integration Tests & Documentation

**Files to Create/Modify**:
- `tests/integration/observability/` (new - comprehensive tests)
- `docs/observability/integration-guide.md` (new)
- `docs/observability/metrics-reference.md` (new)
- `docs/observability/audit-queries.md` (new)
- `docs/observability/health-checks.md` (new)

**Scope**:
1. Integration test suite
   - End-to-end observability tests
   - Multi-request tracing tests
   - Cache + metrics coordination tests
   - Error scenario tests
   - Performance regression tests

2. Documentation
   - Integration guide (getting started)
   - Metrics reference (all metrics)
   - Audit query examples
   - Health check setup
   - Troubleshooting guide

3. Example applications
   - Full-featured example with all observability
   - Kubernetes deployment example
   - Grafana dashboard setup guide

**Tests**:
- 50+ integration tests
- Performance benchmarks
- Documentation examples (runnable)

**Deliverable**: ~60 integration tests + comprehensive documentation

---

## 🧪 Testing Strategy

### Unit Tests (by feature)
- Metrics collector: 15 tests
- Request tracing: 10 tests
- Cache monitoring: 12 tests
- Database monitoring: 12 tests
- Audit queries: 15 tests
- Health checks: 12 tests

### Integration Tests
- Request lifecycle with all components: 8 tests
- Error handling across components: 6 tests
- Multi-request tracing: 4 tests
- Performance regression: 5 tests
- Kubernetes probe simulation: 4 tests

**Total**: ~100 new tests
**Coverage goal**: >85% for new code

---

## ✅ Acceptance Criteria

### Functional
- [x] All metrics collected without breaking existing tests
- [x] Request tracing propagates correctly through pipeline
- [x] Cache hit/miss rates calculated accurately
- [x] Database query timing captured
- [x] Audit log queries return correct data
- [x] Health checks pass for all components
- [x] CLI tools work correctly
- [x] <1ms overhead per request

### Performance
- [x] Metrics collection adds <1ms latency
- [x] Request tracing adds <0.5ms overhead
- [x] Health checks complete in <500ms
- [x] Audit queries execute in <500ms for typical data

### Documentation
- [x] Integration guide complete
- [x] Metrics reference comprehensive
- [x] All examples runnable
- [x] Troubleshooting section covers common issues

### Testing
- [x] All tests passing (5,991+ existing + 100 new)
- [x] No regressions in existing functionality
- [x] 85%+ code coverage on new code
- [x] Performance benchmarks passing

---

## 📈 Success Metrics

After Phase 19 completion, users should be able to:

1. **Monitor**: See all FraiseQL operations with metrics (requests, cache, database)
2. **Trace**: Follow single request through entire pipeline
3. **Query**: Find operations, users, entities in audit logs
4. **Alert**: Set up alerts on error rates, slow queries, pool saturation
5. **Debug**: Quickly identify performance issues and root causes
6. **Comply**: Generate audit reports for compliance

---

## 🚀 Release Notes Preview

```markdown
### Phase 19: Observability Integration

Introduces production-grade observability integration layer connecting audit
logging, caching, database monitoring, and HTTP metrics.

#### New Features
- Unified metrics collection framework
- Request tracing with context propagation
- Cache monitoring (hit rates, coherency)
- Database query performance monitoring
- Audit log query builder with common patterns
- Health check framework (Kubernetes-compatible)
- Observability CLI tools

#### Metrics Added
- 10 new Prometheus metrics covering all layers
- Request tracing with W3C standards support
- Cache coherency tracking
- Database pool and slow query monitoring

#### Breaking Changes
None - fully backward compatible

#### Performance Impact
- <1ms per-request overhead
- All monitoring optional via configuration
```

---

## 🔗 Dependencies

- Phase 14: Audit Logging (required)
- Phase 17A: Redis Cache (required)
- Phase 18: HTTP/2 Server (required)
- Prometheus Python client (existing)
- PostgreSQL (existing)

---

## 📝 Notes

- All metrics are optional and can be disabled via configuration
- Request tracing includes support for distributed tracing (W3C standards)
- Audit queries are optimized for PostgreSQL 12+
- Health checks are Kubernetes-native
- Framework is extensible for custom metrics
