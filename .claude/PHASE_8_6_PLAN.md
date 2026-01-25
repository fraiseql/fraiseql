# Phase 8.6: Job Queue System for Observer Actions

**Version:** 2.0 (COMPLETED)
**Status:** ✅ COMPLETE
**Date:** January 25, 2026
**Actual Effort:** 1 session (Tasks 4-8 completed in continuation)
**Previous Effort:** 3 tasks completed in prior sessions

---

## Executive Summary

Phase 8.6 adds a distributed job queue system for asynchronous action execution. This enables:
- Non-blocking action execution (fire-and-forget for expensive operations)
- Reliable retry logic with exponential backoff
- Dead letter queue for failed jobs
- Job status tracking and monitoring
- Integration with Prometheus metrics from Phase 8.7

**Key Benefit**: Transform synchronous, blocking action execution into scalable async processing.

---

## Context & Problem Statement

### Current State (Phase 8.0-8.7)
✅ Phase 8.0-8.4: Full observer system with caching, dedup, executor, actions
✅ Phase 8.7: Prometheus metrics and monitoring
❌ Action execution is synchronous (blocks event processing)
❌ No job retry for transient failures (retries happen in executor)
❌ No job status visibility after event processing
❌ No job batching or optimization

### Problem
With synchronous action execution:
- Single slow webhook blocks all event processing
- No way to defer expensive operations (batch 1000 emails, send in background)
- Failed actions are retried immediately (wasting resources)
- No observability into job status after event returns
- Can't implement rate limiting per action type

### Solution
Create a distributed job queue backed by Redis (via NATS):
```
Event Processing Flow:
┌─────────────────┐
│ Event arrives   │
└────────┬────────┘
         ↓
┌─────────────────────────────────┐
│ Matcher + Condition + Actions   │
│ (fast, in-memory)               │
└────────┬────────────────────────┘
         ↓
┌─────────────────────────────────┐
│ Queue actions to Job Queue      │  ← NEW: Phase 8.6
│ (Redis-backed via NATS)         │
└────────┬────────────────────────┘
         ↓
┌─────────────────────────────────┐
│ Return execution summary        │
│ (all actions queued)            │
└─────────────────────────────────┘
         │
         └──→ Background Job Workers
              ├─ Execute jobs from queue
              ├─ Retry on failure (exponential backoff)
              ├─ Dead letter queue for permanent failures
              └─ Emit metrics (job_executed, job_failed, job_duration)
```

---

## Architecture

### Core Components

#### 1. Job Definition
```rust
pub struct Job {
    pub id: Uuid,                    // Job ID (for tracking)
    pub event_id: Uuid,              // Link to event
    pub action: ActionConfig,        // Action to execute
    pub created_at: DateTime<Utc>,   // When queued
    pub attempt: u32,                // Current attempt number
    pub max_attempts: u32,           // Max retries
    pub backoff_strategy: BackoffStrategy,
    pub state: JobState,             // Pending, Running, Failed, Completed
    pub last_error: Option<String>,  // Error message if failed
}

pub enum JobState {
    Pending,     // Waiting to execute
    Running,     // Currently executing
    Completed,   // Successfully executed
    Failed,      // Max retries exceeded
    DeadLettered, // Moved to DLQ
}
```

#### 2. Job Queue Interface
```rust
pub trait JobQueue: Send + Sync {
    // Enqueue a job
    async fn enqueue(&self, job: Job) -> Result<()>;

    // Dequeue jobs (for workers)
    async fn dequeue(&self, batch_size: usize, timeout: Duration)
        -> Result<Vec<Job>>;

    // Acknowledge job completion
    async fn acknowledge(&self, job_id: Uuid) -> Result<()>;

    // Mark job as failed (move to DLQ)
    async fn fail(&self, job_id: Uuid, error: String) -> Result<()>;

    // Get job status
    async fn get_status(&self, job_id: Uuid) -> Result<Option<JobState>>;
}
```

#### 3. Redis Job Queue Implementation
```rust
pub struct RedisJobQueue {
    conn: redis::Client,
    queue_key: String,        // Redis list for pending jobs
    processing_key: String,   // Redis sorted set for in-progress
    dlq_key: String,          // Redis list for dead letters
}

// Uses Redis lists:
// - queue:pending  → List of pending Job JSONs
// - queue:processing → Sorted set with expiry (job timeout)
// - queue:dlq     → List of failed Job JSONs
```

#### 4. Job Executor (Worker)
```rust
pub struct JobExecutor {
    queue: Arc<dyn JobQueue>,
    action_executor: Arc<ActionExecutor>,
    metrics: MetricsRegistry,
    worker_id: String,        // For distributed workers
    concurrency: usize,       // Jobs to process in parallel
}

// Main loop:
// 1. Dequeue batch of jobs
// 2. Execute in parallel
// 3. On success: acknowledge
// 4. On failure: retry or move to DLQ
// 5. Emit metrics
```

#### 5. Queued Action Executor (Wrapper)
```rust
pub struct QueuedObserverExecutor {
    inner: Arc<ObserverExecutor>,
    job_queue: Arc<dyn JobQueue>,
    metrics: MetricsRegistry,
}

// New flow:
// 1. Process event normally (condition + matching)
// 2. Instead of executing actions: enqueue as jobs
// 3. Return immediately with summary
// 4. Jobs execute in background
```

### Integration with Existing Components

```
Phase 8.6 Job Queue System:

┌─────────────────────────────────────────────┐
│ ObserverExecutor                            │
│ ├─ Matcher                                  │
│ ├─ Condition Parser                         │
│ └─ QueuedActionExecutor ← NEW WRAPPER       │
│    ├─ Job Queue (Redis-backed) ← NEW        │
│    └─ Job Workers ← NEW                     │
└─────────────────────────────────────────────┘

Metrics Integration (Phase 8.7):
├─ job_queued_total
├─ job_executed_total
├─ job_failed_total
├─ job_duration_seconds
├─ job_queue_depth
└─ job_dlq_items
```

---

## Implementation Tasks

### Task 1: Job Definition & Types (1 day)

**File:** `crates/fraiseql-observers/src/job_queue/mod.rs` (NEW)

Implement:
- `Job` struct with all fields
- `JobState` enum
- `JobError` enum for failure reasons
- Serialization support (serde JSON for Redis)

**File:** `crates/fraiseql-observers/src/job_queue/traits.rs` (NEW)

Implement:
- `JobQueue` trait with all methods
- Mock implementation for testing

**Acceptance Criteria:**
- ✅ Job can be serialized/deserialized to JSON
- ✅ All JobState transitions are valid
- ✅ JobQueue trait is implemented
- ✅ Mock queue works for testing

---

### Task 2: Redis Job Queue Implementation (1 day)

**File:** `crates/fraiseql-observers/src/job_queue/redis.rs` (NEW)

Implement:
- `RedisJobQueue` struct
- Connection pooling
- Enqueue: Push to queue list
- Dequeue: Pop from queue, add to processing set with expiry
- Acknowledge: Remove from processing set
- Fail: Move to DLQ with error message
- Get status: Query all three stores

**File:** `crates/fraiseql-observers/src/job_queue/dlq.rs` (NEW)

Implement:
- Dead Letter Queue storage
- Job status queries
- DLQ inspection operations

**Acceptance Criteria:**
- ✅ Jobs successfully enqueued and dequeued
- ✅ Processing set expires after timeout
- ✅ DLQ stores failed jobs with errors
- ✅ Status queries return correct states
- ✅ Tests use in-memory DashMap for speed

---

### Task 3: Job Executor/Worker (1 day)

**File:** `crates/fraiseql-observers/src/job_queue/executor.rs` (NEW)

Implement:
- `JobExecutor` struct
- Main worker loop (run() method)
- Parallel execution of job batches
- Backoff calculation for retries
- Metrics emission (job_executed, job_failed, job_duration)

**File:** `crates/fraiseql-observers/src/job_queue/backoff.rs` (NEW)

Implement:
- Backoff calculation (linear, exponential, random)
- Follows same BackoffStrategy from executor.rs

**Acceptance Criteria:**
- ✅ Worker successfully dequeues and executes jobs
- ✅ Retries with exponential backoff on failure
- ✅ Moves to DLQ after max attempts
- ✅ Metrics recorded for job lifecycle
- ✅ Worker handles graceful shutdown

---

### Task 4: Queued Executor Wrapper (1 day)

**File:** `crates/fraiseql-observers/src/queued_executor.rs` (NEW)

Implement:
- `QueuedObserverExecutor` wrapper
- Constructor that takes inner executor + job queue
- New process_event() that:
  - Runs matching and condition evaluation (fast)
  - Queues actions instead of executing (fire-and-forget)
  - Returns summary with job IDs
- Preserve all metrics from executor.rs

**Acceptance Criteria:**
- ✅ Actions are queued, not executed immediately
- ✅ Event processing returns quickly
- ✅ Metrics still recorded (via job executor)
- ✅ Job IDs returned in execution summary
- ✅ Backward compatible with existing executor interface

---

### Task 5: Integration & Configuration (1 day)

**File:** `crates/fraiseql-observers/src/factory.rs` (MODIFY)

Add:
- Optional queued execution mode
- Configuration option to enable job queue
- Factory method to build with queued executor

**File:** `crates/fraiseql-observers/src/config.rs` (MODIFY)

Add:
- JobQueueConfig struct
- Redis connection parameters
- Worker configuration (concurrency, batch size)
- Feature flag: `job_queue`

**File:** `Cargo.toml` (MODIFY)

Add:
- Optional redis dependency (if not already present)
- redis feature flag

**Acceptance Criteria:**
- ✅ Configuration loads from environment
- ✅ Feature flag enables/disables gracefully
- ✅ Factory creates correct executor type
- ✅ Workers can be spawned independently
- ✅ All tests pass with/without feature

---

### Task 6: Integration with Metrics (½ day)

**File:** `crates/fraiseql-observers/src/metrics/registry.rs` (MODIFY)

Add metrics:
- `job_queued_total` (counter)
- `job_executed_total` (counter with action_type label)
- `job_failed_total` (counter with action_type, error_type labels)
- `job_duration_seconds` (histogram with action_type label)
- `job_queue_depth` (gauge)
- `job_dlq_items` (gauge)
- `job_retry_attempts` (counter with action_type label)

**File:** `crates/fraiseql-observers/src/job_queue/executor.rs` (MODIFY)

Instrument:
- Record metrics when jobs are executed
- Track retry attempts
- Monitor DLQ growth

**Acceptance Criteria:**
- ✅ All job metrics recorded
- ✅ Queued executor records job_queued_total
- ✅ Worker records job_executed_total and duration
- ✅ Failures tracked with error_type labels
- ✅ DLQ items monitored

---

### Task 7: Documentation & Examples (½ day)

**File:** `docs/monitoring/PHASE_8_6_JOB_QUEUE.md` (NEW)

Document:
- Architecture overview
- Configuration examples
- Running workers
- Monitoring queries
- Troubleshooting
- Performance tuning

**File:** `examples/job_queue_example.rs` (NEW)

Example showing:
- Setting up job queue
- Running worker
- Processing jobs
- Monitoring

**Acceptance Criteria:**
- ✅ Comprehensive architecture documentation
- ✅ Configuration examples
- ✅ Monitoring/alerting queries
- ✅ Runnable example code

---

### Task 8: Tests & Integration Testing (Optional)

**Files:** `tests/job_queue_integration.rs` (NEW)

Test:
- End-to-end job queueing and execution
- Retry logic with backoff
- DLQ handling
- Metrics collection
- Multi-worker scenarios

**Acceptance Criteria:**
- ✅ Jobs successfully queued and executed
- ✅ Retries work correctly
- ✅ DLQ receives permanent failures
- ✅ Metrics collected accurately
- ✅ Multiple workers can process same queue

---

## Files to Create

```
crates/fraiseql-observers/src/
├── job_queue/                    (NEW DIRECTORY)
│   ├── mod.rs                    (Public API, Job struct)
│   ├── traits.rs                 (JobQueue trait, errors)
│   ├── redis.rs                  (RedisJobQueue implementation)
│   ├── dlq.rs                    (Dead letter queue)
│   ├── executor.rs               (JobExecutor worker)
│   └── backoff.rs                (Retry backoff logic)
│
└── queued_executor.rs            (NEW - QueuedObserverExecutor wrapper)

docs/monitoring/
└── PHASE_8_6_JOB_QUEUE.md        (NEW - Documentation)

examples/
└── job_queue_example.rs          (NEW - Runnable example)

tests/
└── job_queue_integration.rs      (NEW - Integration tests)
```

---

## Files to Modify

```
crates/fraiseql-observers/src/
├── lib.rs                        (Add job_queue module)
├── config.rs                     (Add JobQueueConfig)
├── factory.rs                    (Add queued executor creation)
└── metrics/registry.rs           (Add job queue metrics)

Cargo.toml
├── Add redis dependency (if not present)
├── Add job_queue feature flag
```

---

## Feature Flags

```toml
# In Cargo.toml
[features]
job_queue = ["redis"]
phase8 = ["checkpoint", "dedup", "caching", "queue", "search", "metrics", "job_queue"]
```

When `job_queue` feature is disabled:
- `QueuedObserverExecutor` not available
- `JobQueue` implementations are no-ops
- No Redis dependency needed

---

## Architecture Decisions

### Why Redis + NATS?
- Redis: Fast, proven, already optional dependency in Phase 8
- NATS: Reliable delivery, multi-worker support, integrates with existing transport

### Why separate workers?
- Non-blocking event processing
- Horizontal scalability (run N workers)
- Better resource utilization
- Can upgrade/restart workers without stopping event processing

### Why job IDs?
- Track job status after event returns
- Allow client polling for completion
- Enable job replay/inspection

### Why DLQ instead of auto-delete?
- Retain failed jobs for investigation
- Allow manual retry
- Monitor for systemic failures

---

## Integration with Phase 8.7 Metrics

New metrics dashboard panels (add to grafana-dashboard-8.7.json):
- Job queue depth over time
- Job execution rate by action type
- Job retry frequency
- DLQ accumulation rate
- Job execution duration percentiles

PromQL queries:
```promql
# Jobs per second
rate(fraiseql_observer_job_executed_total[1m])

# Job failure rate by action
sum(rate(fraiseql_observer_job_failed_total[5m])) by (action_type)

# Queue depth alert
fraiseql_observer_job_queue_depth > 5000

# DLQ alert
fraiseql_observer_job_dlq_items > 100
```

---

## Hybrid Implementation Strategy

### Phase 1 (Claude - Architecture & Examples):
1. Design job queue architecture
2. Implement Job struct and JobQueue trait
3. Implement RedisJobQueue (core logic)
4. Implement JobExecutor (worker loop)
5. Create QueuedObserverExecutor wrapper
6. Implement metrics integration

### Phase 2 (Local Model - Pattern Application):
1. Apply backoff retry pattern to JobExecutor
2. Generate integration tests
3. Apply metrics recording patterns
4. Generate configuration examples

### Phase 3 (Claude - Verification):
1. Review all code
2. Run full test suite
3. Verify metrics are recorded correctly
4. Final integration and documentation

---

## Success Metrics for Phase 8.6

After completion:
- ✅ Actions execute asynchronously via job queue
- ✅ Event processing returns in <100ms (no action wait)
- ✅ Jobs automatically retry on transient failures
- ✅ Failed jobs move to DLQ for inspection
- ✅ Prometheus metrics track job lifecycle
- ✅ Multiple workers can process same queue
- ✅ Zero data loss (jobs persisted in Redis)
- ✅ Dashboard shows job queue health

---

## Dependencies

Required:
- redis crate (optional, only with job_queue feature)
- existing deps: tokio, serde, uuid, chrono

---

## Acceptance Criteria (Complete)

### Implementation Completeness
- ✅ Job struct with all fields and serialization
- ✅ JobQueue trait with all operations
- ✅ RedisJobQueue implementation
- ✅ JobExecutor worker with retry logic
- ✅ QueuedObserverExecutor wrapper
- ✅ Backoff calculations (linear, exponential, random)
- ✅ Dead letter queue handling
- ✅ Feature flag working (with/without job_queue)

### Metrics & Monitoring
- ✅ Job metrics integrated with Phase 8.7
- ✅ Dashboard panels for job queue
- ✅ PromQL alert examples
- ✅ Worker can emit metrics

### Code Quality
- ✅ No clippy warnings
- ✅ Comprehensive doc comments
- ✅ Thread-safe implementations
- ✅ Zero unsafe code
- ✅ Feature-gated properly

### Testing
- ✅ Unit tests for Job struct
- ✅ Unit tests for JobQueue trait
- ✅ Integration tests for Redis queue
- ✅ Tests for retry/backoff logic
- ✅ Tests for DLQ handling
- ✅ All existing tests still pass

### Documentation
- ✅ Architecture documentation
- ✅ Configuration examples
- ✅ Monitoring queries
- ✅ Troubleshooting guide
- ✅ Runnable example code

---

## DO NOT / Guardrails

❌ **DO NOT** block event processing on job queue failures
- Job enqueueing must be fast or fail gracefully
- Fall back to immediate execution if queue unavailable

❌ **DO NOT** lose jobs
- Use Redis persistence
- Ensure acknowledgment before deletion

❌ **DO NOT** retry forever
- Max attempts limit (default: 5)
- Move to DLQ after max retries

❌ **DO NOT** assume single worker
- Design for multiple workers on same queue
- Use locking for job state transitions

❌ **DO NOT** block event processing on metrics
- Metrics recording is asynchronous
- Failures don't affect job processing

---

## Next Steps After Phase 8.6

After job queue completion:
1. **Phase 8.5**: Elasticsearch Integration (uses job queue for indexing)
2. **Phase 8.8**: Circuit breaker resilience for actions
3. **Phase 8.9**: Multi-database support
4. **Phase 8.10**: Performance optimization

---

**Status:** Ready for implementation with hybrid approach (Claude + Local Model)
**Estimated Timeline:** 3-4 days total

