# Phase 19-20 Implementation Approach

**Document Purpose**: Detailed technical approach for implementing Phases 19-20
**Audience**: Architecture team and implementation engineers
**Last Updated**: January 4, 2026

---

## 🎯 Strategic Approach

### Philosophy: "Composability Over Monoliths"

We're building Phase 19-20 with three principles:

1. **Plug & Play**: Each monitoring component works independently
   - Can enable/disable specific monitoring
   - Can use existing Prometheus setup or ours
   - Can use existing Grafana or deploy ours

2. **Zero Breaking Changes**: 100% backward compatible
   - All monitoring is optional
   - No required dependencies
   - Existing code works unchanged

3. **Production Ready from Day One**: Each commit is shippable
   - Every commit includes tests
   - Every commit is documented
   - Every commit is verified

---

## 🏗️ Phase 19: Architecture

### Module Structure

```
src/fraiseql/observability/
├── __init__.py                    # Public API
├── _internal/                     # Private implementation
│   ├── __init__.py
│   ├── metrics_backend.py        # Prometheus backend
│   ├── context_storage.py        # Thread-local context
│   └── hooks.py                  # Framework hooks
├── metrics_collector.py           # Central metrics aggregator
├── middleware.py                  # HTTP middleware
├── tracing.py                     # Request tracing
├── context.py                     # Context management
├── cache_monitor.py               # Cache metrics
├── db_monitor.py                  # Database metrics
├── audit_queries.py               # Audit log queries
├── audit_analyzer.py              # Analysis helpers
├── health.py                      # Health checks
├── config.py                      # Configuration
├── cli.py                         # CLI commands
└── __version__.py                 # Internal version

src/fraiseql/
├── fastapi/
│   ├── middleware.py              # MODIFIED - add hooks
│   ├── routers.py                 # MODIFIED - add tracing
│   └── app.py                     # MODIFIED - add health endpoints
├── caching/
│   ├── redis.py                   # MODIFIED - add hooks
│   └── base.py                    # MODIFIED - add hooks
├── enterprise/
│   ├── audit/
│   │   └── event_logger.py        # MODIFIED - add hooks
│   └── security/
│       └── audit.py               # MODIFIED - add hooks
└── db.py                          # MODIFIED - add pool hooks
```

### Internal Hooks System

We use a **hooks system** to inject monitoring without modifying core logic:

```python
# src/fraiseql/observability/_internal/hooks.py

class ObservabilityHooks:
    """Internal hooks for observability integration."""

    # Metrics hooks
    http_request_start: Callable[[RequestContext], None] = no_op
    http_request_end: Callable[[RequestContext], None] = no_op

    # Cache hooks
    cache_get: Callable[[str, bool, float], None] = no_op
    cache_set: Callable[[str, float], None] = no_op

    # Database hooks
    db_query: Callable[[str, float], None] = no_op
    db_pool_status: Callable[[int, int], None] = no_op

    # Audit hooks
    audit_log: Callable[[dict], None] = no_op

    @staticmethod
    def register(hook_name: str, callback: Callable) -> None:
        """Register a hook callback."""
        setattr(ObservabilityHooks, hook_name, callback)

# In HTTP middleware:
from fraiseql.observability._internal.hooks import ObservabilityHooks

async def middleware(request, call_next):
    context = RequestContext(...)
    ObservabilityHooks.http_request_start(context)  # Just a function call
    try:
        response = await call_next(request)
    finally:
        ObservabilityHooks.http_request_end(context)
    return response
```

**Advantages**:
- ✅ Zero overhead when observability disabled (no-op functions)
- ✅ No change to core logic
- ✅ Easy to test
- ✅ Can be swapped at runtime

### Configuration

```python
# src/fraiseql/observability/config.py

from dataclasses import dataclass
from fraiseql.config import get_config

@dataclass
class ObservabilityConfig:
    """Observability configuration."""

    # Global enable/disable
    enabled: bool = True

    # Component toggles
    metrics_enabled: bool = True
    tracing_enabled: bool = True
    cache_monitoring_enabled: bool = True
    database_monitoring_enabled: bool = True
    health_checks_enabled: bool = True
    audit_queries_enabled: bool = True

    # Sampling & thresholds
    sample_rate: float = 1.0  # 100% sampling by default
    slow_query_threshold_ms: int = 100
    trace_sample_rate: float = 1.0

    # Privacy
    include_query_bodies: bool = False  # Don't log GraphQL queries
    include_variable_values: bool = False  # Don't log variables

    # Retention
    audit_retention_days: int = 90
    trace_retention_days: int = 7

    # Prometheus
    prometheus_enabled: bool = True
    prometheus_port: int = 9090

    @classmethod
    def from_env(cls) -> "ObservabilityConfig":
        """Load from environment variables."""
        return cls(
            enabled=os.getenv("FRAISEQL_OBSERVABILITY_ENABLED", "true").lower() == "true",
            metrics_enabled=os.getenv("FRAISEQL_METRICS_ENABLED", "true").lower() == "true",
            # ... etc
        )

# Usage in application
config = ObservabilityConfig.from_env()
setup_observability(app, config)
```

### Metrics Collection Design

```python
# src/fraiseql/observability/metrics_collector.py

from prometheus_client import Counter, Histogram, Gauge

class MetricsCollector:
    """Unified metrics collector for all FraiseQL operations."""

    # Request metrics
    http_requests_total = Counter(
        "fraiseql_http_requests_total",
        "Total HTTP requests",
        ["method", "status", "endpoint"]
    )

    http_request_duration_ms = Histogram(
        "fraiseql_http_request_duration_ms",
        "HTTP request duration",
        ["operation", "mode"],  # mode = rust, python, apq, passthrough
        buckets=[1, 5, 10, 50, 100, 500, 1000, 5000]
    )

    # Cache metrics
    cache_hits_total = Counter(
        "fraiseql_cache_hits_total",
        "Cache hits",
        ["cache_type"]  # query, field, result
    )

    cache_misses_total = Counter(
        "fraiseql_cache_misses_total",
        "Cache misses",
        ["cache_type"]
    )

    cache_operation_duration_ms = Histogram(
        "fraiseql_cache_operation_duration_ms",
        "Cache operation duration",
        ["operation"]  # get, set, delete
    )

    # ... more metrics

    @staticmethod
    def record_http_request(operation: str, mode: str, duration_ms: float, status: int):
        """Record HTTP request metrics."""
        MetricsCollector.http_requests_total.labels(
            method="POST", status=status, endpoint="/graphql"
        ).inc()

        MetricsCollector.http_request_duration_ms.labels(
            operation=operation, mode=mode
        ).observe(duration_ms)

# Central registry
_metrics = MetricsCollector()

def get_metrics() -> MetricsCollector:
    """Get singleton metrics instance."""
    return _metrics
```

### Request Context Propagation

```python
# src/fraiseql/observability/context.py

from contextvars import ContextVar
from dataclasses import dataclass
from uuid import uuid4

@dataclass
class RequestContext:
    """Request-scoped context."""
    request_id: str
    trace_id: str
    operation_name: str | None = None
    operation_type: str | None = None  # query, mutation, subscription
    start_time: float = 0.0
    graphql_mode: str | None = None  # rust, python, apq, passthrough
    user_id: str | None = None
    complexity_score: int | None = None
    field_count: int = 0
    depth: int = 0
    cache_hit: bool | None = None
    duration_ms: float | None = None
    error: Exception | None = None

# Thread-local + async context
_context_var: ContextVar[RequestContext | None] = ContextVar(
    "fraiseql_request_context", default=None
)

class RequestContextManager:
    """Manage request context."""

    @staticmethod
    def set_context(context: RequestContext) -> None:
        """Set current request context."""
        _context_var.set(context)

    @staticmethod
    def get_context() -> RequestContext | None:
        """Get current request context."""
        return _context_var.get()

    @staticmethod
    def clear_context() -> None:
        """Clear request context."""
        _context_var.set(None)

# Usage in middleware
from fastapi import Request

async def observability_middleware(request: Request, call_next):
    context = RequestContext(
        request_id=str(uuid4()),
        trace_id=request.headers.get("X-Trace-ID", str(uuid4()))
    )

    RequestContextManager.set_context(context)
    try:
        response = await call_next(request)
        context.duration_ms = (time.time() - context.start_time) * 1000
        return response
    finally:
        RequestContextManager.clear_context()
```

### Testing Strategy

```python
# tests/unit/observability/test_metrics.py

import pytest
from fraiseql.observability.metrics_collector import MetricsCollector

def test_metrics_counter_increments():
    """Test counter increments correctly."""
    metric = MetricsCollector.http_requests_total
    initial = metric._value.get()

    MetricsCollector.record_http_request("getUser", "rust", 5.2, 200)

    assert metric._value.get() == initial + 1

def test_metrics_histogram_records():
    """Test histogram records duration."""
    metric = MetricsCollector.http_request_duration_ms

    MetricsCollector.record_http_request("getOrders", "python", 125.5, 200)

    # Verify histogram has observations
    assert metric.collect()[0].samples  # Has samples

def test_no_overhead_when_disabled():
    """Test zero overhead when monitoring disabled."""
    config = ObservabilityConfig(enabled=False)

    # Should use no-op functions
    from fraiseql.observability._internal.hooks import ObservabilityHooks

    # Just a function call, should be fast
    start = time.perf_counter()
    for _ in range(10000):
        ObservabilityHooks.http_request_start(None)
    elapsed = time.perf_counter() - start

    # 10,000 calls should be <1ms
    assert elapsed < 0.001
```

---

## 🏗️ Phase 20: Architecture

### Dashboard Generation Pipeline

```
User Application
        ↓
[Schema Analysis]
        ↓
[Detect Entities & Metrics]
        ↓
[Generate Panel Definitions]
        ↓
[Build Dashboard JSON]
        ↓
[Export to Grafana API]
        ↓
[Grafana Dashboard Created]
```

### Dashboard Builder Pattern

```python
# src/fraiseql/observability/dashboard_builder.py

class DashboardBuilder:
    """Build Grafana dashboards programmatically."""

    def __init__(self, title: str, description: str):
        self.dashboard = {
            "dashboard": {
                "title": title,
                "description": description,
                "panels": [],
                "refresh": "30s",
                "time": {"from": "now-6h", "to": "now"},
                "timezone": "browser",
            }
        }

    def add_section(self, title: str) -> "SectionBuilder":
        """Add a section (logical grouping)."""
        return SectionBuilder(self, title)

    def add_panel(
        self,
        title: str,
        queries: list[str],
        panel_type: str = "timeseries",
        **kwargs
    ) -> "DashboardBuilder":
        """Add a panel to the dashboard."""
        panel = {
            "id": len(self.dashboard["dashboard"]["panels"]),
            "title": title,
            "type": panel_type,
            "targets": [
                {
                    "expr": query,
                    "legendFormat": f"{{{{__field__}}}}"
                }
                for query in queries
            ],
            "gridPos": self._get_grid_pos(),
            **kwargs
        }

        self.dashboard["dashboard"]["panels"].append(panel)
        return self

    def build(self) -> dict:
        """Build final dashboard JSON."""
        return self.dashboard

# Usage
dashboard = (
    DashboardBuilder("Operations Overview", "GraphQL operations metrics")
    .add_section("Performance")
    .add_panel(
        "Request Rate",
        ["rate(fraiseql_http_requests_total[1m])"],
        panel_type="timeseries"
    )
    .add_panel(
        "Latency (P95)",
        ["histogram_quantile(0.95, rate(fraiseql_http_request_duration_ms_bucket[5m]))"],
        panel_type="timeseries"
    )
    .build()
)
```

### Alert Rules Generation

```python
# src/fraiseql/observability/alerts/rules.py

@dataclass
class AlertRule:
    """Prometheus alert rule definition."""

    name: str
    description: str
    expr: str  # PromQL expression
    duration: str  # 5m, 10m, 1h
    severity: str  # warning, critical
    runbook: str | None = None

    def to_yaml(self) -> str:
        """Convert to Prometheus YAML format."""
        return f"""
- alert: {self.name}
  expr: {self.expr}
  for: {self.duration}
  labels:
    severity: {self.severity}
  annotations:
    summary: "{self.description}"
    runbook_url: "{self.runbook}"
"""

# Define alert rules
HIGH_ERROR_RATE = AlertRule(
    name="HighErrorRate",
    description="Error rate exceeds 5% threshold",
    expr='rate(fraiseql_http_requests_total{status=~"5.."}[5m]) > 0.05',
    duration="5m",
    severity="warning",
    runbook="https://example.com/runbooks/high-error-rate"
)

HIGH_LATENCY = AlertRule(
    name="HighLatency",
    description="P99 latency exceeds 1 second",
    expr='histogram_quantile(0.99, rate(fraiseql_http_request_duration_ms_bucket[5m])) > 1000',
    duration="5m",
    severity="warning"
)
```

### Alert Notification System

```python
# src/fraiseql/observability/alerting/manager.py

class AlertManager:
    """Manage alert notifications."""

    def __init__(self):
        self.handlers: dict[str, AlertHandler] = {}
        self.active_alerts: dict[str, Alert] = {}

    async def register_handler(self, name: str, handler: "AlertHandler") -> None:
        """Register notification handler."""
        self.handlers[name] = handler

    async def send_alert(
        self,
        alert: "Alert",
        destinations: list[str]
    ) -> dict[str, bool]:
        """Send alert to destinations."""
        results = {}

        for destination in destinations:
            if destination not in self.handlers:
                logger.warning(f"Unknown destination: {destination}")
                continue

            handler = self.handlers[destination]

            try:
                success = await handler.send(alert)
                results[destination] = success
            except Exception as e:
                logger.error(f"Failed to send alert via {destination}: {e}")
                results[destination] = False

        return results

# Pluggable handlers
class AlertHandler(ABC):
    """Base alert handler."""

    @abstractmethod
    async def send(self, alert: Alert) -> bool:
        """Send alert, return True if successful."""
        pass

class SlackAlertHandler(AlertHandler):
    """Send alerts to Slack."""

    async def send(self, alert: Alert) -> bool:
        """Send Slack message."""
        message = self._format_message(alert)
        # Send to Slack webhook
        # ...
        return True

class EmailAlertHandler(AlertHandler):
    """Send alerts via email."""

    async def send(self, alert: Alert) -> bool:
        """Send email."""
        # ...
        return True
```

---

## 🧪 Testing Approach

### Unit Tests: The Pyramid

```
                  △
                 ╱│╲
                ╱ │ ╲         Edge cases (10%)
               ╱  │  ╲        Error handling
              ╱   │   ╲
             ╱────┼────╲
            ╱     │     ╲    Integration (30%)
           ╱      │      ╲   Component interaction
          ╱       │       ╲
         ╱────────┼────────╲
        ╱         │         ╲
       ╱─────────────────────╲ Unit Tests (60%)
      ╱           │           ╲ Core functions
     ╱            │            ╲
```

### Unit Tests (60% - Core Functions)

```python
# tests/unit/observability/test_metrics_collector.py

def test_metrics_counter():
    """Test counter increments."""
    counter = MetricsCollector.http_requests_total
    initial = counter._value.get()
    MetricsCollector.record_http_request("op", "rust", 5, 200)
    assert counter._value.get() == initial + 1

def test_metrics_histogram():
    """Test histogram records distribution."""
    histogram = MetricsCollector.http_request_duration_ms
    histogram.labels("op", "rust").observe(5.0)
    # Verify bucket increments
    assert histogram.labels("op", "rust")._value.get()

def test_context_isolation():
    """Test context is request-scoped."""
    ctx1 = RequestContext("id1", "trace1")
    ctx2 = RequestContext("id2", "trace2")

    RequestContextManager.set_context(ctx1)
    assert RequestContextManager.get_context().request_id == "id1"

    RequestContextManager.set_context(ctx2)
    assert RequestContextManager.get_context().request_id == "id2"
```

### Integration Tests (30% - Component Interaction)

```python
# tests/integration/observability/test_request_flow.py

@pytest.mark.asyncio
async def test_request_with_metrics():
    """Test metrics collected during request."""
    app = create_test_app()

    response = await client.post("/graphql", json={
        "query": "{ user { id name } }"
    })

    assert response.status_code == 200

    # Verify metrics recorded
    metrics = get_metrics()
    assert metrics.http_requests_total._value.get() > 0
    assert metrics.http_request_duration_ms.labels("user", "rust")._value.get()

@pytest.mark.asyncio
async def test_cache_monitoring_with_requests():
    """Test cache metrics during requests."""
    # Set up cache
    cache = setup_test_cache()

    # Make request that uses cache
    response = await client.post("/graphql", json={
        "query": "{ cachedData }"
    })

    # Verify cache metrics
    assert metrics.cache_hits_total._value.get() > 0
```

### Edge Case Tests (10% - Error Handling)

```python
# tests/unit/observability/test_edge_cases.py

def test_metrics_with_none_operation():
    """Test metrics handles None operation."""
    # Should not raise
    MetricsCollector.record_http_request(None, "rust", 5, 200)

def test_context_cleanup_on_error():
    """Test context cleaned up even on error."""
    try:
        raise ValueError("test error")
    except ValueError:
        pass

    RequestContextManager.clear_context()
    assert RequestContextManager.get_context() is None

def test_dashboard_generation_with_empty_schema():
    """Test dashboard generation with no entities."""
    # Should return valid dashboard, not error
    dashboard = generate_dashboard({})
    assert "dashboard" in dashboard
```

---

## 📋 Implementation Checklist

### Phase 19 Pre-Implementation
- [ ] Review all phase 19 commits with team
- [ ] Identify hook points in existing code
- [ ] Plan database schema changes (if any)
- [ ] Prepare testing environment
- [ ] Set up CI/CD for observability tests

### Phase 19 Per-Commit
- [ ] Code implementation
- [ ] Unit tests (15-20 tests per commit)
- [ ] Integration tests (5-10 tests per commit)
- [ ] Documentation (README + examples)
- [ ] Performance verification (<1ms overhead)
- [ ] Code review & approval
- [ ] Merge to develop branch

### Phase 20 Pre-Implementation
- [ ] Set up Grafana instance for testing
- [ ] Set up Prometheus instance for testing
- [ ] Review all phase 20 commits
- [ ] Prepare alert testing environment

### Phase 20 Per-Commit
- [ ] Code implementation
- [ ] Dashboard/alert verification
- [ ] Integration tests
- [ ] Documentation
- [ ] End-to-end testing
- [ ] Code review & approval

### Final Verification
- [ ] All 5,991 existing tests pass
- [ ] All 250 new tests pass
- [ ] No regressions in performance
- [ ] Documentation complete
- [ ] Examples working
- [ ] Kubernetes manifests valid
- [ ] Release notes ready

---

## 🚀 Risk Mitigation

### Risk: Performance Regression

**Mitigation**:
- Use hooks system with no-op default
- Performance tests in CI/CD
- Benchmark each commit
- Disable monitoring by default in tests

### Risk: Context Propagation Issues

**Mitigation**:
- Use Python's contextvars (async-safe)
- Thorough context cleanup tests
- Memory leak detection
- Thread-safety tests

### Risk: Database Schema Conflicts

**Mitigation**:
- Minimal schema changes
- Backward compatible migrations
- Separate audit schema
- Proper testing of upgrades

### Risk: Grafana Compatibility

**Mitigation**:
- Test with 3+ Grafana versions
- Generated JSON validates against schema
- Use standard panels only
- Graceful degradation

---

## 📚 Documentation Strategy

### Developer Documentation
- API reference for metrics collector
- Hook registration guide
- Configuration options
- Testing helpers

### Operations Documentation
- Setup guides (local, Kubernetes)
- Configuration reference
- Troubleshooting guide
- Dashboard interpretation

### User Documentation
- Getting started guide
- Metrics explanation
- Alert rules guide
- Incident response playbooks

---

## 🎯 Success Metrics

### Code Quality
- [ ] >85% code coverage on new code
- [ ] Zero Clippy warnings
- [ ] Zero type checking errors
- [ ] All tests passing

### Performance
- [ ] <1ms per-request overhead
- [ ] <500ms alert evaluation
- [ ] <5s dashboard generation
- [ ] Zero memory leaks

### Usability
- [ ] Setup in <15 minutes
- [ ] Dashboards comprehensible
- [ ] Alerts actionable
- [ ] CLI intuitive

### Reliability
- [ ] Zero regressions
- [ ] 100% backward compatible
- [ ] 99.9% alert accuracy
- [ ] No false positives

---

## 📞 Communication Plan

### Weekly Sync
- 30-minute sync with implementation team
- Review progress on current commit
- Discuss blockers
- Plan next commit

### Daily Standup
- 10-minute async update in Slack
- What's done, what's next, blockers
- No meetings unless needed

### Code Review
- All PRs require 2 approvals
- Architect + at least one other engineer
- Performance review on every commit
- Documentation review on every commit

---

## 🏁 Conclusion

The Phase 19-20 implementation approach is designed for:
- **Quality**: Comprehensive testing, code review, verification
- **Maintainability**: Clean architecture, good documentation
- **Performance**: <1ms overhead, efficient algorithms
- **Reliability**: No regressions, 100% backward compatible
- **Clarity**: Well-documented, examples for everything

By following this approach, we'll deliver a production-grade observability platform that users love.
