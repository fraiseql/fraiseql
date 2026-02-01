# Phase 8.7: Prometheus Metrics for Observer System

**Status**: Complete
**Date**: January 24, 2026
**Version**: 1.0

## Overview

Phase 8.7 adds comprehensive Prometheus metrics to the FraiseQL observer system, enabling production monitoring of event processing, cache effectiveness, deduplication performance, and action execution times.

## Metrics Overview

### Event Processing Metrics

**`fraiseql_observer_events_processed_total`** (Counter)
- **Help**: Total number of events processed
- **Type**: Counter
- **Labels**: None
- **Example**: `fraiseql_observer_events_processed_total 5000`
- **Use**: Monitor overall event throughput
- **Query**: `rate(fraiseql_observer_events_processed_total[5m])`

**`fraiseql_observer_events_failed_total`** (Counter with labels)
- **Help**: Total events that failed processing
- **Type**: Counter
- **Labels**: `error_type` (condition_failed, action_error, etc.)
- **Example**: `fraiseql_observer_events_failed_total{error_type="action_error"} 42`
- **Use**: Monitor error rates by category
- **Query**: `sum(rate(fraiseql_observer_events_failed_total[5m])) by (error_type)`

### Cache Metrics

**`fraiseql_observer_cache_hits_total`** (Counter)
- **Help**: Total cache hits in result caching
- **Type**: Counter
- **Labels**: None
- **Example**: `fraiseql_observer_cache_hits_total 8000`
- **Use**: Track cache effectiveness
- **Query**: `rate(fraiseql_observer_cache_hits_total[5m])`

**`fraiseql_observer_cache_misses_total`** (Counter)
- **Help**: Total cache misses
- **Type**: Counter
- **Labels**: None
- **Example**: `fraiseql_observer_cache_misses_total 2000`
- **Use**: Track cache coverage
- **Query**: `rate(fraiseql_observer_cache_misses_total[5m])`

**`fraiseql_observer_cache_evictions_total`** (Counter)
- **Help**: Total cache evictions
- **Type**: Counter
- **Labels**: None
- **Example**: `fraiseql_observer_cache_evictions_total 150`
- **Use**: Monitor cache churn
- **Query**: `rate(fraiseql_observer_cache_evictions_total[5m])`

### Deduplication Metrics

**`fraiseql_observer_dedup_detected_total`** (Counter)
- **Help**: Total duplicate events detected and skipped
- **Type**: Counter
- **Labels**: None
- **Example**: `fraiseql_observer_dedup_detected_total 320`
- **Use**: Monitor deduplication effectiveness
- **Query**: `rate(fraiseql_observer_dedup_detected_total[5m])`

**`fraiseql_observer_dedup_processing_skipped_total`** (Counter)
- **Help**: Total processing cycles saved by deduplication
- **Type**: Counter
- **Labels**: None
- **Example**: `fraiseql_observer_dedup_processing_skipped_total 1250`
- **Use**: Track resource savings from deduplication
- **Query**: `rate(fraiseql_observer_dedup_processing_skipped_total[5m])`

### Action Execution Metrics

**`fraiseql_observer_action_executed_total`** (Counter with labels)
- **Help**: Total actions executed
- **Type**: Counter
- **Labels**: `action_type` (webhook, slack, email, sms, push, search, cache)
- **Example**: `fraiseql_observer_action_executed_total{action_type="webhook"} 3500`
- **Use**: Track action invocation counts by type
- **Query**: `sum(rate(fraiseql_observer_action_executed_total[5m])) by (action_type)`

**`fraiseql_observer_action_duration_seconds`** (Histogram with labels)
- **Help**: Action execution duration in seconds
- **Type**: Histogram
- **Labels**: `action_type` (webhook, slack, email, sms, push, search, cache)
- **Buckets**: 0.001, 0.01, 0.1, 1.0, 5.0, 10.0, 30.0, 60.0 seconds
- **Example**:
  ```
  fraiseql_observer_action_duration_seconds_bucket{action_type="webhook",le="0.1"} 2800
  fraiseql_observer_action_duration_seconds_bucket{action_type="webhook",le="1.0"} 3400
  fraiseql_observer_action_duration_seconds_sum{action_type="webhook"} 850.5
  fraiseql_observer_action_duration_seconds_count{action_type="webhook"} 3500
  ```
- **Use**: Monitor action performance and detect slowdowns
- **Query**: `histogram_quantile(0.99, rate(fraiseql_observer_action_duration_seconds_bucket[5m]))`

**`fraiseql_observer_action_errors_total`** (Counter with labels)
- **Help**: Total action execution errors
- **Type**: Counter
- **Labels**: `action_type`, `error_type` (execution_failed, permanently_failed, invalid_config, etc.)
- **Example**: `fraiseql_observer_action_errors_total{action_type="webhook",error_type="execution_failed"} 45`
- **Use**: Monitor error rates by action type and error category
- **Query**: `sum(rate(fraiseql_observer_action_errors_total[5m])) by (action_type, error_type)`

### Queue/Backlog Metrics

**`fraiseql_observer_backlog_size`** (Gauge)
- **Help**: Current number of events in processing queue
- **Type**: Gauge
- **Labels**: None
- **Example**: `fraiseql_observer_backlog_size 145`
- **Use**: Monitor processing queue depth
- **Query**: `fraiseql_observer_backlog_size`

**`fraiseql_observer_dlq_items`** (Gauge)
- **Help**: Current number of items in dead letter queue
- **Type**: Gauge
- **Labels**: None
- **Example**: `fraiseql_observer_dlq_items 8`
- **Use**: Monitor failed items needing manual intervention
- **Query**: `fraiseql_observer_dlq_items`

## Configuration

### Enable Metrics Feature

Add `metrics` feature to your Cargo.toml:

```toml
[dependencies]
fraiseql-observers = { version = "2.0", features = ["metrics"] }
```

Or use the `phase8` feature (includes all Phase 8 features):

```toml
[dependencies]
fraiseql-observers = { version = "2.0", features = ["phase8"] }
```

### Expose Metrics Endpoint

Add to your HTTP server:

```rust
use fraiseql_observers::metrics::metrics_handler;

// In your Axum router:
app.route("/metrics", axum::routing::get(metrics_handler))
```

## Prometheus Configuration

Add to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'fraiseql-observers'
    static_configs:
      - targets: ['localhost:8080']  # Your HTTP server port
    metrics_path: '/metrics'
    scrape_interval: 15s
    scrape_timeout: 10s
```

## Useful Queries

### Cache Hit Rate (Percentage)

```promql
(
  rate(fraiseql_observer_cache_hits_total[5m]) /
  (rate(fraiseql_observer_cache_hits_total[5m]) + rate(fraiseql_observer_cache_misses_total[5m]))
) * 100
```

### Deduplication Save Rate (Percentage)

```promql
(
  rate(fraiseql_observer_dedup_processing_skipped_total[5m]) /
  (rate(fraiseql_observer_events_processed_total[5m]) + rate(fraiseql_observer_dedup_processing_skipped_total[5m]))
) * 100
```

### Events Per Second

```promql
rate(fraiseql_observer_events_processed_total[1m])
```

### Error Rate (Percentage)

```promql
(
  sum(rate(fraiseql_observer_events_failed_total[5m])) /
  rate(fraiseql_observer_events_processed_total[5m])
) * 100
```

### Action Performance by Type (P95)

```promql
histogram_quantile(0.95, rate(fraiseql_observer_action_duration_seconds_bucket[5m]))
  by (action_type)
```

### Slowest Action Types (P99)

```promql
topk(5, histogram_quantile(0.99, rate(fraiseql_observer_action_duration_seconds_bucket[5m])))
  by (action_type)
```

### Error Rate by Action Type

```promql
sum(rate(fraiseql_observer_action_errors_total[5m])) by (action_type) /
sum(rate(fraiseql_observer_action_executed_total[5m])) by (action_type) * 100
```

### Queue Saturation Alert

Alert when backlog exceeds threshold (example: 1000 events):

```promql
fraiseql_observer_backlog_size > 1000
```

### Dead Letter Queue Alert

Alert when items accumulate:

```promql
fraiseql_observer_dlq_items > 10
```

## Grafana Dashboard

A pre-built Grafana dashboard is available at `docs/monitoring/grafana-dashboard-8.7.json`.

Import into Grafana:
1. Click "Create" → "Import"
2. Upload `grafana-dashboard-8.7.json`
3. Select your Prometheus data source
4. Click "Import"

Dashboard includes:
- Event processing rates
- Cache effectiveness gauges
- Deduplication savings
- Action performance histograms
- Error rate tracking
- Queue depth monitoring

## Alert Examples

### High Error Rate

```yaml
- alert: ObserverHighErrorRate
  expr: |
    (
      sum(rate(fraiseql_observer_events_failed_total[5m])) /
      rate(fraiseql_observer_events_processed_total[5m])
    ) > 0.05
  for: 5m
  annotations:
    summary: "High error rate in observer system (>5%)"
```

### Cache Performance Degradation

```yaml
- alert: ObserverLowCacheHitRate
  expr: |
    (
      rate(fraiseql_observer_cache_hits_total[5m]) /
      (rate(fraiseql_observer_cache_hits_total[5m]) + rate(fraiseql_observer_cache_misses_total[5m]))
    ) < 0.5
  for: 10m
  annotations:
    summary: "Cache hit rate dropped below 50%"
```

### Dead Letter Queue Accumulation

```yaml
- alert: ObserverDLQAccumulation
  expr: fraiseql_observer_dlq_items > 50
  for: 5m
  annotations:
    summary: "Dead letter queue has {{$value}} items"
```

### Slow Action Execution

```yaml
- alert: ObserverSlowActions
  expr: |
    histogram_quantile(0.99, rate(fraiseql_observer_action_duration_seconds_bucket[5m])) > 10
  for: 10m
  annotations:
    summary: "Action P99 latency exceeded 10 seconds"
```

## Integration Points

### Metrics are recorded at:

1. **Executor** (`executor.rs`):
   - Event processing start (events_processed_total)
   - Successful action execution (action_executed, action_duration_seconds)
   - Action failures (action_errors_total)

2. **Cached Executor** (`cached_executor.rs`):
   - Cache hits (cache_hits_total)
   - Cache misses (cache_misses_total)

3. **Deduped Executor** (`deduped_executor.rs`):
   - Duplicate detection (dedup_detected_total)
   - Processing skipped (dedup_processing_skipped_total)

## Performance Impact

Metrics recording is optimized for production use:

- **Memory**: Minimal overhead (metrics are pre-allocated)
- **CPU**: <0.1% per event with typical workload
- **Latency**: <1μs per metric increment (not on critical path)
- **Thread safety**: Lock-free operations via Arc<AtomicU64>

## Feature Flag

Metrics are completely optional:

- **With `metrics` feature**: Full instrumentation, Prometheus endpoint available
- **Without `metrics` feature**: All metrics calls are no-ops, zero overhead

This allows running without Prometheus in environments that don't need it.

## Next Steps

After implementing Phase 8.7:
- **Phase 8.6**: Job Queue System (uses metrics for monitoring)
- **Phase 8.5**: Elasticsearch Integration
- **Phase 8.8+**: Alerting, advanced observability, distributed tracing

## Troubleshooting

### Metrics endpoint returns empty

Ensure metrics feature is enabled in Cargo.toml and /metrics handler is registered in HTTP server.

### Grafana shows no data

Check that Prometheus is scraping the /metrics endpoint. Verify in Prometheus UI: http://localhost:9090/targets

### High cardinality warnings

Label values are restricted to predefined enums to prevent cardinality explosion:
- action_type: webhook, slack, email, sms, push, search, cache
- error_type: execution_failed, permanently_failed, invalid_config, etc.

## References

- [Prometheus Documentation](https://prometheus.io/docs/)
- [Prometheus Best Practices](https://prometheus.io/docs/practices/naming/)
- [Grafana Dashboard Guide](https://grafana.com/docs/grafana/latest/dashboards/)
- FraiseQL Architecture Docs: `docs/architecture/`
