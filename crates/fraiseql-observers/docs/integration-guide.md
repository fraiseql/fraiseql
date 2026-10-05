# Phase 8 Feature Integration Guide

This guide provides step-by-step integration instructions for each Phase 8 feature.

## Table of Contents

1. [Phase 8.1: Persistent Checkpoints](#phase-81-persistent-checkpoints)
2. [Phase 8.3: Event Deduplication](#phase-83-event-deduplication)
3. [Phase 8.4: Redis Caching](#phase-84-redis-caching)
4. [Phase 8.5: Elasticsearch Integration](#phase-85-elasticsearch-integration)
5. [Phase 8.6: Job Queue System](#phase-86-job-queue-system)
6. [Phase 8.7: Prometheus Metrics](#phase-87-prometheus-metrics)
7. [Phase 8.8: Circuit Breaker](#phase-88-circuit-breaker)
8. [Phase 8.9: Multi-Listener Failover](#phase-89-multi-listener-failover)
9. [Phase 8.10: CLI Tools](#phase-810-cli-tools)

---

## Phase 8.1: Persistent Checkpoints

**Purpose**: Guarantee zero-event-loss recovery on restart

### Prerequisites

- PostgreSQL 18+
- Network access to database

### Integration Steps

#### Step 1: Create Database Migration

```sql
-- checkpoint.sql
CREATE TABLE observer_checkpoints (
    id BIGSERIAL PRIMARY KEY,
    listener_id VARCHAR(255) NOT NULL UNIQUE,
    event_id BIGINT NOT NULL,
    last_processed_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_listener_id ON observer_checkpoints(listener_id);
CREATE INDEX idx_updated_at ON observer_checkpoints(updated_at);

-- Run migration
psql postgresql://user:pass@localhost/db < checkpoint.sql
```

#### Step 2: Add Checkpoint Feature

In `Cargo.toml`:

```toml
[features]
checkpoint = []
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
 id | listener_id | event_id | last_processed_at
----+-------------+----------+-------------------
  1 | listener-1  |      100 | 2026-01-22 12:00:00
```

---

## Phase 8.3: Event Deduplication

**Purpose**: Prevent duplicate side effects from event retries

### Prerequisites

- Redis 6.0+
- Network access to Redis
- Understanding of event hashing

### Integration Steps

#### Step 1: Add Redis Dependency

```toml
[dependencies]
redis = { version = "0.25", features = ["aio", "connection-manager"] }

[features]
dedup = ["redis"]
```

#### Step 2: Initialize Dedup Store

```rust
use fraiseql_observers::dedup::RedisDeduplicationStore;

let dedup_store = Arc::new(
    RedisDeduplicationStore::new(
        "redis://localhost:6379",
        300,  // 5-minute window
    )
    .await?
);
```

#### Step 3: Integrate with Executor

```rust
use fraiseql_observers::{ObserverExecutor, deduped_executor::DedupedObserverExecutor};

let executor = DedupedObserverExecutor::new(ObserverExecutor::new(matcher, dlq), dedup_store);
```

Deduplication wraps the executor; it is not a method on it. The example compiled in the
rustdoc of `DedupedObserverExecutor` shows the whole construction.

#### Step 4: Verify Deduplication

```bash
# 1. Send event
INSERT INTO tb_entity_change_log (object_type, object_id, modification_type, object_data)
VALUES ('Order', 'order-1', 'INSERT', '{"status": "new"}');

# Check: Action executed (webhook called, email sent)

# 2. Send duplicate
INSERT INTO tb_entity_change_log (object_type, object_id, modification_type, object_data)
VALUES ('Order', 'order-1', 'INSERT', '{"status": "new"}');

# Check metrics
fraiseql-observers metrics --metric observer_dedup_skips_total
# Should show: 1 (one event skipped)
```

#### Step 5: Monitor Effectiveness

```bash
# Check dedup rate
fraiseql-observers metrics | grep dedup_skips_total

# Calculate dedup rate
dedup_skips / events_processed = dedup_rate
# Expect: 10-40% depending on retry patterns
```

---

## Phase 8.4: Redis Caching

**Purpose**: Achieve 100x performance improvement with caching

### Prerequisites

- Redis 6.0+
- Understanding of cache invalidation
- Stable query/computation patterns

### Integration Steps

#### Step 1: Configure Cache

```toml
[features]
caching = ["redis"]
```

#### Step 2: Initialize Cache Backend

```rust
use fraiseql_observers::cache::RedisCacheBackend;

let cache = Arc::new(
    RedisCacheBackend::new(
        "redis://localhost:6379",
        Duration::from_secs(300),  // 5-minute TTL
    )
    .await?
);
```

#### Step 3: Configure Cache Keys

```rust
// Cache key strategy: action_type:entity_type:entity_id
pub fn cache_key(action: &Action, event: &EntityEvent) -> String {
    format!(
        "action:{}:{}:{}",
        action.action_type,
        event.entity_type,
        event.entity_id
    )
}
```

#### Step 4: Integrate with Actions

```rust
// Before: External API call
let result = external_api.get_user(user_id).await?;

// After: With cache
let cache_key = format!("user:{}", user_id);
if let Some(cached) = cache.get(&cache_key).await? {
    return Ok(cached);
}

let result = external_api.get_user(user_id).await?;
cache.set(&cache_key, result.clone(), Duration::from_secs(300)).await?;
Ok(result)
```

#### Step 5: Benchmark Cache Impact

```bash
# Without cache
time cargo run --example 1000_webhook_calls
# Output: real  0m32.450s

# With cache
time cargo run --example 1000_webhook_calls --features caching
# Output: real  0m0.285s  (114x faster!)
```

#### Step 6: Monitor Cache Effectiveness

```rust
// Metrics to track
observer_cache_hits_total
observer_cache_misses_total
observer_cache_hit_rate  // Should be 70-80%+
```

---

## Phase 8.5: Elasticsearch Integration

**Purpose**: Enable full-text search and compliance audit trail

### Prerequisites

- Elasticsearch 7.0+
- Network access to Elasticsearch
- Understanding of document indexing

### Integration Steps

#### Step 1: Install Elasticsearch

```bash
# Docker
docker run -d \
  -p 9200:9200 \
  -p 9300:9300 \
  -e discovery.type=single-node \
  docker.elastic.co/elasticsearch/elasticsearch:8.0.0
```

#### Step 2: Create Index Template

```bash
curl -X PUT "localhost:9200/_index_template/fraiseql_events" \
  -H "Content-Type: application/json" \
  -d '{
    "index_patterns": ["fraiseql_events-*"],
    "template": {
      "settings": {
        "number_of_shards": 1,
        "number_of_replicas": 0,
        "index.lifecycle.name": "fraiseql_events_policy"
      },
      "mappings": {
        "properties": {
          "event_id": { "type": "keyword" },
          "entity_type": { "type": "keyword" },
          "entity_id": { "type": "keyword" },
          "event_kind": { "type": "keyword" },
          "timestamp": { "type": "date" },
          "observer_id": { "type": "keyword" },
          "action_type": { "type": "keyword" },
          "status": { "type": "keyword" },
          "error": { "type": "text" },
          "data": { "type": "object", "enabled": false }
        }
      }
    }
  }'
```

#### Step 3: Configure Search Backend

```toml
[features]
search = []
```

```rust
use fraiseql_observers::search::HttpSearchBackend;

let search = Arc::new(
    HttpSearchBackend::new(
        "http://localhost:9200",
        Duration::from_secs(30),
    )
);
```

#### Step 4: Index Events

```rust
let event = EntityEvent::new(
    EventKind::Created,
    "Order".to_string(),
    entity_id,
    data,
);

search.index_event(&event).await?;
```

#### Step 5: Query Events

```bash
# Find all Order events in the last 24 hours
curl -X GET "localhost:9200/fraiseql_events-*/_search" \
  -H "Content-Type: application/json" \
  -d '{
    "query": {
      "bool": {
        "must": [
          { "term": { "entity_type": "Order" } },
          { "range": { "timestamp": { "gte": "now-24h" } } }
        ]
      }
    }
  }'

# Find failed webhook actions
curl -X GET "localhost:9200/fraiseql_events-*/_search" \
  -H "Content-Type: application/json" \
  -d '{
    "query": {
      "bool": {
        "must": [
          { "term": { "action_type": "webhook" } },
          { "term": { "status": "failed" } }
        ]
      }
    }
  }'
```

#### Step 6: Set Up Index Lifecycle

```bash
# Create 30-day retention policy
curl -X PUT "localhost:9200/_ilm/policy/fraiseql_events_policy" \
  -H "Content-Type: application/json" \
  -d '{
    "policy": "fraiseql_events_policy",
    "phases": {
      "hot": {
        "min_age": "0d",
        "actions": {
          "rollover": {
            "max_primary_store_size": "50gb"
          }
        }
      },
      "delete": {
        "min_age": "30d",
        "actions": {
          "delete": {}
        }
      }
    }
  }'
```

---

## Phase 8.6: Job Queue System

**Purpose**: Handle async long-running operations

### Prerequisites

- Redis 6.0+
- Understanding of job processing
- Need for async task handling

### Integration Steps

#### Step 1: Configure Job Queue

```toml
[features]
queue = ["redis"]
```

#### Step 2: Initialize Job Queue

```rust
use fraiseql_observers::queue::RedisJobQueue;

let job_queue = Arc::new(
    RedisJobQueue::with_workers(
        "redis://localhost:6379",
        50,  // 50 worker threads
    )
    .await?
);
```

#### Step 3: Enqueue Long-Running Actions

```rust
// Instead of:
// webhook_action.execute(event).await?;  // Blocks 30 seconds

// Do:
let job = Job::new(
    "webhook_action",
    event_id,
    serde_json::json!({
        "url": "https://api.example.com/notify",
        "body": event.data,
    }),
);

job_queue.enqueue(job).await?;
// Returns immediately!
```

#### Step 4: Process Jobs

```rust
// Worker loop (runs in background)
loop {
    if let Some(job) = job_queue.dequeue().await? {
        match execute_job(&job).await {
            Ok(_) => job_queue.mark_complete(&job.id).await?,
            Err(e) => {
                // Retry with backoff
                job_queue.requeue_with_backoff(&job, &backoff).await?;
            }
        }
    }
}
```

#### Step 5: Monitor Job Processing

```bash
# Check job metrics
fraiseql-observers metrics | grep job_queue

# Check queue depth
fraiseql-observers metrics --metric observer_job_queue_depth

# Check worker health
fraiseql-observers status | grep workers
```

---

## Phase 8.7: Prometheus Metrics

**Purpose**: Production monitoring and alerting

### Prerequisites

- Prometheus 2.0+
- Grafana (optional)
- Understanding of metrics

### Integration Steps

#### Step 1: Enable Metrics Feature

```toml
[features]
metrics = ["prometheus"]
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
  - job_name: 'fraiseql-observer'
    static_configs:
      - targets: ['localhost:8000']
```

#### Step 4: Expose Metrics Endpoint

```rust
use actix_web::{web, App, HttpServer, HttpResponse};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let metrics = Arc::new(ObserverMetrics::new());

    HttpServer::new(move || {
        App::new()
            .route("/metrics", web::get().to(|| async {
                HttpResponse::Ok()
                    .content_type("text/plain")
                    .body(metrics.export())
            }))
    })
    .bind("127.0.0.1:8000")?
    .run()
    .await
}
```

#### Step 5: Create Dashboard

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
            "expr": "rate(observer_events_processed_total[5m])"
          }
        ]
      },
      {
        "title": "Action Failure Rate",
        "targets": [
          {
            "expr": "rate(observer_actions_failed_total[5m]) / rate(observer_actions_executed_total[5m])"
          }
        ]
      }
    ]
  }
}
```

#### Step 6: Set Up Alerting

```yaml
# alerts.yml
groups:
  - name: fraiseql_alerts
    rules:
      - alert: HighActionFailureRate
        expr: rate(observer_actions_failed_total[5m]) / rate(observer_actions_executed_total[5m]) > 0.05
        for: 5m
        annotations:
          summary: "Action failure rate > 5%"
```

---

## Phase 8.8: Circuit Breaker

**Purpose**: Prevent cascading failures

> **Not available yet.** `fraiseql_observers::resilience` (`ResilientExecutor`,
> `CircuitBreakerConfig`) is exported but not installed by the executor or the server; there is
> no `ObserverExecutor::with_circuit_breaker`. Tracked in
> [#1451](https://github.com/fraiseql/fraiseql/issues/1451).

## Phase 8.9: Multi-Listener Failover

**Purpose**: High availability with automatic failover

### Prerequisites

- Multiple listener instances available
- Shared checkpoint store (PostgreSQL)
- Understanding of HA concepts

### Integration Steps

#### Step 1: Configure Multi-Listener Coordinator

```rust
use fraiseql_observers::listener::MultiListenerCoordinator;

let coordinator = Arc::new(
    MultiListenerCoordinator::new()
);

let config = MultiListenerConfig {
    num_listeners: 3,
    health_check_interval: Duration::from_secs(5),
    failover_threshold: Duration::from_secs(60),
};
```

#### Step 2: Register Multiple Listeners

```rust
// Each listener registers itself
coordinator.register_listener("listener-1".to_string()).await?;
coordinator.register_listener("listener-2".to_string()).await?;
coordinator.register_listener("listener-3".to_string()).await?;

// All use same checkpoint store
let checkpoint_store = Arc::new(
    PostgresCheckpointStore::new(
        "postgresql://localhost/fraiseql",
        "observer_checkpoints"
    )
    .await?
);
```

#### Step 3: Initialize Failover Manager

```rust
use fraiseql_observers::listener::FailoverManager;

let failover_manager = FailoverManager::new(coordinator.clone());

// Start health monitoring
let mut failover_rx = failover_manager.start_health_monitor().await;

tokio::spawn(async move {
    while let Some(failover_event) = failover_rx.recv().await {
        println!("Failover occurred: {:?}", failover_event);
        // Handle failover (update leader, notify clients, etc.)
    }
});
```

#### Step 4: Test Failover

```bash
# 1. Start all 3 listeners
cargo run --example multi_listener

# 2. Verify leader elected
fraiseql-observers status | grep Leader

# 3. Kill primary listener
kill <primary_pid>

# 4. Verify automatic failover (within 60 seconds)
sleep 65
fraiseql-observers status | grep Leader
# Should show: Different listener now leader

# 5. Resume listener
cargo run --example listener-2

# 6. Verify re-registration
fraiseql-observers status
# Should show: All 3 listeners healthy
```

---

## Phase 8.10: CLI Tools

**Purpose**: Developer experience and debugging

### Prerequisites

- Rust toolchain
- Observer system running
- Understanding of CLI usage

### Integration Steps

#### Step 1: Build CLI

```bash
cd crates/fraiseql-observers
cargo build --release --bin fraiseql-observers
```

#### Step 2: Install CLI

```bash
cargo install --path crates/fraiseql-observers --bin fraiseql-observers

# Verify installation
fraiseql-observers --version
```

#### Step 3: Common Commands

```bash
# Check status
fraiseql-observers status
fraiseql-observers status --listener listener-1 --detailed

# Debug event
fraiseql-observers debug-event --event-id evt-123
fraiseql-observers debug-event --entity-type Order --kind created --history 10

# Manage DLQ
fraiseql-observers dlq list --limit 20
fraiseql-observers dlq show dlq-001
fraiseql-observers dlq retry dlq-001
fraiseql-observers dlq retry-all --observer obs-webhook --dry-run

# Validate config
fraiseql-observers validate-config observers.yaml --detailed

# View metrics
fraiseql-observers metrics
fraiseql-observers metrics --metric observer_events_processed_total
```

#### Step 4: Integrate into Scripts

```bash
#!/bin/bash
# deployment/health_check.sh

# Check observer health
STATUS=$(fraiseql-observers status --format json)
HEALTHY=$(echo $STATUS | jq '.healthy_listeners')

if [ "$HEALTHY" -lt 3 ]; then
    echo "ALERT: Only $HEALTHY listeners healthy (expected 3)"
    exit 1
fi

echo "Observer health check passed"
exit 0
```

---

## Integration Checklist

After integrating each feature, verify:

- [ ] Dependencies added to Cargo.toml
- [ ] Feature flag created
- [ ] Configuration initialized
- [ ] Integration tests passing
- [ ] Metrics tracking enabled
- [ ] Monitoring/alerts configured
- [ ] Documentation updated
- [ ] Performance verified
- [ ] Error handling tested
- [ ] Failover scenarios tested (for HA features)

---

## Common Integration Patterns

### Pattern 1: Minimal setup (checkpoint only)

A plain `ObserverExecutor::new(matcher, dlq)`, driven by the checkpointed loop in
[Checkpoints](#step-3-enable-in-configuration).

### Pattern 2: Production setup

Let the factory compose the executor from `[observers.runtime]`. It wraps the base executor in
deduplication and/or result caching according to `performance.enable_dedup` and
`performance.enable_caching`, and contacts Redis only when one of them is on:

```rust
use fraiseql_observers::factory::ExecutorFactory;

let executor = ExecutorFactory::build(&runtime_config, dlq).await?;
// Drive it with the checkpointed loop above: `executor.process_event(&event)`.
```

Search indexing, the job queue (`QueuedObserverExecutor`) and metrics are separate components,
not builder methods on the executor. A matched observer's actions run one after another;
there is no concurrent mode ([#1451](https://github.com/fraiseql/fraiseql/issues/1451)).

### Pattern 3: Migration (add features gradually)

Each step is a configuration change, not a code change, once the factory builds the executor:

1. Checkpoints: drive the executor with the checkpointed loop. Verify that a restart neither
   loses nor re-delivers events beyond one batch.
2. Caching: set `performance.enable_caching = true` with a `[observers.runtime.redis]` section.
3. Deduplication: set `performance.enable_dedup = true`. Verify that a redelivered event inside
   the window is skipped.

---

## Next Steps

1. Choose integration path (minimal, production, or gradual)
2. Follow step-by-step instructions for each feature
3. Verify integration with provided tests
4. Configure monitoring and alerts
5. Deploy to staging environment
6. Run failover/stress tests
7. Deploy to production
8. Monitor metrics continuously

---

## Support

For integration help:

- Check Architecture Guide: `../../../../docs/architecture/overview.md`
- Review Configuration Examples: `configuration-examples.md`
- Troubleshoot Issues: `troubleshooting.md`
- Check CLI Documentation: `cli-tools.md`
