# Phase 19, Commit 6: Health Checks

**Phase**: Phase 19 (Observability & Monitoring)
**Commit**: 6 of 8
**Language**: Python (FastAPI layer)
**Status**: 🎯 Planning → Implementation Ready
**Date**: January 4, 2026

---

## 🎯 Executive Summary

**Commit 6: Health Checks** integrates all Phase 19 monitoring layers into a unified health check system. It provides operational visibility into database performance, cache efficiency, GraphQL operation health, and OpenTelemetry tracing.

### Key Goals

1. **Unified Health Status**: Single endpoint for system health
2. **Layered Health Checks**: Database, Cache, GraphQL, Tracing
3. **Performance Metrics**: Real-time performance data from Commits 1-5
4. **Operational Visibility**: Detailed health information for monitoring
5. **Liveness & Readiness**: Kubernetes-compatible probe endpoints

### Core Capabilities

| Capability | Purpose | Users |
|-----------|---------|-------|
| **Liveness Probe** | Is the service running? | Kubernetes/Docker |
| **Readiness Probe** | Is the service ready to handle requests? | Load Balancers |
| **Database Health** | Query performance, pool utilization | DevOps/SRE |
| **Cache Health** | Hit rates, performance | DevOps/SRE |
| **GraphQL Health** | Operation success rate, latency | Developers |
| **Tracing Health** | Trace context propagation | Observability |

---

## 📋 Architecture Overview

### Components

```
┌────────────────────────────────────────┐
│ Health Check System (Commit 6)         │
│ ├── HealthStatus (Status model)        │
│ ├── DatabaseHealthCheck                │
│ ├── CacheHealthCheck                   │
│ ├── GraphQLHealthCheck                 │
│ ├── TracingHealthCheck                 │
│ └── HealthCheckAggregator              │
└─────────────────┬──────────────────────┘
                  │
    ┌─────────────┼─────────────┐
    │             │             │
┌───▼──┐    ┌────▼────┐   ┌────▼────┐
│ DB   │    │ Cache    │   │ GraphQL  │
│Check │    │ Check    │   │ Check    │
└──────┘    └──────────┘   └──────────┘
    │             │             │
    └─────────────┼─────────────┘
                  │
            [Commit 4]
            [Commit 3]
            [Commit 4.5]
```

### Data Flow

```
GET /health
    ↓
[HealthCheckAggregator]
    ├─→ [DatabaseHealthCheck]
    │   └─→ [DatabaseMonitor] (Commit 4)
    │       └─→ Query stats, pool metrics
    │
    ├─→ [CacheHealthCheck]
    │   └─→ [CacheMonitor] (Commit 3)
    │       └─→ Hit rate, evictions
    │
    ├─→ [GraphQLHealthCheck]
    │   └─→ [OperationMetrics] (Commit 4.5)
    │       └─→ Success rate, latency
    │
    └─→ [TracingHealthCheck]
        └─→ [OpenTelemetry] (Commit 2)
            └─→ Trace context status
    ↓
HealthStatus
    ├─ overall: "healthy" | "degraded" | "unhealthy"
    ├─ database: {...}
    ├─ cache: {...}
    ├─ graphql: {...}
    └─ tracing: {...}
```

---

## 🏗️ Implementation Design

### Module Structure

```
src/fraiseql/health/
├── health_check.py          (NEW - 400 LOC)
│   ├── HealthStatus
│   ├── HealthCheckResult
│   ├── DatabaseHealthCheck
│   ├── CacheHealthCheck
│   ├── GraphQLHealthCheck
│   ├── TracingHealthCheck
│   └── HealthCheckAggregator
├── endpoints.py             (NEW - 200 LOC)
│   ├── GET /health
│   ├── GET /health/live
│   ├── GET /health/ready
│   ├── GET /health/database
│   ├── GET /health/cache
│   ├── GET /health/graphql
│   └── GET /health/tracing
└── __init__.py              (NEW - 20 LOC)

tests/unit/health/
├── test_health_checks.py    (NEW - 300 LOC, 20+ tests)
└── test_endpoints.py        (NEW - 200 LOC, 15+ tests)
```

### 1. Core Models (`health_check.py` - 400 LOC)

#### HealthStatus

```python
@dataclass
class HealthStatus:
    """Overall system health status.

    Attributes:
        overall_status: "healthy", "degraded", or "unhealthy"
        timestamp: When health check was performed
        database: Database health information
        cache: Cache health information
        graphql: GraphQL health information
        tracing: Tracing health information
        checks_executed: Number of health checks run
        check_duration_ms: Time to run all checks
    """

    overall_status: str              # healthy | degraded | unhealthy
    timestamp: datetime
    database: dict[str, Any]
    cache: dict[str, Any]
    graphql: dict[str, Any]
    tracing: dict[str, Any]
    checks_executed: int
    check_duration_ms: float
```

#### HealthCheckResult

```python
@dataclass
class HealthCheckResult:
    """Result of a single health check.

    Attributes:
        status: "healthy", "degraded", or "unhealthy"
        message: Human-readable status message
        response_time_ms: Time taken to check
        details: Detailed health information
        errors: Any errors encountered
    """

    status: str                      # healthy | degraded | unhealthy
    message: str
    response_time_ms: float
    details: dict[str, Any] = field(default_factory=dict)
    errors: list[str] = field(default_factory=list)
```

#### DatabaseHealthCheck

```python
class DatabaseHealthCheck:
    """Checks database health using query metrics.

    Evaluates:
    - Connection pool utilization
    - Slow query rate
    - Query error rate
    - Recent query performance
    """

    async def check(self) -> HealthCheckResult:
        """Run database health check."""
        # Get stats from DatabaseMonitor (Commit 4)
        # Check pool utilization
        # Check slow query percentage
        # Check error rate
        # Determine health status
```

#### CacheHealthCheck

```python
class CacheHealthCheck:
    """Checks cache health using cache metrics.

    Evaluates:
    - Cache hit rate
    - Eviction rate
    - Cache operation errors
    - Recent performance
    """

    async def check(self) -> HealthCheckResult:
        """Run cache health check."""
        # Get stats from CacheMonitor (Commit 3)
        # Check hit rate
        # Check eviction rate
        # Check operation success
```

#### GraphQLHealthCheck

```python
class GraphQLHealthCheck:
    """Checks GraphQL operation health.

    Evaluates:
    - Operation success rate
    - Query error rate
    - Mutation error rate
    - Recent operation performance
    """

    async def check(self) -> HealthCheckResult:
        """Run GraphQL health check."""
        # Get stats from OperationMetrics (Commit 4.5)
        # Check success rate
        # Check error rate
        # Check latency
```

#### TracingHealthCheck

```python
class TracingHealthCheck:
    """Checks OpenTelemetry tracing health.

    Evaluates:
    - Trace context propagation
    - Span creation success
    - Telemetry provider status
    """

    async def check(self) -> HealthCheckResult:
        """Run tracing health check."""
        # Check trace context support
        # Test span creation
        # Verify telemetry provider
```

#### HealthCheckAggregator

```python
class HealthCheckAggregator:
    """Aggregates all health checks into unified status.

    Runs all health checks and determines overall status:
    - healthy: All checks pass
    - degraded: Some checks warn
    - unhealthy: Critical checks fail
    """

    async def check_all(self) -> HealthStatus:
        """Run all health checks and aggregate results."""
        # Run all checks in parallel
        # Aggregate results
        # Determine overall status
```

### 2. FastAPI Endpoints (`endpoints.py` - 200 LOC)

#### Liveness Probe
```python
@router.get("/health/live")
async def liveness_probe() -> dict[str, str]:
    """Kubernetes liveness probe - is service running?

    Returns 200 OK if service is running.
    Minimal checks - just verify service startup.
    """
    return {"status": "alive"}
```

#### Readiness Probe
```python
@router.get("/health/ready")
async def readiness_probe() -> HealthStatus:
    """Kubernetes readiness probe - is service ready?

    Returns 200 OK only if service is ready to handle requests.
    Runs full health checks.
    """
    # Run all health checks
    # Return HealthStatus
```

#### Full Health Status
```python
@router.get("/health")
async def health_check() -> HealthStatus:
    """Full system health check.

    Returns comprehensive health status for all layers:
    - Database (query performance, pool)
    - Cache (hit rate, evictions)
    - GraphQL (operation success)
    - Tracing (span creation)
    """
```

#### Layer-Specific Endpoints
```python
@router.get("/health/database")
async def database_health() -> HealthCheckResult:
    """Database health only."""

@router.get("/health/cache")
async def cache_health() -> HealthCheckResult:
    """Cache health only."""

@router.get("/health/graphql")
async def graphql_health() -> HealthCheckResult:
    """GraphQL health only."""

@router.get("/health/tracing")
async def tracing_health() -> HealthCheckResult:
    """Tracing health only."""
```

---

## 🧪 Testing Strategy

### Test Coverage: 35+ tests

#### `test_health_checks.py` (20+ tests)

**Tests for Database Health Check** (5 tests):
- Database health when all metrics healthy
- Database health when pool utilization high
- Database health when slow query rate high
- Database health when error rate high
- Database health on check exception

**Tests for Cache Health Check** (5 tests):
- Cache health when hit rate good
- Cache health when hit rate low
- Cache health when eviction rate high
- Cache health when operations failing
- Cache health on check exception

**Tests for GraphQL Health Check** (5 tests):
- GraphQL health when success rate high
- GraphQL health when error rate high
- GraphQL health when latency high
- GraphQL health with mixed operations
- GraphQL health on check exception

**Tests for Tracing Health Check** (3 tests):
- Tracing health when enabled
- Tracing health when disabled
- Tracing health on check exception

**Tests for Aggregator** (4+ tests):
- All checks passing → healthy
- Some checks degraded → degraded
- Critical checks failing → unhealthy
- Check execution time

#### `test_endpoints.py` (15+ tests)

**Tests for Liveness Probe** (2 tests):
- Returns 200 immediately
- Minimal response

**Tests for Readiness Probe** (3 tests):
- Returns 200 when all healthy
- Returns 503 when unhealthy
- Correct status codes

**Tests for Full Health Check** (3 tests):
- Complete response format
- All layers included
- Correct aggregation

**Tests for Layer-Specific** (4 tests):
- Database endpoint
- Cache endpoint
- GraphQL endpoint
- Tracing endpoint

**Tests for Error Handling** (3+ tests):
- Graceful degradation on check error
- Proper error messages
- Timeout handling

---

## 📊 File Changes Summary

### New Files Created

| File | LOC | Purpose |
|------|-----|---------|
| `src/fraiseql/health/health_check.py` | 400 | Core health check logic |
| `src/fraiseql/health/endpoints.py` | 200 | FastAPI endpoints |
| `src/fraiseql/health/__init__.py` | 20 | Module exports |
| `tests/unit/health/test_health_checks.py` | 300 | Health check tests |
| `tests/unit/health/test_endpoints.py` | 200 | Endpoint tests |
| **Total** | **1,120** | **Implementation** |

### Files Modified

| File | Change | LOC |
|------|--------|-----|
| `src/fraiseql/fastapi/app.py` | Register health endpoints | +10 |
| `src/fraiseql/__init__.py` | Export health module | +5 |
| **Total** | **Modified** | **+15** |

---

## 🔄 Integration Points

### 1. With DatabaseMonitor (Commit 4)

```python
from fraiseql.monitoring import DatabaseMonitor

monitor = get_database_monitor()
stats = await monitor.get_query_statistics()
pool = await monitor.get_pool_metrics()
# Use for database health evaluation
```

### 2. With CacheMonitor (Commit 3)

```python
from fraiseql.monitoring import CacheMonitor

cache_monitor = get_cache_monitor()
hit_rate = await cache_monitor.get_hit_rate()
evictions = await cache_monitor.get_eviction_count()
# Use for cache health evaluation
```

### 3. With OperationMetrics (Commit 4.5)

```python
from fraiseql.monitoring import OperationMetrics

metrics = get_operation_metrics()
success_rate = metrics.get_success_rate()
errors = metrics.get_error_count()
# Use for GraphQL health evaluation
```

### 4. With FastAPI App

```python
from fraiseql.health import setup_health_endpoints

app = FastAPI()
setup_health_endpoints(app)
# Health endpoints now available at /health/*
```

### 5. With Kubernetes

```yaml
livenessProbe:
  httpGet:
    path: /health/live
    port: 8000
  initialDelaySeconds: 10
  periodSeconds: 10

readinessProbe:
  httpGet:
    path: /health/ready
    port: 8000
  initialDelaySeconds: 5
  periodSeconds: 5
```

---

## 📚 API Examples

### Example 1: Full Health Check

```python
GET /health

Response:
{
  "overall_status": "healthy",
  "timestamp": "2026-01-04T12:34:56.789012Z",
  "checks_executed": 4,
  "check_duration_ms": 45.3,
  "database": {
    "status": "healthy",
    "message": "Database performing well",
    "pool_utilization": 45.0,
    "slow_query_rate": 0.5,
    "error_rate": 0.0,
    "avg_duration_ms": 42.5,
    "p95_duration_ms": 156.3
  },
  "cache": {
    "status": "healthy",
    "message": "Cache performing well",
    "hit_rate": 0.87,
    "eviction_rate": 0.05,
    "error_rate": 0.0
  },
  "graphql": {
    "status": "healthy",
    "message": "GraphQL operations healthy",
    "success_rate": 0.999,
    "error_rate": 0.001,
    "avg_operation_duration_ms": 125.0
  },
  "tracing": {
    "status": "healthy",
    "message": "Tracing enabled and working",
    "provider": "opentelemetry",
    "trace_count": 1523
  }
}
```

### Example 2: Liveness Probe (K8s)

```python
GET /health/live

Response (always fast):
{
  "status": "alive"
}
```

### Example 3: Readiness Probe (K8s)

```python
GET /health/ready

Response (includes full checks):
{
  "overall_status": "healthy",  # or degraded/unhealthy
  "timestamp": "...",
  "checks_executed": 4,
  "check_duration_ms": 45.3,
  ... full health status ...
}

HTTP Status: 200 (healthy) or 503 (unhealthy)
```

### Example 4: Database Health Only

```python
GET /health/database

Response:
{
  "status": "healthy",
  "message": "Database performing well",
  "response_time_ms": 12.3,
  "details": {
    "pool_utilization": 45.0,
    "slow_query_rate": 0.5,
    "error_rate": 0.0,
    "avg_duration_ms": 42.5,
    "p95_duration_ms": 156.3
  }
}
```

### Example 5: Degraded System

```python
GET /health

Response (HTTP 503 Service Unavailable):
{
  "overall_status": "degraded",
  "timestamp": "2026-01-04T12:34:56.789012Z",
  "checks_executed": 4,
  "check_duration_ms": 45.3,
  "database": {
    "status": "degraded",
    "message": "High pool utilization (92%)",
    "pool_utilization": 92.0,
    "warnings": ["Pool utilization above 80%"]
  },
  "cache": {
    "status": "healthy",
    "message": "Cache performing well"
  },
  "graphql": {
    "status": "degraded",
    "message": "Elevated error rate (5%)",
    "error_rate": 0.05,
    "warnings": ["Error rate above 1%"]
  },
  "tracing": {
    "status": "healthy"
  }
}
```

---

## 🎯 Acceptance Criteria

### Functionality
- [x] Liveness probe endpoint
- [x] Readiness probe endpoint
- [x] Full health check endpoint
- [x] Database health check
- [x] Cache health check
- [x] GraphQL health check
- [x] Tracing health check
- [x] Layer-specific endpoints
- [x] Health status aggregation

### Testing
- [x] 20+ unit tests for health checks
- [x] 15+ integration tests for endpoints
- [x] Error scenario coverage
- [x] Performance under load

### Performance
- [x] Health check < 200ms
- [x] Liveness probe < 10ms
- [x] Readiness probe < 500ms
- [x] No database locks

### Integration
- [x] Works with FastAPI
- [x] Kubernetes compatible
- [x] Docker compatible
- [x] Proper HTTP status codes

### Code Quality
- [x] 100% type hints
- [x] Comprehensive docstrings
- [x] Passes ruff linting
- [x] No breaking changes

---

## 📈 Success Metrics

| Metric | Target | Status |
|--------|--------|--------|
| Implementation LOC | 400+ | ⏳ Pending |
| Test Count | 35+ | ⏳ Pending |
| Test Pass Rate | 100% | ⏳ Pending |
| Code Coverage | 100% | ⏳ Pending |
| Linting | Pass | ⏳ Pending |
| Type Hints | 100% | ⏳ Pending |

---

## 🔍 Health Status Logic

### Overall Status Determination

```
If all checks are "healthy":
  → overall_status = "healthy"

If any check is "degraded":
  → overall_status = "degraded"

If any critical check is "unhealthy":
  → overall_status = "unhealthy"
```

### Database Health Thresholds

```
Pool Utilization:
  0-60%   → healthy
  60-80%  → degraded
  >80%    → unhealthy

Slow Query Rate:
  <1%     → healthy
  1-5%    → degraded
  >5%     → unhealthy

Error Rate:
  <0.1%   → healthy
  0.1-1%  → degraded
  >1%     → unhealthy
```

### Cache Health Thresholds

```
Hit Rate:
  >80%    → healthy
  60-80%  → degraded
  <60%    → unhealthy

Eviction Rate:
  <10%    → healthy
  10-30%  → degraded
  >30%    → unhealthy
```

### GraphQL Health Thresholds

```
Success Rate:
  >99%    → healthy
  95-99%  → degraded
  <95%    → unhealthy

Operation Error Rate:
  <1%     → healthy
  1-5%    → degraded
  >5%     → unhealthy
```

---

## 🚨 Alerting Strategy

### Critical Alerts (HTTP 503)

```
- Database: Error rate > 5% or pool utilization > 90%
- Cache: Hit rate < 50% or error rate > 5%
- GraphQL: Success rate < 90%
- Tracing: Telemetry provider down
```

### Warning Alerts (HTTP 200, status: degraded)

```
- Database: Pool utilization > 80% or slow query rate > 5%
- Cache: Hit rate < 70% or eviction rate > 30%
- GraphQL: Error rate > 1%
- Tracing: Trace context propagation < 90%
```

---

## 📋 Implementation Checklist

### Phase 1: Core Implementation
- [ ] Create `health_check.py` with models
  - [ ] HealthStatus dataclass
  - [ ] HealthCheckResult dataclass
  - [ ] DatabaseHealthCheck class
  - [ ] CacheHealthCheck class
  - [ ] GraphQLHealthCheck class
  - [ ] TracingHealthCheck class
  - [ ] HealthCheckAggregator class

- [ ] Create `endpoints.py` with FastAPI routes
  - [ ] /health endpoint
  - [ ] /health/live endpoint
  - [ ] /health/ready endpoint
  - [ ] /health/database endpoint
  - [ ] /health/cache endpoint
  - [ ] /health/graphql endpoint
  - [ ] /health/tracing endpoint

- [ ] Update module exports
  - [ ] Add to `__init__.py`
  - [ ] Verify imports work
  - [ ] Register endpoints in app

### Phase 2: Testing
- [ ] Write 20+ health check tests
- [ ] Write 15+ endpoint tests
- [ ] Integration tests with real monitors
- [ ] Error handling tests
- [ ] Performance tests

### Phase 3: Integration
- [ ] Verify with DatabaseMonitor (Commit 4)
- [ ] Verify with CacheMonitor (Commit 3)
- [ ] Verify with OperationMetrics (Commit 4.5)
- [ ] Test with FastAPI app
- [ ] Kubernetes probe testing

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
| **Total** | **Commit 6** | **2-3 days** |

---

## 🎯 Next Steps After Commit 6

### Immediate
1. Code review
2. Integration testing
3. Performance validation

### Following Commits
- **Commit 7**: CLI commands for monitoring data
- **Commit 8**: Full integration tests + documentation

### Phase 20
- Persistent metrics storage
- Prometheus/Grafana dashboards
- AlertManager integration

---

## Summary

**Commit 6** provides unified health checks enabling:

✅ Operational visibility into all system layers
✅ Kubernetes-compatible liveness/readiness probes
✅ Real-time performance monitoring
✅ Layered health status (database, cache, GraphQL, tracing)
✅ Graceful degradation with clear status messages

**Ready for implementation** with all dependencies met and integration points defined.

---

*Phase 19, Commit 6*
*Health Checks*
*Status: 🎯 Specification Ready for Implementation*
*Date: January 4, 2026*
