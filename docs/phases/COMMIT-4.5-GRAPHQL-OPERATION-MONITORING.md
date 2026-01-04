# Phase 19, Commit 4.5: GraphQL Operation Monitoring (Axum)

**Status**: Planning
**Target**: Implement in Rust (Axum HTTP layer)
**Estimated Effort**: 2-3 days
**Tests**: 40+ integration tests
**LOC**: ~250 core + ~150 tests

---

## 🎯 Objective

Implement GraphQL operation-level monitoring at the **HTTP handler layer** (Axum) to detect slow queries, mutations, and subscriptions. This sits between Commits 2 (W3C Trace Context) and Commit 5 (Audit Logs), providing the critical missing layer for mutation slow detection.

### Key Problem Solved

**Where can we detect slow mutations?**

```
Database Layer (Commit 4): ❌ Only sees SQL query time
Query/Mutation Level (Commit 4.5): ✅ Sees full GraphQL operation time including:
  - Schema traversal & type resolution
  - Field resolver execution
  - Authorization checks
  - Custom business logic
Business Layer (Commit 5): ❌ Too late for per-operation metrics
```

### Why Axum (not FastAPI)?

1. **Performance**: Native async, no Python GIL overhead
2. **Integration**: Direct access to request lifecycle
3. **Trace Context**: Direct middleware integration with W3C headers (Commit 2)
4. **Type Safety**: Compile-time guarantees on operation metrics
5. **Architecture**: Aligns with "Axum-focused" HTTP server approach

---

## 📐 Architecture

### Layer Placement

```
┌─────────────────────────────────────────┐
│ HTTP Request (Axum Handler)             │
├─────────────────────────────────────────┤
│ OperationMetricsMiddleware              │ ← NEW (Commit 4.5)
│   • Intercept request                   │
│   • Extract operation type & name       │
│   • Start timing & span                 │
├─────────────────────────────────────────┤
│ GraphQL Pipeline                        │
│   (Queries, Mutations, Subscriptions)   │
├─────────────────────────────────────────┤
│ OperationMetricsMiddleware              │ ← NEW (Commit 4.5)
│   • Record timing & metrics             │
│   • Link to trace context (Commit 2)    │
│   • Detect slow operations              │
├─────────────────────────────────────────┤
│ HTTP Response                           │
└─────────────────────────────────────────┘
```

### Module Structure

```
fraiseql_rs/src/http/
├── operation_metrics.rs        # NEW - Core metrics tracking
├── operation_monitor.rs        # NEW - Slow operation detection
├── graphql_operation_detector.rs # NEW - Parse operation type/name
└── (existing middleware)
    ├── observability_middleware.rs (extend with operation context)
    └── axum_server.rs (integrate operation metrics)
```

### Component Responsibilities

#### 1. **OperationMetrics** (`operation_metrics.rs`)

```rust
/// Records metrics for a single GraphQL operation
#[derive(Clone)]
pub struct OperationMetrics {
    // Identity
    pub operation_id: String,          // Unique ID for this operation
    pub operation_name: Option<String>, // Named operations only
    pub operation_type: GraphQLOperationType, // query, mutation, subscription

    // Timing
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub duration_ms: f64,

    // Trace Context (from Commit 2)
    pub trace_id: String,              // W3C trace ID
    pub span_id: String,               // Current span ID
    pub parent_span_id: Option<String>,

    // GraphQL Specifics
    pub query_length: usize,           // Characters in query
    pub variables_count: usize,        // Number of variables
    pub response_size_bytes: usize,    // Size of response

    // Execution
    pub status: OperationStatus,       // success, error, timeout
    pub error_count: usize,            // GraphQL errors
    pub field_count: usize,            // Fields selected
    pub alias_count: usize,            // Aliased fields

    // Performance
    pub is_slow: bool,                 // Exceeded threshold
    pub slow_threshold_ms: f64,        // Configured threshold
}

pub enum GraphQLOperationType {
    Query,
    Mutation,
    Subscription,
    Unknown,
}

pub enum OperationStatus {
    Success,
    PartialError,  // Has errors but returned data
    Error,         // GraphQL errors only
    Timeout,
}
```

#### 2. **GraphQLOperationDetector** (`graphql_operation_detector.rs`)

```rust
/// Parse GraphQL query to extract operation details
pub struct GraphQLOperationDetector;

impl GraphQLOperationDetector {
    /// Extract operation type (query, mutation, subscription) and name
    pub fn detect(query: &str) -> (GraphQLOperationType, Option<String>)

    /// Count fields in GraphQL operation
    pub fn count_fields(query: &str) -> usize

    /// Count aliases used in query
    pub fn count_aliases(query: &str) -> usize
}
```

#### 3. **GraphQLOperationMonitor** (`operation_monitor.rs`)

```rust
/// Monitors operations for slow execution
pub struct GraphQLOperationMonitor {
    config: OperationMonitorConfig,
    metrics_storage: Arc<MetricsStorage>,
}

pub struct OperationMonitorConfig {
    /// Threshold for marking operations as slow (milliseconds)
    pub slow_query_threshold_ms: f64,     // default: 100ms
    pub slow_mutation_threshold_ms: f64,  // default: 500ms
    pub slow_subscription_threshold_ms: f64, // default: 1000ms

    /// Maximum operations to keep in memory
    pub max_recent_operations: usize,     // default: 10,000

    /// Sampling rate (0.0-1.0)
    pub sampling_rate: f64,               // default: 1.0 (all)
}

impl GraphQLOperationMonitor {
    /// Record operation metrics
    pub async fn record(&self, metrics: OperationMetrics) -> Result<()>

    /// Get recent slow operations
    pub async fn get_slow_operations(
        &self,
        operation_type: Option<GraphQLOperationType>,
        limit: usize,
    ) -> Vec<OperationMetrics>

    /// Get operation statistics
    pub async fn get_statistics(&self) -> OperationStatistics
}

pub struct OperationStatistics {
    pub total_operations: u64,
    pub slow_operations: u64,
    pub slow_percentage: f64,
    pub avg_duration_ms: f64,
    pub p50_duration_ms: f64,
    pub p95_duration_ms: f64,
    pub p99_duration_ms: f64,
}
```

#### 4. **ObservabilityMiddleware Extension**

Integrate with existing `observability_middleware.rs`:

```rust
/// Extend existing ObservabilityContext to include operation metrics
pub struct ObservabilityContext {
    // ... existing fields ...
    pub operation_metrics: Option<OperationMetrics>,  // NEW
    pub trace_context: TraceContext,  // From Commit 2
}
```

### Request Lifecycle with Commit 4.5

```
1. HTTP Request arrives
   └─> OperationMetricsMiddleware::extract()
       • Parse GraphQL request (query, variables)
       • Extract operation type & name
       • Get trace context (Commit 2)
       • Create OperationMetrics with start_time

2. GraphQL Pipeline executes
   └─> GraphQL resolver execution
       • Query/mutation/subscription processing
       • Field resolution
       • Authorization

3. Response constructed
   └─> OperationMetricsMiddleware::record()
       • Calculate duration
       • Count fields, errors
       • Measure response size
       • Determine if slow
       • Store metrics
       • Emit observability span (OpenTelemetry)

4. HTTP Response sent
   └─> Include trace context in response headers
       (from Commit 2)
```

---

## 🧪 Test Strategy (40+ Tests)

### 1. **Operation Detection Tests** (10 tests)
```rust
#[cfg(test)]
mod operation_detection_tests {
    // Detect query operations
    #[test]
    fn test_detect_named_query()

    #[test]
    fn test_detect_anonymous_query()

    // Detect mutation operations
    #[test]
    fn test_detect_named_mutation()

    #[test]
    fn test_detect_anonymous_mutation()

    // Detect subscriptions
    #[test]
    fn test_detect_subscription()

    // Complex cases
    #[test]
    fn test_multiple_operations_first()

    #[test]
    fn test_field_counting()

    #[test]
    fn test_alias_counting()

    #[test]
    fn test_nested_field_counting()

    #[test]
    fn test_invalid_query_handling()
}
```

### 2. **Metrics Recording Tests** (12 tests)
```rust
#[cfg(test)]
mod metrics_recording_tests {
    #[test]
    fn test_record_successful_query()

    #[test]
    fn test_record_slow_mutation()

    #[test]
    fn test_record_operation_with_errors()

    #[test]
    fn test_timing_accuracy()

    #[test]
    fn test_trace_context_integration()

    #[test]
    fn test_slow_threshold_detection()

    #[test]
    fn test_different_thresholds_by_type()

    #[test]
    fn test_response_size_calculation()

    #[test]
    fn test_error_counting()

    #[test]
    fn test_field_and_alias_counting()

    #[test]
    fn test_sampling_rate_applied()

    #[test]
    fn test_operation_id_generation()
}
```

### 3. **Integration Tests** (12 tests)
```rust
#[cfg(test)]
mod integration_tests {
    #[tokio::test]
    async fn test_middleware_in_request_pipeline()

    #[tokio::test]
    async fn test_trace_context_propagation()

    #[tokio::test]
    async fn test_slow_query_detection()

    #[tokio::test]
    async fn test_slow_mutation_detection()

    #[tokio::test]
    async fn test_get_recent_slow_operations()

    #[tokio::test]
    async fn test_statistics_calculation()

    #[tokio::test]
    async fn test_concurrent_operation_monitoring()

    #[tokio::test]
    async fn test_max_operations_limit()

    #[tokio::test]
    async fn test_operation_context_in_audit_log()

    #[tokio::test]
    async fn test_metrics_json_serialization()

    #[tokio::test]
    async fn test_error_handling_in_monitoring()

    #[tokio::test]
    async fn test_performance_under_load()
}
```

### 4. **Edge Cases** (6 tests)
```rust
#[cfg(test)]
mod edge_case_tests {
    #[test]
    fn test_empty_query()

    #[test]
    fn test_very_large_query()

    #[test]
    fn test_malformed_json_request()

    #[test]
    fn test_timeout_handling()

    #[test]
    fn test_memory_limit_exceeded()

    #[test]
    fn test_concurrent_operations_same_trace()
}
```

---

## 📦 Implementation Steps

### Step 1: Core Metrics Structures (1 day)
- [ ] Create `fraiseql_rs/src/http/operation_metrics.rs`
  - Define `OperationMetrics` dataclass
  - Implement serialization (Serde)
  - Add methods for computed properties (duration, is_slow, etc.)

- [ ] Create `fraiseql_rs/src/http/graphql_operation_detector.rs`
  - Implement operation type detection (regex-based)
  - Implement field/alias counting
  - Add tests for detection logic

### Step 2: Monitoring & Detection (1 day)
- [ ] Create `fraiseql_rs/src/http/operation_monitor.rs`
  - Implement `GraphQLOperationMonitor`
  - Build metrics storage (thread-safe)
  - Implement slow operation detection
  - Add statistics calculation

- [ ] Extend `fraiseql_rs/src/http/observability_middleware.rs`
  - Add operation metrics field to `ObservabilityContext`
  - Integrate trace context (Commit 2)

### Step 3: Middleware Integration (0.5 days)
- [ ] Create middleware layer in `axum_server.rs`
  - Extract operation details from request
  - Inject operation metrics into response
  - Hook into response status detection

- [ ] Integrate with existing middleware stack
  - Ensure W3C trace context available
  - Link operation metrics to trace span

### Step 4: Testing & Documentation (0.5-1 day)
- [ ] Write 40+ tests
- [ ] Document API and examples
- [ ] Create integration guide

---

## 🔗 Integration Points

### With Commit 2 (W3C Trace Context)
```rust
// Use trace context from request
let trace_context = extract_trace_context(&headers);
metrics.trace_id = trace_context.trace_id;
metrics.span_id = generate_span_id();
metrics.parent_span_id = trace_context.parent_span_id;
```

### With Commit 1 (Configuration)
```rust
// Get slow detection thresholds from config
let config = FraiseQLConfig::from_env();
let monitor = GraphQLOperationMonitor::new(OperationMonitorConfig {
    slow_query_threshold_ms: config.observability.slow_query_threshold_ms,
    slow_mutation_threshold_ms: config.observability.slow_mutation_threshold_ms,
    // ...
});
```

### With Commit 5 (Audit Logs)
```rust
// Operation metrics available for audit logging
if let Some(op_metrics) = &observability_context.operation_metrics {
    audit_logger.log_operation(op_metrics);
}
```

---

## 📊 Expected Metrics Output

### Per-Operation Metrics
```json
{
  "operation_id": "abc123def456",
  "operation_name": "GetUserProfile",
  "operation_type": "query",
  "duration_ms": 45.2,
  "is_slow": false,
  "slow_threshold_ms": 100.0,
  "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
  "span_id": "00f067aa0ba902b7",
  "query_length": 156,
  "variables_count": 1,
  "response_size_bytes": 2048,
  "status": "success",
  "error_count": 0,
  "field_count": 7,
  "alias_count": 2
}
```

### Aggregate Statistics
```json
{
  "total_operations": 15234,
  "slow_operations": 342,
  "slow_percentage": 2.25,
  "avg_duration_ms": 52.3,
  "p50_duration_ms": 35.0,
  "p95_duration_ms": 125.5,
  "p99_duration_ms": 250.0
}
```

---

## 🎯 Success Criteria

- [x] Architecture design complete (this document)
- [ ] Core metrics structures implemented
- [ ] Operation detection working (10+ tests passing)
- [ ] Monitoring middleware integrated (12+ tests passing)
- [ ] Trace context properly linked
- [ ] 40+ integration tests passing
- [ ] <1ms overhead per operation
- [ ] Documentation complete
- [ ] Ready for Commit 5 (Audit Logs) dependency

---

## 📈 Performance Targets

- **Overhead per operation**: <1ms
- **Memory per operation**: <500 bytes
- **Storage capacity**: 10,000 recent operations
- **Query parsing time**: <5ms
- **Metrics serialization**: <2ms

---

## 🔄 Dependency Chain

```
Commit 4.5 GraphQL Operation Monitoring (THIS)
    ↓ Depends on
Commit 2: W3C Trace Context ✅
    ↓ Depends on
Commit 1: Config & CLI ✅

Commit 5: Audit Logs
    ↓ Depends on
Commit 4.5: GraphQL Operation Monitoring (THIS)
```

---

## 📝 File Plan

### New Files (3)
1. `fraiseql_rs/src/http/operation_metrics.rs` (150 LOC)
2. `fraiseql_rs/src/http/operation_monitor.rs` (200 LOC)
3. `fraiseql_rs/src/http/graphql_operation_detector.rs` (150 LOC)

### Modified Files (2)
1. `fraiseql_rs/src/http/mod.rs` - Export new modules
2. `fraiseql_rs/src/http/observability_middleware.rs` - Integrate operation metrics
3. `fraiseql_rs/src/http/axum_server.rs` - Integrate middleware

### Test Files (1)
1. `fraiseql_rs/src/http/{operation_metrics,operation_monitor,graphql_operation_detector}_tests.rs`

---

## 🚀 Why Axum (Not FastAPI)

| Aspect | Axum | FastAPI |
|--------|------|---------|
| **Native Async** | ✅ Built-in | ⚠️ Via Starlette |
| **Type Safety** | ✅ Compile-time | ❌ Runtime only |
| **HTTP/2** | ✅ Native | ⚠️ Via Uvicorn |
| **Trace Context** | ✅ Easy middleware | ⚠️ Overhead |
| **Performance** | ✅ Zero-copy | ⚠️ GIL overhead |
| **Serialization** | ✅ Serde (fast) | ⚠️ Pydantic |
| **Backwards Compat** | ✅ Optional | ✅ Can keep FastAPI |

**Decision**: Implement in Rust (Axum) as the primary path, with optional FastAPI support for backward compatibility if needed.

---

## 📋 Acceptance Checklist

- [ ] All 40+ tests passing
- [ ] <1ms overhead per operation
- [ ] Trace context properly propagated
- [ ] Slow mutations detected reliably
- [ ] Ready for integration with Commit 5
- [ ] Documentation complete
- [ ] Code review ready
- [ ] Performance benchmarks acceptable

---

## 📚 References

- W3C Trace Context: https://www.w3.org/TR/trace-context/
- OpenTelemetry: https://opentelemetry.io/
- GraphQL Specification: https://spec.graphql.org/
- Axum Documentation: https://docs.rs/axum/

---

**Next Steps**:
1. Approve this plan
2. Begin implementation (Step 1: Core Metrics)
3. Integrate with Commits 1 & 2
4. Prepare for Commit 5 (Audit Logs)
