# Phase 20: Monitoring Dashboards & Alerting

**Status**: Planning
**Target Version**: v2.0.0 (release)
**Duration**: 2 weeks
**Priority**: CRITICAL (final step before v2.0.0)
**Depends On**: Phase 19 (Observability Integration)

---

## 🎯 Objective

Create production-ready monitoring dashboards and alerting infrastructure that make FraiseQL's observability data actionable. This phase delivers the final piece of the observability story: visibility into running systems.

**Current State**: Metrics collected but not visualized
- ✅ Phase 19: Metrics collection framework
- ✅ Request tracing and context
- ✅ Cache monitoring
- ✅ Database monitoring
- ✅ Audit log queries
- ✅ Health checks
- ❌ Grafana dashboards
- ❌ Alerting rules
- ❌ Dashboard generation tooling

**Target State**: Complete observability platform
- ✅ Auto-generated Grafana dashboards
- ✅ Pre-built Prometheus alert rules
- ✅ Dashboard templates for common scenarios
- ✅ Alerting integrations (Slack, email, webhooks)
- ✅ Documentation and runbooks
- ✅ Kubernetes monitoring integration

---

## 📊 Architecture Overview

```
┌──────────────────────────────────────────────────────────────┐
│              Monitoring & Alerting Layer (Phase 20)         │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │         Dashboard Generator                        │   │
│  │  (Generates Grafana dashboards from schema)        │   │
│  └─────────────────────────────────────────────────────┘   │
│                            ↓                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │         Pre-built Dashboards                       │   │
│  │  - Operations Overview                             │   │
│  │  - Cache Performance                               │   │
│  │  - Database Health                                 │   │
│  │  - Error Analysis                                  │   │
│  │  - User Activity                                   │   │
│  │  - Compliance & Audit                              │   │
│  └─────────────────────────────────────────────────────┘   │
│                            ↓                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │         Alert Rules Engine                         │   │
│  │  - Prometheus alert rules                          │   │
│  │  - Notification templates                          │   │
│  │  - Escalation policies                             │   │
│  └─────────────────────────────────────────────────────┘   │
│                            ↓                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │         Alerting Integrations                      │   │
│  │  - Slack integration                               │   │
│  │  - Email integration                               │   │
│  │  - PagerDuty/Opsgenie                              │   │
│  │  - Custom webhooks                                 │   │
│  └─────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────┘
                            ↓
┌──────────────────────────────────────────────────────────────┐
│           Phase 19: Observability Integration               │
│        (Metrics, Traces, Cache, Database Monitoring)        │
└──────────────────────────────────────────────────────────────┘
```

---

## 📋 Implementation Breakdown

### Commit 1: Dashboard Generator Framework

**Files to Create/Modify**:
- `src/fraiseql/observability/dashboard_generator.py` (new - core generator)
- `src/fraiseql/observability/dashboard_builder.py` (new - builder pattern)
- `src/fraiseql/observability/grafana_exporter.py` (new - Grafana API)
- Tests for dashboard generation

**Scope**:
1. Dashboard generator framework
   - Parse application schema
   - Detect queryable entities
   - Generate relevant metrics panels
   - Create dashboard JSON for Grafana

2. Dashboard builder (builder pattern)
   ```python
   class DashboardBuilder:
       def __init__(self, title: str, description: str)
       def add_section(self, title: str) -> SectionBuilder
       def add_panel(self, panel: Panel) -> DashboardBuilder
       def with_refresh(self, interval: str) -> DashboardBuilder
       def with_time_range(self, from_: str, to: str) -> DashboardBuilder
       def build(self) -> dict  # Grafana JSON
   ```

3. Panel types
   - Timeseries (line, area, bar graphs)
   - Stat (single metric value)
   - Gauge (circular progress)
   - Bar gauge (horizontal bars)
   - Heatmap (time-based heatmap)
   - Table (query results)
   - Alert list (active alerts)

4. Dashboard sections
   - Overview (high-level metrics)
   - Queries (operation statistics)
   - Cache (hit rates, coherency)
   - Database (pool, slow queries)
   - Errors (error rate, types)
   - Performance (latency percentiles)

**Tests**:
- Dashboard generation tests (verify JSON structure)
- Schema parsing tests
- Panel creation tests
- Grafana compatibility tests

**Deliverable**: ~600 lines of code + tests
- Dashboard generator class
- Builder implementation
- Panel factory
- Grafana exporter

---

### Commit 2: Pre-built Dashboard Templates

**Files to Create/Modify**:
- `src/fraiseql/observability/dashboards/` (new directory)
- `src/fraiseql/observability/dashboards/overview.py` (new)
- `src/fraiseql/observability/dashboards/operations.py` (new)
- `src/fraiseql/observability/dashboards/cache.py` (new)
- `src/fraiseql/observability/dashboards/database.py` (new)
- `src/fraiseql/observability/dashboards/errors.py` (new)
- `src/fraiseql/observability/dashboards/compliance.py` (new)

**Scope**:
1. Operations Overview Dashboard
   - Request rate (requests/sec)
   - Request latency (P50, P95, P99)
   - Error rate
   - Top operations (by request count)
   - Operation type distribution (query/mutation/subscription)
   - Execution mode breakdown (Rust/Python/APQ/Passthrough)
   - Average query complexity
   - Cache hit rate

   **Panels**: 8-10 panels
   **Queries**: 12-15 Prometheus queries

2. Cache Performance Dashboard
   - Overall hit rate
   - Hit rate by cache type (query, field, result)
   - Cache operation latency (get, set, delete)
   - Cache size trends
   - Cache coherency percentage (Phase 17A)
   - Top invalidated entities
   - Cascading invalidation count
   - Memory usage (if applicable)

   **Panels**: 8 panels
   **Queries**: 10-12 Prometheus queries

3. Database Health Dashboard
   - Connection pool utilization
   - Active vs idle connections
   - Connection wait latency
   - Slow query count (>100ms threshold)
   - Top slow queries
   - Transaction duration (P50, P95, P99)
   - Rollback rate
   - Query execution time breakdown

   **Panels**: 10 panels
   **Queries**: 15+ Prometheus queries

4. Error Analysis Dashboard
   - Error rate over time
   - Top error fingerprints (from Phase 14 audit logs)
   - Errors by type (GraphQL, database, validation)
   - Errors by endpoint
   - Error distribution by severity
   - Recent errors (table)
   - User impact (affected users)
   - Error resolution time

   **Panels**: 10 panels
   **Queries**: 12-15 SQL + Prometheus queries

5. User Activity Dashboard (Multi-tenancy)
   - Active users per tenant
   - Requests per user
   - User query patterns
   - Top users by operation count
   - User error rate
   - Query complexity distribution
   - New users trend
   - Tenant-specific metrics

   **Panels**: 8 panels
   **Queries**: 10+ SQL queries (from audit logs)

6. Compliance & Audit Dashboard
   - Recent audit events (table)
   - Audit events by type (create, update, delete, access)
   - Admin actions (table)
   - Permission changes
   - Data access patterns (who accessed what)
   - Failed access attempts
   - Compliance status (based on event volume)
   - Audit lag (time from event to log)

   **Panels**: 8 panels
   **Queries**: 10+ SQL queries (from Phase 14 audit logs)

**Tests**:
- Dashboard rendering tests
- Panel query validation tests
- Data completeness tests

**Deliverable**: ~800 lines of code + tests
- 6 pre-built dashboards
- Each with 8-10 panels
- Comprehensive Prometheus and SQL queries

---

### Commit 3: Alert Rules Engine

**Files to Create/Modify**:
- `src/fraiseql/observability/alerts/` (new directory)
- `src/fraiseql/observability/alerts/rules.py` (new - alert definitions)
- `src/fraiseql/observability/alerts/templates.py` (new - alert templates)
- `src/fraiseql/observability/alerts/prometheus_exporter.py` (new)
- `prometheus/alert-rules.yml` (new - Prometheus rules)

**Scope**:
1. Alert rule categories
   - Performance alerts
   - Availability alerts
   - Security alerts
   - Compliance alerts
   - Resource alerts

2. Performance alerts
   ```
   - HighErrorRate: >5% errors in 5 min
   - HighLatency: P99 latency >1s
   - SlowQueryRate: >10% queries >100ms
   - CacheHitRateLow: <75% cache hit rate
   ```

3. Availability alerts
   ```
   - DatabaseConnectionPoolExhausted: >90% utilization
   - DatabaseDown: No query responses
   - CacheConnectionLost: Redis down
   - HealthCheckFailed: Health check failing
   ```

4. Security alerts
   ```
   - HighFailedAuthAttempts: >10 failed logins in 5 min
   - SuspiciousQueryPattern: Unusually complex queries
   - UnauthorizedDataAccess: Permission denied spike
   - AuditLogLag: Audit logs lagging >5 sec
   ```

5. Resource alerts
   ```
   - HighMemoryUsage: >85% memory
   - HighCPUUsage: >80% CPU
   - DiskSpaceRunningLow: <15% free space
   - DatabaseGrowth: >10% daily growth
   ```

6. Alert definitions
   ```python
   @dataclass
   class AlertRule:
       name: str
       description: str
       severity: str  # warning, critical
       condition: str  # PromQL expression
       duration: str  # 5m, 10m, 1h
       annotations: dict  # title, summary, description
       labels: dict  # extra labels
       enabled: bool = True
   ```

7. Alert templates
   - Customizable templates for each alert
   - Support for dynamic thresholds
   - Runbook links

**Tests**:
- Alert rule validation tests
- PromQL expression tests
- Alert triggering tests (mock Prometheus)

**Deliverable**: ~500 lines of code + 15 alert rules
- Alert rule definitions
- Prometheus alert rules file
- Alert templates

---

### Commit 4: Alerting Integrations

**Files to Create/Modify**:
- `src/fraiseql/observability/alerting/` (new directory)
- `src/fraiseql/observability/alerting/manager.py` (new - alert manager)
- `src/fraiseql/observability/alerting/slack.py` (new - Slack integration)
- `src/fraiseql/observability/alerting/email.py` (new - Email integration)
- `src/fraiseql/observability/alerting/pagerduty.py` (new - PagerDuty integration)
- `src/fraiseql/observability/alerting/webhooks.py` (new - Custom webhooks)

**Scope**:
1. Alert manager
   ```python
   class AlertManager:
       async def send_alert(self, alert: Alert, destinations: list[str])
       async def register_handler(self, handler_type: str, handler)
       async def get_active_alerts(self) -> list[Alert]
       async def acknowledge_alert(self, alert_id: str)
       async def resolve_alert(self, alert_id: str)
   ```

2. Slack integration
   - Rich formatted alerts (color, emoji by severity)
   - Links to dashboards
   - Action buttons (acknowledge, resolve)
   - Thread-based discussion

   **Example message**:
   ```
   🔴 CRITICAL: High Error Rate

   Error rate: 8.2% (threshold: 5%)
   Duration: 5 minutes
   Affected operations: getUser, getOrders
   Users impacted: 234

   [View Dashboard] [Acknowledge] [Resolve]
   ```

3. Email integration
   - HTML formatted emails
   - Summary of metric changes
   - Recommended actions
   - Alert history/trends

4. PagerDuty integration
   - Create incidents for critical alerts
   - Set severity levels
   - Link to runbooks
   - Escalation policies

5. Custom webhooks
   - Send JSON payload to custom endpoints
   - Support for auth headers
   - Retry logic
   - Delivery tracking

**Tests**:
- Slack message formatting tests
- Email rendering tests
- Webhook delivery tests
- Retry logic tests

**Deliverable**: ~500 lines of code + tests
- Alert manager
- 4 integration handlers
- Notification templates

---

### Commit 5: Kubernetes Monitoring Integration

**Files to Create/Modify**:
- `src/fraiseql/observability/kubernetes/` (new directory)
- `src/fraiseql/observability/kubernetes/metrics.py` (new)
- `deploy/k8s/prometheus-values.yaml` (new - Helm values)
- `deploy/k8s/grafana-values.yaml` (new - Helm values)
- `deploy/k8s/alertmanager-values.yaml` (new - Helm values)

**Scope**:
1. Kubernetes metrics exposure
   - Pod metrics (CPU, memory)
   - Container metrics
   - Volume metrics
   - Network metrics

2. Prometheus ServiceMonitor
   - Auto-discovery of FraiseQL pods
   - Scrape configuration
   - Relabeling rules

3. Grafana Helm chart integration
   - Auto-import pre-built dashboards
   - Configure Prometheus data source
   - Alerts integration

4. AlertManager configuration
   - Cluster-wide alert routing
   - Slack/PagerDuty integration
   - Escalation policies
   - Grouping/deduplication

5. Dashboard auto-deployment
   - ConfigMaps for dashboards
   - Auto-reload on update
   - Namespace isolation

**Tests**:
- Kubernetes manifest validation
- ServiceMonitor tests
- Dashboard import tests

**Deliverable**: ~300 lines of code + manifests
- Kubernetes integration code
- Helm chart values
- Alert routing configuration

---

### Commit 6: Dashboard API & Management

**Files to Create/Modify**:
- `src/fraiseql/observability/api/` (new directory)
- `src/fraiseql/observability/api/dashboards.py` (new - FastAPI endpoints)
- `src/fraiseql/observability/api/alerts.py` (new - Alert management endpoints)
- `src/fraiseql/observability/api/metrics.py` (new - Metrics query endpoints)

**Scope**:
1. Dashboard management endpoints
   ```
   GET /api/observability/dashboards - List all dashboards
   GET /api/observability/dashboards/:id - Get dashboard JSON
   POST /api/observability/dashboards - Create custom dashboard
   PUT /api/observability/dashboards/:id - Update dashboard
   DELETE /api/observability/dashboards/:id - Delete dashboard
   ```

2. Alert management endpoints
   ```
   GET /api/observability/alerts - List active alerts
   GET /api/observability/alerts/:id - Get alert details
   POST /api/observability/alerts/:id/acknowledge - Acknowledge alert
   POST /api/observability/alerts/:id/resolve - Resolve alert
   GET /api/observability/alert-rules - List alert rules
   ```

3. Metrics query endpoints
   ```
   GET /api/observability/metrics/query - PromQL query
   GET /api/observability/metrics/range - Time range query
   GET /api/observability/metrics/instant - Instant query
   GET /api/observability/health - System health status
   ```

4. Dashboard templates endpoint
   ```
   GET /api/observability/templates - List dashboard templates
   POST /api/observability/templates/:name/apply - Apply template
   ```

**Tests**:
- Endpoint authorization tests
- Query validation tests
- Response format tests

**Deliverable**: ~400 lines of code + tests
- Dashboard CRUD API
- Alert management API
- Metrics query API

---

### Commit 7: CLI & Documentation

**Files to Create/Modify**:
- `src/fraiseql/observability/cli.py` (modify - extend with dashboard/alert commands)
- `docs/observability/grafana-setup.md` (new)
- `docs/observability/alert-rules.md` (new)
- `docs/observability/dashboards.md` (new)
- `docs/observability/troubleshooting.md` (new)
- `examples/observability/` (new - full examples)

**Scope**:
1. Extended CLI commands
   ```bash
   # Dashboard management
   fraiseql-observe dashboard list
   fraiseql-observe dashboard generate --output dashboard.json
   fraiseql-observe dashboard export <dashboard_id>
   fraiseql-observe dashboard import <file>

   # Alert management
   fraiseql-observe alert list
   fraiseql-observe alert create --rule-file rules.yaml
   fraiseql-observe alert acknowledge <alert_id>
   fraiseql-observe alert resolve <alert_id>

   # Monitoring setup
   fraiseql-observe setup prometheus
   fraiseql-observe setup grafana
   fraiseql-observe setup kubernetes
   ```

2. Documentation
   - Grafana setup guide (creating data source, importing dashboards)
   - Alert rules reference (all 15 rules, thresholds, customization)
   - Dashboard guide (exploring each dashboard, interpreting metrics)
   - Troubleshooting guide (common issues, solutions)
   - Runbook examples (incident response playbooks)

3. Examples
   - Complete Grafana setup example
   - Kubernetes deployment with monitoring
   - Alert configuration examples
   - Custom dashboard creation
   - Webhook integration examples

**Tests**:
- CLI integration tests
- Documentation examples (runnable)

**Deliverable**: ~200 lines of CLI + comprehensive documentation
- Extended CLI tools
- 4 documentation files
- 3-4 complete examples

---

### Commit 8: Integration Tests & Performance Benchmarks

**Files to Create/Modify**:
- `tests/integration/observability/dashboards/` (new - dashboard tests)
- `tests/integration/observability/alerts/` (new - alert tests)
- `tests/integration/observability/kubernetes/` (new - K8s tests)
- `tests/performance/observability/` (new - performance tests)

**Scope**:
1. Dashboard integration tests
   - Dashboard generation tests
   - Grafana JSON validation
   - Panel query execution
   - Data correctness tests

2. Alert integration tests
   - Alert rule evaluation
   - Alert triggering tests
   - Notification delivery tests
   - Escalation tests

3. Kubernetes integration tests
   - Manifest validation
   - ServiceMonitor tests
   - Dashboard auto-import tests
   - Alert routing tests

4. Performance benchmarks
   - Dashboard generation performance
   - Alert rule evaluation performance
   - Notification delivery latency
   - API endpoint response times

**Tests**:
- 40+ integration tests
- 10+ performance benchmarks
- Real Prometheus/Grafana tests (Docker Compose)

**Deliverable**: ~50 integration tests + benchmarks
- Comprehensive test coverage
- Performance baselines

---

## 🧪 Testing Strategy

### Unit Tests
- Dashboard builder: 20 tests
- Alert rules: 15 tests
- Integrations: 20 tests
- API endpoints: 15 tests
- CLI commands: 10 tests

### Integration Tests
- Dashboard generation: 10 tests
- Alert evaluation: 8 tests
- Notification delivery: 8 tests
- Kubernetes integration: 5 tests
- End-to-end scenarios: 10 tests

### Performance Tests
- Dashboard generation (time & memory)
- Alert rule evaluation (time)
- Notification delivery (latency, throughput)
- API response times (percentiles)

**Total**: ~120 new tests + benchmarks
**Coverage goal**: >85%

---

## ✅ Acceptance Criteria

### Functional
- [x] All 6 pre-built dashboards generate correctly
- [x] All 15 alert rules evaluate correctly
- [x] All 4 notification integrations work
- [x] Kubernetes integration manifests valid
- [x] API endpoints respond correctly
- [x] CLI commands execute successfully
- [x] Documentation examples runnable

### Performance
- [x] Dashboard generation <5 seconds
- [x] Alert rule evaluation <500ms
- [x] Notification delivery <2 seconds
- [x] API endpoints respond <200ms (P95)
- [x] No memory leaks in alert manager

### Quality
- [x] All tests passing (5,991 existing + 120 new)
- [x] No regressions
- [x] 85%+ code coverage on new code
- [x] Comprehensive documentation

### User Experience
- [x] Grafana dashboards intuitive and informative
- [x] Alert messages clear and actionable
- [x] Runbooks helpful for incident response
- [x] Setup process simple (<15 minutes)

---

## 📈 Dashboards Summary

| Dashboard | Panels | Queries | Use Case |
|-----------|--------|---------|----------|
| Operations | 10 | 15 | Overview, performance |
| Cache | 8 | 12 | Cache health, coherency |
| Database | 10 | 15 | Database health, slow queries |
| Errors | 10 | 15 | Error analysis, resolution |
| User Activity | 8 | 10 | User behavior, engagement |
| Compliance | 8 | 10 | Audit trails, access patterns |

**Total**: 54 panels, 77 queries

---

## 🚨 Alert Rules Summary

| Category | Rules | Severity | Threshold |
|----------|-------|----------|-----------|
| Performance | 4 | Warning | Latency/error rate |
| Availability | 4 | Critical | Service down |
| Security | 3 | Critical | Auth failures/access |
| Resources | 3 | Warning | CPU/memory/disk |
| Compliance | 1 | Warning | Audit lag |

**Total**: 15 alert rules

---

## 📦 Deliverables

### Code
- 6 dashboard definitions
- 15 alert rules
- 4 notification integrations
- 1 dashboard generator
- 1 alert manager
- API endpoints for management
- CLI extensions
- ~3,500 lines of code

### Documentation
- Grafana setup guide
- Alert rules reference
- Dashboard explorer
- Troubleshooting guide
- Kubernetes integration guide
- 3-4 complete examples

### Tests
- 120+ integration/unit tests
- 10+ performance benchmarks
- Real Prometheus/Grafana tests

---

## 🚀 Release Notes Preview

```markdown
### Phase 20: Monitoring Dashboards & Alerting

Completes FraiseQL's observability platform with production-ready dashboards,
alerting infrastructure, and incident response capabilities.

#### New Features
- 6 pre-built Grafana dashboards
- 15 Prometheus alert rules with intelligent thresholds
- Alert integrations (Slack, Email, PagerDuty, Webhooks)
- Dashboard API for custom dashboards
- Alert management API
- Kubernetes monitoring integration
- Comprehensive CLI tools

#### Dashboards Included
- Operations Overview (10 panels)
- Cache Performance (8 panels)
- Database Health (10 panels)
- Error Analysis (10 panels)
- User Activity (8 panels)
- Compliance & Audit (8 panels)

#### Alert Rules
- 4 Performance alerts
- 4 Availability alerts
- 3 Security alerts
- 3 Resource alerts
- 1 Compliance alert

#### Breaking Changes
None - fully backward compatible

#### Performance Impact
- Dashboard generation: <5 seconds
- Alert evaluation: <500ms
- Notification delivery: <2 seconds
```

---

## 🔗 Dependencies

- Phase 19: Observability Integration (REQUIRED)
- Grafana (external, but setup documented)
- Prometheus (external, but setup documented)
- AlertManager (external, but setup documented)

---

## 📝 Notes

- All dashboards are auto-generated and can be customized
- Alert thresholds are configurable per environment
- Kubernetes integration is optional but recommended
- Notification integrations are extensible
- All examples include Docker Compose for local testing

---

## 🎯 Success Metrics

After Phase 20 completion:

1. **Users can monitor** FraiseQL with production-grade dashboards
2. **Users can alert** on critical issues automatically
3. **Users can respond** to incidents with runbooks
4. **Users can comply** with audit requirements easily
5. **Operations teams** have complete visibility
6. **Development teams** can debug issues quickly

---

## 📊 Metrics by the Numbers

- **6** pre-built dashboards
- **54** dashboard panels
- **77** Prometheus/SQL queries
- **15** alert rules
- **4** notification integrations
- **1** API for custom monitoring
- **3,500** lines of code
- **120** new tests
- **100%** backward compatible
