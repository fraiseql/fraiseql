# Phase 8 Feature Integration Guide

This guide provides step-by-step integration instructions for each Phase 8 feature.

## Table of Contents

1. [Phase 8.1: Persistent Checkpoints](#phase-81-persistent-checkpoints)
2. [Phase 8.2: Concurrent Action Execution](#phase-82-concurrent-action-execution)
3. [Phase 8.3: Event Deduplication](#phase-83-event-deduplication)
4. [Phase 8.4: Redis Caching](#phase-84-redis-caching)
5. [Phase 8.5: Elasticsearch Integration](#phase-85-elasticsearch-integration)
6. [Phase 8.6: Job Queue System](#phase-86-job-queue-system)
7. [Phase 8.7: Prometheus Metrics](#phase-87-prometheus-metrics)
8. [Phase 8.8: Circuit Breaker](#phase-88-circuit-breaker)
9. [Phase 8.9: Multi-Listener Failover](#phase-89-multi-listener-failover)
10. [Phase 8.10: CLI Tools](#phase-810-cli-tools)

---

## Phase 8.1: Persistent Checkpoints

**Purpose**: Guarantee zero-event-loss recovery on restart

### Prerequisites
- PostgreSQL 12+
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
use fraiseql_observers::checkpoint::PostgresCheckpointStore;

let checkpoint_store = Arc::new(
    PostgresCheckpointStore::new(
        "postgresql://user:pass@localhost/db",
        "observer_checkpoints"
    )
    .await?
);

let executor = ObserverExecutor::with_checkpoint_store(
    matcher,
    checkpoint_store,
    dlq,
);
```

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

## Phase 8.2: Concurrent Action Execution

**Purpose**: Execute multiple actions in parallel for 5x latency improvement

### Prerequisites
- Phase 1-7 already working
- Understanding of action execution

### Integration Steps

#### Step 1: Update Cargo.toml

```toml
[dependencies]
futures = "0.3"

[features]
concurrent = []
```

#### Step 2: Wrap Executor

**Before**:
```rust
let executor = ObserverExecutor::new(matcher, dlq);

// Executes: A (100ms) → B (100ms) → C (100ms) = 300ms
executor.execute_actions(actions).await?;
```

**After**:
```rust
use fraiseql_observers::concurrent::ConcurrentActionExecutor;

let base_executor = ObserverExecutor::new(matcher, dlq);
let executor = ConcurrentActionExecutor::new(
    base_executor,
    Duration::from_secs(30),  // Per-action timeout
);

// Executes: A, B, C in parallel = 100ms
executor.execute_actions(actions).await?;
```

#### Step 3: Configuration

```rust
let executor = ConcurrentActionExecutor::with_config(
    base_executor,
    ConcurrentConfig {
        timeout: Duration::from_secs(30),
        max_parallelism: 100,  // Max concurrent actions
        buffer_size: 10000,
    }
);
```

#### Step 4: Benchmark

```bash
# Before optimization
time cargo run --example process_100_events
# Output: real  0m5.234s (300ms per event × 100)

# After optimization
time cargo run --example process_100_events --features concurrent
# Output: real  0m1.845s (100ms per event × 100) = 2.8x improvement
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
let executor = ObserverExecutor::with_dedup(
    matcher,
    dlq,
    checkpoint_store,
    dedup_store,
);
```

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

```rust
use fraiseql_observers::metrics::ObserverMetrics;

let metrics = Arc::new(ObserverMetrics::new());

let executor = ObserverExecutor::with_metrics(
    matcher,
    dlq,
    metrics.clone(),
);
```

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

### Prerequisites
- Understanding of circuit breaker pattern
- External service reliability concerns

### Integration Steps

#### Step 1: Configure Circuit Breaker

```rust
use fraiseql_observers::resilience::CircuitBreakerConfig;

let cb_config = CircuitBreakerConfig {
    failure_threshold: 0.5,      // Open at 50% failure rate
    success_threshold: 0.8,      // Close at 80% success rate
    timeout: Duration::from_secs(60),
    sample_size: 100,
};

let executor = ObserverExecutor::with_circuit_breaker(
    matcher,
    dlq,
    cb_config,
);
```

#### Step 2: Test Circuit Breaker

```rust
#[test]
fn test_circuit_breaker_opens_on_failures() {
    // Simulate 50 consecutive failures
    for _ in 0..50 {
        let result = executor.execute_action(action_that_fails).await;
        assert!(result.is_err());
    }

    // Next request should fail immediately (fast-fail)
    let start = Instant::now();
    let result = executor.execute_action(action_that_fails).await;
    let elapsed = start.elapsed();

    // Should fail fast (<1ms) without calling external service
    assert!(elapsed.as_millis() < 10);
}
```

#### Step 3: Monitor Circuit State

```bash
# Check circuit state
fraiseql-observers status | grep -i circuit

# Expected output:
# Circuit Breaker: CLOSED (normal)
# Circuit Breaker: OPEN (failing fast)
# Circuit Breaker: HALF_OPEN (testing recovery)
```

---

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

### Pattern 1: Minimal Setup (Checkpoint Only)

```rust
let checkpoint_store = PostgresCheckpointStore::new(...).await?;
let executor = ObserverExecutor::with_checkpoint_store(
    matcher,
    checkpoint_store,
    dlq,
);
```

### Pattern 2: Production Setup (All Features)

```rust
let checkpoint = PostgresCheckpointStore::new(...).await?;
let dedup = RedisDeduplicationStore::new(...).await?;
let cache = RedisCacheBackend::new(...).await?;
let search = HttpSearchBackend::new(...);
let queue = RedisJobQueue::new(...).await?;
let metrics = ObserverMetrics::new();

let executor = ObserverExecutor::new(matcher, dlq)
    .with_checkpoint(checkpoint)
    .with_dedup(dedup)
    .with_cache(cache)
    .with_search(search)
    .with_queue(queue)
    .with_metrics(metrics)
    .with_circuit_breaker(cb_config)
    .with_concurrent_execution(concurrent_config);
```

### Pattern 3: Migration (Add Features Gradually)

```rust
// Week 1: Add checkpoints
let executor = ObserverExecutor::with_checkpoint_store(
    matcher, checkpoint_store, dlq
);
// Test, verify zero event loss

// Week 2: Add caching
let executor = executor.with_cache(cache);
// Benchmark, verify 100x speedup for cache hits

// Week 3: Add deduplication
let executor = executor.with_dedup(dedup);
// Monitor, verify duplicate prevention

// Week 4: Add remaining features
let executor = executor
    .with_search(search)
    .with_metrics(metrics);
```

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
- Check Architecture Guide: `ARCHITECTURE_PHASE_8.md`
- Review Configuration Examples: `CONFIGURATION_EXAMPLES.md`
- Troubleshoot Issues: `TROUBLESHOOTING.md`
- Check CLI Documentation: `CLI_TOOLS.md`

