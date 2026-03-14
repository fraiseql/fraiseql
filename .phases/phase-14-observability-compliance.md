# Phase 14: Observability & Compliance

**Objective**: Complete observability stack and compliance documentation

**Duration**: 1-2 weeks

**Estimated LOC**: 800-1200 (mostly configuration and documentation)

**Dependencies**: Phases 10-13 complete

---

## Success Criteria

- [ ] OpenTelemetry integration with tracing backend (Jaeger/Zipkin)
- [ ] Prometheus metrics endpoint complete
- [ ] Structured logging with correlation IDs
- [ ] Performance baselines documented
- [ ] Compliance templates created (NIST, ISO, FedRAMP)
- [ ] Security audit runbooks
- [ ] SLA/performance targets documented
- [ ] All tests passing
- [ ] Zero clippy warnings

---

## TDD Cycles

### Cycle 14.1: OpenTelemetry Integration

**Objective**: Wire OpenTelemetry for distributed tracing

#### Files
- `crates/fraiseql-server/src/tracing/mod.rs` (update from Phase 4)
- `crates/fraiseql-server/src/tracing/otel.rs`

#### Tests
```rust
#[tokio::test]
async fn test_otel_tracer_initialized() {
    let tracer_provider = setup_tracer_provider("jaeger").await?;
    let tracer = tracer_provider.tracer("fraiseql-server");

    let span = tracer.start("test_operation");
    assert!(!span.span_context().trace_id().to_string().is_empty());
}

#[tokio::test]
async fn test_graphql_query_traced() {
    let tracer_provider = setup_tracer_provider("jaeger").await?;
    let mut graphql_server = setup_graphql_server_with_otel(tracer_provider).await?;

    let query = "query { users { id name } }";
    let result = graphql_server.execute(query).await?;

    // Verify trace was created and exported
    let traces = query_jaeger_traces().await?;
    assert!(traces.len() > 0);
    assert!(traces[0].contains("users"));
}

#[tokio::test]
async fn test_database_queries_traced() {
    // Database operations should create spans
    let span = tracer.start("database_query");
    let _guard = span.make_current();

    let rows = execute_query("SELECT * FROM users").await?;
    assert!(rows.len() > 0);

    // Verify span includes query details
    let exported = export_spans().await?;
    assert!(exported[0].attributes.contains("query"));
}
```

#### GREEN: Implement OpenTelemetry
```rust
pub async fn initialize_tracing(config: &TracingConfig) -> Result<()> {
    let tracer = match config.exporter {
        ExporterType::Jaeger => {
            let tracer_provider = jaeger_pipeline()
                .install_simple()
                .map_err(|e| format!("Failed to initialize jaeger pipeline: {}", e))?;

            tracer_provider.tracer("fraiseql-server")
        }
        ExporterType::Zipkin => {
            let tracer_provider = zipkin_pipeline()
                .install_simple()
                .map_err(|e| format!("Failed to initialize zipkin pipeline: {}", e))?;

            tracer_provider.tracer("fraiseql-server")
        }
        ExporterType::Otlp => {
            let tracer_provider = opentelemetry_otlp::new_pipeline()
                .trace()
                .with_exporter(opentelemetry_otlp::new_exporter().http())
                .install_simple()
                .map_err(|e| format!("Failed to initialize OTLP pipeline: {}", e))?;

            tracer_provider.tracer("fraiseql-server")
        }
    };

    let tracer = tracing_opentelemetry::OpenTelemetryLayer::new(tracer);
    let subscriber = tracing_subscriber::registry().with(tracer);

    tracing::subscriber::set_global_default(subscriber)
        .map_err(|e| format!("Failed to set global tracer: {}", e))?;

    Ok(())
}

// Middleware to trace GraphQL queries
pub struct OtelGraphQLMiddleware;

impl GraphQLMiddleware for OtelGraphQLMiddleware {
    async fn on_query_execution(&self, query: &str) -> Result<()> {
        let span = tracing::info_span!("graphql_query", query = %query);
        let _guard = span.make_current();

        tracing::info!("Executing GraphQL query");
        Ok(())
    }

    async fn on_query_complete(&self, duration_ms: f64) -> Result<()> {
        tracing::info!(duration_ms = duration_ms, "Query completed");
        Ok(())
    }
}
```

#### CLEANUP
- Verify spans exported to tracing backend
- Test with Jaeger UI

---

### Cycle 14.2: Prometheus Metrics Endpoint & Alert Rules

**Objective**: Expose comprehensive Prometheus metrics with alert thresholds

#### Files
- `crates/fraiseql-server/src/metrics/mod.rs` (enhance existing)
- `crates/fraiseql-server/src/routes/metrics.rs`
- `deploy/prometheus/fraiseql.rules.yml` (alert rules)
- `deploy/prometheus/fraiseql-alerts.yml` (alert configuration)

#### Metrics
```prometheus
# GraphQL metrics
fraiseql_graphql_queries_total{status="success|error|timeout"} counter
fraiseql_graphql_query_duration_seconds{percentile="p50|p95|p99"} histogram
fraiseql_graphql_query_complexity{status="accepted|rejected"} gauge

# Cache metrics
fraiseql_cache_hits_total{cache="query|schema"} counter
fraiseql_cache_misses_total{cache="query|schema"} counter
fraiseql_cache_size_bytes{cache="query|schema"} gauge
fraiseql_cache_evictions_total counter

# Database metrics
fraiseql_database_connections_active gauge
fraiseql_database_connections_total{status="open|closed"} counter
fraiseql_database_query_duration_seconds histogram
fraiseql_database_slow_queries_total counter
fraiseql_database_connection_pool_wait_time_ms histogram

# Authentication metrics
fraiseql_auth_requests_total{provider="jwt|oauth|oidc"} counter
fraiseql_auth_failures_total{reason="invalid_token|expired|unauthorized"} counter
fraiseql_token_validations_total counter
fraiseql_rate_limit_exceeded_total counter

# System metrics
fraiseql_memory_bytes gauge
fraiseql_cpu_usage_percent gauge
fraiseql_uptime_seconds counter
fraiseql_build_info{version,git_commit} gauge

# Business metrics
fraiseql_tenant_count gauge
fraiseql_schema_compiled_time_ms histogram
fraiseql_query_cache_hit_rate_percent gauge
```

#### Alert Rules
```yaml
# deploy/prometheus/fraiseql.rules.yml

groups:
  - name: fraiseql.rules
    interval: 30s
    rules:
      # High error rate (>1%)
      - alert: HighGraphQLErrorRate
        expr: |
          rate(fraiseql_graphql_queries_total{status="error"}[5m])
          / ignoring(status)
          rate(fraiseql_graphql_queries_total[5m]) > 0.01
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High GraphQL error rate ({{ $value | humanizePercentage }})"
          description: "FraiseQL is experiencing >1% error rate on GraphQL queries"

      # Query timeout spike
      - alert: QueryTimeoutSpike
        expr: |
          rate(fraiseql_graphql_queries_total{status="timeout"}[5m]) > 0.001
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Query timeouts detected"
          description: "{{ $value }} timeouts/sec in last 5 min"

      # Cache hit rate degradation (<70%)
      - alert: CacheHitRateDegradation
        expr: |
          fraiseql_query_cache_hit_rate_percent < 70
        for: 10m
        labels:
          severity: info
        annotations:
          summary: "Cache hit rate below target ({{ $value }}%)"
          description: "Query cache effectiveness declining, check for cache evictions"

      # Database slow queries
      - alert: SlowDatabaseQueries
        expr: |
          rate(fraiseql_database_slow_queries_total[5m]) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Elevated slow database query rate"
          description: "{{ $value }} slow queries/sec (threshold: 0.1)"

      # Connection pool exhaustion
      - alert: ConnectionPoolNearCapacity
        expr: |
          fraiseql_database_connections_active / 20 > 0.9
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Database connection pool near capacity"
          description: "{{ $value | humanizePercentage }} of connections in use (max: 20)"

      # High memory usage
      - alert: HighMemoryUsage
        expr: |
          fraiseql_memory_bytes > 500_000_000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "FraiseQL memory usage high ({{ $value | humanize }}B)"
          description: "Memory usage exceeding 500MB threshold"

      # Authentication failures spike
      - alert: AuthFailureSpike
        expr: |
          rate(fraiseql_auth_failures_total[5m]) > 0.5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Spike in authentication failures"
          description: "{{ $value }} auth failures/sec (possible attack)"

      # Rate limiting triggered
      - alert: RateLimitingActive
        expr: |
          rate(fraiseql_rate_limit_exceeded_total[5m]) > 0.1
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "Rate limiting is active"
          description: "{{ $value }} rate limit violations/sec"
```

#### Alert Thresholds Reference
```markdown
# Alert Thresholds

## Baseline Performance
- GraphQL error rate: <1% (warning at >1%, critical at >5%)
- Query cache hit rate: >70% for production
- Database slow queries: <0.1/sec
- P99 query latency: <200ms for simple queries

## Resource Thresholds
- Memory: warning at >400MB, critical at >600MB
- CPU: warning at >70%, critical at >90%
- Connection pool: warning at 80%, critical at 95%

## Security Thresholds
- Auth failures: warning at >0.5/sec (possible bruteforce)
- Rate limit violations: info at >0.01/sec, warning at >0.1/sec
- Suspicious activities: audit log reviewed daily

## SLO Targets
- Availability: 99.9% (27 min/month downtime allowed)
- Latency (p99): <500ms for 99% of queries
- Error rate: <0.1%
```

#### Tests
```rust
#[tokio::test]
async fn test_metrics_endpoint() {
    let client = setup_test_client().await;

    let response = client
        .get("/metrics")
        .send()
        .await?;

    assert_eq!(response.status(), 200);
    let body = response.text().await?;

    // Verify Prometheus format
    assert!(body.contains("# HELP"));
    assert!(body.contains("# TYPE"));
    assert!(body.contains("fraiseql_"));
}

#[tokio::test]
async fn test_metrics_accuracy() {
    let client = setup_test_client().await;

    // Execute 5 successful queries
    for _ in 0..5 {
        execute_test_query(&client).await?;
    }

    let response = client.get("/metrics").send().await?;
    let body = response.text().await?;

    // Parse Prometheus format
    let queries_total = extract_metric(&body, "fraiseql_graphql_queries_total");
    assert_eq!(queries_total, 5);
}
```

#### GREEN: Implement metrics collection
```rust
pub fn setup_metrics() -> Result<()> {
    prometheus::register_counter_vec_with_registry!(
        "fraiseql_graphql_queries_total",
        "Total GraphQL queries executed",
        &["status"],
        prometheus::REGISTRY
    )?;

    prometheus::register_histogram_vec_with_registry!(
        "fraiseql_graphql_query_duration_seconds",
        "GraphQL query duration in seconds",
        &["percentile"],
        prometheus::REGISTRY
    )?;

    // ... register other metrics

    Ok(())
}

pub async fn metrics_handler() -> String {
    prometheus::TextEncoder::new()
        .encode_to_string(&prometheus::REGISTRY.gather(), &mut vec![])
        .unwrap_or_default()
}
```

---

### Cycle 14.2b: Tracing Backend Selection & Custom Instrumentation

**Objective**: Guide selection of distributed tracing backend and custom instrumentation patterns

#### Tracing Backend Decision Matrix

```markdown
# Tracing Backend Comparison

| Aspect | Jaeger | Zipkin | OTLP (Tempo) | AWS X-Ray |
|--------|--------|--------|--------------|-----------|
| **Deployment** | Self-hosted | Self-hosted | Cloud/self-hosted | AWS only |
| **Retention** | Configurable | Configurable | Long-term (object store) | 30 days max |
| **Storage** | In-memory/DB | In-memory/DB | S3/GCS/Blob | DynamoDB |
| **UI Quality** | Excellent | Good | Grafana | Limited |
| **Sampling** | Head-based | Head-based | Head & tail-based | Tail-based |
| **Cost (low volume)** | ~$0 | ~$0 | ~$50/mo | ~$0 |
| **Cost (high volume)** | ~$200/mo | ~$200/mo | ~$500+/mo | $0.50/mo span |
| **Best For** | Dev/staging | Small deployments | Enterprise | AWS-only shops |

## Recommendation

- **Development**: Jaeger (excellent UI, easy setup)
- **Production**: OTLP+Tempo (long-term storage, tail-based sampling)
- **Migration Path**: Start with Jaeger, migrate to OTLP/Tempo later
```

#### Custom Instrumentation Patterns

```rust
// crates/fraiseql-server/src/instrumentation/mod.rs

/// Macro for automatic span creation with context preservation
#[macro_export]
macro_rules! trace_async {
    ($name:expr, $body:expr) => {
        {
            let span = tracing::info_span!($name);
            span.in_scope(|| $body).await
        }
    };
}

/// Example: Trace GraphQL query execution
pub async fn execute_query(query: &str) -> Result<Value> {
    let span = tracing::info_span!(
        "graphql_query",
        query = query,
        query_hash = compute_hash(query),
    );

    let _guard = span.enter();

    tracing::debug!("Starting query execution");

    // Nested spans for query stages
    let plan_span = tracing::debug_span!("query_planning");
    let query_plan = create_query_plan(query)
        .in_scope(|| plan_span).await?;

    let exec_span = tracing::debug_span!("query_execution");
    let result = execute_plan(&query_plan)
        .in_scope(|| exec_span).await?;

    tracing::info!(
        latency_ms = result.duration_ms,
        "Query execution complete"
    );

    Ok(result)
}

/// Example: Trace database operations with slow query detection
pub async fn execute_db_query(sql: &str) -> Result<Vec<Row>> {
    let span = tracing::debug_span!(
        "db_query",
        sql_hash = compute_hash(sql),
        query_type = detect_query_type(sql),
    );

    let start = Instant::now();
    let _guard = span.enter();

    let result = db.query(sql).await;

    let duration = start.elapsed();
    let duration_ms = duration.as_millis() as f64;

    if duration.as_millis() > 100 {
        tracing::warn!(
            duration_ms = duration_ms,
            "Slow database query detected"
        );
    } else {
        tracing::trace!(duration_ms = duration_ms, "Database query complete");
    }

    result
}

/// Example: Trace cache operations
pub async fn cache_lookup(key: &str) -> Result<Option<Value>> {
    let span = tracing::debug_span!(
        "cache_operation",
        operation = "get",
        key_hash = compute_hash(key),
    );

    let _guard = span.enter();

    let result = cache.get(key).await?;

    match result {
        Some(v) => {
            tracing::trace!("Cache hit");
            Ok(Some(v))
        }
        None => {
            tracing::trace!("Cache miss");
            Ok(None)
        }
    }
}
```

#### Tests
```rust
#[tokio::test]
async fn test_trace_span_creation() {
    let (tracing_layer, mut rx) = setup_test_tracer();

    let span = tracing::info_span!("test_operation");
    let _guard = span.enter();

    tracing::info!("Operation starting");

    drop(_guard);

    // Verify span exported
    let events = rx.try_recv().unwrap();
    assert!(events.contains("test_operation"));
}

#[tokio::test]
async fn test_nested_spans() {
    let (tracing_layer, mut rx) = setup_test_tracer();

    let outer = tracing::info_span!("outer_operation");
    let _outer_guard = outer.enter();

    let inner = tracing::debug_span!("inner_operation");
    let _inner_guard = inner.enter();

    tracing::trace!("Nested event");

    // Verify span hierarchy
    let spans = rx.try_recv().unwrap();
    assert!(spans.contains("outer_operation"));
    assert!(spans.contains("inner_operation"));
}

#[tokio::test]
async fn test_span_context_propagation() {
    // Verify correlation ID flows through spans
    let correlation_id = "abc-123";
    let outer = tracing::info_span!("operation", correlation_id = correlation_id);

    let _guard = outer.enter();

    let events = collect_events();

    // All events should include correlation ID
    for event in events {
        assert!(event.contains(correlation_id));
    }
}
```

---

### Cycle 14.3: Structured Logging with Correlation IDs

**Objective**: Add correlation IDs to trace requests across services

#### Files
- `crates/fraiseql-server/src/logging/correlation.rs` (new)
- `crates/fraiseql-server/src/middleware/logging.rs` (update)

#### Features
- Generate unique correlation ID per request
- Include in logs, traces, and responses
- Propagate across services

#### Tests
```rust
#[tokio::test]
async fn test_correlation_id_generated() {
    let middleware = CorrelationIdMiddleware::new();
    let request = create_test_request();

    let request = middleware.process_request(request).await?;

    assert!(request.headers().contains_key("x-correlation-id"));
}

#[tokio::test]
async fn test_correlation_id_in_logs() {
    let request = create_test_request_with_correlation_id("abc-123");
    execute_query_with_request(request).await?;

    let logs = read_logs();
    assert!(logs.contains("correlation_id=\"abc-123\""));
}
```

---

### Cycle 14.4: Performance Baseline Documentation

**Objective**: Document performance targets and baselines

#### Files
- `docs/PERFORMANCE_BASELINES.md`
- `docs/PERFORMANCE_TUNING.md`
- `tools/benchmark-runner.sh`

#### Baselines (Sample)
```markdown
# Performance Baselines

## Query Execution
- Simple query (1 table, 5 fields): < 5ms p99
- Medium query (3 joins, 20 fields): < 50ms p99
- Complex query (5 joins, nested aggregates): < 200ms p99

## Throughput
- Single server: 10,000 QPS sustained
- With 3-node cluster: 30,000 QPS sustained
- Cache hit rate: > 70% for production workloads

## Memory
- Per-process baseline: 256MB
- Per-concurrent-connection: < 1MB
- Query cache: configurable (default 1GB)

## Database Connection Pool
- Min size: 5 connections
- Max size: 20 connections
- Timeout: 30 seconds

## Network
- GraphQL endpoint latency: < 10ms median (w/o query exec)
- Arrow Flight: < 5ms median per batch
```

#### Tests
```rust
#[test]
#[ignore]  // Benchmark test
fn bench_simple_query() {
    let executor = setup_executor();
    let query = "query { users { id name } }";

    let mut durations = vec![];
    for _ in 0..1000 {
        let start = Instant::now();
        executor.execute(query).unwrap();
        durations.push(start.elapsed());
    }

    let p99 = calculate_percentile(&durations, 99);
    assert!(p99 < Duration::from_millis(5), "p99: {:?}", p99);
}
```

---

### Cycle 14.5: NIST 800-53 Compliance Mapping

**Objective**: Document compliance with NIST 800-53 controls

#### Files
- `docs/compliance/NIST_800_53_MAPPING.md`
- `docs/compliance/NIST_800_53_CHECKLIST.md`

#### Example Mapping
```markdown
# NIST 800-53 Compliance Mapping

## AC (Access Control)
- AC-2 Account Management
  - ✅ Implemented: RBAC with role hierarchy
  - ✅ Configuration: roles.yaml in version control
  - ✅ Testing: test_rbac_role_hierarchy

- AC-3 Access Enforcement
  - ✅ Implemented: @require_permission directive
  - ✅ Enforcement: Applied at field execution
  - ✅ Testing: test_unauthorized_field_access_denied

## AU (Audit & Accountability)
- AU-2 Audit Events
  - ✅ Implemented: AuditLogger with multiple backends
  - ✅ Events: user_login, query_execution, schema_change
  - ✅ Storage: PostgreSQL + Elasticsearch

- AU-3 Content of Audit Records
  - ✅ Includes: timestamp, user, action, result, ip_address
  - ✅ Immutable: Written to append-only table

## CM (Configuration Management)
- CM-1 Policy & Procedures
  - ✅ Implemented: fraiseql.toml configuration
  - ✅ Validation: config validation on startup

## IA (Identification & Authentication)
- IA-2 Authentication
  - ✅ Implemented: JWT + OAuth2/OIDC
  - ✅ MFA: Optional, via external provider

## SC (System & Communications Protection)
- SC-7 Boundary Protection
  - ✅ Implemented: Network policies in Kubernetes
  - ✅ Firewall: TLS for all connections
  - ✅ Docs: deployment-security-guide.md

- SC-13 Use of Cryptography
  - ✅ Implemented: AES-256-GCM for field encryption
  - ✅ Key Management: Vault integration
```

---

### Cycle 14.6: ISO 27001:2022 Compliance Mapping

**Objective**: Document compliance with ISO 27001:2022 controls

#### Files
- `docs/compliance/ISO_27001_MAPPING.md`

#### Coverage
- A.5: Organizational Controls (policies, roles)
- A.6: People Controls (training, responsibility)
- A.7: Physical Controls (deployment environment)
- A.8: Technological Controls (encryption, access, audit)

---

### Cycle 14.7: FedRAMP Moderate Alignment

**Objective**: Document alignment with FedRAMP Moderate baseline

#### Files
- `docs/compliance/FEDRAMP_MODERATE_ALIGNMENT.md`

#### Controls
- Access Control (AC)
- Audit & Accountability (AU)
- System & Communications Protection (SC)
- Identification & Authentication (IA)

---

### Cycle 14.8: Security Audit Runbooks

**Objective**: Create operational runbooks for security operations

#### Files
- `docs/SECURITY_RUNBOOK.md`
- `docs/INCIDENT_RESPONSE.md`
- `docs/BREACH_NOTIFICATION.md`

#### Content
```markdown
# Security Audit Runbook

## Daily Checks
1. Review audit log for suspicious activities
   - Failed login attempts > threshold
   - Permission escalation attempts
   - Schema modification history

2. Verify backup integrity
3. Check certificate expiration dates
4. Review access logs

## Weekly Audit Review
1. Aggregate audit statistics
2. Review role/permission changes
3. Verify data retention policies
4. Check external auth provider sync

## Monthly Security Review
1. Credential rotation verification
2. Vulnerability scanning results
3. Compliance gap analysis
4. Performance vs. baselines
```

---

## Verification

```bash
# Tracing tests
cargo test --lib tracing

# Metrics tests
cargo test --lib metrics

# Integration
cargo nextest run observability_integration

# Compliance documentation verification
grep -r "NIST_800_53" docs/compliance/
grep -r "ISO_27001" docs/compliance/
grep -r "FEDRAMP" docs/compliance/
```

---

## Status

- [ ] Not Started
- [ ] In Progress (Cycle X)
- [ ] Complete

---

## Next Phase

→ Phase 15: Finalize (Production Readiness)
