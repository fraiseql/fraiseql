# Phase 9.2.A - Prometheus Metrics Collection Guide

**Status**: Complete
**Last Updated**: January 22, 2026

---

## Overview

Phase 9.2.A provides comprehensive Prometheus metrics for the FraiseQL Observer System, extending Phase 9.1 tracing with quantitative performance data. This guide covers metrics setup, integration, and usage.

---

## Quick Start

### Step 1: Enable Metrics Feature

```toml
# Cargo.toml
[features]
default = ["phase8"]  # Includes metrics
phase8 = ["checkpoint", "dedup", "caching", "queue", "search", "metrics"]
```

### Step 2: Initialize Metrics at Startup

```rust
use fraiseql_observers::metrics::ObserverMetrics;
use prometheus::Registry;

#[tokio::main]
async fn main() -> Result<()> {
    // Create Prometheus registry
    let registry = Registry::new();

    // Initialize metrics
    let metrics = ObserverMetrics::new(&registry)?;

    // Store metrics in application state
    let app_state = AppState {
        metrics,
        // ... other fields
    };

    Ok(())
}
```

### Step 3: Expose Metrics Endpoint

```rust
use axum::{Router, Json};

// Add to your Axum router
let app = Router::new()
    .route("/metrics", axum::routing::get(metrics_endpoint))
    // ... other routes
    .with_state(AppState);

async fn metrics_endpoint(
    State(state): State<AppState>,
) -> String {
    // Gather and return Prometheus metrics
    let encoder = prometheus::TextEncoder::new();
    let metric_families = state.metrics.registry.gather();
    encoder.encode(&metric_families, &mut vec![])
        .unwrap_or_else(|_| String::from("Failed to encode metrics"))
}
```

### Step 4: Scrape with Prometheus

```yaml
# prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'observer'
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: '/metrics'
```

### Step 5: View in Grafana

1. Add Prometheus data source: `http://localhost:9090`
2. Create dashboard
3. Query metrics: `events_processed_total`, `action_duration_ms`, etc.

---

## Available Metrics

### Event Processing Metrics

**`events_processed_total` (Counter)**
- Description: Total number of events processed
- Type: Counter (always increasing)
- Use: Monitor event throughput

```rust
metrics.events_processed_total.inc();
metrics.events_processed_total.inc_by(5.0);
```

**`event_processing_duration_ms` (Histogram)**
- Description: Distribution of event processing duration
- Type: Histogram (P50, P95, P99, etc.)
- Use: Analyze latency distribution

```rust
metrics.event_processing_duration_ms.observe(150.0);
```

**`events_in_flight` (Gauge)**
- Description: Number of events currently being processed
- Type: Gauge (can increase/decrease)
- Use: Monitor concurrency

```rust
metrics.events_in_flight.inc();    // Processing started
metrics.events_in_flight.dec();    // Processing completed
```

### Action Execution Metrics

**`actions_executed_total` (Counter)**
- Description: Total number of actions executed
- Type: Counter
- Use: Monitor action volume

```rust
metrics.actions_executed_total.inc();
```

**`action_success_total` (Counter)**
- Description: Total number of successful actions
- Type: Counter
- Use: Calculate success rate

```rust
metrics.action_success_total.inc();
```

**`action_failure_total` (Counter)**
- Description: Total number of failed actions
- Type: Counter
- Use: Monitor error rate

```rust
metrics.action_failure_total.inc();
```

**`action_duration_ms` (Histogram)**
- Description: Action execution duration distribution
- Type: Histogram
- Use: Analyze action performance

```rust
metrics.action_duration_ms.observe(50.0);
```

**`action_timeout_total` (Counter)**
- Description: Total number of action timeouts
- Type: Counter
- Use: Monitor timeout frequency

```rust
metrics.action_timeout_total.inc();
```

### Queue Metrics

**`queue_depth` (Gauge)**
- Description: Current number of pending jobs
- Type: Gauge
- Use: Monitor queue backlog

```rust
metrics.queue_depth.set(42.0);
metrics.queue_depth.inc();
metrics.queue_depth.dec();
```

**`queue_jobs_enqueued_total` (Counter)**
- Description: Total jobs enqueued
- Type: Counter
- Use: Monitor job throughput

```rust
metrics.queue_jobs_enqueued_total.inc();
```

**`queue_jobs_processed_total` (Counter)**
- Description: Total jobs processed from queue
- Type: Counter
- Use: Monitor processing rate

```rust
metrics.queue_jobs_processed_total.inc();
```

**`queue_retry_total` (Counter)**
- Description: Total jobs retried
- Type: Counter
- Use: Monitor retry frequency

```rust
metrics.queue_retry_total.inc();
```

**`queue_deadletter_total` (Counter)**
- Description: Total jobs moved to dead letter queue
- Type: Counter
- Use: Monitor permanent failures

```rust
metrics.queue_deadletter_total.inc();
```

### Cache Metrics

**`cache_hits_total` (Counter)**
- Description: Total cache hits
- Type: Counter
- Use: Monitor cache effectiveness

```rust
metrics.cache_hits_total.inc();
```

**`cache_misses_total` (Counter)**
- Description: Total cache misses
- Type: Counter
- Use: Monitor cache misses

```rust
metrics.cache_misses_total.inc();
```

**`cache_hit_rate` (Gauge)**
- Description: Current cache hit rate (0-100%)
- Type: Gauge
- Use: Monitor cache efficiency

```rust
let total = hits + misses;
let rate = (hits / total) * 100.0;
metrics.cache_hit_rate.set(rate);
```

### Deduplication Metrics

**`dedup_checks_total` (Counter)**
- Description: Total deduplication checks performed
- Type: Counter
- Use: Monitor dedup volume

```rust
metrics.dedup_checks_total.inc();
```

**`dedup_duplicates_found_total` (Counter)**
- Description: Total duplicates found
- Type: Counter
- Use: Monitor duplicate rate

```rust
metrics.dedup_duplicates_found_total.inc();
```

**`dedup_hit_rate` (Gauge)**
- Description: Deduplication hit rate (0-100%)
- Type: Gauge
- Use: Monitor duplicate detection effectiveness

```rust
let rate = (duplicates / checks) * 100.0;
metrics.dedup_hit_rate.set(rate);
```

### Checkpoint Metrics

**`checkpoint_saves_total` (Counter)**
- Description: Total checkpoint saves
- Type: Counter
- Use: Monitor checkpoint frequency

```rust
metrics.checkpoint_saves_total.inc();
```

**`checkpoint_save_duration_ms` (Histogram)**
- Description: Checkpoint save duration distribution
- Type: Histogram
- Use: Monitor checkpoint performance

```rust
metrics.checkpoint_save_duration_ms.observe(75.0);
```

**`checkpoint_recovery_total` (Counter)**
- Description: Total checkpoint recovery operations
- Type: Counter
- Use: Monitor recovery frequency

```rust
metrics.checkpoint_recovery_total.inc();
```

---

## Integration with Phase 9.1 Tracing

### Combining Traces and Metrics

**Metrics provide the "what"** (what happened)
**Traces provide the "why"** (why it happened)

Example: High latency

```
Metrics show: event_processing_duration_ms is high (P99 = 2000ms)
                ↓
Trace shows: webhook action took 1800ms waiting for TCP connection
               ↓
Root cause: Network issue with webhook endpoint
```

### Recording Metrics During Event Processing

```rust
// In listener
let start = Instant::now();
let batch_size = events.len();

// Process events
let result = process_events(&events).await;

// Record metrics
let duration_ms = start.elapsed().as_millis() as u128;
metrics.event_processing_duration_ms.observe(duration_ms as f64);
metrics.events_in_flight.dec();

if result.is_err() {
    metrics.events_processed_total.inc_by(0.0);  // Don't count failed events
} else {
    metrics.events_processed_total.inc_by(batch_size as f64);
}
```

### Recording Metrics During Action Execution

```rust
// In action executor
let start = Instant::now();

match execute_action(&action).await {
    Ok(result) => {
        metrics.actions_executed_total.inc();
        metrics.action_success_total.inc();

        let duration_ms = start.elapsed().as_millis() as f64;
        metrics.action_duration_ms.observe(duration_ms);
    }
    Err(e) => {
        metrics.actions_executed_total.inc();
        metrics.action_failure_total.inc();

        if is_timeout(&e) {
            metrics.action_timeout_total.inc();
        }

        let duration_ms = start.elapsed().as_millis() as f64;
        metrics.action_duration_ms.observe(duration_ms);
    }
}
```

---

## Common Queries

### PromQL Examples for Grafana

**Event Processing Rate (events/sec)**
```promql
rate(events_processed_total[1m])
```

**Event Processing Latency (P99)**
```promql
histogram_quantile(0.99, event_processing_duration_ms)
```

**Action Success Rate (%)**
```promql
(action_success_total / action_executed_total) * 100
```

**Queue Depth Over Time**
```promql
queue_depth
```

**Cache Hit Rate**
```promql
cache_hit_rate
```

**Deduplication Effectiveness**
```promql
dedup_hit_rate
```

---

## Dashboard Setup

### System Overview Dashboard

Key panels:
- Event processing rate (events/sec)
- Event latency P50, P95, P99
- Action success rate (%)
- Queue depth
- Cache hit rate
- Error rate

Query examples:
```
Panel: Event Processing Rate
Query: rate(events_processed_total[1m])

Panel: Event Latency P99
Query: histogram_quantile(0.99, event_processing_duration_ms)

Panel: Action Success Rate
Query: (action_success_total / (action_success_total + action_failure_total)) * 100

Panel: Queue Depth
Query: queue_depth
```

### Troubleshooting Dashboard

Key panels:
- Failed actions (count)
- Action timeouts (count)
- Dead letter queue size
- Failed checkpoints
- Error rate over time

Query examples:
```
Panel: Failed Actions
Query: action_failure_total

Panel: Action Timeouts
Query: rate(action_timeout_total[5m])

Panel: Dead Letter Queue
Query: queue_deadletter_total

Panel: Error Rate
Query: rate(action_failure_total[1m]) / rate(action_executed_total[1m])
```

---

## Performance Considerations

### Metric Overhead

| Operation | Overhead | Notes |
|-----------|----------|-------|
| Counter increment | < 0.1ms | Thread-safe atomic operation |
| Gauge set/inc | < 0.1ms | Simple value operation |
| Histogram observe | < 0.5ms | Bucket calculation |
| Registry gather | 1-5ms | Per scrape cycle |

### Optimization Tips

1. **Batch Metric Updates**
   ```rust
   // Bad: update each metric individually
   for event in events {
       metrics.events_processed_total.inc();
   }

   // Good: batch update
   metrics.events_processed_total.inc_by(events.len() as f64);
   ```

2. **Use Gauges for Ranges**
   ```rust
   // Use gauge for current values (can go up/down)
   metrics.queue_depth.set(queue.len() as f64);

   // Don't use counter for ranges
   // metrics.queue_depth_count.inc_by(...);  // Wrong!
   ```

3. **Sample Histograms**
   ```rust
   // For high-frequency operations, consider sampling
   if rand::random::<f32>() < 0.01 {  // 1% sample
       metrics.action_duration_ms.observe(duration as f64);
   }
   ```

---

## Alerting Rules

### Example Prometheus Alert Rules

```yaml
groups:
- name: observer_alerts
  rules:

  # High action failure rate
  - alert: HighActionFailureRate
    expr: |
      (
        rate(action_failure_total[5m]) /
        rate(action_executed_total[5m])
      ) > 0.05
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "High action failure rate ({{ $value | humanizePercentage }})"

  # Queue backing up
  - alert: QueueDepthHigh
    expr: queue_depth > 1000
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "Queue depth critically high: {{ $value }}"

  # Many timeouts
  - alert: ManyActionTimeouts
    expr: rate(action_timeout_total[5m]) > 1
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "{{ $value | humanize }} timeouts per second"

  # Low cache hit rate
  - alert: LowCacheHitRate
    expr: cache_hit_rate < 20
    for: 10m
    labels:
      severity: info
    annotations:
      summary: "Cache hit rate low: {{ $value }}%"
```

---

## Testing Metrics

### Unit Test Example

```rust
#[test]
fn test_event_metrics() {
    let registry = Registry::new();
    let metrics = ObserverMetrics::new(&registry).unwrap();

    // Simulate event processing
    metrics.events_processed_total.inc_by(100.0);
    metrics.event_processing_duration_ms.observe(150.0);
    metrics.event_processing_duration_ms.observe(200.0);

    // Verify metrics
    let families = registry.gather();
    assert!(!families.is_empty());

    // Check counter value
    let counter_value = metrics
        .events_processed_total
        .metric()
        .get_counter()
        .get_value();
    assert_eq!(counter_value, 100.0);
}
```

### Integration Test Example

```rust
#[tokio::test]
async fn test_full_event_processing_metrics() {
    let registry = Registry::new();
    let metrics = ObserverMetrics::new(&registry).unwrap();

    // Simulate complete event processing
    metrics.events_in_flight.inc();
    metrics.event_processing_duration_ms.observe(100.0);
    metrics.events_processed_total.inc();
    metrics.events_in_flight.dec();

    // Verify final state
    assert_eq!(
        metrics.events_in_flight.metric().get_gauge().get_value(),
        0.0
    );
}
```

---

## Production Deployment

### Docker Setup

```dockerfile
FROM rust:latest

WORKDIR /app

# Enable metrics feature
ENV FEATURES="phase8"

COPY . .
RUN cargo build --release --features $FEATURES

EXPOSE 8080 9090

CMD ["./target/release/observer"]
```

### Kubernetes Deployment

```yaml
apiVersion: v1
kind: Service
metadata:
  name: observer-metrics
spec:
  selector:
    app: observer
  ports:
  - name: metrics
    port: 9090
    targetPort: 8080

---
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: observer
spec:
  selector:
    matchLabels:
      app: observer
  endpoints:
  - port: metrics
    interval: 30s
    path: /metrics
```

---

## Troubleshooting

### Metrics Not Appearing

**Check 1**: Is metrics feature enabled?
```bash
cargo build --features phase8
```

**Check 2**: Is metrics endpoint responding?
```bash
curl http://localhost:8080/metrics
```

**Check 3**: Is Prometheus scraping?
```
Check Prometheus Targets: http://localhost:9090/targets
Should show: localhost:8080/metrics UP
```

### High Prometheus Memory Usage

**Solution**: Reduce cardinality
```promql
# Check metric cardinality
topk(10, count by (__name__) (ALERTS))
```

**Action**: Limit label values if using labels

### Gaps in Metrics

**Cause**: Listener not recording metrics
**Solution**: Verify metrics recording code

```rust
// Ensure you're calling metric operations
metrics.events_processed_total.inc_by(batch_size as f64);
```

---

## Next Phase: Phase 9.2.B

After metrics collection is working, Phase 9.2.B adds:
- Automatic span creation via macros
- `#[traced]` macro for automatic instrumentation
- `#[instrument]` macro for structured logging

---

**Document**: Phase 9.2.A - Prometheus Metrics Collection Guide
**Status**: Complete
**Last Updated**: January 22, 2026
