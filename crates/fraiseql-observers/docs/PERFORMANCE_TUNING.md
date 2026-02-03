# Performance Tuning Guide - Phase 8

## Overview

Phase 8 features provide multiple performance optimization paths. This guide helps you identify bottlenecks and apply the right optimizations.

## Performance Diagnosis

### Step 1: Establish Baseline

```bash
# Measure current performance
fraiseql-observers metrics > baseline-metrics.txt

# Key metrics to capture

- observer_events_processed_total
- observer_action_duration_seconds (P50, P95, P99)
- observer_cache_hit_rate
- observer_actions_failed_total
```

### Step 2: Identify Bottleneck

```promql
# Is processing throughput too low?
rate(observer_events_processed_total[5m]) < target_rate

# Is latency too high?
histogram_quantile(0.99, observer_action_duration_seconds) > acceptable_latency

# Is failure rate too high?
rate(observer_actions_failed_total[5m]) / rate(observer_actions_executed_total[5m]) > threshold

# Is cache effectiveness low?
(observer_cache_hits_total / (observer_cache_hits_total + observer_cache_misses_total)) < 0.6
```

### Step 3: Profile

```bash
# CPU profiling
RUST_LOG=debug cargo flamegraph --features phase8 -- --example process_100k_events

# Memory profiling
HEAPPROFILE=/tmp/heap cargo run --release --features phase8

# Database profiling
# In PostgreSQL:
SELECT query, calls, mean_time, max_time
FROM pg_stat_statements
ORDER BY mean_time DESC LIMIT 10;
```

---

## Optimization Strategies

### 1. Cache Optimization (100x+ potential improvement)

**When**: Repeated queries/computations, read-heavy workloads

**Implementation**:

```rust
// Current (no cache)
for event in events {
    let user = api.get_user(&event.user_id).await?;  // 200ms × 1000 = 200s
}

// With 80% hit rate
for event in events {
    if let Some(user) = cache.get(&key).await? {     // <1ms × 800 = 0.8s
        use_user(user);
    } else {
        let user = api.get_user(&event.user_id).await?;  // 200ms × 200 = 40s
        cache.set(&key, user).await?;
    }
}
// Total: ~40.8s (4.8x improvement)
```

**Tuning Parameters**:

```rust
// Parameter 1: Cache TTL
cache_ttl: Duration::from_secs(300),  // 5 minutes
// - Increase for: Stable data, rare changes
// - Decrease for: Frequently updated data

// Parameter 2: Max cache size
cache_size: 100_000,  // entries
// - Increase for: Many unique queries
// - Decrease for: Memory constraints

// Parameter 3: Eviction policy
eviction: EvictionPolicy::LRU,  // LRU, LFU, Random
// - LRU: Works well for real-world patterns
// - LFU: Optimizes for frequency
```

**Verification**:

```bash
# Measure cache impact
fraiseql-observers metrics | grep cache_hit_rate
# Target: >70% for good performance
# Excellent: >85%
```

---

### 2. Concurrent Execution (3-5x improvement)

**When**: Multiple independent actions per event

**Before**:
```rust
// Sequential: 100ms + 100ms + 100ms = 300ms
executor.execute_action(&action1).await?;
executor.execute_action(&action2).await?;
executor.execute_action(&action3).await?;
```

**After**:
```rust
// Parallel: max(100ms, 100ms, 100ms) = 100ms
use futures::future::join_all;

let futures = vec![
    executor.execute_action(&action1),
    executor.execute_action(&action2),
    executor.execute_action(&action3),
];

join_all(futures).await;
```

**Configuration**:

```rust
// Max parallelism
max_parallelism: 100,  // Don't exceed 100-200
// - Higher = more throughput but resource usage
// - Lower = less resource usage but throughput limited

// Per-action timeout
timeout: Duration::from_secs(30),
// - Prevents hung requests
// - Trade-off: May kill slow-but-working requests
```

**Measurement**:

```bash
# Compare latency before/after
time cargo run --example 100_events
time cargo run --release --example 100_events  # 100ms per event
time CONCURRENT=1 cargo run --release --example 100_events  # 30ms per event (3.3x improvement)
```

---

### 3. Batch Checkpoint Writes (2-3x improvement)

**When**: Throughput-focused scenarios, acceptable event loss window

**Current**:
```rust
// Save every event (slowest but safest)
checkpoint_batch_size: 1,

// Performance: ~5,000 events/second
// On crash: Lose 0 events
```

**Optimized**:
```rust
// Save every N events (balance)
checkpoint_batch_size: 100,

// Performance: ~50,000 events/second  (10x improvement!)
// On crash: Lose up to 100 events
```

**Risk/Reward Table**:

| Batch Size | Throughput | Data Loss Risk | Use Case |
|-----------|-----------|----------------|----------|
| 1 | 5k/s | None | Financial, healthcare |
| 10 | 15k/s | Up to 10 events | Most production |
| 100 | 50k/s | Up to 100 events | High-throughput, less critical |
| 1000 | 80k/s | Up to 1000 events | Analytics, non-critical |

**Decision Guide**:

- Batch size = Expected events during system restart time
- If restart takes 30 seconds at 1000 events/sec → Batch size 1000 is acceptable

---

### 4. Database Connection Pooling

**Problem**: Connection creation overhead, connection leaks

**Optimization**:

```rust
PostgresCheckpointStore::with_pool_config(
    "postgresql://localhost/db",
    PoolConfig {
        min_connections: 5,      // Minimum idle connections
        max_connections: 50,     // Maximum total connections
        connection_timeout: Duration::from_secs(30),
        idle_timeout: Duration::from_secs(300),
    }
)
.await?
```

**Tuning**:

```
min_connections = Number of concurrent listeners
max_connections = min_connections × 3-5
// Account for: checkpoint saves, queries, failover

Example: 3 listeners
min_connections: 3
max_connections: 10-15
```

**Verification**:

```bash
# Monitor connection usage
SELECT count(*) FROM pg_stat_activity WHERE datname = 'fraiseql_observers';

# Should be around min_connections at idle
# Should not exceed max_connections
```

---

### 5. Index Optimization

**Problem**: Slow checkpoint queries, slow dedup checks

**PostgreSQL Indexes**:

```sql
-- For checkpoint retrieval (by listener_id)
CREATE INDEX CONCURRENTLY idx_checkpoint_listener
ON observer_checkpoints(listener_id);

-- For cleanup (by timestamp)
CREATE INDEX CONCURRENTLY idx_checkpoint_created
ON observer_checkpoints(created_at);

-- For failover (recent updates)
CREATE INDEX CONCURRENTLY idx_checkpoint_updated
ON observer_checkpoints(updated_at DESC);

-- Verify indexes
SELECT * FROM pg_stat_user_indexes
WHERE relname LIKE '%checkpoint%';
```

**Redis Dedup Optimization**:

```bash
# Monitor Redis memory
redis-cli INFO memory

# If memory usage high:
# 1. Reduce dedup window
dedup_window: Duration::from_secs(300),  # Was 600

# 2. Use memory eviction policy
redis-cli CONFIG SET maxmemory-policy allkeys-lru
```

---

### 6. Elasticsearch Tuning

**Problem**: Slow indexing, large disk usage

**Index Configuration**:

```bash
# Reduce replica shards in dev
curl -X PUT "localhost:9200/fraiseql_events-*/_settings" \
  -H "Content-Type: application/json" \
  -d '{
    "index": {
      "number_of_replicas": 0  # Was 1
    }
  }'

# Increase refresh interval (fewer index writes)
curl -X PUT "localhost:9200/fraiseql_events-*/_settings" \
  -H "Content-Type: application/json" \
  -d '{
    "index": {
      "refresh_interval": "30s"  # Was 1s
    }
  }'
```

**Retention Policy**:

```bash
# Delete old indices (save disk)
curl -X DELETE "localhost:9200/fraiseql_events-2026.01.01"

# Or use ILM (recommended)
curl -X PUT "localhost:9200/_ilm/policy/fraiseql_events_policy" \
  -H "Content-Type: application/json" \
  -d '{
    "policy": "fraiseql_events_policy",
    "phases": {
      "hot": { "min_age": "0d" },
      "warm": { "min_age": "3d" },
      "delete": { "min_age": "30d", "actions": { "delete": {} } }
    }
  }'
```

---

### 7. Worker Pool Sizing

**Problem**: Too few workers (bottleneck), too many (resource exhaustion)

**Calculation**:

```
Optimal workers = (CPU cores × 2) to (CPU cores × 4)
// Accounts for I/O wait during network calls

Example: 8-core machine
- Conservative: 16 workers
- Moderate: 32 workers
- Aggressive: 64 workers
```

**Configuration**:

```rust
let queue = RedisJobQueue::with_workers(
    "redis://localhost",
    16,  // Workers = CPUs × 2
)
.await?;
```

**Verification**:

```bash
# Monitor CPU and throughput
watch -n 1 'fraiseql-observers metrics | grep queue'

# If CPU ~100% and throughput low:
#   → Increase workers
# If CPU low and throughput high:
#   → Can add more workers
# If many context switches:
#   → Too many workers, reduce
```

---

## Complete Optimization Pipeline

### Phase 1: Low Hanging Fruit (Week 1)

```rust
// Step 1: Add concurrent execution
let executor = ConcurrentActionExecutor::new(
    base_executor,
    Duration::from_secs(30),
);

// Expected: 3-5x latency improvement
// Effort: Minimal
// Risk: Low
```

### Phase 2: Caching (Week 2)

```rust
// Step 2: Add caching
let cache = RedisCacheBackend::new(
    "redis://localhost",
    Duration::from_secs(300),
);

// Expected: 10-100x for cache hits
// Effort: Moderate (need to identify cacheable operations)
// Risk: Low (configurable TTL)
```

### Phase 3: Batch Writes (Week 3)

```rust
// Step 3: Optimize checkpoint batch size
checkpoint_batch_size: 100,  // Was 1

// Expected: 10x throughput improvement
// Effort: Minimal
// Risk: Acceptable event loss window during crash
```

### Phase 4: Resource Tuning (Week 4)

```rust
// Step 4: Tune resources
PostgresCheckpointStore::with_pool_config(
    "postgresql://localhost/db",
    PoolConfig {
        min_connections: 5,
        max_connections: 50,
    }
)
.await?;

let queue = RedisJobQueue::with_workers(
    "redis://localhost",
    32,  // CPUs × 2
).await?;

// Expected: Stable performance, no bottlenecks
// Effort: Low
// Risk: Resource exhaustion if over-tuned
```

---

## Performance Testing

### Load Test Script

```bash
#!/bin/bash
# performance_test.sh

# Generate N events
N=${1:-10000}
RATE=${2:-1000}  # events/second

echo "Running performance test: $N events at $RATE/sec"

# Start observer
cargo run --release --features phase8 &
OBSERVER_PID=$!

sleep 5

# Insert events
for i in $(seq 1 $N); do
    psql $DATABASE_URL << EOF
INSERT INTO tb_entity_change_log (object_type, object_id, modification_type, object_data)
VALUES ('Order', 'order-$i', 'INSERT', '{"status": "new"}');
EOF

    # Rate limit
    sleep $(echo "scale=3; 1 / $RATE" | bc)
done

# Measure
echo "Events processed:"
fraiseql-observers metrics | grep events_processed_total

# Cleanup
kill $OBSERVER_PID
```

### Benchmark Scenarios

```bash
# Baseline (no Phase 8)
cargo run --release --example 100k_events
# Expected: ~30 seconds (300ms per event × 100k)

# With concurrent execution
CONCURRENT=1 cargo run --release --example 100k_events
# Expected: ~10 seconds (100ms per event × 100k) = 3x

# With caching + concurrent
CONCURRENT=1 CACHE=1 cargo run --release --example 100k_events
# Expected: ~2 seconds (<20ms per event × 100k) = 15x

# With all optimizations
cargo run --release --features phase8 --example 100k_events
# Expected: ~1 second (<10ms per event × 100k) = 30x
```

---

## Monitoring After Optimization

### Key Metrics to Track

```bash
# Throughput
rate(observer_events_processed_total[5m])

# Latency (P99)
histogram_quantile(0.99, observer_action_duration_seconds)

# Cache effectiveness
observer_cache_hit_rate

# Error rate
rate(observer_actions_failed_total[5m]) / rate(observer_actions_executed_total[5m])

# Resource usage
process_resident_memory_bytes
process_cpu_seconds_total
```

### Alert Thresholds

```yaml
- alert: PerformanceDegradation
  expr: histogram_quantile(0.99, observer_action_duration_seconds) > 2
  for: 5m

- alert: CacheHitRateLow
  expr: observer_cache_hit_rate < 0.7
  for: 10m

- alert: HighActionFailureRate
  expr: rate(observer_actions_failed_total[5m]) / rate(observer_actions_executed_total[5m]) > 0.05
  for: 5m
```

---

## Common Pitfalls

### Pitfall 1: Over-Aggressive Batching

**Problem**: Batch size 10000 on system that crashes every minute

**Solution**: Batch size should be ≤ expected events during MTT R (Mean Time To Recovery)

```
Batch size = Events/sec × MTTR
Example: 1000 events/sec × 30 sec MTTR = 30,000 acceptable batch size
```

### Pitfall 2: Cache TTL Too Long

**Problem**: Stale data causing business logic errors

**Solution**: Set TTL based on data freshness requirements

```
Financial data: TTL 60 seconds
User profiles: TTL 300 seconds
Product catalogs: TTL 3600 seconds
```

### Pitfall 3: Connection Pool Exhaustion

**Problem**: Occasional failures with "connection pool exhausted"

**Solution**: Increase pool size, add connection monitoring

```rust
max_connections: std::cmp::max(50, num_listeners * 10),
```

### Pitfall 4: Too Many Workers

**Problem**: High CPU, many context switches, thread thrashing

**Solution**: Limit to CPU cores × 2-4

```bash
num_workers: (num_cpus::get() * 2).min(64)
```

---

## Optimization Checklist

- [ ] Baseline metrics captured
- [ ] Bottleneck identified
- [ ] Concurrent execution enabled
- [ ] Caching configured
- [ ] Checkpoint batch size optimized
- [ ] Connection pooling tuned
- [ ] Indices created
- [ ] Worker pool sized appropriately
- [ ] Monitoring configured
- [ ] Load testing passed
- [ ] Performance verified
- [ ] Regression tests pass

