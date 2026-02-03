# Phase 8: Advanced Features for Observer System

**Phase**: 8 - Advanced Features
**Objective**: Add persistent checkpoints, caching, search, and job queues
**Status**: 📋 PLANNING
**Estimated Duration**: 5-7 days

---

## 🎯 Executive Summary

Phase 8 builds on the production-ready Observer System (Phase 1-7) by adding enterprise-grade advanced features:

1. **Persistent Checkpoints** - Durably store listener state in database
2. **Redis Caching Layer** - Optimize action execution with Redis integration
3. **Elasticsearch Integration** - Full-text search for events and action results
4. **Job Queue System** - Async job execution for long-running actions (emails, webhooks)
5. **Metrics & Observability** - Prometheus metrics and structured logging
6. **Multiple Listeners** - Failover and horizontal scaling support

---

## 📊 Current Status (Phase 1-7 Completed)

### What's Completed

- ✅ Phase 1-6: Core observer system, action execution, event processing
- ✅ Phase 7: ChangeLogListener polling, Debezium integration, E2E tests
- ✅ Quality: 100/100 tests passing, clippy-pedantic compliant, production-ready

### Phase 7 Quick Stats

- 100 tests (74 Phase 1-6 + 26 Phase 7)
- 500+ LOC Phase 7 implementation
- Zero unsafe code
- Full error handling with exponential backoff
- Multi-tenant isolation

---

## 🏗️ Phase 8 Architecture

### 8.1 Persistent Checkpoints

**Problem**: Current checkpoints are in-memory. On restart, listener may reprocess events.

**Solution**: Store checkpoints in database table `observer_checkpoints`

```sql
CREATE TABLE observer_checkpoints (
    id BIGSERIAL PRIMARY KEY,
    listener_id UUID NOT NULL UNIQUE,
    last_processed_id BIGINT NOT NULL,
    last_processed_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_observer_checkpoints_listener_id
  ON observer_checkpoints(listener_id);
```

**Implementation**:

- Add `CheckpointStore` trait (like `DeadLetterQueue`, `EventSource`)
- Create `PostgresCheckpointStore` implementation
- Update `ChangeLogListener` to:
  - Load checkpoint on startup
  - Persist checkpoint after successful batch processing
  - Use checkpoint for resume-from-id

**Benefits**:

- ✅ Zero event loss on crash/restart
- ✅ Exactly-once semantics (with DLQ)
- ✅ Audit trail of processing
- ✅ Horizontal scaling (each listener tracks its progress)

---

### 8.2 Redis Caching Layer

**Problem**: Action execution may be slow (webhooks, external APIs). Repeated events hit the same endpoints.

**Solution**: Cache action results in Redis with TTL

```rust
pub trait CacheBackend: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<String>>;
    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<()>;
    async fn delete(&self, key: &str) -> Result<()>;
}

pub struct RedisCacheBackend {
    client: redis::Client,
}

pub struct ActionResultCache {
    cache: Arc<dyn CacheBackend>,
    ttl_seconds: u64,
}

impl ActionResultCache {
    pub async fn get_or_execute(
        &self,
        event: &EntityEvent,
        action: &ActionConfig,
        executor: &dyn ActionExecutor,
    ) -> Result<ActionResult> {
        // Generate cache key from event + action
        let cache_key = format!("observer:action:{}:{}", event.entity_id, action_id);

        // Try cache first
        if let Ok(Some(cached)) = self.cache.get(&cache_key).await {
            return serde_json::from_str(&cached)
                .map_err(|_| ObserverError::CacheParse { ... });
        }

        // Execute action
        let result = executor.execute(event, action).await?;

        // Cache result
        let serialized = serde_json::to_string(&result)?;
        let _ = self.cache.set(&cache_key, &serialized, self.ttl_seconds).await;

        Ok(result)
    }
}
```

**Integration Points**:

- New `cache` module with `RedisCacheBackend`
- Update `ObserverExecutor` to optionally use `ActionResultCache`
- Configuration for Redis connection + TTL
- Cache invalidation hooks when related entities change

**Benefits**:

- ✅ Reduced API calls (webhooks, external services)
- ✅ Faster observer processing
- ✅ Better reliability (fallback to cache on API failures)
- ✅ Configurable TTL per action type

---

### 8.3 Elasticsearch Integration

**Problem**: Need to search through processed events and action execution history.

**Solution**: Index events to Elasticsearch for full-text search

```rust
pub trait SearchBackend: Send + Sync {
    async fn index(&self, id: &str, doc: &Value) -> Result<()>;
    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>>;
    async fn delete(&self, id: &str) -> Result<()>;
}

pub struct ElasticsearchBackend {
    client: elasticsearch::Elasticsearch,
}

pub struct EventSearchIndex {
    search: Arc<dyn SearchBackend>,
}

impl EventSearchIndex {
    pub async fn index_event(&self, event: &EntityEvent) -> Result<()> {
        let doc = json!({
            "entity_type": event.entity_type,
            "entity_id": event.entity_id,
            "event_type": event.event_type.as_str(),
            "user_id": event.user_id,
            "timestamp": event.timestamp.to_rfc3339(),
            "data": event.data,
            "changes": event.changes,
        });

        self.search.index(&event.entity_id.to_string(), &doc).await
    }

    pub async fn search_events(&self, query: &SearchQuery) -> Result<Vec<EntityEvent>> {
        let results = self.search.search(query).await?;
        // Convert results back to EntityEvent
        // ...
    }
}
```

**Integration Points**:

- New `search` module with `SearchBackend` trait
- `ElasticsearchBackend` implementation
- Hook into `ObserverExecutor` to index events after processing
- API endpoint `/api/observers/search` for querying events

**Features**:

- Full-text search on entity data
- Filter by event type, entity type, date range
- Faceted search (group by entity type, etc.)
- Audit trail of all events

**Benefits**:

- ✅ Complete audit trail queryable
- ✅ Troubleshooting and debugging
- ✅ Compliance and regulatory requirements
- ✅ Analytics on event patterns

---

### 8.4 Job Queue System

**Problem**: Long-running actions (batch emails, exports) block observer processing.

**Solution**: Offload to async job queue (Redis Queue, Bull, Celery alternative)

```rust
pub trait JobQueue: Send + Sync {
    async fn enqueue(&self, job: &Job) -> Result<String>;
    async fn dequeue(&self, queue_name: &str) -> Result<Option<Job>>;
    async fn mark_completed(&self, job_id: &str) -> Result<()>;
    async fn mark_failed(&self, job_id: &str, error: &str) -> Result<()>;
}

pub struct Job {
    pub id: String,
    pub action_type: String,
    pub event: EntityEvent,
    pub action: ActionConfig,
    pub retry_count: u32,
    pub created_at: DateTime<Utc>,
}

pub struct JobQueueActionExecutor {
    queue: Arc<dyn JobQueue>,
    direct_executor: Arc<dyn ActionExecutor>,
}

impl JobQueueActionExecutor {
    pub async fn execute(
        &self,
        event: &EntityEvent,
        action: &ActionConfig,
    ) -> Result<ActionResult> {
        // Determine if action should be queued
        if should_queue(action) {
            // Enqueue for async execution
            let job_id = self.queue.enqueue(&job).await?;
            return Ok(ActionResult {
                action_type: action.action_type().to_string(),
                success: true,
                message: format!("Queued as job {}", job_id),
                duration_ms: 0.0,
            });
        }

        // Execute immediately for fast actions
        self.direct_executor.execute(event, action).await
    }
}

pub async fn process_job_queue(queue: Arc<dyn JobQueue>, executor: Arc<dyn ActionExecutor>) {
    loop {
        if let Ok(Some(job)) = queue.dequeue("default").await {
            match executor.execute(&job.event, &job.action).await {
                Ok(result) => {
                    let _ = queue.mark_completed(&job.id).await;
                }
                Err(e) => {
                    if job.retry_count < MAX_RETRIES {
                        // Re-enqueue with incremented retry count
                        let mut retry_job = job.clone();
                        retry_job.retry_count += 1;
                        let _ = queue.enqueue(&retry_job).await;
                    } else {
                        let _ = queue.mark_failed(&job.id, &e.to_string()).await;
                    }
                }
            }
        }

        // Sleep briefly before next poll
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
```

**Database Schema**:
```sql
CREATE TABLE job_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    action_type VARCHAR(50) NOT NULL,
    event JSONB NOT NULL,
    action JSONB NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    retry_count INT NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    processed_at TIMESTAMP WITH TIME ZONE,
    completed_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_job_queue_status ON job_queue(status);
CREATE INDEX idx_job_queue_created_at ON job_queue(created_at DESC);
```

**Benefits**:

- ✅ Non-blocking observer processing
- ✅ Automatic retries with backoff
- ✅ Job tracking and audit trail
- ✅ Horizontal scaling (multiple workers)
- ✅ Better UX (webhook queued response)

---

### 8.5 Metrics & Observability

**Problem**: No visibility into observer performance and health.

**Solution**: Add Prometheus metrics

```rust
pub struct ObserverMetrics {
    // Counters
    events_processed: IntCounter,
    events_failed: IntCounter,
    actions_executed: IntCounter,
    actions_failed: IntCounter,
    dlq_items: IntCounter,

    // Gauges
    listener_backoff_level: IntGauge,
    pending_jobs: IntGauge,
    cache_hit_rate: Gauge,

    // Histograms
    event_processing_duration: Histogram,
    action_execution_duration: Histogram,
}

impl ObserverMetrics {
    pub fn record_event_processed(&self, duration_ms: f64) {
        self.events_processed.inc();
        self.event_processing_duration.observe(duration_ms);
    }

    pub fn record_action_executed(&self, action_type: &str, duration_ms: f64, success: bool) {
        if success {
            self.actions_executed.inc();
        } else {
            self.actions_failed.inc();
        }
        self.action_execution_duration.observe(duration_ms);
    }
}
```

**Metrics to Track**:

- Events processed per second
- Action execution time by type (webhook, email, cache, etc.)
- DLQ queue size
- Listener health (backoff level, consecutive errors)
- Cache hit rate
- Job queue depth
- Observer matching success rate

**Benefits**:

- ✅ Production monitoring
- ✅ Performance debugging
- ✅ Alerting on anomalies
- ✅ Capacity planning
- ✅ SLA tracking

---

### 8.6 Multiple Listeners (Failover)

**Problem**: Single listener is a point of failure.

**Solution**: Support multiple concurrent listeners with leader election

```rust
pub struct MultiListener {
    listeners: Vec<Arc<Mutex<ChangeLogListener>>>,
    executor: Arc<ObserverExecutor>,
    last_checkpoint: Arc<Mutex<i64>>,
}

impl MultiListener {
    pub async fn spawn_all(self: Arc<Self>) -> Vec<JoinHandle<Result<()>>> {
        self.listeners
            .iter()
            .enumerate()
            .map(|(idx, listener)| {
                let executor = self.executor.clone();
                let listener = listener.clone();

                tokio::spawn(async move {
                    loop {
                        // Each listener polls independently
                        // They share checkpoints via database
                        // Any listener can process any batch
                    }
                })
            })
            .collect()
    }
}
```

**Benefits**:

- ✅ Horizontal scaling
- ✅ Automatic failover
- ✅ Load distribution
- ✅ High availability

---

## 📋 Implementation Phases (Phase 8)

### 8.0: Planning & Architecture Review (1 day)

- [ ] Review existing Observer System design
- [ ] Design persistent checkpoint storage
- [ ] Design Redis caching integration
- [ ] Design Elasticsearch integration
- [ ] Design job queue system
- [ ] Create detailed implementation plan

### 8.1: Persistent Checkpoints (1 day)

- [ ] Create `CheckpointStore` trait
- [ ] Implement `PostgresCheckpointStore`
- [ ] Add migrations for `observer_checkpoints` table
- [ ] Update `ChangeLogListener` to use checkpoints
- [ ] Write tests for checkpoint persistence
- [ ] Verify recovery on restart

### 8.2: Redis Caching Layer (1.5 days)

- [ ] Create `CacheBackend` trait
- [ ] Implement `RedisCacheBackend`
- [ ] Create `ActionResultCache` wrapper
- [ ] Integrate with `ObserverExecutor`
- [ ] Add configuration for Redis + TTL
- [ ] Write cache hit/miss tests
- [ ] Add metrics for cache operations

### 8.3: Elasticsearch Integration (1.5 days)

- [ ] Create `SearchBackend` trait
- [ ] Implement `ElasticsearchBackend`
- [ ] Create `EventSearchIndex` wrapper
- [ ] Add indexing to event processing
- [ ] Create search query API
- [ ] Add integration tests
- [ ] Document search capabilities

### 8.4: Job Queue System (1.5 days)

- [ ] Create `JobQueue` trait
- [ ] Implement `PostgresJobQueue`
- [ ] Add migrations for `job_queue` table
- [ ] Create `JobQueueActionExecutor`
- [ ] Implement job worker/processor
- [ ] Add retries and error handling
- [ ] Write job processing tests

### 8.5: Metrics & Observability (1 day)

- [ ] Create `ObserverMetrics` struct
- [ ] Integrate Prometheus instrumentation
- [ ] Add metrics to executor
- [ ] Add metrics to listener
- [ ] Create metrics endpoint (`/metrics`)
- [ ] Write metrics validation tests

### 8.6: Multiple Listeners (1 day)

- [ ] Design leader election (or shared polling)
- [ ] Implement `MultiListener` coordinator
- [ ] Update checkpoint system for multiple listeners
- [ ] Add concurrency tests
- [ ] Test failover scenarios
- [ ] Document scaling patterns

### 8.7: Documentation & Examples (1 day)

- [ ] Update README with Phase 8 features
- [ ] Create example: Persistent checkpoints
- [ ] Create example: Redis caching
- [ ] Create example: Event search
- [ ] Create example: Job queue
- [ ] Create monitoring guide
- [ ] Add troubleshooting guide

### 8.8: Quality Assurance & Polish (1 day)

- [ ] Run full clippy check
- [ ] Run all tests
- [ ] Performance benchmarks
- [ ] Integration testing
- [ ] Documentation review
- [ ] Final quality report

---

## 📁 File Structure for Phase 8

```
crates/fraiseql-observers/
├── src/
│   ├── checkpoint/
│   │   ├── mod.rs
│   │   ├── store.rs              # CheckpointStore trait
│   │   └── postgres.rs           # PostgresCheckpointStore
│   ├── cache/
│   │   ├── mod.rs
│   │   ├── backend.rs            # CacheBackend trait
│   │   ├── redis.rs              # RedisCacheBackend
│   │   └── action_cache.rs       # ActionResultCache
│   ├── search/
│   │   ├── mod.rs
│   │   ├── backend.rs            # SearchBackend trait
│   │   ├── elasticsearch.rs      # ElasticsearchBackend
│   │   └── index.rs              # EventSearchIndex
│   ├── queue/
│   │   ├── mod.rs
│   │   ├── traits.rs             # JobQueue trait
│   │   ├── postgres.rs           # PostgresJobQueue
│   │   └── processor.rs          # Job processor
│   ├── metrics/
│   │   ├── mod.rs
│   │   └── prometheus.rs         # ObserverMetrics
│   ├── multi_listener.rs         # MultiListener coordinator
│   └── ...existing files...
├── migrations/
│   ├── 001_observer_checkpoints.sql
│   ├── 002_job_queue.sql
│   └── ...
└── ...
```

---

## 🧪 Testing Strategy for Phase 8

### Unit Tests

- Checkpoint persistence and recovery
- Cache hit/miss scenarios
- Search indexing and querying
- Job queue operations
- Metrics collection

### Integration Tests

- Listener restart with checkpoint recovery
- Cache invalidation flows
- End-to-end job processing
- Multi-listener coordination
- Failover scenarios

### Performance Tests

- Checkpoint load/save performance
- Cache lookup latency
- Elasticsearch query performance
- Job queue throughput
- Metrics overhead

---

## 🎯 Success Criteria for Phase 8

- [ ] Persistent checkpoints: Zero event loss on restart
- [ ] Redis caching: 10x faster repeated action execution
- [ ] Elasticsearch: All events searchable within 1 second
- [ ] Job queues: Long-running actions don't block observer processing
- [ ] Metrics: All key operations instrumented
- [ ] Multiple listeners: Automatic failover working
- [ ] Tests: 150+ new tests, all passing
- [ ] Quality: 100% clippy pedantic compliant
- [ ] Documentation: Complete examples for all features

---

## 🔄 Integration with Existing Code

Phase 8 builds on Phase 1-7 with:

- ✅ Uses existing `EntityEvent` and `ActionConfig` types
- ✅ Extends trait-based architecture (new traits for each feature)
- ✅ Maintains `#![forbid(unsafe_code)]`
- ✅ Follows existing error handling patterns
- ✅ Integrates with existing `ObserverExecutor`
- ✅ Uses existing database connection pool

---

## 📊 Phase 8 Impact

### Before Phase 8

- ✅ Core observer system works
- ⚠️ Events lost on crash
- ⚠️ No audit trail of processing
- ⚠️ Long-running actions block
- ⚠️ No visibility into health
- ⚠️ Single point of failure

### After Phase 8

- ✅ Zero event loss (persistent checkpoints)
- ✅ Complete audit trail (Elasticsearch)
- ✅ Non-blocking long-running actions (job queue)
- ✅ Full system observability (Prometheus metrics)
- ✅ High availability (multiple listeners)
- ✅ Better performance (Redis caching)

---

## ⏱️ Timeline

| Phase | Task | Duration |
|-------|------|----------|
| 8.0 | Planning & Review | 1 day |
| 8.1 | Checkpoints | 1 day |
| 8.2 | Redis Caching | 1.5 days |
| 8.3 | Elasticsearch | 1.5 days |
| 8.4 | Job Queues | 1.5 days |
| 8.5 | Metrics | 1 day |
| 8.6 | Multiple Listeners | 1 day |
| 8.7 | Documentation | 1 day |
| 8.8 | QA & Polish | 1 day |
| **TOTAL** | **All Phase 8** | **~10 days** |

---

## 🚀 Next Steps

1. **Review this plan** with user feedback
2. **Start 8.1: Persistent Checkpoints** (highest priority for reliability)
3. **Parallelize 8.2-8.4** (caching, search, queues can be developed independently)
4. **Complete 8.5-8.8** (metrics, multi-listener, docs, QA)

---

## 📞 Questions to Clarify

1. **Priority**: Which feature is most important? (Checkpoints > Job Queues > Caching > Search > Metrics > Multi-Listener?)
2. **Dependencies**: Do we need all external systems (Redis, Elasticsearch) or prioritize some?
3. **Database**: Use same PostgreSQL or separate databases for job queue/checkpoints?
4. **Scope**: Focus on quality or breadth? (Fewer features, production-ready vs. all features, 80% ready?)

