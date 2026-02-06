# Federation Observability Integration Plan (Revised)

## Overview

This document outlines the strategy for integrating federation query monitoring into FraiseQL's existing observability infrastructure. The plan is grounded in realistic implementation details, validated metrics, and a phased approach that prioritizes foundational observability before advanced features.

**Scope**: Instrumentation of federation operations (entity resolution, subgraph communication, mutation execution) with metrics, traces, health checks, and structured logging.

**Timeline**: 4 weeks (1 week refinement + 3 weeks implementation)

**Current Status**: ✅ Observability infrastructure exists | ❌ Federation instrumentation missing

**Success Criteria**: All federation operations emitting metrics, traces, and structured logs with < 2% latency overhead.

---

## Part 1: Terminology Glossary

To avoid confusion, terms are defined precisely:

| Term | Definition | Example |
|------|-----------|---------|
| **Hop** | One subgraph call in federation query | Query traverses users → orders → products = 3 hops |
| **Hop Level** | Sequential position (1st, 2nd, 3rd) | First subgraph call = hop level 1 |
| **Max Hops** | Total number of subgraph hops in query | If query calls users + orders = max_hops: 2 |
| **Typename** | GraphQL type being resolved | "User", "Order", "Product" |
| **Resolution Strategy** | How entity is retrieved | "local", "db", "http" |
| **Entity Batch** | Group of same-type entities resolved together | 10 User entities in one batch |
| **Subgraph** | Independent GraphQL service | users-subgraph, orders-subgraph |
| **Subgraph Request** | HTTP call to subgraph | POST /graphql to orders-subgraph |
| **Deduplication** | Removing duplicate entities before resolution | 25 User IDs → 20 unique IDs |

---

## Part 2: Current Observability Infrastructure

### 2.1 Available Infrastructure

**Metrics Collection** (`crates/fraiseql-server/src/metrics_server.rs`)

- AtomicU64 counters (lock-free)
- Histogram buckets: 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s, 10s
- Prometheus text export via `/metrics`

**Distributed Tracing** (`crates/fraiseql-server/src/tracing_server.rs`)

- W3C Trace Context format (trace_id, parent_span_id, trace_flags)
- TraceSpan with attributes, events, status tracking
- Structured span hierarchy support

**Query Tracing** (`crates/fraiseql-core/src/runtime/query_tracing.rs`)

- Phase-level timing: parse, validate, execute, format
- Microsecond precision

**Structured Logging** (`crates/fraiseql-server/src/logging.rs`)

- RequestId (UUID) for correlation
- RequestContext with user/tenant info
- JSON serializable StructuredLogEntry

**Health Checks** (`crates/fraiseql-server/src/lifecycle/health.rs`)

- Liveness: `/health/live`
- Readiness: `/health/ready`
- Startup: `/health/startup`
- DatabaseStatus tracking

---

## Part 3: Federation-Specific Observability Design

### 3.1 Metrics Design (Revised for Cardinality Control)

#### Core Principle: Reduce Labels to 2-3 per Metric

High-cardinality data (typename, subgraph names, detailed error info) goes in **structured logs**, not metrics. Metrics track aggregated behavior.

#### 3.1.1 Entity Resolution Metrics

```
federation_entity_resolutions_total (counter)
  Labels: strategy (local|db|http), status (success|error)
  Example: federation_entity_resolutions_total{strategy="db",status="success"} = 15234

federation_entity_resolution_duration_ms (histogram)
  Labels: strategy
  Buckets: 1, 5, 10, 25, 50, 100, 250, 500, 1000
  Meaning: Duration to resolve a batch of entities (mixed types)

federation_entity_batch_size (histogram)
  Labels: strategy
  Buckets: 1, 5, 10, 25, 50, 100, 250, 500, 1000
  Meaning: How many entities in each batch

federation_entity_deduplication_ratio (gauge)
  No labels
  Range: 0.0 - 1.0
  Meaning: (unique_entities / total_requested) averaged over last minute
  Calculation: Emit gauge update after each batch: unique_count / original_count
```

**Data moved to structured logs**:

- Specific typename being resolved → logged in entity_resolution event
- Which subgraph provided data → logged in subgraph_request event
- Hop level → logged in federation_query event

#### 3.1.2 Subgraph Communication Metrics

```
federation_subgraph_requests_total (counter)
  Labels: subgraph, status (success|timeout|error|partial)
  Example: federation_subgraph_requests_total{subgraph="orders-subgraph",status="success"} = 5420

federation_subgraph_request_duration_ms (histogram)
  Labels: subgraph
  Buckets: 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000
  Meaning: HTTP request latency to subgraph

federation_subgraph_available (gauge)
  Labels: subgraph
  Value: 1 (available) or 0 (unavailable)
  Updated: Every 30 seconds by health check runner

federation_subgraph_error_rate_percent (gauge)
  Labels: subgraph
  Range: 0.0 - 100.0
  Meaning: Error % over last 5 minutes (sliding window)
  Updated: Every 30 seconds
```

**Data moved to structured logs**:

- HTTP status code → logged in subgraph_request event
- Entity counts → logged in subgraph_request event
- Specific error message → logged in error event

#### 3.1.3 Federation Query Latency

```
federation_query_total_duration_ms (histogram)
  Labels: None
  Buckets: 10, 25, 50, 100, 250, 500, 1000, 2500, 5000
  Meaning: End-to-end federation query time (from parse to format)
  Tracked in: Main query executor

federation_query_subgraph_calls_total (counter)
  Labels: None
  Meaning: Number of subgraph HTTP calls in each query
  Example: federation_query_subgraph_calls_total = 25420 (25k calls across all queries)
```

**Data moved to structured logs**:

- Hop level (1, 2, or 3) → logged in federation_query event
- Subgraph names called → logged in federation_query event
- Query complexity → logged in federation_query event

#### 3.1.4 Cache Metrics

```
federation_entity_cache_hits_total (counter)
  Labels: None
  Meaning: Count of entity resolution cache hits

federation_entity_cache_misses_total (counter)
  Labels: None
  Meaning: Count of entity resolution cache misses

federation_cache_hit_rate_percent (gauge)
  Labels: None
  Range: 0.0 - 100.0
  Meaning: (hits / (hits + misses)) × 100, averaged last 5min
```

#### 3.1.5 Mutation Metrics

```
federation_mutations_total (counter)
  Labels: status (success|error)
  Meaning: Federation mutation execution count

federation_mutation_duration_ms (histogram)
  Labels: None
  Buckets: 10, 50, 100, 250, 500, 1000, 2500, 5000
  Meaning: Time from mutation start to completion
```

**Metrics NOT included** (not implemented):

- ~~`federation_mutation_conflicts_total`~~ - Conflict detection not in code
- ~~`federation_mutation_sync_failures`~~ - Replication sync not in code

#### 3.1.6 Additional Metrics (Pool & Complexity)

```
federation_http_pool_connections_active (gauge)
  Labels: None
  Meaning: Current active HTTP connections to subgraphs

federation_query_complexity (histogram)
  Labels: None
  Buckets: 1, 3, 5, 10, 25, 50
  Meaning: Field count in resolved query (proxy for complexity)
```

#### 3.1.7 Error Metrics

```
federation_errors_total (counter)
  Labels: error_type (db|http|parse|validation|timeout)
  Meaning: Count of federation-specific errors
```

**Summary: Total of 18 metrics, all with max 1-2 labels, cardinality ~30 unique combinations** ✅

### 3.2 Tracing Design

#### 3.2.1 Tracing Backend Choice

**Selected**: Jaeger (via OpenTelemetry OTLP HTTP export)

**Rationale**:

- Simple all-in-one deployment (single Docker container)
- OTLP HTTP protocol (no gRPC complexity)
- 4-hour retention by default (sufficient for dev/staging)
- UI already familiar to team
- Lower resource overhead than Tempo

**Configuration**:

```
Jaeger collector: localhost:4317 (OTLP HTTP)
Sampling: 100% in dev, 10% in production
Retention: 4 hours
Max span per trace: 1000
```

#### 3.2.2 Span Hierarchy (Simplified)

```
span: "federation.query" [start → end]
├── span: "federation.parse_entities" [start → end]
│   └── Attributes: entity_count
├── span: "federation.batch_by_strategy" [start → end]
│   └── Attributes: batch_count, strategies
├── span: "federation.resolve_batch[strategy]" [start → end]  # Dynamic per strategy
│   ├── span: "federation.resolve_db" [start → end] (if DB strategy)
│   │   └── Attributes: query_count, entity_count
│   ├── span: "federation.resolve_http" [start → end] (if HTTP strategy)
│   │   ├── span: "federation.subgraph_request" [start → end]
│   │   │   └── Attributes: subgraph_name, entity_count, http_status
│   │   └── Attributes: subgraph_name, entity_count, response_count
│   └── Attributes: total_resolved, total_errors
└── span: "federation.project_fields" [start → end]
    └── Attributes: field_count
```

#### 3.2.3 Span Attributes (Reduced to Essential Only)

Per-span attributes (max 7 per span):

```
Federation Query Span:
  - federation.query_id: string (UUID)
  - federation.entity_count: int (total requested)
  - federation.max_hops: int (1, 2, or 3)
  - federation.strategies: string ("local,db,http")
  - federation.status: string ("success"|"partial"|"error")

Parse Entities Span:
  - federation.entity_input_count: int
  - federation.parse_status: string

Batch Resolution Span:
  - federation.batch_count: int
  - federation.resolution_status: string
  - federation.error_count: int

Subgraph Request Span:
  - federation.subgraph_name: string
  - federation.entity_count: int
  - federation.http_status: int
  - federation.response_entity_count: int
```

**Total: 5-7 attributes per span** ✅

#### 3.2.4 Trace Context Propagation

For each HTTP request to subgraph:

```rust
let trace_context = span.trace_context();
let request = client
    .post(subgraph_url)
    .header("traceparent", trace_context.w3c_traceparent())
    .header("tracestate", trace_context.w3c_tracestate())
    .json(&body);
```

Subgraph returns in response headers (if it supports tracing):

- `traceparent`: Updated with subgraph's span info

### 3.3 Structured Logging Design

#### 3.3.1 Federation Log Context

```json
{
  "timestamp": "2026-01-28T15:30:45.123Z",
  "level": "info",
  "request_id": "uuid",
  "trace_id": "w3c-trace-id",
  "span_id": "span-id",
  "message": "Federation entity resolution completed",
  "federation": {
    "operation_type": "entity_resolution|subgraph_request|mutation",
    "query_id": "uuid",
    "entity_count": 25,
    "entity_count_unique": 20,
    "resolution_strategy": "db|http|local",
    "typename": "User|Order|Product|...",
    "subgraph_name": "users-subgraph|...",
    "duration_ms": 45.6,
    "status": "success|error|partial",
    "error_message": "optional"
  }
}
```

#### 3.3.2 Log Emission Points

1. **Entity Resolution Start**

   ```json
   {message: "Entity resolution started", federation: {operation_type: "entity_resolution", entity_count: 25, ...}}
   ```

2. **Per-Strategy Resolution**

   ```json
   {message: "Resolved entities via DB", federation: {operation_type: "entity_resolution", resolution_strategy: "db", typename: "User", entity_count: 15, duration_ms: 22.3, status: "success"}}
   ```

3. **Subgraph Request**

   ```json
   {message: "Subgraph request", federation: {operation_type: "subgraph_request", subgraph_name: "orders-subgraph", entity_count: 10, http_status: 200, duration_ms: 25.3, status: "success"}}
   ```

4. **Error**

   ```json
   {level: "error", message: "Entity resolution failed", federation: {operation_type: "entity_resolution", typename: "Order", status: "error", error_message: "Connection timeout"}}
   ```

---

## Part 4: Health Checks

### 4.1 Subgraph Health Probes

#### 4.1.1 Probe Types

**Liveness Probe** (fast check, frequent)

- Endpoint: Subgraph GraphQL endpoint
- Query: `query { __typename }`
- Timeout: 2 seconds
- Interval: 5 seconds
- Success: HTTP 200 + valid JSON response

**Readiness Probe** (comprehensive check, less frequent)

- Endpoint: Subgraph GraphQL endpoint
- Query: Full introspection query (`__schema { types { name } }`)
- Timeout: 10 seconds
- Interval: 30 seconds
- Success: HTTP 200 + schema includes expected types

#### 4.1.2 Health Check Runner Implementation

**Location**: `crates/fraiseql-server/src/federation/health_checker.rs` (new)

```rust
pub struct SubgraphHealthChecker {
    http_client: HttpClient,
    check_interval: Duration,
    error_window: RollingErrorWindow,  // Tracks errors in rolling 60-second window
}

pub struct SubgraphHealthStatus {
    pub name: String,
    pub available: bool,
    pub latency_ms: f64,
    pub last_check: DateTime<Utc>,
    pub error_count_last_60s: u32,
    pub error_rate_percent: f64,
}

impl SubgraphHealthChecker {
    pub async fn run_background_checks(self: Arc<Self>, metrics: Arc<MetricsCollector>) {
        loop {
            // Every 30 seconds, check all subgraphs
            for subgraph in self.get_configured_subgraphs() {
                let status = self.check_subgraph(&subgraph).await;

                // Update metrics
                metrics.set_subgraph_available(&subgraph, status.available);
                metrics.set_subgraph_error_rate(&subgraph, status.error_rate_percent);

                // Store status for /health/federation endpoint
                self.cache_status(&subgraph, status).await;
            }

            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    }

    async fn check_subgraph(&self, name: &str) -> SubgraphHealthStatus {
        let start = Instant::now();

        let result = self.http_client
            .post(&self.get_url(name))
            .json(&json!({"query": "{ __typename }"}))
            .timeout(Duration::from_secs(2))
            .send()
            .await;

        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        match result {
            Ok(resp) if resp.status() == 200 => {
                self.error_window.record_success();
                SubgraphHealthStatus {
                    name: name.to_string(),
                    available: true,
                    latency_ms,
                    last_check: Utc::now(),
                    error_count_last_60s: self.error_window.error_count(),
                    error_rate_percent: self.error_window.error_rate_percent(),
                }
            }
            _ => {
                self.error_window.record_error();
                SubgraphHealthStatus {
                    name: name.to_string(),
                    available: false,
                    latency_ms,
                    last_check: Utc::now(),
                    error_count_last_60s: self.error_window.error_count(),
                    error_rate_percent: self.error_window.error_rate_percent(),
                }
            }
        }
    }
}

pub struct RollingErrorWindow {
    // Time-bucketed storage: [0]=last 10s, [1]=prev 10s, etc. (6 buckets = 60s)
    buckets: Mutex<VecDeque<ErrorBucket>>,
}

struct ErrorBucket {
    timestamp: DateTime<Utc>,
    errors: u32,
    total: u32,
}

impl RollingErrorWindow {
    pub fn error_count(&self) -> u32 {
        let buckets = self.buckets.lock();
        let now = Utc::now();
        buckets.iter()
            .filter(|b| now.signed_duration_since(b.timestamp).num_seconds() < 60)
            .map(|b| b.errors)
            .sum()
    }

    pub fn error_rate_percent(&self) -> f64 {
        let buckets = self.buckets.lock();
        let now = Utc::now();
        let recent: Vec<_> = buckets.iter()
            .filter(|b| now.signed_duration_since(b.timestamp).num_seconds() < 300)
            .collect();

        if recent.is_empty() { return 0.0; }

        let total_errors: u32 = recent.iter().map(|b| b.errors).sum();
        let total_checks: u32 = recent.iter().map(|b| b.total).sum();

        (total_errors as f64 / total_checks as f64) * 100.0
    }
}
```

#### 4.1.3 Health Endpoint

**Location**: `crates/fraiseql-server/src/routes/health.rs` (modify)

```rust
#[get("/health/federation")]
pub async fn federation_health(
    checker: web::Data<Arc<SubgraphHealthChecker>>,
) -> HttpResponse {
    let subgraphs = checker.get_cached_statuses().await;

    let overall = if subgraphs.iter().all(|s| s.available) {
        "healthy"
    } else if subgraphs.iter().any(|s| s.available) {
        "degraded"
    } else {
        "unhealthy"
    };

    let response = json!({
        "status": overall,
        "timestamp": Utc::now(),
        "subgraphs": subgraphs.iter().map(|s| json!({
            "name": s.name,
            "available": s.available,
            "latency_ms": s.latency_ms,
            "error_rate_percent": s.error_rate_percent,
            "last_check": s.last_check,
        })).collect::<Vec<_>>(),
    });

    HttpResponse::Ok().json(response)
}
```

---

## Part 5: SLO & Alert Design

### 5.1 SLO Definitions

| Scenario | P50 | P90 | P99 | Alert Threshold |
|----------|-----|-----|-----|---|
| Entity resolution (batch) | <10ms | <30ms | <100ms | >150ms for 5min |
| Subgraph request (HTTP) | <20ms | <50ms | <100ms | >150ms for 5min |
| 1-hop federation query | <5ms | <15ms | <30ms | >45ms for 5min |
| 2-hop federation query | <20ms | <50ms | <100ms | >150ms for 5min |
| 3-hop federation query | <50ms | <100ms | <250ms | >375ms for 5min |
| Mutation execution | <50ms | <100ms | <500ms | >750ms for 5min |

### 5.2 Alert Rules

**Alert: Subgraph Unavailable**

```
Condition: federation_subgraph_available == 0
Duration: 1 minute
Severity: Critical
Action: Page on-call engineer
Runbook: See section 5.3
```

**Alert: High Subgraph Error Rate**

```
Condition: federation_subgraph_error_rate_percent > 5
Duration: 5 minutes
Severity: Warning
Action: Check subgraph logs, evaluate capacity
Runbook: See section 5.3
```

**Alert: Entity Resolution Slow**

```
Condition: federation_entity_resolution_duration_ms[5m:p99] > 150ms
Duration: 5 minutes
Severity: Warning
Action: Check batch size, strategy selection
Runbook: See section 5.3
```

**Alert: Federation Query Slow**

```
Condition: federation_query_total_duration_ms[5m:p99] > 375ms
Duration: 5 minutes
Severity: Warning
Action: Check hop count, subgraph latencies
Runbook: See section 5.3
```

**Alert: Cache Hit Rate Low**

```
Condition: federation_cache_hit_rate_percent < 50
Duration: 10 minutes
Severity: Info
Action: Monitor, may indicate workload change
Runbook: See section 5.3
```

### 5.3 Alert Runbook Template

**Alert: Subgraph Unavailable**

**Severity**: 🔴 Critical

**When to page**: Immediately

**Resolution Steps**:

1. Check subgraph status: `curl http://localhost:4001/health`
2. Check subgraph logs: `docker-compose logs users-subgraph`
3. Check network connectivity: `ping users-subgraph`
4. Check database connectivity: `docker-compose logs postgres-users`
5. Restart if needed: `docker-compose restart users-subgraph`

**Escalation**: If unresolved in 5 minutes, page database team

---

## Part 6: Implementation Roadmap (Revised)

### Key Changes from Original Plan

- **Reordered phases** to build health checks first (dependency for other phases)
- **Simplified scope**: Phase 1 has 2 dashboards instead of 5 (others deferred to Phase 2)
- **Removed mutation conflict metrics** (not implemented)
- **Defined performance overhead budget**: <2% latency, <1% CPU
- **Added actual test code examples** instead of checkboxes
- **Specified tracing backend**: Jaeger with OTLP HTTP
- **Realistic effort estimates** based on detailed specs

### Health Checks & Observability Foundation (Week 1)

**Objective**: Establish subgraph health monitoring and basic infrastructure

**Deliverables**:

1. Implement SubgraphHealthChecker with background runner
   - Liveness probes every 5 seconds
   - Readiness probes every 30 seconds
   - Rolling error window (60-second)
   - Metrics update: `federation_subgraph_available`, `federation_subgraph_error_rate_percent`

2. Create `/health/federation` endpoint with status for all subgraphs

3. Add federation metrics to MetricsCollector (counters + histograms, no gauges yet)

4. Set up Jaeger container in docker-compose

**Files to Create**:

- `crates/fraiseql-server/src/federation/health_checker.rs`
- `crates/fraiseql-server/src/federation/mod.rs` (or extend existing)

**Files to Modify**:

- `crates/fraiseql-server/src/metrics_server.rs` (add federation counters & histograms)
- `crates/fraiseql-server/src/routes/health.rs` (add /health/federation endpoint)
- `tests/integration/docker-compose.yml` (add Jaeger service)

**Tests**:

```rust
#[test]
fn test_health_checker_detects_subgraph_down() {
    // Setup: Start mock HTTP server on localhost:9999
    // Action: Create health checker pointed at non-existent subgraph
    // Assert: status.available == false after first check
    // Assert: error_count_last_60s > 0
}

#[test]
fn test_health_endpoint_returns_all_subgraphs() {
    // Setup: Configure 3 subgraphs
    // Action: GET /health/federation
    // Assert: Response includes all 3 subgraph statuses
    // Assert: overall status is "healthy" if all available
}

#[test]
fn test_rolling_error_window_counts_correctly() {
    // Setup: Create RollingErrorWindow
    // Action: Record 10 errors, 40 total checks
    // Assert: error_rate_percent == 25.0
    // Assert: After 60s elapsed, error_count returns 0
}

#[test]
fn test_metrics_updated_from_health_checks() {
    // Setup: Configure metrics collector + health checker
    // Action: Run health check
    // Assert: federation_subgraph_available{subgraph="users"} == 1
    // Assert: federation_subgraph_error_rate_percent{subgraph="users"} == 0
}
```

**Acceptance Criteria**:

- [ ] Health checker runs continuously in background
- [ ] Metrics update every 30 seconds
- [ ] Error window tracks last 60 seconds
- [ ] /health/federation endpoint responds in < 10ms
- [ ] No overhead to federation queries (measured in Phase 6)

**Effort**: 3-4 days

---

### Federation Tracing Integration (Week 1-2)

**Objective**: Add OpenTelemetry tracing to federation operations

**Deliverables**:

1. Initialize OpenTelemetry with Jaeger exporter
   - OTLP HTTP to localhost:4317
   - 100% sampling in dev, configurable in production
   - Max 1000 spans per trace

2. Add TraceSpan wrappers to key federation functions:
   - `batch_load_entities()` → span "federation.query"
   - `resolve_entities_from_db()` → span "federation.resolve_db"
   - `resolve_entities_via_http()` → span "federation.resolve_http" + "federation.subgraph_request"
   - `execute_mutation()` → span "federation.mutation"

3. Implement trace context propagation:
   - Extract traceparent from incoming GraphQL requests
   - Inject into HTTP headers for subgraph calls
   - Continue trace through federation call chain

4. Add span attributes (5-7 per span as specified in section 3.2.3)

**Files to Create**:

- `crates/fraiseql-core/src/federation/tracing.rs` (federation-specific tracing utilities)

**Files to Modify**:

- `crates/fraiseql-core/src/federation/entity_resolver.rs`
- `crates/fraiseql-core/src/federation/http_resolver.rs`
- `crates/fraiseql-core/src/federation/direct_db_resolver.rs`
- `crates/fraiseql-core/src/federation/mutation_executor.rs`
- `crates/fraiseql-server/src/observability/tracing.rs` (add federation tracer setup)

**Tests**:

```rust
#[tokio::test]
async fn test_federation_query_creates_root_span() {
    // Setup: Initialize tracing with test exporter
    // Action: Execute federation query with 2 hops
    // Assert: Root span "federation.query" created
    // Assert: Span has attribute federation.query_id
    // Assert: Span has attribute federation.max_hops == 2
}

#[tokio::test]
async fn test_subgraph_request_creates_span() {
    // Setup: Initialize tracing
    // Action: Call subgraph via HTTP resolver
    // Assert: Span "federation.subgraph_request" created
    // Assert: Span has attribute federation.subgraph_name
    // Assert: Span has attribute federation.http_status
    // Assert: Span parent is "federation.resolve_http"
}

#[tokio::test]
async fn test_trace_context_propagated_to_subgraph() {
    // Setup: Mock HTTP server that captures headers
    // Action: Call subgraph with active trace context
    // Assert: traceparent header sent to subgraph
    // Assert: Header format: "00-<trace-id>-<parent-span-id>-01"
}

#[tokio::test]
async fn test_span_attributes_set_correctly() {
    // Setup: Create federation query with 15 entities
    // Action: Execute query
    // Assert: Root span has attribute federation.entity_count == 15
    // Assert: All span attributes are valid types
}
```

**Acceptance Criteria**:

- [ ] All federation functions wrapped in spans
- [ ] Trace context propagates through subgraph calls
- [ ] Jaeger UI shows complete trace waterfall
- [ ] No missing span attributes
- [ ] Latency overhead < 2% (validated in Phase 5)

**Effort**: 3-4 days

---

### Federation Metrics Recording (Week 2)

**Objective**: Record federation-specific metrics throughout query execution

**Deliverables**:

1. Implement metrics recording methods in MetricsCollector:
   - `record_entity_resolution(strategy, duration_ms, batch_size, success)`
   - `record_subgraph_request(subgraph, duration_ms, http_status, entity_count)`
   - `record_mutation(duration_ms, success)`
   - `record_entity_cache_hit/miss()`

2. Call metrics methods from federation operations:
   - After each entity resolution batch
   - After each subgraph HTTP request
   - After mutation execution
   - On all error paths

3. Verify all metrics visible in `/metrics` Prometheus endpoint

**Files to Modify**:

- `crates/fraiseql-server/src/metrics_server.rs` (add record_* methods)
- `crates/fraiseql-core/src/federation/entity_resolver.rs` (call record_entity_resolution)
- `crates/fraiseql-core/src/federation/http_resolver.rs` (call record_subgraph_request)
- `crates/fraiseql-core/src/federation/mutation_executor.rs` (call record_mutation)
- `crates/fraiseql-core/src/federation/cache.rs` (call record_entity_cache_hit/miss)

**Tests**:

```rust
#[test]
fn test_entity_resolution_metric_recorded() {
    // Setup: Create metrics collector
    // Action: Call record_entity_resolution("db", 45.6, 10, true)
    // Assert: Counter federation_entity_resolutions_total{strategy="db"} incremented
    // Assert: Histogram federation_entity_resolution_duration_ms has observation 45.6
    // Assert: Histogram federation_entity_batch_size has observation 10
}

#[test]
fn test_subgraph_request_metric_recorded() {
    // Setup: Create metrics collector
    // Action: Call record_subgraph_request("orders-subgraph", 25.3, 200, 10)
    // Assert: Counter federation_subgraph_requests_total{subgraph="orders-subgraph", status="success"} incremented
    // Assert: Histogram federation_subgraph_request_duration_ms has observation 25.3
}

#[test]
fn test_metrics_visible_in_prometheus() {
    // Setup: Run server with metrics enabled
    // Action: GET /metrics
    // Assert: Response includes "federation_entity_resolutions_total"
    // Assert: Response includes "federation_subgraph_requests_total"
    // Assert: Response includes all federation metrics
}

#[test]
fn test_cache_metrics_recorded() {
    // Setup: Create metrics collector
    // Action: Record 10 hits, 5 misses
    // Assert: federation_entity_cache_hits_total == 10
    // Assert: federation_entity_cache_misses_total == 5
    // Assert: Calculated hit rate == 66.7%
}
```

**Acceptance Criteria**:

- [ ] All federation operations record metrics
- [ ] No metrics lost on error paths
- [ ] Error metrics incremented correctly
- [ ] All metrics visible in `/metrics` with correct labels
- [ ] Cardinality < 50 unique metric combinations (validated)

**Effort**: 2-3 days

---

### Structured Logging (Week 2-3)

**Objective**: Add federation context to all structured logs

**Deliverables**:

1. Create `FederationLogContext` struct with fields:
   - operation_type, query_id, entity_count, entity_count_unique
   - resolution_strategy, typename, subgraph_name
   - duration_ms, status, error_message (optional)

2. Emit structured logs from:
   - Entity resolution start/complete/error
   - Per-strategy resolution results
   - Subgraph requests (start/complete/timeout/error)
   - Mutation execution (start/complete/error)

3. Ensure trace_id and request_id included in all logs

**Files to Create**:

- `crates/fraiseql-core/src/federation/logging.rs` (federation log context)

**Files to Modify**:

- `crates/fraiseql-server/src/logging.rs` (add FederationLogContext to StructuredLogEntry)
- `crates/fraiseql-core/src/federation/entity_resolver.rs` (emit logs)
- `crates/fraiseql-core/src/federation/http_resolver.rs` (emit logs)
- `crates/fraiseql-core/src/federation/mutation_executor.rs` (emit logs)

**Tests**:

```rust
#[test]
fn test_entity_resolution_logs_emitted() {
    // Setup: Create test logger sink
    // Action: Resolve 10 User entities via DB
    // Assert: Log entry emitted with federation context
    // Assert: Log includes entity_count, typename, strategy, duration_ms
    // Assert: JSON parsing succeeds
}

#[test]
fn test_trace_id_in_federation_logs() {
    // Setup: Create span with known trace_id
    // Action: Execute federation query
    // Assert: All federation logs include matching trace_id
    // Assert: Request ID correlates logs
}

#[test]
fn test_error_logs_include_error_message() {
    // Setup: Create scenario where subgraph returns 500
    // Action: Execute federation query
    // Assert: Error log emitted
    // Assert: error_message field populated
    // Assert: status field == "error"
}
```

**Acceptance Criteria**:

- [ ] All federation operations emit structured logs
- [ ] Trace context (trace_id) in all logs
- [ ] JSON format valid
- [ ] No sensitive data in logs
- [ ] Request correlation works end-to-end

**Effort**: 2 days

---

### Performance Testing & Overhead Validation (Week 3)

**Objective**: Verify observability has acceptable performance impact

**Deliverables**:

1. Measure baseline performance (no tracing):
   - 1-hop query latency
   - 2-hop query latency
   - Entity resolution duration
   - Subgraph request latency

2. Measure with observability enabled:
   - Same queries with tracing + metrics + logging
   - Record latency increase

3. Validate against budget:
   - Total latency increase < 2%
   - CPU usage increase < 1%
   - Memory increase < 5%

4. Profile to identify hotspots if budget exceeded

**Files to Create**:

- `crates/fraiseql-core/tests/federation_observability_perf.rs`

**Tests**:

```rust
#[test]
fn test_latency_overhead_within_budget() {
    // Setup: Run federation query 100x without observability
    // Record: Mean latency (baseline)

    // Action: Run same 100x WITH observability enabled
    // Record: Mean latency (with observability)

    // Assert: (with_observability - baseline) / baseline < 0.02 (2%)
}

#[test]
fn test_cpu_overhead_within_budget() {
    // Setup: Measure CPU before federation queries
    // Action: Run federation queries for 10 seconds
    // Measure: CPU after queries
    // Assert: CPU increase < 1%
}

#[test]
fn test_memory_overhead_within_budget() {
    // Setup: Measure memory before federation queries
    // Action: Run federation queries (10k queries)
    // Measure: Memory after queries
    // Assert: Memory increase < 5%
}
```

**Acceptance Criteria**:

- [ ] Latency overhead < 2% validated
- [ ] CPU overhead < 1% validated
- [ ] Memory overhead < 5% validated
- [ ] If budget exceeded, optimization plan created

**Effort**: 2 days

---

### Dashboards & Monitoring (Week 3)

**Objective**: Create observability dashboards for operations team

**Deliverables**:

1. **Federation Overview Dashboard**
   - Subgraph status (table: name, availability, latency p99, error rate)
   - Federation query latency (line chart: last 4 hours, p50/p90/p99)
   - Entity resolution throughput (line chart: batches/sec)
   - Cache hit rate (gauge: current %)
   - Error count (time series: last 6 hours)

2. **Entity Resolution Dashboard**
   - Resolution rate (time series)
   - Duration distribution (histogram: p50/p90/p99)
   - Batch size distribution (bar chart)
   - Strategy split (pie: local vs db vs http %)
   - Error trend (time series)

**Deferred to Phase 2 (not Phase 1)**:

- Mutation execution dashboard
- Hop latency breakdown dashboard
- Query complexity analysis dashboard

**Files to Create**:

- `tests/integration/dashboards/federation_overview.json`
- `tests/integration/dashboards/entity_resolution.json`
- `tests/integration/alerts.yml` (alert rule definitions)

**Tests**:

```rust
#[test]
fn test_federation_overview_dashboard_renders() {
    // Setup: Load dashboard JSON
    // Action: Import into test Grafana instance
    // Assert: Dashboard renders without errors
    // Assert: All panels have valid queries
}

#[test]
fn test_alert_rules_valid() {
    // Setup: Load alerts.yml
    // Action: Validate with alertmanager
    // Assert: No validation errors
    // Assert: All alert thresholds are realistic
}
```

**Acceptance Criteria**:

- [ ] Both dashboards render without errors
- [ ] Metrics queries return data
- [ ] Alerts trigger on test data
- [ ] Runbooks documented for each alert

**Effort**: 2-3 days

---

### End-to-End Testing & Documentation (Week 3-4)

**Objective**: Validate everything works together and document for operators

**Deliverables**:

1. End-to-end integration tests:
   - Execute complete federation query
   - Verify span in Jaeger
   - Verify metrics in Prometheus
   - Verify logs in structured log aggregator

2. Runbook documentation:
   - Alert response guides
   - Common troubleshooting scenarios
   - Performance tuning guide

3. Operator documentation:
   - How to read Jaeger traces
   - How to interpret metrics
   - How to respond to alerts

**Files to Create**:

- `tests/integration/FEDERATION_OBSERVABILITY_RUNBOOK.md`
- `tests/integration/FEDERATION_OBSERVABILITY_OPERATIONS_GUIDE.md`
- `crates/fraiseql-core/tests/federation_observability_integration.rs`

**Tests**:

```rust
#[tokio::test]
async fn test_federation_query_complete_observability() {
    // Setup: Configure tracing, metrics, logging
    // Action: Execute federation query (2-hop)
    //
    // Assertions:
    // - Span exists in Jaeger (query-level + sub-spans)
    // - Metrics recorded: federation_entity_resolutions_total++, federation_subgraph_requests_total++
    // - Logs emitted: federation_query, entity_resolution, subgraph_request
    // - All have matching trace_id
    // - No errors in observability pipeline
}

#[test]
fn test_runbook_completeness() {
    // Setup: Load FEDERATION_OBSERVABILITY_RUNBOOK.md
    // Assert: Each alert has resolution steps
    // Assert: Each step is actionable (e.g., not "fix the bug")
    // Assert: Escalation path defined
}
```

**Acceptance Criteria**:

- [ ] Complete end-to-end observability validated
- [ ] All runbooks documented
- [ ] Operators trained on dashboards
- [ ] No missing pieces

**Effort**: 2-3 days

---

## Part 7: Implementation Effort Summary

| Phase | Duration | Key Deliverables | Effort |
|-------|----------|---|--------|
| 1. Health Checks | 3-4 days | Health checker, /health/federation endpoint | 3-4 days |
| 2. Tracing | 3-4 days | Jaeger integration, span hierarchy, trace context | 3-4 days |
| 3. Metrics | 2-3 days | Federation metrics recording, Prometheus export | 2-3 days |
| 4. Logging | 2 days | Federation log context, structured logs | 2 days |
| 5. Performance Testing | 2 days | Overhead validation, budget verification | 2 days |
| 6. Dashboards | 2-3 days | Grafana dashboards (2), alert rules | 2-3 days |
| 7. Testing & Docs | 2-3 days | End-to-end tests, runbooks, operator guide | 2-3 days |
| **Total** | **~3 weeks** | **Complete observability** | **~20 days** |

**Pre-Implementation Refinement**: 1 week (document review, architecture decisions)

**Total Project Timeline**: 4 weeks

---

## Part 8: Success Criteria

### All Phases Complete

- [ ] All federation operations emit metrics (18 total metrics, <50 combinations)
- [ ] All federation operations emit traces (with Jaeger backend)
- [ ] All federation operations emit structured logs (with trace correlation)
- [ ] Subgraph health checked every 30 seconds with availability metrics
- [ ] Latency overhead < 2%, CPU overhead < 1%
- [ ] 2 dashboards operational (overview + entity resolution)
- [ ] All alert rules defined and validated
- [ ] Operator runbooks complete
- [ ] End-to-end integration tests passing
- [ ] No open questions about implementation

---

## Part 9: Key Files Reference

### Files to Create (17 total)

```
Core Federation Observability:

- crates/fraiseql-server/src/federation/health_checker.rs
- crates/fraiseql-core/src/federation/tracing.rs
- crates/fraiseql-core/src/federation/logging.rs
- crates/fraiseql-core/tests/federation_observability_perf.rs
- crates/fraiseql-core/tests/federation_observability_integration.rs

Dashboards & Alerts:

- tests/integration/dashboards/federation_overview.json
- tests/integration/dashboards/entity_resolution.json
- tests/integration/alerts.yml

Documentation:

- tests/integration/FEDERATION_OBSERVABILITY_RUNBOOK.md
- tests/integration/FEDERATION_OBSERVABILITY_OPERATIONS_GUIDE.md
```

### Files to Modify (8 total)

```
Server-side:

- crates/fraiseql-server/src/metrics_server.rs
- crates/fraiseql-server/src/routes/health.rs
- crates/fraiseql-server/src/observability/tracing.rs
- crates/fraiseql-server/src/logging.rs

Federation-side:

- crates/fraiseql-core/src/federation/entity_resolver.rs
- crates/fraiseql-core/src/federation/http_resolver.rs
- crates/fraiseql-core/src/federation/direct_db_resolver.rs
- crates/fraiseql-core/src/federation/mutation_executor.rs

Configuration:

- tests/integration/docker-compose.yml (add Jaeger)
```

---

## Part 10: Performance Overhead Budget

| Component | Latency Impact | CPU Impact | Memory Impact |
|-----------|---|---|---|
| Tracing (spans) | <0.5ms per span | <0.2% | <1MB per 1000 spans |
| Metrics (recording) | <0.1ms per metric | <0.2% | <100KB per metric type |
| Logging (emit) | <0.2ms per log | <0.3% | <50KB per 1000 logs |
| **Total Budget** | **<2% latency increase** | **<1% CPU increase** | **<5% memory increase** |

If any component exceeds budget, optimization work happens in Phase 5.

---

## Related Documentation

- [FEDERATION_INTEGRATION_REPORT.md](./FEDERATION_INTEGRATION_REPORT.md) - Federation architecture
- [QUICK_REFERENCE.md](./QUICK_REFERENCE.md) - Federation quick start
- [APOLLO_ROUTER.md](./APOLLO_ROUTER.md) - Apollo Router details

---

**Document Status**: ✅ Revised and corrected - Ready for implementation

**Last Updated**: 2026-01-28

**Total Lines**: 842 (this document, revised from 847 with issues addressed)
