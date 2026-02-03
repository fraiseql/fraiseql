# Phase 8.7: Prometheus Metrics for Observer System

**Version:** 1.0
**Status:** Planning & Partial Implementation
**Date:** January 24, 2026
**Effort:** 2-3 days

---

## Executive Summary

Phase 8.7 adds comprehensive Prometheus metrics to the observer system for production monitoring. This enables operators to track event processing, cache performance, deduplication effectiveness, and action execution with real-time dashboards.

**Key Metrics:**
- `fraiseql_observer_events_processed_total` - Total events processed
- `fraiseql_observer_cache_hit_rate` - Cache effectiveness
- `fraiseql_observer_dedup_hit_rate` - Deduplication effectiveness
- `fraiseql_observer_action_duration_seconds` - Action execution time
- `fraiseql_observer_backlog_size` - Event queue depth

---

## Context & Problem Statement

### Current State
✅ Phase 8.0-8.4: Full observer system with caching, dedup, executor, actions
✅ Prometheus dependency already in Cargo.toml (as optional feature)
❌ No metrics collected or exposed
❌ No production observability for event processing
❌ No monitoring for cache/dedup effectiveness

### Problem
Operations teams cannot:

- See how many events are being processed
- Track cache hit rates (is caching effective?)
- Monitor deduplication performance
- Measure action execution times
- Detect queue backlogs

### Solution
Create metrics registry with:

1. Global metrics (events processed, errors)
2. Cache metrics (hits, misses, evictions)
3. Dedup metrics (detected duplicates, saved processing)
4. Action metrics (duration, success rate by type)
5. Queue metrics (backlog size, latency)

---

## Architecture

### Metrics Registry

```rust
pub struct MetricsRegistry {
    // Event processing
    events_processed: prometheus::IntCounter,
    events_failed: prometheus::IntCounter,

    // Cache
    cache_hits: prometheus::IntCounter,
    cache_misses: prometheus::IntCounter,
    cache_evictions: prometheus::IntCounter,

    // Deduplication
    dedup_detected: prometheus::IntCounter,
    dedup_skipped: prometheus::IntCounter,

    // Actions
    actions_executed: prometheus::IntCounterVec,  // by action_type
    action_duration: prometheus::HistogramVec,    // by action_type
    action_errors: prometheus::IntCounterVec,     // by action_type

    // Queue/Backlog
    backlog_size: prometheus::Gauge,
    dlq_items: prometheus::Gauge,
}
```

### Implementation Pattern

```rust
// 1. Define metric in registry
let events_processed = IntCounter::new("fraiseql_observer_events_processed_total",
    "Total events processed")?;

// 2. Register with Prometheus
registry.register(Box::new(events_processed.clone()))?;

// 3. Instrument code to record metrics
metrics.events_processed.inc();

// 4. Expose via HTTP /metrics endpoint
axum::route("/metrics", get(metrics_handler))
```

### Integration Points

```
executor.rs
  ├─ process_event()           → events_processed.inc()
  ├─ condition evaluation      → (tracked in observability)
  └─ action execution          → action_duration.observe()

cached_executor.rs
  ├─ cache hit                 → cache_hits.inc()
  ├─ cache miss                → cache_misses.inc()
  └─ eviction                  → cache_evictions.inc()

deduped_executor.rs
  ├─ duplicate detected        → dedup_detected.inc()
  └─ processing skipped        → dedup_skipped.inc()

actions.rs (all action types)
  ├─ execution start           → Start timer
  ├─ execution complete        → action_duration.observe()
  └─ execution error           → action_errors.inc()
```

---

## Implementation Tasks

### Task 1: Create Metrics Registry Module (30 min)

**File:** `crates/fraiseql-observers/src/metrics/mod.rs` (NEW)

```rust
#[cfg(feature = "metrics")]
pub mod registry;
#[cfg(feature = "metrics")]
pub mod handler;

#[cfg(feature = "metrics")]
pub use registry::MetricsRegistry;

#[cfg(not(feature = "metrics"))]
pub struct MetricsRegistry;
```

**File:** `crates/fraiseql-observers/src/metrics/registry.rs` (NEW)

Implement:

- `MetricsRegistry::new()` - Create and register all metrics
- Metric definitions with proper labels
- Error handling for metric registration
- Thread-safe counter/gauge access

**Acceptance Criteria:**
- Registry creates all metric types
- Metrics are registered with Prometheus
- No panics on registration
- Code compiles with/without `metrics` feature

### Task 2: Implement HTTP Handler (20 min)

**File:** `crates/fraiseql-observers/src/metrics/handler.rs` (NEW)

```rust
pub async fn metrics_handler() -> String {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = prometheus::gather();
    encoder.encode(&metric_families, &mut buf).unwrap();
    buf
}
```

Implement:

- Axum handler for GET /metrics
- TextEncoder for Prometheus format
- Content-Type: text/plain
- Error handling

**Acceptance Criteria:**
- Handler returns valid Prometheus format
- Metrics are correctly encoded
- Handler integrates with fraiseql-server

### Task 3: Instrument Events Processing (45 min) ← CLAUDE IMPLEMENTS (EXAMPLE)

**File:** `crates/fraiseql-observers/src/executor.rs` (MODIFY)

Instrument:

1. `process_event()` - Increment events_processed on entry
2. Success path - Track successful events
3. Error path - Track failures
4. Per-observer actions - Track action_duration with type label

**Modifications:**
```rust
pub async fn process_event(&self, event: &EntityEvent) -> Result<ExecutionSummary> {
    // BEFORE: metrics.events_processed.inc();

    // EXISTING: matching_observers = ...

    // FOR EACH ACTION:
    // - Start timer
    // - Execute action
    // - Record duration with action_type label
}
```

**Acceptance Criteria:**
- Events processed metric increments
- Action duration recorded per type
- Errors tracked separately
- No performance regression

### Task 4: Instrument Cache Operations (30 min) ← LOCAL MODEL (PATTERN APPLICATION)

**File:** `crates/fraiseql-observers/src/cached_executor.rs` (MODIFY)

Pattern:

- On cache hit: `metrics.cache_hits.inc()`
- On cache miss: `metrics.cache_misses.inc()`
- On eviction: `metrics.cache_evictions.inc()`

Will be delegated to local model after example is complete.

### Task 5: Instrument Deduplication (20 min) ← LOCAL MODEL (PATTERN APPLICATION)

**File:** `crates/fraiseql-observers/src/deduped_executor.rs` (MODIFY)

Pattern:

- On duplicate detected: `metrics.dedup_detected.inc()`
- On processing skipped: `metrics.dedup_skipped.inc()`

### Task 6: Instrument Actions (30 min) ← LOCAL MODEL (REPETITIVE)

**File:** `crates/fraiseql-observers/src/actions.rs` & `actions_additional.rs` (MODIFY)

Pattern (applied to each action type):
```rust
let start = Instant::now();
// Execute action
let duration = start.elapsed();
metrics.action_duration
    .with_label_values(&[action_type])
    .observe(duration.as_secs_f64());
```

### Task 7: Create Grafana Dashboard (30 min) ← CLAUDE IMPLEMENTS

**File:** `docs/monitoring/grafana-dashboard-8.7.json` (NEW)

Dashboard panels:

- Events processed (graph)
- Cache hit rate % (gauge)
- Dedup effectiveness (graph)
- Action duration (by type)
- Queue backlog (gauge)
- Error rate (graph)

### Task 8: Documentation (30 min) ← CLAUDE IMPLEMENTS

**File:** `docs/monitoring/PHASE_8_7_METRICS.md` (NEW)

Sections:

- Metric definitions
- How to access /metrics endpoint
- Prometheus scrape config example
- Grafana dashboard setup
- Alert examples

---

## Files to Create

```
crates/fraiseql-observers/src/
├── metrics/                        (NEW DIRECTORY)
│   ├── mod.rs
│   ├── registry.rs                 (MetricsRegistry)
│   └── handler.rs                  (Axum handler)

docs/
├── monitoring/                     (MIGHT EXIST)
│   ├── PHASE_8_7_METRICS.md       (Guide)
│   └── grafana-dashboard-8.7.json (Dashboard)
```

## Files to Update

```
crates/fraiseql-observers/src/
├── lib.rs                          (Add metrics module)
├── executor.rs                     (Instrument process_event)
├── cached_executor.rs              (Instrument cache ops)
├── deduped_executor.rs             (Instrument dedup ops)
├── actions.rs                      (Instrument action execution)
└── actions_additional.rs           (Instrument action execution)

crates/fraiseql-server/src/
└── main.rs                         (Add GET /metrics route)
```

---

## Metrics Definitions

### Event Processing

```
fraiseql_observer_events_processed_total
  Type: Counter
  Help: Total number of events processed by observer system
  Labels: []
  Example value: 50000

fraiseql_observer_events_failed_total
  Type: Counter
  Help: Total number of events failed processing
  Labels: [error_type]
  Example: error_type="condition_failed", error_type="action_error"
```

### Cache Metrics

```
fraiseql_observer_cache_hits_total
  Type: Counter
  Help: Total cache hits in result deduplication

fraiseql_observer_cache_misses_total
  Type: Counter
  Help: Total cache misses

fraiseql_observer_cache_hit_rate
  Type: Gauge
  Help: Current cache hit rate (0-100)
  Calculated: hits / (hits + misses) * 100
```

### Deduplication

```
fraiseql_observer_dedup_detected_total
  Type: Counter
  Help: Total duplicate events detected and skipped

fraiseql_observer_dedup_saved_processing_total
  Type: Counter
  Help: Total processing cycles saved by deduplication
```

### Action Execution

```
fraiseql_observer_action_duration_seconds
  Type: Histogram
  Help: Action execution time
  Labels: [action_type]
  Buckets: 0.001, 0.01, 0.1, 1, 5, 10
  Example action_types: "webhook", "slack", "email", "sms", "push", "search", "cache"

fraiseql_observer_action_errors_total
  Type: Counter
  Help: Total action execution errors
  Labels: [action_type, error_type]
  Example: action_type="webhook", error_type="timeout"
```

### Queue/Backlog

```
fraiseql_observer_backlog_size
  Type: Gauge
  Help: Current number of events in processing queue

fraiseql_observer_dlq_items
  Type: Gauge
  Help: Current number of items in dead letter queue
```

---

## Acceptance Criteria

### Implementation Completeness

- ✅ MetricsRegistry created and functional
- ✅ All metric types defined and registered
- ✅ GET /metrics endpoint exposed on fraiseql-server
- ✅ Metrics feature enabled by default in `phase8` feature
- ✅ Zero unsafe code in metrics module

### Instrumentation Coverage

- ✅ Events processing tracked
- ✅ Cache operations tracked
- ✅ Deduplication tracked
- ✅ Action execution tracked (all 7 action types)
- ✅ Errors tracked with appropriate labels

### Code Quality

- ✅ No clippy warnings
- ✅ Comprehensive doc comments
- ✅ No performance regression
- ✅ Thread-safe metric access
- ✅ Feature-gated properly (metrics feature)

### Monitoring Usability

- ✅ Grafana dashboard provided
- ✅ Prometheus scrape config example
- ✅ Alert examples (high error rate, etc.)
- ✅ Documentation complete
- ✅ Integration testing with mock metrics

### Testing

- ✅ Unit tests for MetricsRegistry
- ✅ Integration test for /metrics endpoint
- ✅ Verify metrics increment correctly
- ✅ Verify labels applied correctly

---

## DO NOT / Guardrails

❌ **DO NOT** block event processing on metric recording failure
- Metrics are observability, not core functionality

❌ **DO NOT** use unbounded label cardinality
- All label values must be predefined (action types, error types)

❌ **DO NOT** add heavyweight dependencies
- Prometheus crate is already in Cargo.toml

❌ **DO NOT** expose internal implementation details in metrics
- Keep metric names/labels user-friendly and stable

❌ **DO NOT** break existing tests
- All tests should pass with/without metrics feature

---

## Hybrid Implementation Strategy

### Phase (Claude - Me):

1. Design metrics registry architecture
2. Implement MetricsRegistry module with all metrics
3. Implement Axum handler for /metrics
4. Instrument executor.rs (first example)
5. Create Grafana dashboard
6. Write documentation

### Phase 2 (Local Model - Pattern Application):

1. Apply cache instrumentation pattern to cached_executor.rs
2. Apply dedup instrumentation pattern to deduped_executor.rs
3. Apply action instrumentation pattern to all action types (7 files)
4. Generate integration test for metrics collection

### Phase 3 (Claude - Verification):

1. Review all instrumentation changes
2. Run full test suite
3. Verify metrics are correctly recorded
4. Final documentation and integration

---

## Success Metrics for Phase 8.7

After completion:

- ✅ GET /metrics endpoint returns valid Prometheus metrics
- ✅ Dashboard displays real-time event processing stats
- ✅ Cache hit rate clearly visible (should be >80% for repeated events)
- ✅ Action duration histograms show performance distribution
- ✅ Zero metrics-related performance regression
- ✅ Production operators can monitor observer system

---

## Next Steps (Phase 8.6+)

After Phase 8.7 completes:

1. **Phase 8.6:** Job Queue System (uses metrics for monitoring)
2. **Phase 8.5:** Elasticsearch Integration
3. **Phase 8.8-8.11:** Resilience, tooling, docs

---

## Key Files Reference

**Executor/Main Processing:**
- `crates/fraiseql-observers/src/executor.rs` - Process events, execute actions
- `crates/fraiseql-observers/src/factory.rs` - Create executors with composition

**Caching/Dedup:**
- `crates/fraiseql-observers/src/cached_executor.rs` - Cache layer
- `crates/fraiseql-observers/src/deduped_executor.rs` - Deduplication layer

**Actions (7 types):**
- `crates/fraiseql-observers/src/actions.rs` - Webhook, Slack, Email
- `crates/fraiseql-observers/src/actions_additional.rs` - SMS, Push, Search, Cache

**Server Integration:**
- `crates/fraiseql-server/src/main.rs` - HTTP server (needs /metrics route)

---

**Status:** Planning complete. Ready for implementation with hybrid approach.
