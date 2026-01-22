# Phase 8.7: Prometheus Metrics - Implementation Plan

**Date**: January 22, 2026
**Objective**: Comprehensive instrumentation for production monitoring and observability
**Target**: 150+ tests passing (138 + 12 new), production-ready metrics

## Problem Statement

**Without Metrics**:
- No visibility into system performance
- Can't track event processing rates
- No alerting on failures or slowdowns
- Hard to debug production issues
- Can't optimize bottlenecks

**With Prometheus Metrics**:
- Real-time monitoring dashboards
- Performance trend tracking
- Automated alerting on thresholds
- Easy root cause analysis
- Data-driven optimization

## Architecture Overview

### Metrics Categories

```
System Metrics
├─ Event Processing
│  ├─ events_processed_total (counter)
│  ├─ event_processing_duration_ms (histogram)
│  └─ events_in_flight (gauge)
│
├─ Action Execution
│  ├─ actions_executed_total (counter)
│  ├─ action_success_total (counter)
│  ├─ action_failure_total (counter)
│  ├─ action_duration_ms (histogram)
│  └─ action_timeout_total (counter)
│
├─ Queue Metrics
│  ├─ queue_depth (gauge)
│  ├─ queue_jobs_enqueued_total (counter)
│  ├─ queue_jobs_processed_total (counter)
│  ├─ queue_retry_total (counter)
│  └─ queue_deadletter_total (counter)
│
├─ Cache Metrics
│  ├─ cache_hits_total (counter)
│  ├─ cache_misses_total (counter)
│  ├─ cache_hit_rate (gauge)
│  └─ cache_size_bytes (gauge)
│
├─ Dedup Metrics
│  ├─ dedup_checks_total (counter)
│  ├─ dedup_duplicates_found_total (counter)
│  └─ dedup_hit_rate (gauge)
│
└─ Checkpoint Metrics
   ├─ checkpoint_saves_total (counter)
   ├─ checkpoint_save_duration_ms (histogram)
   └─ checkpoint_recovery_total (counter)
```

### Metric Types

| Type | Purpose | Example |
|------|---------|---------|
| **Counter** | Monotonically increasing | events_processed_total |
| **Gauge** | Current snapshot value | queue_depth |
| **Histogram** | Distribution of values | action_duration_ms |
| **Summary** | Percentiles (alternative to histogram) | request_latency_seconds |

## Implementation Steps

### Step 1: Metrics Registry (50 lines)
**File**: `src/metrics/mod.rs`

Central registry for all metrics:
```rust
pub struct ObserverMetrics {
    // Event metrics
    pub events_processed_total: Counter,
    pub event_processing_duration_ms: Histogram,
    pub events_in_flight: Gauge,

    // Action metrics
    pub actions_executed_total: Counter,
    pub action_success_total: Counter,
    pub action_failure_total: Counter,
    pub action_duration_ms: Histogram,
    pub action_timeout_total: Counter,

    // Queue metrics
    pub queue_depth: Gauge,
    pub queue_jobs_enqueued_total: Counter,
    pub queue_jobs_processed_total: Counter,
    pub queue_retry_total: Counter,
    pub queue_deadletter_total: Counter,

    // Cache metrics
    pub cache_hits_total: Counter,
    pub cache_misses_total: Counter,
    pub cache_hit_rate: Gauge,

    // Dedup metrics
    pub dedup_checks_total: Counter,
    pub dedup_duplicates_found_total: Counter,

    // Checkpoint metrics
    pub checkpoint_saves_total: Counter,
    pub checkpoint_save_duration_ms: Histogram,
}

impl ObserverMetrics {
    pub fn new(registry: &prometheus::Registry) -> Result<Self> { ... }
}
```

Tests (2):
- test_metrics_creation
- test_metrics_registration

### Step 2: Event Processing Instrumentation (60 lines)
**File**: `src/executor.rs` (modifications)

Add metrics collection to observer executor:
```rust
pub async fn process_event(&self, event: &EntityEvent) -> Result<ExecutionSummary> {
    let start = Instant::now();
    self.metrics.events_in_flight.inc();

    let result = self.execute_observers(event).await;

    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    self.metrics.event_processing_duration_ms.observe(duration_ms);
    self.metrics.events_processed_total.inc();
    self.metrics.events_in_flight.dec();

    Ok(ExecutionSummary { ... })
}
```

Tests (3):
- test_event_processing_metrics
- test_event_metrics_on_success
- test_event_metrics_on_failure

### Step 3: Action Execution Instrumentation (80 lines)
**File**: `src/traits.rs` (modifications)

Wrap action executor to track metrics:
```rust
pub struct MetricsActionExecutor<E> {
    inner: E,
    metrics: Arc<ObserverMetrics>,
}

impl<E: ActionExecutor> ActionExecutor for MetricsActionExecutor<E> {
    async fn execute(&self, event: &EntityEvent, action: &ActionConfig) -> Result<ActionResult> {
        let start = Instant::now();
        self.metrics.actions_executed_total.inc();

        let result = match self.inner.execute(event, action).await {
            Ok(action_result) => {
                if action_result.success {
                    self.metrics.action_success_total.inc();
                } else {
                    self.metrics.action_failure_total.inc();
                }
                Ok(action_result)
            }
            Err(e) => {
                self.metrics.action_failure_total.inc();
                Err(e)
            }
        };

        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        self.metrics.action_duration_ms.observe(duration_ms);

        result
    }
}
```

Tests (5):
- test_action_success_metrics
- test_action_failure_metrics
- test_action_duration_histogram
- test_action_timeout_tracking
- test_metrics_wrapper_transparent

### Step 4: Queue Metrics (50 lines)
**File**: `src/queue/mod.rs` (modifications)

Add metrics to job queue operations:
```rust
pub async fn enqueue(&self, job: &Job) -> Result<String> {
    let job_id = self.enqueue_internal(job).await?;
    self.metrics.queue_jobs_enqueued_total.inc();
    self.metrics.queue_depth.inc();
    Ok(job_id)
}

pub async fn mark_success(&self, job_id: &str) -> Result<()> {
    self.mark_success_internal(job_id).await?;
    self.metrics.queue_jobs_processed_total.inc();
    self.metrics.queue_depth.dec();
    Ok(())
}

pub async fn mark_retry(&self, job_id: &str, next_retry_at: i64) -> Result<()> {
    self.mark_retry_internal(job_id, next_retry_at).await?;
    self.metrics.queue_retry_total.inc();
    Ok(())
}

pub async fn mark_deadletter(&self, job_id: &str, reason: &str) -> Result<()> {
    self.mark_deadletter_internal(job_id, reason).await?;
    self.metrics.queue_deadletter_total.inc();
    Ok(())
}
```

Tests (4):
- test_queue_enqueue_metrics
- test_queue_retry_metrics
- test_queue_deadletter_metrics
- test_queue_depth_tracking

### Step 5: Cache Metrics (40 lines)
**File**: `src/cache/mod.rs` (modifications)

Track cache hit rates:
```rust
pub async fn get(&self, key: &str) -> Result<Option<CachedResult>> {
    if let Some(result) = self.get_internal(key).await? {
        self.metrics.cache_hits_total.inc();
        self.update_hit_rate();
        Ok(Some(result))
    } else {
        self.metrics.cache_misses_total.inc();
        self.update_hit_rate();
        Ok(None)
    }
}

fn update_hit_rate(&self) {
    let total_hits = self.metrics.cache_hits_total.get_value();
    let total_misses = self.metrics.cache_misses_total.get_value();
    let total = total_hits + total_misses;
    if total > 0.0 {
        let rate = (total_hits / total) * 100.0;
        self.metrics.cache_hit_rate.set(rate);
    }
}
```

Tests (3):
- test_cache_hit_metrics
- test_cache_miss_metrics
- test_cache_hit_rate_calculation

### Step 6: Dedup Metrics (30 lines)
**File**: `src/dedup/mod.rs` (modifications)

Track deduplication effectiveness:
```rust
pub async fn is_duplicate(&self, key: &str) -> Result<bool> {
    self.metrics.dedup_checks_total.inc();

    let is_dup = self.is_duplicate_internal(key).await?;

    if is_dup {
        self.metrics.dedup_duplicates_found_total.inc();
    }

    self.update_dedup_rate();
    Ok(is_dup)
}

fn update_dedup_rate(&self) {
    let checks = self.metrics.dedup_checks_total.get_value();
    let dups = self.metrics.dedup_duplicates_found_total.get_value();
    if checks > 0.0 {
        let rate = (dups / checks) * 100.0;
        self.metrics.dedup_hit_rate.set(rate);
    }
}
```

Tests (3):
- test_dedup_check_metrics
- test_dedup_duplicate_tracking
- test_dedup_rate_calculation

### Step 7: Checkpoint Metrics (30 lines)
**File**: `src/checkpoint/mod.rs` (modifications)

Track checkpoint operations:
```rust
pub async fn save(&self, listener_id: &str, state: &CheckpointState) -> Result<()> {
    let start = Instant::now();
    self.save_internal(listener_id, state).await?;

    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    self.metrics.checkpoint_saves_total.inc();
    self.metrics.checkpoint_save_duration_ms.observe(duration_ms);

    Ok(())
}

pub async fn recover(&self, listener_id: &str) -> Result<Option<CheckpointState>> {
    if self.recover_internal(listener_id).await?.is_some() {
        self.metrics.checkpoint_recovery_total.inc();
    }
    Ok(state)
}
```

Tests (3):
- test_checkpoint_save_metrics
- test_checkpoint_recovery_metrics
- test_checkpoint_duration_tracking

### Step 8: HTTP Metrics Endpoint (100 lines)
**File**: `src/metrics/http.rs`

Expose metrics via HTTP for Prometheus scraping:
```rust
use axum::{response::IntoResponse, routing::get, Router};
use prometheus::TextEncoder;

pub fn create_metrics_router(metrics: Arc<ObserverMetrics>) -> Router {
    Router::new()
        .route("/metrics", get(handle_metrics).with_state(metrics))
}

async fn handle_metrics(
    State(metrics): State<Arc<ObserverMetrics>>,
) -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = metrics.registry.gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    ([(("Content-Type", "text/plain")],
     String::from_utf8(buffer).unwrap())
}
```

**Optional HTTP framework support**:
- Works with axum (already used in some services)
- Can also integrate with hyper or actix
- Simple standalone or part of larger API

Tests (2):
- test_metrics_endpoint_response
- test_prometheus_format_compliance

### Step 9: Metrics Configuration (40 lines)
**File**: `src/config.rs` (modifications)

Add metrics configuration:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,
    /// Histogram buckets for timing metrics
    pub histogram_buckets: Vec<f64>,
    /// Sample rate for expensive metrics (0.0-1.0)
    pub sample_rate: f64,
    /// Export interval in seconds
    pub export_interval_secs: u64,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            histogram_buckets: vec![
                1.0, 5.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0, 10000.0
            ],
            sample_rate: 1.0,
            export_interval_secs: 60,
        }
    }
}
```

Tests (2):
- test_metrics_config_defaults
- test_metrics_config_custom

### Step 10: Tests & Integration (100 lines)
**File**: `src/metrics/tests.rs`

Comprehensive test suite:
- Metrics creation and registration (2 tests)
- Counter increment/reset (3 tests)
- Gauge set/update (3 tests)
- Histogram observation and percentiles (3 tests)
- End-to-end event tracking (5 tests)
- HTTP endpoint compliance (2 tests)

**Total: 18 new tests** → 138 + 18 = 156 tests passing

## Dependencies Required

Check Cargo.toml:
- `prometheus` ✅ (already in optional dependencies)
- `axum` (optional, for HTTP endpoint)
- `hyper` (optional, for standalone server)

## File Structure

```
src/metrics/
├── mod.rs           (200 lines: ObserverMetrics, registry)
├── http.rs          (100 lines: HTTP endpoint, Prometheus format)
└── tests.rs         (100 lines: Comprehensive test suite)

Modified files:
├── src/executor.rs  (add event metrics)
├── src/queue/mod.rs (add queue metrics)
├── src/cache/mod.rs (add cache metrics)
├── src/dedup/mod.rs (add dedup metrics)
├── src/checkpoint/mod.rs (add checkpoint metrics)
└── src/traits.rs    (MetricsActionExecutor wrapper)

Total: ~600 lines of new code, ~200 lines of modifications
```

Module exports in `src/lib.rs`:
```rust
pub mod metrics;

pub use metrics::{ObserverMetrics, MetricsConfig};

#[cfg(feature = "metrics")]
pub use metrics::http::create_metrics_router;
```

Feature flag in `Cargo.toml`:
```toml
[features]
metrics = ["prometheus"]
phase8 = ["checkpoint", "dedup", "caching", "queue", "search", "metrics"]
```

## Success Criteria

✅ **Functional**:
- [ ] All metrics correctly tracked
- [ ] Counter increments on events
- [ ] Gauges update on state changes
- [ ] Histograms record durations
- [ ] HTTP endpoint serves Prometheus format

✅ **Quality**:
- [ ] 150+ tests passing (18 new)
- [ ] 100% Clippy compliant
- [ ] Zero unsafe code
- [ ] All error paths tested

✅ **Performance**:
- [ ] Metrics collection < 1ms overhead
- [ ] Counter operations < 1μs
- [ ] Gauge updates < 10μs
- [ ] Histogram observations < 100μs

✅ **Reliability**:
- [ ] Metrics survive component failures
- [ ] No data loss during high load
- [ ] Thread-safe metric operations
- [ ] Backward compatible with Phase 1-7

## Grafana Dashboard Example

```json
{
  "panels": [
    {
      "title": "Events Processed",
      "targets": [{"expr": "rate(events_processed_total[5m])"}]
    },
    {
      "title": "Action Success Rate",
      "targets": [{"expr": "rate(action_success_total[5m]) / rate(actions_executed_total[5m])"}]
    },
    {
      "title": "Queue Depth",
      "targets": [{"expr": "queue_depth"}]
    },
    {
      "title": "Cache Hit Rate",
      "targets": [{"expr": "cache_hit_rate"}]
    },
    {
      "title": "Action Duration P99",
      "targets": [{"expr": "histogram_quantile(0.99, action_duration_ms)"}]
    }
  ]
}
```

## Alerting Rules Example

```yaml
groups:
  - name: observer_alerts
    rules:
      - alert: HighActionFailureRate
        expr: (rate(action_failure_total[5m]) / rate(actions_executed_total[5m])) > 0.05
        for: 5m
        annotations:
          summary: "Action failure rate > 5%"

      - alert: QueueDepthHigh
        expr: queue_depth > 1000
        for: 10m
        annotations:
          summary: "Queue backed up with 1000+ jobs"

      - alert: CacheHitRateLow
        expr: cache_hit_rate < 50
        for: 10m
        annotations:
          summary: "Cache effectiveness degraded"
```

## Estimated Time

- Metrics registry: 30 min
- Event instrumentation: 30 min
- Action instrumentation: 40 min
- Queue/Cache/Dedup/Checkpoint: 60 min
- HTTP endpoint: 40 min
- Integration & tests: 60 min
- **Total: ~4 hours**

## Phase 8 Progress After Completion

```
Phase 8.7: Prometheus Metrics ✅ Complete
Total Progress: 61.5% (8 of 13 subphases)
```

Ready for Phase 8.8: Circuit Breaker Pattern 🚀
