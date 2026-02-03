# Redis + NATS Integration Architecture

**Date**: January 24, 2026
**Status**: Integration Design Proposal
**Phase**: Linking Phase 8 (Redis) with Phase 2 (NATS)

---

## Executive Summary

This document defines the integration architecture for combining:

- **Phase 2 (NATS)**: Event transport with at-least-once delivery
- **Phase 8.1 (Checkpoints)**: Zero-event-loss recovery (already integrated)
- **Phase 8.2 (Concurrent)**: Parallel action execution
- **Phase 8.3 (Dedup)**: Event deduplication (Redis-backed)
- **Phase 8.4 (Cache)**: Action result caching (Redis-backed)

**Key Insight**: These features are **composable layers** around the core `ObserverExecutor`. Each layer is optional and configured via feature flags.

---

## 1. Current Architecture (Before Integration)

### 1.1 Event Processing Flow

```
┌────────────────────────────────────────────────────────────┐
│                    Current Flow (Phase 1-7)                 │
└────────────────────────────────────────────────────────────┘

EventTransport (PostgresNotify/NATS/MySQL/MSSQL)
    ↓
ObserverExecutor.process_event()
    ├─ EventMatcher.find_matches()
    ├─ ConditionParser.parse_and_evaluate()
    ├─ ActionExecutor.execute() (sequential)
    │   ├─ WebhookAction
    │   ├─ EmailAction
    │   ├─ SlackAction
    │   └─ etc.
    └─ DeadLetterQueue (on failure)
```

### 1.2 Isolated Phase 8 Features (Not Yet Integrated)

**Phase 8.1: CheckpointStore**
- ✅ Already integrated with NATS bridges (`PostgresNatsBridge`, `MySQLNatsBridge`, `MSSQLNatsBridge`)
- Stores last processed cursor (BIGINT pk)
- Enables crash recovery

**Phase 8.2: ConcurrentActionExecutor**
- ✅ Implemented but not integrated with `ObserverExecutor`
- Wraps `ActionExecutor` for parallel execution
- 5x latency reduction (300ms → 100ms)

**Phase 8.3: DeduplicationStore**
- ✅ Trait defined, Redis implementation exists
- ⚠️ **Not integrated** with event processing flow
- Redis-backed with 5-minute TTL window

**Phase 8.4: CacheBackend**
- ✅ Trait defined, Redis implementation exists
- ⚠️ **Not integrated** with action execution
- 100x faster for cache hits (<1ms)

---

## 2. Proposed Integrated Architecture

### 2.1 Full Processing Pipeline

```
┌──────────────────────────────────────────────────────────────────┐
│          Integrated Architecture (Phase 2 + Phase 8)              │
└──────────────────────────────────────────────────────────────────┘

EventTransport (NATS/Postgres/MySQL/MSSQL)
    ↓
┌─────────────────────────────────────────────┐
│ Layer 1: Deduplication (Phase 8.3)         │
│ - Check if event.id already processed      │
│ - Skip if duplicate (within 5-min window)  │
└─────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────┐
│ Layer 2: ObserverExecutor.process_event()  │
│ - EventMatcher.find_matches()              │
│ - ConditionParser.parse_and_evaluate()     │
└─────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────┐
│ Layer 3: ConcurrentActionExecutor (8.2)    │
│ - Parallel execution of all actions        │
│ - Per-action timeout (30s default)         │
└─────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────┐
│ Layer 4: CachedActionExecutor (8.4)        │
│ - Check cache for previous result          │
│ - Execute if cache miss                    │
│ - Store result in cache (60s TTL)          │
└─────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────┐
│ Layer 5: ActionExecutor (Core)             │
│ - WebhookAction                            │
│ - EmailAction                              │
│ - SlackAction                              │
│ - etc.                                     │
└─────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────┐
│ Layer 6: DeadLetterQueue (on failure)      │
└─────────────────────────────────────────────┘
```

### 2.2 Integration Points

| Layer | Feature | Integration Point | When | Why |
|-------|---------|------------------|------|-----|
| **Transport** | CheckpointStore (8.1) | NATS/MySQL/MSSQL Bridge | Before publish | Crash recovery |
| **Dedup** | DeduplicationStore (8.3) | Before process_event() | On event arrival | Prevent duplicate processing |
| **Executor** | ObserverExecutor | Core processing | Always | Match + evaluate |
| **Concurrent** | ConcurrentActionExecutor (8.2) | Around actions | When >1 action | Parallel execution |
| **Cache** | CachedActionExecutor (8.4) | Around each action | Before execution | Skip expensive ops |
| **Core** | ActionExecutor | Final execution | Cache miss | Actual work |

---

## 3. Implementation Strategy

### 3.1 Layer 1: Deduplication Wrapper

**Location**: New file `crates/fraiseql-observers/src/deduped_executor.rs`

```rust
//! Deduplication wrapper around ObserverExecutor.

use crate::dedup::DeduplicationStore;
use crate::error::Result;
use crate::event::EntityEvent;
use crate::executor::{ObserverExecutor, ExecutionSummary};
use std::sync::Arc;
use tracing::{debug, warn};

/// ObserverExecutor wrapper with deduplication support.
///
/// This wrapper prevents duplicate processing of events by checking
/// a deduplication store before delegating to the inner executor.
pub struct DedupedObserverExecutor<D: DeduplicationStore> {
    inner: Arc<ObserverExecutor>,
    dedup_store: D,
}

impl<D: DeduplicationStore> DedupedObserverExecutor<D> {
    /// Create a new deduplication wrapper.
    pub fn new(executor: ObserverExecutor, dedup_store: D) -> Self {
        Self {
            inner: Arc::new(executor),
            dedup_store,
        }
    }

    /// Process event with deduplication check.
    ///
    /// Returns early if event is duplicate (within time window).
    pub async fn process_event(&self, event: &EntityEvent) -> Result<ExecutionSummary> {
        // Generate deduplication key from event.id (UUIDv4)
        let event_key = format!("event:{}", event.id);

        // Check if already processed
        if self.dedup_store.is_duplicate(&event_key).await? {
            debug!(
                "Event {} is duplicate (within {}-second window), skipping",
                event.id,
                self.dedup_store.window_seconds()
            );

            return Ok(ExecutionSummary {
                total_observers: 0,
                actions_executed: 0,
                actions_succeeded: 0,
                actions_failed: 0,
                conditions_skipped: 0,
                duplicate_skipped: true, // Add this field to ExecutionSummary
            });
        }

        // Process event (not a duplicate)
        let summary = self.inner.process_event(event).await?;

        // Mark as processed (only if processing succeeded)
        if summary.actions_failed == 0 {
            self.dedup_store.mark_processed(&event_key).await?;
            debug!("Marked event {} as processed", event.id);
        } else {
            warn!(
                "Event {} had {} failed actions, NOT marking as processed (will retry)",
                event.id, summary.actions_failed
            );
        }

        Ok(summary)
    }
}
```

**Key Decisions**:

- ✅ Dedup key = `event.id` (UUIDv4) - globally unique across all transports
- ✅ Check dedup **before** processing (early return for duplicates)
- ✅ Mark as processed **only if all actions succeeded** (allow retries on failure)
- ✅ Configurable time window (default 5 minutes)

---

### 3.2 Layer 2: Cached Action Executor

**Location**: New file `crates/fraiseql-observers/src/cached_executor.rs`

```rust
//! Cached action executor wrapper.

use crate::cache::redis::RedisCacheBackend;
use crate::config::ActionConfig;
use crate::error::Result;
use crate::event::EntityEvent;
use crate::traits::{ActionExecutor, ActionResult};
use std::sync::Arc;
use tracing::{debug, info};

/// ActionExecutor wrapper with caching support.
///
/// Checks cache before executing action, stores result after execution.
pub struct CachedActionExecutor<E: ActionExecutor> {
    inner: E,
    cache: Arc<RedisCacheBackend>,
}

impl<E: ActionExecutor> CachedActionExecutor<E> {
    /// Create a new cached executor wrapper.
    pub fn new(executor: E, cache: RedisCacheBackend) -> Self {
        Self {
            inner: executor,
            cache: Arc::new(cache),
        }
    }

    /// Generate cache key from event and action.
    fn cache_key(event: &EntityEvent, action: &ActionConfig) -> String {
        // Hash: event.id + action type + action params
        let action_repr = format!("{:?}", action);
        format!("action_result:{}:{}", event.id, action_repr)
    }
}

#[async_trait::async_trait]
impl<E: ActionExecutor + Send + Sync> ActionExecutor for CachedActionExecutor<E> {
    async fn execute(&self, event: &EntityEvent, action: &ActionConfig) -> Result<ActionResult> {
        let cache_key = Self::cache_key(event, action);

        // Check cache first
        if let Ok(Some(cached_result)) = self.cache.get(&cache_key).await {
            debug!("Cache HIT for action {}", cache_key);
            // Deserialize cached ActionResult
            if let Ok(result) = serde_json::from_slice::<ActionResult>(&cached_result) {
                return Ok(result);
            }
        }

        debug!("Cache MISS for action {}, executing", cache_key);

        // Cache miss - execute action
        let result = self.inner.execute(event, action).await?;

        // Store in cache (only cache successful results)
        if result.success {
            if let Ok(serialized) = serde_json::to_vec(&result) {
                // Default TTL: 60 seconds (configurable)
                let ttl_seconds = 60;
                if let Err(e) = self.cache.set(&cache_key, &serialized, ttl_seconds).await {
                    tracing::warn!("Failed to cache action result: {}", e);
                }
            }
        }

        Ok(result)
    }
}
```

**Key Decisions**:

- ✅ Cache key = `event.id` + action params hash
- ✅ Cache **only successful** action results (don't cache failures)
- ✅ Default TTL: 60 seconds (configurable per cache backend)
- ✅ Cache miss → execute → cache result

---

### 3.3 Layer 3: Concurrent + Cached Composition

**Location**: Update `crates/fraiseql-observers/src/concurrent/mod.rs`

```rust
// Modify ConcurrentActionExecutor to accept any ActionExecutor,
// including CachedActionExecutor

impl<E: ActionExecutor + Clone + Send + Sync + 'static> ConcurrentActionExecutor<E> {
    // ... existing implementation works with any E: ActionExecutor
    // No changes needed - composition just works!
}
```

**Composition Pattern**:

```rust
// Core executor
let webhook_action = WebhookAction::new();

// Wrap with cache
let cached_webhook = CachedActionExecutor::new(webhook_action, redis_cache);

// Wrap with concurrency
let concurrent_cached = ConcurrentActionExecutor::new(cached_webhook, 30000);

// Each action gets: parallel execution + cache checking
```

**Result**: Actions execute in parallel, each checking cache first before expensive operations.

---

## 4. Configuration Integration

### 4.1 TOML Configuration (Complete)

```toml
# fraiseql-observer.toml

[observer]
# Transport selection
transport = "nats"  # or "postgres", "mysql", "mssql", "in_memory"

# Feature toggles
enable_deduplication = true   # Phase 8.3
enable_caching = true          # Phase 8.4
enable_concurrent = true       # Phase 8.2
enable_checkpoint = true       # Phase 8.1 (always on for NATS)

[database]
url = "postgresql://localhost/mydb"

# ===== NATS Configuration =====
[nats]
url = "nats://localhost:4222"
stream_name = "fraiseql.entity_changes"
consumer_name = "observer-worker-1"
subject_prefix = "entity.change"

[nats.jetstream]
retention_max_messages = 1_000_000
retention_max_bytes = 1_073_741_824  # 1GB
ack_wait_secs = 30

[nats.bridge]
# Enable bridge for this instance (main server = true, workers = false)
run_bridge = true
batch_size = 100
transport_name = "postgres_to_nats"

# ===== Redis Configuration (Phase 8) =====
[redis]
url = "redis://localhost:6379"
pool_size = 10

# Deduplication settings (Phase 8.3)
[redis.dedup]
enabled = true
window_seconds = 300  # 5 minutes
key_prefix = "dedup"

# Cache settings (Phase 8.4)
[redis.cache]
enabled = true
default_ttl_seconds = 60
key_prefix = "cache"

# Checkpoint settings (Phase 8.1)
[redis.checkpoint]
enabled = false  # Use PostgreSQL checkpoint store instead
key_prefix = "checkpoint"

# ===== Checkpoint Store Configuration =====
[checkpoint]
# Backend: "postgres", "mysql", "mssql", "redis"
backend = "postgres"
table_name = "tb_transport_checkpoint"

# ===== Dead Letter Queue =====
[dlq]
backend = "postgres"
table_name = "tb_observer_dlq"
max_retries = 3

# ===== Concurrency Settings (Phase 8.2) =====
[concurrency]
enabled = true
action_timeout_ms = 30000  # 30 seconds per action
max_concurrent_actions = 10  # Limit parallelism
```

### 4.2 Environment Variable Overrides

```bash
# Transport
FRAISEQL_OBSERVER_TRANSPORT=nats

# NATS
FRAISEQL_NATS_URL=nats://nats-cluster:4222
FRAISEQL_NATS_CONSUMER_NAME=worker-2

# Redis
FRAISEQL_REDIS_URL=redis://redis-cluster:6379

# Deduplication
FRAISEQL_REDIS_DEDUP_ENABLED=true
FRAISEQL_REDIS_DEDUP_WINDOW_SECONDS=600

# Cache
FRAISEQL_REDIS_CACHE_ENABLED=true
FRAISEQL_REDIS_CACHE_TTL_SECONDS=120

# Concurrency
FRAISEQL_CONCURRENCY_ENABLED=true
FRAISEQL_CONCURRENCY_TIMEOUT_MS=45000
```

---

## 5. Feature Flag Strategy

### 5.1 Cargo.toml Features

```toml
[features]
# Default: PostgreSQL-only, no Redis
default = ["postgres"]

# Database backends
postgres = ["sqlx/postgres"]
mysql = ["sqlx/mysql"]
mssql = ["tiberius", "tokio-util", "bb8", "bb8-tiberius"]

# NATS transport
nats = ["async-nats"]

# Phase 8 features (composable)
checkpoint = []                    # Phase 8.1 (always on for NATS)
dedup = ["redis"]                  # Phase 8.3 (requires Redis)
caching = ["redis"]                # Phase 8.4 (requires Redis)
concurrent = []                    # Phase 8.2 (no extra deps)
queue = ["redis"]                  # Phase 8.6 (future)
metrics = ["prometheus"]           # Phase 8.7 (future)

# Meta-features
phase8 = ["checkpoint", "dedup", "caching", "concurrent"]
multi-db = ["postgres", "mysql"]
all-db = ["postgres", "mysql", "mssql"]
full = ["phase8", "all-db", "nats", "metrics"]
```

### 5.2 Conditional Compilation

```rust
// Deduplication wrapper (only compiled if feature enabled)
#[cfg(feature = "dedup")]
pub mod deduped_executor;

// Cached executor (only compiled if feature enabled)
#[cfg(feature = "caching")]
pub mod cached_executor;

// Concurrent executor (always available, no deps)
pub mod concurrent;
```

---

## 6. Deployment Topologies

### 6.1 Topology 1: PostgreSQL-Only (No Redis, No NATS)

**Use Case**: Simple monolithic deployment, low traffic

```toml
[observer]
transport = "postgres"
enable_deduplication = false  # No Redis
enable_caching = false         # No Redis
enable_concurrent = false      # Sequential execution
```

**Architecture**:
```
PostgreSQL LISTEN/NOTIFY → ObserverExecutor → Sequential Actions
```

**Benefits**:

- ✅ Simplest deployment (single binary + database)
- ✅ No external dependencies
- ✅ Low latency (<10ms)

**Limitations**:

- ❌ Single instance only
- ❌ No deduplication (trust database triggers)
- ❌ Sequential action execution (slower)

---

### 6.2 Topology 2: PostgreSQL + Redis (No NATS)

**Use Case**: Monolithic with performance boost

```toml
[observer]
transport = "postgres"
enable_deduplication = true   # Redis-backed
enable_caching = true          # Redis-backed
enable_concurrent = true       # Parallel actions
```

**Architecture**:
```
PostgreSQL LISTEN/NOTIFY
    ↓
DedupedObserverExecutor (Redis check)
    ↓
ConcurrentActionExecutor
    ↓
CachedActionExecutor (Redis cache)
    ↓
ActionExecutor
```

**Benefits**:

- ✅ 5x faster (concurrent execution)
- ✅ 100x faster for cache hits
- ✅ Duplicate prevention
- ✅ Still simple deployment (1 process)

**Limitations**:

- ❌ Single instance only
- ❌ Requires Redis

---

### 6.3 Topology 3: NATS + Redis (Distributed)

**Use Case**: Multi-region, horizontal scaling

**Main Server** (runs bridge):
```toml
[observer]
transport = "nats"
enable_deduplication = false  # Bridge doesn't consume events
enable_concurrent = false

[nats.bridge]
run_bridge = true
run_executors = false  # Don't execute observers here
```

**Observer Workers** (N instances):
```toml
[observer]
transport = "nats"
enable_deduplication = true   # Critical for at-least-once
enable_caching = true
enable_concurrent = true

[nats]
consumer_name = "observer-worker-pool"  # Shared durable consumer

[nats.bridge]
run_bridge = false
run_executors = true
```

**Architecture**:
```
PostgreSQL → PostgresNatsBridge → NATS JetStream
                                       ↓
                 ┌─────────────────────┴─────────────────────┐
                 ↓                     ↓                     ↓
           Worker 1              Worker 2              Worker N
    (dedup + cache + concurrent)
```

**Benefits**:

- ✅ Horizontal scaling (add workers as needed)
- ✅ Multi-region capable
- ✅ Fault isolation (observer failures don't affect main app)
- ✅ Load balancing (NATS competing consumers)

**Requirements**:

- ✅ NATS cluster
- ✅ Redis cluster
- ✅ Shared dedup store (critical for exactly-once effects)

---

### 6.4 Topology 4: Multi-Database + NATS (Enterprise)

**Use Case**: Unified event stream across heterogeneous databases

**Database Bridges**:

- PostgreSQL → PostgresNatsBridge → NATS
- MySQL → MySQLNatsBridge → NATS
- SQL Server → MSSQLNatsBridge → NATS

**Observer Workers**: Same as Topology 3

**Architecture**:
```
PostgreSQL → PG Bridge ──┐
MySQL → MySQL Bridge ────┤→ NATS JetStream → Workers (dedup + cache)
MSSQL → MSSQL Bridge ────┘
```

**Benefits**:

- ✅ Unified event stream across all databases
- ✅ Database-agnostic observers
- ✅ Cross-database workflows

---

## 7. Integration Checklist

### 7.1 Phase 1: Wire Up Deduplication (1-2 days)

- [ ] Create `deduped_executor.rs`
- [ ] Add `duplicate_skipped` field to `ExecutionSummary`
- [ ] Update `ObserverExecutor` to optionally wrap with dedup
- [ ] Add config parsing for `enable_deduplication`
- [ ] Write unit tests (dedup hit/miss scenarios)
- [ ] Write integration test (Redis-backed dedup)
- [ ] Update documentation

### 7.2 Phase 2: Wire Up Caching (1-2 days)

- [ ] Create `cached_executor.rs`
- [ ] Make `ActionResult` implement `Serialize`/`Deserialize`
- [ ] Update action executors to optionally wrap with cache
- [ ] Add config parsing for `enable_caching`
- [ ] Write unit tests (cache hit/miss scenarios)
- [ ] Write integration test (Redis-backed cache)
- [ ] Benchmark cache performance

### 7.3 Phase 3: Wire Up Concurrency (1 day)

- [ ] Update `ObserverExecutor` to use `ConcurrentActionExecutor`
- [ ] Add config parsing for `enable_concurrent`
- [ ] Test composition: concurrent + cached
- [ ] Verify timeout behavior
- [ ] Benchmark parallel vs sequential

### 7.4 Phase 4: Configuration System (1-2 days)

- [ ] Create `config/mod.rs` for TOML parsing
- [ ] Implement environment variable overrides
- [ ] Add config validation
- [ ] Write config examples for each topology
- [ ] Document configuration options

### 7.5 Phase 5: End-to-End Testing (2-3 days)

- [ ] Integration test: NATS + dedup + cache + concurrent
- [ ] Load test: 10K events/sec with full pipeline
- [ ] Chaos test: Worker crash during processing (dedup prevents duplicates)
- [ ] Verify cache hit rate (>80% for repeated events)
- [ ] Verify dedup hit rate (>40% with at-least-once)

### 7.6 Phase 6: Documentation (1-2 days)

- [ ] Deployment topology guides
- [ ] Configuration reference
- [ ] Troubleshooting guide
- [ ] Performance tuning guide
- [ ] Migration guide from PostgreSQL-only

**Total Effort**: 7-12 days

---

## 8. Performance Characteristics

### 8.1 Expected Performance (Fully Integrated)

| Scenario | Without Phase 8 | With Phase 8 | Improvement |
|----------|----------------|--------------|-------------|
| **3 actions, no cache** | 300ms (sequential) | 100ms (parallel) | 3x faster |
| **3 actions, cache hit** | 300ms | 3ms | 100x faster |
| **Duplicate event** | 300ms | <1ms (dedup skip) | 300x faster |
| **Throughput** | ~3 events/sec | ~30 events/sec | 10x faster |

### 8.2 Cache Hit Rate Expectations

| Scenario | Cache Hit Rate | Dedup Hit Rate |
|----------|---------------|----------------|
| Normal operations | 60-80% | 5-10% |
| NATS at-least-once | 60-80% | 20-40% |
| Replay scenario | 90-95% | 80-90% |

### 8.3 Resource Usage

| Component | CPU | Memory | Network |
|-----------|-----|--------|---------|
| Dedup check (Redis) | <1ms | ~10KB per event | ~1KB per check |
| Cache hit (Redis) | <1ms | ~50KB per result | ~5KB per hit |
| Concurrent execution | +20% | +50MB | Same |
| NATS transport | +10% | +20MB | +10% |

---

## 9. Migration Path

### 9.1 From PostgreSQL-Only to PostgreSQL + Redis

**Step 1**: Add Redis (no code changes)
```bash
docker run -d -p 6379:6379 redis:latest
```

**Step 2**: Enable features via config
```toml
[observer]
enable_deduplication = true
enable_caching = true
enable_concurrent = true

[redis]
url = "redis://localhost:6379"
```

**Step 3**: Restart observer process
```bash
systemctl restart fraiseql-observer
```

**Rollback**: Set `enable_*` to `false`, restart

**Risk**: Low (backward compatible)

---

### 9.2 From PostgreSQL + Redis to NATS + Redis

**Step 1**: Deploy NATS cluster
```bash
docker run -d -p 4222:4222 nats:latest -js
```

**Step 2**: Run bridge alongside existing listener
```toml
[observer]
transport = "postgres"  # Keep existing

[nats.bridge]
run_bridge = true  # Publish to NATS too
```

**Step 3**: Deploy observer workers (NATS consumers)
```toml
[observer]
transport = "nats"
enable_deduplication = true  # Critical!
```

**Step 4**: Validate workers process events correctly

**Step 5**: Switch main server to NATS-only
```toml
[observer]
transport = "nats"

[nats.bridge]
run_bridge = true
run_executors = false
```

**Rollback**: Switch back to `transport = "postgres"`

**Risk**: Medium (requires NATS validation)

---

## 10. Key Design Decisions

### 10.1 Why Dedup at Event Level (Not Action Level)?

**Decision**: Check dedup before `process_event()`, not before each action

**Rationale**:

- ✅ Prevents entire observer execution (faster)
- ✅ Single Redis lookup per event (not N lookups)
- ✅ Clearer semantics: "event processed once"
- ❌ Can't deduplicate individual actions (but rarely needed)

**Alternative Considered**: Dedup per action
- More granular but slower (N Redis lookups)
- Complexity not justified

---

### 10.2 Why Cache Action Results (Not Event Results)?

**Decision**: Cache individual action results, not entire event processing

**Rationale**:

- ✅ Higher hit rate (same action config reused across events)
- ✅ Composable with concurrent execution
- ✅ Finer-grained TTL control
- ✅ Supports partial cache hits (some actions cached, others not)

**Alternative Considered**: Cache entire event
- Lower granularity, less useful

---

### 10.3 Why Not Mark Dedup on Failure?

**Decision**: Only mark event as processed if **all actions succeeded**

**Rationale**:

- ✅ Allows retry on transient failures
- ✅ At-least-once execution preserved
- ✅ Dead Letter Queue handles permanent failures

**Alternative Considered**: Always mark processed
- Would lose failed events (not acceptable)

---

### 10.4 Why Separate Checkpoint Store from Dedup Store?

**Decision**: Checkpoint store for bridges, dedup store for event processing

**Rationale**:

- ✅ Different semantics (cursor vs event identity)
- ✅ Different lifetimes (checkpoint = permanent, dedup = 5 min)
- ✅ Different backends (checkpoint = PostgreSQL, dedup = Redis)
- ✅ Clear separation of concerns

**Alternative Considered**: Single store
- Conflates two different concerns

---

## 11. Testing Strategy

### 11.1 Unit Tests

```rust
#[tokio::test]
async fn test_dedup_prevents_duplicate_processing() {
    let dedup_store = InMemoryDedupStore::new();
    let executor = ObserverExecutor::new(...);
    let deduped = DedupedObserverExecutor::new(executor, dedup_store);

    let event = create_test_event();

    // First processing
    let summary1 = deduped.process_event(&event).await.unwrap();
    assert_eq!(summary1.actions_executed, 3);

    // Second processing (duplicate)
    let summary2 = deduped.process_event(&event).await.unwrap();
    assert_eq!(summary2.actions_executed, 0);
    assert!(summary2.duplicate_skipped);
}

#[tokio::test]
async fn test_cache_speeds_up_repeated_actions() {
    let cache = RedisCacheBackend::new("redis://localhost:6379");
    let webhook = WebhookAction::new();
    let cached = CachedActionExecutor::new(webhook, cache);

    let event = create_test_event();
    let action = ActionConfig::Webhook { ... };

    // First execution (cache miss)
    let start = Instant::now();
    cached.execute(&event, &action).await.unwrap();
    let first_duration = start.elapsed();

    // Second execution (cache hit)
    let start = Instant::now();
    cached.execute(&event, &action).await.unwrap();
    let second_duration = start.elapsed();

    assert!(second_duration < first_duration / 10);  // 10x faster
}
```

### 11.2 Integration Tests

```rust
#[tokio::test]
async fn test_full_pipeline_nats_redis_dedup_cache() {
    // Setup NATS + Redis
    let nats = start_embedded_nats().await;
    let redis = start_redis_container().await;

    // Create bridge
    let bridge = PostgresNatsBridge::new(...);
    tokio::spawn(bridge.run());

    // Create observer worker with all features
    let dedup_store = RedisDeduplicationStore::new(&redis);
    let cache = RedisCacheBackend::new(&redis);
    let executor = ObserverExecutor::new(...);
    let cached = CachedActionExecutor::new(executor, cache);
    let concurrent = ConcurrentActionExecutor::new(cached, 30000);
    let deduped = DedupedObserverExecutor::new(concurrent, dedup_store);

    // Publish 100 events (with 50% duplicates)
    for i in 0..100 {
        insert_change_log_entry(i).await;
    }

    // Wait for processing
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify metrics
    let stats = get_dedup_stats();
    assert_eq!(stats.total_checked, 100);
    assert!(stats.duplicates_skipped >= 40);  // At-least-once duplicates

    let cache_stats = get_cache_stats();
    assert!(cache_stats.hit_rate > 0.6);  // 60%+ cache hit rate
}
```

---

## 12. Monitoring & Observability

### 12.1 Metrics to Track (Phase 8.7)

```rust
// Deduplication metrics
fraiseql_observer_dedup_total{result="skipped"}
fraiseql_observer_dedup_total{result="processed"}
fraiseql_observer_dedup_hit_rate

// Cache metrics
fraiseql_observer_cache_total{result="hit"}
fraiseql_observer_cache_total{result="miss"}
fraiseql_observer_cache_hit_rate

// Concurrency metrics
fraiseql_observer_action_duration_seconds{concurrent="true"}
fraiseql_observer_action_duration_seconds{concurrent="false"}

// NATS metrics
fraiseql_observer_nats_messages_received
fraiseql_observer_nats_messages_acked
fraiseql_observer_nats_checkpoint_cursor
```

### 12.2 Health Checks

```rust
// GET /health/observer
{
  "status": "healthy",
  "transport": "nats",
  "features": {
    "deduplication": true,
    "caching": true,
    "concurrent": true
  },
  "dedup_store": {
    "status": "healthy",
    "window_seconds": 300,
    "hit_rate": 0.42
  },
  "cache": {
    "status": "healthy",
    "hit_rate": 0.78,
    "ttl_seconds": 60
  },
  "nats": {
    "status": "connected",
    "consumer": "observer-worker-1",
    "pending_messages": 23
  },
  "checkpoint": {
    "last_cursor": 12345,
    "last_updated": "2026-01-24T10:30:00Z"
  }
}
```

---

## 13. Recommendations

### 13.1 Immediate (This Week)

1. **Implement DedupedObserverExecutor** (1-2 days)
   - Critical for NATS at-least-once delivery
   - Prevents duplicate side effects
   - Simple wrapper pattern

2. **Implement CachedActionExecutor** (1-2 days)
   - 100x performance boost for cache hits
   - Composable with concurrent execution
   - Low risk, high reward

### 13.2 Short Term (Next 2 Weeks)

3. **Configuration System** (1-2 days)
   - TOML config parsing
   - Environment variable overrides
   - Feature toggle support

4. **Integration Testing** (2-3 days)
   - Full pipeline test (NATS + Redis + all features)
   - Load testing with metrics
   - Chaos testing (crash scenarios)

### 13.3 Medium Term (Next Month)

5. **Prometheus Metrics** (Phase 8.7, 3-5 days)
   - Essential for production monitoring
   - Track dedup/cache hit rates
   - Alert on degradation

6. **Documentation** (2-3 days)
   - Deployment topology guides
   - Configuration reference
   - Troubleshooting guide

---

## 14. Conclusion

**The integration is straightforward**: Each Phase 8 feature is a **composable wrapper** around the core executor. The architecture naturally supports:

- ✅ **Opt-in adoption** via feature flags
- ✅ **Progressive enhancement** (start simple, add features as needed)
- ✅ **Zero breaking changes** (backward compatible)
- ✅ **Clear separation of concerns** (each layer has one job)

**Total effort**: 7-12 days to fully integrate

**Risk**: Low (additive changes only, well-tested patterns)

**Impact**: High (5-300x performance improvements, zero-loss guarantees)

---

**Next Steps**: Start with `DedupedObserverExecutor` (most critical for NATS), then add caching and configuration system.
