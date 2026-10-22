# Observer Library Integration Guide

This guide is for code that embeds the `fraiseql-observers` library directly. To run
observers in a deployment, declare them under `[observers]` in `fraiseql.toml` and run them
inside `fraiseql-server`. See [Operating Observers](../../../docs/operations/observers.md).
The server already runs the checkpointed driver loop, the poll lease, cache invalidation
and metrics described here.

## Table of Contents

1. [Persistent Checkpoints](#persistent-checkpoints)
2. [Redis Cache Actions and Result Cache](#redis-cache-actions-and-result-cache)
3. [Prometheus Metrics](#prometheus-metrics)
4. [Circuit Breaker](#circuit-breaker)
5. [Running Several Replicas](#running-several-replicas)

---

## Persistent Checkpoints

**Purpose**: Guarantee zero-event-loss recovery on restart

### Prerequisites

- PostgreSQL 18+
- Network access to database

### Integration Steps

#### Step 1: Create the Checkpoint Table

Apply the shipped migration `crates/fraiseql-observers/migrations/02_create_observer_checkpoints.sql`:

```bash
psql "$DATABASE_URL" -f crates/fraiseql-observers/migrations/02_create_observer_checkpoints.sql
```

#### Step 2: Add Checkpoint Feature

In `Cargo.toml`:

```toml
[dependencies]
fraiseql-observers = { version = "*", features = ["checkpoint"] }
```

#### Step 3: Enable in Configuration

```rust
use std::time::Duration;

use fraiseql_observers::{
    ChangeLogListener, ChangeLogListenerConfig, CheckpointState, CheckpointStore,
    ObserverExecutor, PostgresCheckpointStore,
};

async fn drive(pool: sqlx::PgPool, executor: ObserverExecutor) -> fraiseql_observers::Result<()> {
    const LISTENER: &str = "orders";
    let checkpoints = PostgresCheckpointStore::new(pool.clone());

    // Restore the cursor BEFORE building the listener.
    let mut config = ChangeLogListenerConfig::new(pool).with_listener_id(LISTENER);
    if let Some(state) = checkpoints.load(LISTENER).await? {
        config = config.with_resume_from(state.last_processed_id);
    }
    let mut listener = ChangeLogListener::new(config);

    let mut processed = 0;
    loop {
        let batch = listener.next_batch().await?;
        let Some(last) = batch.last() else {
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        };
        for entry in &batch {
            executor.process_event(&entry.to_entity_event()?).await?;
        }
        // Record, then advance the cursor: both AFTER the actions ran (at-least-once).
        listener.record_dispatched(&batch).await?;
        processed += batch.len();
        let state = CheckpointState {
            listener_id:       LISTENER.to_string(),
            last_processed_id: last.id,
            last_processed_at: chrono::Utc::now(),
            batch_size:        batch.len(),
            event_count:       processed,
        };
        checkpoints.save(LISTENER, &state).await?;
    }
}
```

The checkpoint belongs to the driver loop, not to the executor. This is the example compiled
in the rustdoc of `fraiseql_observers::checkpoint`; `fraiseql-server` runs the same loop.

#### Step 4: Verify Integration

```bash
# 1. Create test event
psql $DATABASE_URL << EOF
INSERT INTO tb_entity_change_log (object_type, object_id, modification_type, object_data)
VALUES ('Order', 'test-123', 'INSERT', '{"status": "new"}');
EOF

# 2. Process event
cargo run --features checkpoint

# 3. Check checkpoint saved
psql $DATABASE_URL -c "SELECT * FROM observer_checkpoints;"

# 4. Verify recovery
# Restart and verify event not reprocessed
```

**Expected Output**:

```
observer_checkpoints:
 listener_id | last_processed_id | last_processed_at
-------------+-------------------+-------------------------------
 orders      |               100 | 2026-01-22 12:00:00+00
```

---

## Redis Cache Actions and Result Cache

**Purpose**: Invalidate Redis keys from `cache` actions, and cache action results

### Prerequisites

- Redis 6.0+
- The `caching` feature

```toml
[dependencies]
fraiseql-observers = { version = "*", features = ["caching"] }
```

### Cache Invalidation Actions

A `cache` action deletes the Redis keys that match its `key_pattern`. Mount the Redis
transport on the executor with `with_cache_invalidation`. Build the invalidator from the
same `RedisConfig` the server reads from `[observers.runtime.redis]`:

```rust
use std::sync::Arc;

use fraiseql_observers::{ObserverExecutor, RedisCacheInvalidator};

let invalidator = Arc::new(RedisCacheInvalidator::connect(&redis_config).await?);
let executor = ObserverExecutor::new(matcher, dlq).with_cache_invalidation(invalidator);
```

Without a mounted invalidator, a `cache` action fails instead of reporting success.

### Action Result Cache

`ObserverExecutor::with_cache` takes an optional cache backend. `RedisCacheBackend` stores
results in Redis with a fixed TTL:

```rust
use std::sync::Arc;

use fraiseql_observers::{ObserverExecutor, RedisCacheBackend};

let client = redis::Client::open("redis://localhost:6379")?;
let conn = redis::aio::ConnectionManager::new(client).await?;
let backend = Arc::new(RedisCacheBackend::new(conn, 300)); // 5-minute TTL
let executor = ObserverExecutor::with_cache(matcher, dlq, Some(backend));
```

`fraiseql-server` runs no action-result cache.

### Monitor Cache Effectiveness

```promql
fraiseql_observer_cache_hits_total
  / (fraiseql_observer_cache_hits_total + fraiseql_observer_cache_misses_total)
```

---

## Prometheus Metrics

**Purpose**: Production monitoring and alerting

### Prerequisites

- Prometheus 2.0+
- Grafana (optional)
- Understanding of metrics

### Integration Steps

#### Step 1: Enable Metrics Feature

```toml
[dependencies]
fraiseql-observers = { version = "*", features = ["metrics"] }
```

#### Step 2: Initialize Metrics

Nothing to construct: with the `metrics` feature on, every `ObserverExecutor` records into the
process-global Prometheus registry (`MetricsRegistry::global()`). `fraiseql-server` serves it on
its metrics endpoint.

#### Step 3: Configure Prometheus

```yaml
# prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'fraiseql-server'
    authorization:
      credentials: '<metrics_token>'
    static_configs:
      - targets: ['localhost:8000']
```

#### Step 4: Create Dashboard

```json
// Grafana dashboard JSON
{
  "dashboard": {
    "title": "FraiseQL Observer Metrics",
    "panels": [
      {
        "title": "Events Processed",
        "targets": [
          {
            "expr": "rate(fraiseql_observer_events_processed_total[5m])"
          }
        ]
      },
      {
        "title": "Action Failure Rate",
        "targets": [
          {
            "expr": "sum(rate(fraiseql_observer_action_errors_total[5m])) / (sum(rate(fraiseql_observer_action_executed_total[5m])) + sum(rate(fraiseql_observer_action_errors_total[5m])))"
          }
        ]
      }
    ]
  }
}
```

#### Step 5: Set Up Alerting

```yaml
# alerts.yml
groups:
  - name: fraiseql_alerts
    rules:
      - alert: HighActionFailureRate
        expr: |
          sum(rate(fraiseql_observer_action_errors_total[5m]))
            / (sum(rate(fraiseql_observer_action_executed_total[5m]))
               + sum(rate(fraiseql_observer_action_errors_total[5m]))) > 0.05
        for: 5m
        annotations:
          summary: "Action failure rate > 5%"
```

---

## Circuit Breaker

**Purpose**: Prevent cascading failures

`fraiseql_observers::resilience` (`CircuitBreaker`, `ResilientExecutor`,
`CircuitBreakerConfig`) is exported but not installed by the executor or the server. There is
no `ObserverExecutor::with_circuit_breaker`.

---

## Running Several Replicas

`CheckpointLease::postgres` (in `listener/lease.rs`) is a PostgreSQL advisory lock keyed on
a listener id. Replicas that share a listener id share one checkpoint. Have the driver loop
poll only while it holds the lease, so the others stand by and take over when the holder
stops. [Operating Observers](../../../docs/operations/observers.md) describes how
`fraiseql-server` uses this.

---

## Integration Checklist

After integrating each feature, verify:

- [ ] Dependencies and features added to Cargo.toml
- [ ] Configuration initialized
- [ ] Integration tests passing
- [ ] Metrics tracking enabled
- [ ] Monitoring/alerts configured
- [ ] Error handling tested
- [ ] Restart and takeover tested (for several replicas)

---

## Common Integration Patterns

### Pattern 1: Minimal setup (checkpoint only)

A plain `ObserverExecutor::new(matcher, dlq)`, driven by the checkpointed loop in
[Persistent Checkpoints](#persistent-checkpoints).

### Pattern 2: With Redis cache actions

The same loop, with the executor built as
`ObserverExecutor::new(matcher, dlq).with_cache_invalidation(invalidator)`.

A matched observer's actions run one after another. There is no concurrent mode.

---

## Support

- Architecture: `../../../../docs/architecture/overview.md`
- Troubleshooting: `troubleshooting.md`
- Operating observers under `fraiseql-server`: `../../../docs/operations/observers.md`
