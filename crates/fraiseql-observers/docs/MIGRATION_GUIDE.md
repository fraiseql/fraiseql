# Migration Guide: Phase 1-7 → Phase 8

This guide helps you safely migrate from Phase 1-7 (baseline observer system) to Phase 8 (production-grade system).

## Migration Strategies

### Strategy 1: Gradual Rollout (Recommended)

Enable features one at a time, verify each works before moving to the next.

**Timeline**: 4-6 weeks

**Risk**: Low (easy to rollback each step)

### Strategy 2: Big Bang

Enable all Phase 8 features at once in production.

**Timeline**: 1-2 weeks

**Risk**: Medium (harder to isolate issues)

### Strategy 3: Canary

Run Phase 8 alongside Phase 1-7 for subset of events.

**Timeline**: 2-3 weeks

**Risk**: Medium (requires dual-write logic)

---

## Gradual Rollout Plan (Recommended)

### Week 1: Phase 8.1 - Persistent Checkpoints

**Goal**: Enable zero-event-loss guarantee

#### Step 1: Create Database Schema

```bash
psql $DATABASE_URL << EOF
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
EOF
```

#### Step 2: Deploy Phase 8.1

```toml
[features]
checkpoint = []
```

```rust
let checkpoint_store = Arc::new(
    PostgresCheckpointStore::new(
        $DATABASE_URL,
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

#### Step 3: Testing

```bash
# 1. Verify events processed normally
cargo test --features checkpoint

# 2. Verify checkpoint saved
psql $DATABASE_URL -c "SELECT * FROM observer_checkpoints LIMIT 5;"

# 3. Test recovery: stop observer, restart, verify no re-processing
pkill fraiseql-observer
sleep 2
cargo run --features checkpoint

# 4. Verify metrics
fraiseql-observers metrics | grep checkpoint
```

#### Step 4: Monitoring

```bash
# Track saved checkpoints
SELECT COUNT(*) FROM observer_checkpoints;

# Verify no gaps
SELECT event_id FROM observer_checkpoints ORDER BY event_id DESC LIMIT 10;
```

#### Rollback Plan

```bash
# If issues detected:
# 1. Disable checkpoint store in code
let executor = ObserverExecutor::new(matcher, dlq);  # No checkpoint

# 2. Drop table if needed
psql $DATABASE_URL -c "DROP TABLE observer_checkpoints;"

# 3. Redeploy previous version
git checkout HEAD~1
cargo build --release
```

---

### Week 2: Phase 8.2 - Concurrent Execution

**Goal**: 5x latency improvement

#### Step 1: Add Dependency

```toml
[dependencies]
futures = "0.3"
```

#### Step 2: Wrap Executor

```rust
use fraiseql_observers::concurrent::ConcurrentActionExecutor;

let base_executor = ObserverExecutor::with_checkpoint_store(
    matcher,
    checkpoint_store,
    dlq,
);

let executor = ConcurrentActionExecutor::new(
    base_executor,
    Duration::from_secs(30),  // Per-action timeout
);
```

#### Step 3: Testing

```bash
# Benchmark before/after
time cargo run --release --example 1000_events --features checkpoint
# Before: ~5 seconds

cargo run --release --example 1000_events --features checkpoint,concurrent
# After: ~2 seconds (2.5x improvement)
```

#### Step 4: Monitor

```bash
fraiseql-observers metrics | grep action_duration_seconds
# Expect: P99 latency reduced to 30-50% of previous
```

#### Verify No Regressions

```bash
# Run full test suite
cargo test --features checkpoint,concurrent

# Check all actions still work
fraiseql-observers debug-event --history 20
# Verify each event has matching actions
```

---

### Week 3: Phase 8.3 - Deduplication

**Goal**: Prevent duplicate side effects

#### Step 1: Deploy Redis

```bash
# Docker
docker run -d -p 6379:6379 redis:7.0

# Or: Use managed Redis (AWS ElastiCache, etc.)
```

#### Step 2: Configure Dedup

```toml
[features]
checkpoint = []
concurrent = []
dedup = ["redis"]
```

```rust
let dedup_store = Arc::new(
    RedisDeduplicationStore::new(
        "redis://localhost:6379",
        300,  // 5-minute window
    )
    .await?
);

let executor = executor.with_dedup(dedup_store);
```

#### Step 3: Testing

```bash
# 1. Send event twice (simulate retry)
INSERT INTO tb_entity_change_log (...) VALUES (...);
INSERT INTO tb_entity_change_log (...) VALUES (...);  # Duplicate

# 2. Verify first processed, second skipped
fraiseql-observers metrics | grep dedup_skips_total
# Should show: 1 skip

# 3. Verify action not executed twice
# Check webhook logs, email logs, etc.
```

#### Step 4: Monitor

```bash
fraiseql-observers metrics | grep dedup_rate
# Expect: 5-20% for normal workloads (retries, duplicates)
```

#### Verify No Regressions

```bash
cargo test --features checkpoint,concurrent,dedup

# Verify unique events still processed
# (Ensure not filtering legitimate events)
```

---

### Week 4: Phase 8.4 - Caching

**Goal**: 10-100x performance for cache hits

#### Step 1: Configure Cache

```toml
[features]
checkpoint = []
concurrent = []
dedup = ["redis"]
caching = ["redis"]  # Add this
```

```rust
let cache = Arc::new(
    RedisCacheBackend::new(
        "redis://localhost:6379",
        Duration::from_secs(300),  // 5-minute TTL
    )
    .await?
);

let executor = executor.with_cache(cache);
```

#### Step 2: Identify Cacheable Operations

```rust
// Example: User lookups can be cached
// (Assuming user data doesn't change during 5-minute window)

let cache_key = format!("user:{}", user_id);
if let Some(cached_user) = cache.get(&cache_key).await? {
    return Ok(cached_user);  // Cache hit: <1ms
}

let user = fetch_user(user_id).await?;  // Cache miss: 200ms
cache.set(&cache_key, user.clone(), Duration::from_secs(300)).await?;
Ok(user)
```

#### Step 3: Testing

```bash
# Benchmark cache impact
time cargo run --release --example 1000_webhook_calls --features all
# Without cache: ~32 seconds
# With cache: ~0.3 seconds (100x improvement!)

# Verify correctness
cargo test --features checkpoint,concurrent,dedup,caching

# Check cache hit rate
fraiseql-observers metrics | grep cache_hit_rate
# Expect: 70-80%+ for typical workloads
```

#### Step 4: Monitor

```bash
fraiseql-observers metrics | grep cache
# Track: hit_rate, memory_usage, eviction_rate

# Alert if hit_rate drops below 60%
# (May indicate too short TTL or too small cache)
```

#### Rollback Plan

```bash
# If cache causes data inconsistency:
# 1. Reduce TTL
cache_ttl: Duration::from_secs(60),  # Was 300

# 2. Or disable cache
// cache_backend: Arc::new(NullCacheBackend::new()),
```

---

### Beyond Week 4: Advanced Features (Optional)

#### Phase 8.5: Elasticsearch Integration

```toml
[features]
search = []
```

Provides: Full-text search, compliance audit trail

**When to enable**: Month 2, for compliance/auditing

#### Phase 8.6: Job Queue

```toml
[features]
queue = ["redis"]
```

Provides: Async long-running operations

**When to enable**: Month 2, for long operations

#### Phase 8.7: Prometheus Metrics

```toml
[features]
metrics = ["prometheus"]
```

Provides: Production monitoring

**When to enable**: Immediately (no performance cost)

#### Phase 8.8: Circuit Breaker

```toml
[features]
circuit_breaker = []
```

Provides: Resilience against cascading failures

**When to enable**: After load testing

#### Phase 8.9: Multi-Listener Failover

Provides: High availability

**When to enable**: Month 2, for production HA

---

## Testing During Migration

### Unit Tests

```bash
# Run with each feature
cargo test --lib --features checkpoint
cargo test --lib --features checkpoint,concurrent
cargo test --lib --features checkpoint,concurrent,dedup
cargo test --lib --features checkpoint,concurrent,dedup,caching
```

### Integration Tests

```bash
# Full integration with external services
cargo test --test integration --features all -- --test-threads=1

# Key scenarios to test:
# 1. Event → checkpoint → recovery
# 2. Concurrent actions complete correctly
# 3. Duplicate events skipped
# 4. Cache hits used, misses handled
```

### Load Tests

```bash
# Simulate production load
cargo bench --features all

# Run for 1 hour with 1000 events/second
cargo run --release --features all -- \
  --events-per-second 1000 \
  --duration 3600
```

### Failover Tests

```bash
# Test recovery scenarios
./tests/failover_test.sh

# Test migration with live traffic
./tests/canary_deployment.sh
```

---

## Data Migration

### Checkpoint Migration

If starting from Phase 1-7 (no checkpoints):

```bash
# Option 1: Start fresh
# Checkpoint starts from latest event (no replay)
# Risk: May lose in-flight events

# Option 2: Replay from beginning
# Set checkpoint to event 0
# Risk: Re-execute all historical events

psql $DATABASE_URL << EOF
INSERT INTO observer_checkpoints (listener_id, event_id, last_processed_at)
VALUES ('listener-1', 0, NOW());
EOF

# Listener will re-process all events (slow, but safe)
```

### Redis Data

No data migration needed for Redis:

- Checkpoint is just metadata
- Cache is ephemeral (can be cleared)
- Dedup can start fresh

---

## Deployment Strategies

### Strategy 1: Blue-Green Deployment

```bash
# Phase 1-7: Blue environment (running)
# Phase 8: Green environment (staging)

# 1. Deploy Phase 8 to green
docker build -t observer:phase8 .
docker-compose -f docker-compose.green.yml up

# 2. Verify green is healthy
curl http://green-observer:8000/health
fraiseql-observers status --host green-observer

# 3. Switch traffic from blue to green
docker-compose down
docker-compose -f docker-compose.green.yml up -d

# 4. Monitor
fraiseql-observers metrics

# 5. Rollback if needed
docker-compose down
docker-compose -f docker-compose.blue.yml up -d
```

### Strategy 2: Canary Deployment

```bash
# Run small percentage with Phase 8
# Monitor for errors before full rollout

# 1. Deploy Phase 8 alongside Phase 1-7
docker-compose scale observer-v1=2 observer-v8=1

# 2. Route 10% of events to Phase 8
load_balancer.weight("observer-v8", 0.1)

# 3. Monitor error rates
prometheus.query("rate(observer_actions_failed[5m])")

# 4. Gradually increase
load_balancer.weight("observer-v8", 0.25)
# ... monitor ...
load_balancer.weight("observer-v8", 0.5)
# ... monitor ...
load_balancer.weight("observer-v8", 1.0)  # 100%

# 5. Remove v1
docker-compose scale observer-v1=0
```

### Strategy 3: Feature Flags

```rust
if env::var("USE_PHASE_8").unwrap_or("false") == "true" {
    // Use Phase 8 features
    let executor = ConcurrentActionExecutor::new(...);
} else {
    // Use Phase 1-7 (fallback)
    let executor = ObserverExecutor::new(...);
}
```

Deploy Phase 8 code, but gate features behind flags:

```bash
# Day 1: Deploy code, flags OFF
USE_PHASE_8=false cargo run

# Day 2: Enable for internal testing
USE_PHASE_8=true cargo run

# Day 3: Enable for 10% of traffic (feature flag in load balancer)

# Day 5: Enable for all traffic
```

---

## Rollback Procedures

### Rollback Level 1: Feature Disable

Fastest, no data loss:

```bash
# Disable most recent feature
# E.g., disable caching:
cache_backend: Arc::new(NullCacheBackend::new()),

// Redeploy
cargo build --release
docker build -t observer:rollback .
docker-compose up
```

**Time to recover**: <5 minutes

### Rollback Level 2: Version Revert

Requires clean database:

```bash
# Stop Phase 8
docker-compose down

# Revert to Phase 1-7
git checkout release/phase-1-7
cargo build --release

# Drop Phase 8 tables (if needed)
psql $DATABASE_URL << EOF
DROP TABLE observer_checkpoints;
EOF

# Start Phase 1-7
docker-compose -f docker-compose.phase7.yml up
```

**Time to recover**: 5-10 minutes

**Data loss**: Possible (depends on how cleanly database can be reset)

### Rollback Level 3: Full System Revert

For catastrophic failures:

```bash
# Restore from pre-migration backup
AWS S3: s3://backups/observer-pre-phase8-2026-01-22.tar.gz

# Extract and restore database
tar xzf observer-pre-phase8-2026-01-22.tar.gz
psql $DATABASE_URL < observer.sql

# Restart with Phase 1-7
docker-compose -f docker-compose.phase7.yml up -d
```

**Time to recover**: 30-60 minutes

**Data loss**: Events since last backup (usually acceptable)

---

## Validation Checklist

Before moving to each phase:

- [ ] All tests passing
- [ ] Performance benchmarks acceptable
- [ ] No error rate increase
- [ ] No memory leaks
- [ ] Database connections stable
- [ ] DLQ empty (no action failures)
- [ ] Metrics show expected improvement
- [ ] Alerting configured
- [ ] Rollback plan verified
- [ ] Team trained on new features
- [ ] Runbook updated

---

## Post-Migration Tasks

### Week 1 After Phase 8 Migration

```bash
# 1. Monitor metrics continuously
watch -n 5 'fraiseql-observers metrics'

# 2. Check DLQ daily
fraiseql-observers dlq stats

# 3. Verify checkpoints working
psql $DATABASE_URL -c "SELECT COUNT(*) FROM observer_checkpoints;"

# 4. Review logs for issues
docker logs observer-listener | grep -i error

# 5. Test failover scenario (if HA configured)
kill $(pgrep fraiseql)
# Verify automatic recovery
```

### Month 1 After Phase 8 Migration

```bash
# 1. Analyze performance improvements
# Compare: events_processed_total, action_duration_seconds

# 2. Review cache effectiveness
# Target: Hit rate > 70%, memory usage stable

# 3. Plan infrastructure scaling
# Do we need more resources now that we can handle more throughput?

# 4. Document lessons learned
# What worked? What didn't? What would we do differently?

# 5. Plan Phase 8.5+ features
# Which advanced features would provide most value?
```

---

## Support During Migration

### Escalation Path

1. Check Troubleshooting Guide: `TROUBLESHOOTING.md`
2. Review Configuration Examples: `CONFIGURATION_EXAMPLES.md`
3. Check Architecture Guide: `ARCHITECTURE_PHASE_8.md`
4. Contact Platform Team

### Key Contacts

- Database (PostgreSQL): database-team@company.com
- Infrastructure (Redis, Elasticsearch): infra-team@company.com
- Monitoring (Prometheus): observability-team@company.com

---

## Timeline Summary

```
Week 0:     Preparation (setup databases, review docs)
Week 1:     Phase 8.1 (Checkpoints) + Testing
Week 2:     Phase 8.2 (Concurrent) + Performance verification
Week 3:     Phase 8.3 (Dedup) + Duplicate testing
Week 4:     Phase 8.4 (Caching) + Benchmark
Week 5:     Optional Phase 8.5-9 (Search, Queue, Metrics, etc.)
Week 6-8:   Monitoring, optimization, runbook refinement
Month 3+:   Full production Phase 8 deployment
```

---

## Migration Success Metrics

After Phase 8 migration, verify:

| Metric | Target | Actual |
|--------|--------|--------|
| Event loss on crash | 0 | ✓ |
| Action latency P99 | <500ms | ✓ |
| Cache hit rate | >70% | ✓ |
| Dedup effectiveness | >10% | ✓ |
| DLQ backlog | <10 items | ✓ |
| Data corruption incidents | 0 | ✓ |
| Rollback needed | 0 times | ✓ |

---

## Conclusion

Phase 8 migration is a methodical process that can be done safely with:

1. Gradual feature rollout
2. Comprehensive testing at each stage
3. Continuous monitoring
4. Clear rollback plans
5. Team communication

Good luck with your migration!

