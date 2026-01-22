# Phase 8.6: Job Queue System - Implementation Plan

**Date**: January 22, 2026
**Objective**: Enable async long-running action processing with worker pools and resilient retries
**Target**: 140+ tests passing (127 + 13 new), production-ready job queue

## Problem Statement

**Without Job Queue**:
- Long-running actions block event processing
- No way to process actions with long timeouts
- Failed actions don't retry automatically
- Worker capacity unconstrained

**With Job Queue**:
- Actions enqueued asynchronously
- Worker pool processes jobs concurrently
- Exponential backoff retries (configurable)
- Bounded worker pools prevent resource exhaustion

## Architecture Overview

### Components

```
EntityEvent
    ↓
Observer matched
    ↓
Action execution
    ├─ Quick actions (webhook, email) → inline
    └─ Slow actions (ML inference, batch processing) → Job Queue
        ↓
    JobQueue (Redis-backed, durable)
        ↓
    Job fetched by Worker
        ↓
    Job executed with retry logic
    ├─ Attempt 1 (immediate)
    ├─ Attempt 2 (5 second backoff)
    ├─ Attempt 3 (25 second backoff)
    └─ Attempt N (exponential capped at max_delay)
        ↓
    Success → Complete
    Failure → Dead Letter Queue (manual retry)
```

### Three Trait Abstractions

**1. JobQueue Trait**
- Abstract job persistence and retrieval
- Redis implementation (RedisPersistentJobQueue)
- Support for multiple workers

**2. JobWorker Trait**
- Worker pool implementation
- Handles job execution and retries
- Per-job timeout support

**3. JobRetryPolicy Trait**
- Configurable retry strategies
- Exponential backoff
- Maximum attempt limits

## Implementation Steps

### Step 1: Job Data Structures (40 lines)
**File**: `src/queue/mod.rs`

Create fundamental types:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub action_id: String,
    pub event: EntityEvent,
    pub action_config: ActionConfig,
    pub attempt: u32,
    pub created_at: i64,
    pub next_retry_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Processing,
    Success,
    Failed,
    Retrying,
    Deadletter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: String,
    pub status: JobStatus,
    pub action_result: ActionResult,
    pub attempts: u32,
    pub duration_ms: f64,
}
```

Tests (3):
- test_job_creation
- test_job_status_transitions
- test_job_result_serialization

### Step 2: JobQueue Trait (100 lines)
**File**: `src/queue/mod.rs`

Abstract queue interface:
```rust
#[async_trait::async_trait]
pub trait JobQueue: Send + Sync + Clone {
    // Enqueue a new job
    async fn enqueue(&self, job: &Job) -> Result<String>;

    // Get next job for processing
    async fn dequeue(&self, worker_id: &str) -> Result<Option<Job>>;

    // Mark job as processing
    async fn mark_processing(&self, job_id: &str) -> Result<()>;

    // Mark job as success
    async fn mark_success(&self, job_id: &str, result: &JobResult) -> Result<()>;

    // Mark job for retry
    async fn mark_retry(&self, job_id: &str, next_retry_at: i64) -> Result<()>;

    // Move job to dead letter queue
    async fn mark_deadletter(&self, job_id: &str, reason: &str) -> Result<()>;

    // Get queue statistics
    async fn get_stats(&self) -> Result<QueueStats>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub pending_jobs: u64,
    pub processing_jobs: u64,
    pub retry_jobs: u64,
    pub successful_jobs: u64,
    pub failed_jobs: u64,
    pub avg_processing_time_ms: f64,
}
```

Tests (4):
- test_enqueue_job
- test_dequeue_order
- test_mark_success
- test_stats_calculation

### Step 3: Redis JobQueue Implementation (250 lines)
**File**: `src/queue/redis.rs`

Redis-backed persistent queue:
```rust
#[derive(Clone)]
pub struct RedisJobQueue {
    conn: ConnectionManager,
    pending_key: String,
    processing_key: String,
    retry_key: String,
    deadletter_key: String,
    completed_key: String,
}

impl RedisJobQueue {
    pub fn new(conn: ConnectionManager) -> Self { ... }

    // Helper to generate job key
    fn job_key(job_id: &str) -> String { ... }

    // Atomically dequeue and mark processing
    async fn dequeue_atomic(&self, worker_id: &str) -> Result<Option<Job>> { ... }
}
```

Key operations:
- Use Redis BLMOVE for atomic dequeue+move to processing
- Store job metadata in Redis hashes
- Sorted sets for retry scheduling by timestamp
- Use Lua scripts for atomic operations

Tests (6):
- test_redis_enqueue
- test_redis_dequeue
- test_redis_mark_processing
- test_redis_mark_success
- test_redis_mark_retry
- test_redis_deadletter

### Step 4: Retry Policy Trait (60 lines)
**File**: `src/queue/mod.rs`

Configurable retry strategies:
```rust
pub trait RetryPolicy: Send + Sync + Clone {
    fn should_retry(&self, attempt: u32) -> bool;
    fn get_backoff_ms(&self, attempt: u32) -> u64;
}

#[derive(Debug, Clone)]
pub struct ExponentialBackoffPolicy {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub multiplier: f64,
}

impl RetryPolicy for ExponentialBackoffPolicy {
    fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }

    fn get_backoff_ms(&self, attempt: u32) -> u64 {
        // Exponential: initial * multiplier^attempt, capped at max_delay
        let delay = (self.initial_delay_ms as f64 *
                    self.multiplier.powi(attempt as i32)) as u64;
        delay.min(self.max_delay_ms)
    }
}

#[derive(Debug, Clone)]
pub struct LinearBackoffPolicy {
    pub max_attempts: u32,
    pub delay_increment_ms: u64,
}

#[derive(Debug, Clone)]
pub struct FixedBackoffPolicy {
    pub max_attempts: u32,
    pub delay_ms: u64,
}
```

Tests (5):
- test_exponential_backoff_calculation
- test_exponential_backoff_cap
- test_linear_backoff
- test_fixed_backoff
- test_no_retry_after_max_attempts

### Step 5: Worker Pool (200 lines)
**File**: `src/queue/worker.rs`

Concurrent job processing:
```rust
pub struct JobWorker<Q, E>
where
    Q: JobQueue,
    E: ActionExecutor,
{
    queue: Q,
    executor: E,
    worker_id: String,
    concurrency: usize,
    retry_policy: Box<dyn RetryPolicy>,
    job_timeout_ms: u64,
}

impl<Q, E> JobWorker<Q, E>
where
    Q: JobQueue,
    E: ActionExecutor,
{
    pub fn new(queue: Q, executor: E, concurrency: usize) -> Self { ... }

    pub async fn run(&self) -> Result<()> {
        // Main worker loop: continuously fetch and process jobs
        loop {
            match self.queue.dequeue(&self.worker_id).await {
                Ok(Some(job)) => self.process_job(job).await?,
                Ok(None) => tokio::time::sleep(Duration::from_millis(100)).await,
                Err(e) => {
                    error!("Dequeue error: {}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    async fn process_job(&self, job: Job) -> Result<()> {
        // Mark as processing
        self.queue.mark_processing(&job.id).await?;

        // Execute with timeout
        let result = timeout(
            Duration::from_millis(self.job_timeout_ms),
            self.executor.execute(&job.event, &job.action_config)
        ).await;

        match result {
            Ok(Ok(action_result)) => {
                let job_result = JobResult {
                    job_id: job.id.clone(),
                    status: JobStatus::Success,
                    action_result,
                    attempts: job.attempt,
                    duration_ms: start.elapsed().as_secs_f64() * 1000.0,
                };
                self.queue.mark_success(&job.id, &job_result).await?;
            }
            Ok(Err(e)) => self.handle_job_failure(&job, &e).await?,
            Err(_) => self.handle_job_timeout(&job).await?,
        }

        Ok(())
    }

    async fn handle_job_failure(&self, job: &Job, error: &ObserverError) -> Result<()> {
        if self.retry_policy.should_retry(job.attempt + 1) {
            let backoff_ms = self.retry_policy.get_backoff_ms(job.attempt + 1);
            let next_retry_at = chrono::Utc::now().timestamp() + (backoff_ms as i64 / 1000);

            let mut retry_job = job.clone();
            retry_job.attempt += 1;
            retry_job.next_retry_at = next_retry_at;

            self.queue.mark_retry(&job.id, next_retry_at).await?;
        } else {
            self.queue.mark_deadletter(&job.id, &error.to_string()).await?;
        }

        Ok(())
    }

    async fn handle_job_timeout(&self, job: &Job) -> Result<()> {
        self.handle_job_failure(job, &ObserverError::ActionExecutionFailed {
            reason: format!("Job timeout after {}ms", self.job_timeout_ms)
        }).await
    }
}
```

Tests (8):
- test_worker_processes_job
- test_worker_handles_success
- test_worker_handles_failure_with_retry
- test_worker_handles_timeout
- test_worker_deadletter_after_max_attempts
- test_worker_concurrent_processing
- test_worker_dequeue_order
- test_worker_error_recovery

### Step 6: JobWorkerPool Manager (150 lines)
**File**: `src/queue/worker.rs`

Manage multiple worker instances:
```rust
pub struct JobWorkerPool<Q, E>
where
    Q: JobQueue,
    E: ActionExecutor,
{
    workers: Vec<JobHandle>,
    pool_size: usize,
    is_running: Arc<AtomicBool>,
}

impl<Q, E> JobWorkerPool<Q, E>
where
    Q: JobQueue,
    E: ActionExecutor + 'static,
{
    pub fn new(
        queue: Q,
        executor: E,
        pool_size: usize,
        concurrency_per_worker: usize,
    ) -> Self { ... }

    pub async fn start(&mut self) -> Result<()> {
        self.is_running.store(true, Ordering::SeqCst);

        for i in 0..self.pool_size {
            let worker = JobWorker::new(
                self.queue.clone(),
                self.executor.clone(),
                self.concurrency_per_worker,
            );

            let handle = tokio::spawn(async move {
                if let Err(e) = worker.run().await {
                    error!("Worker {} error: {}", i, e);
                }
            });

            self.workers.push(handle);
        }

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.is_running.store(false, Ordering::SeqCst);

        // Wait for all workers to finish current jobs
        for handle in &self.workers {
            let _ = handle.await;
        }

        Ok(())
    }
}
```

Tests (4):
- test_pool_starts_workers
- test_pool_processes_jobs
- test_pool_stops_gracefully
- test_pool_stats

### Step 7: Action Integration (80 lines)
**File**: `src/executor.rs`

Integrate queue with observer executor:
```rust
pub struct ObserverExecutor {
    // ... existing fields
    pub job_queue: Option<Arc<dyn JobQueue>>,
    pub queue_enabled_actions: HashSet<String>,  // e.g., ["batch_email", "ml_inference"]
}

impl ObserverExecutor {
    pub async fn execute_action(
        &self,
        event: &EntityEvent,
        action: &ActionConfig,
    ) -> Result<ActionResult> {
        let action_name = action.action_type();

        // Check if this action should be queued
        if self.queue_enabled_actions.contains(&action_name) {
            if let Some(queue) = &self.job_queue {
                let job = Job {
                    id: format!("job-{}", uuid::Uuid::new_v4()),
                    action_id: action_name.clone(),
                    event: event.clone(),
                    action_config: action.clone(),
                    attempt: 1,
                    created_at: chrono::Utc::now().timestamp(),
                    next_retry_at: chrono::Utc::now().timestamp(),
                };

                let job_id = queue.enqueue(&job).await?;

                return Ok(ActionResult {
                    action_type: action_name,
                    success: true,
                    message: format!("Job queued: {}", job_id),
                    duration_ms: 1.0,
                });
            }
        }

        // Execute inline (existing behavior)
        self.execute_inline(event, action).await
    }
}
```

Tests (3):
- test_quick_action_inline
- test_slow_action_queued
- test_queue_disabled_inline

### Step 8: Tests & Integration (150 lines)
**File**: `src/queue/tests.rs`

Comprehensive test suite:
- Job lifecycle tests (create → queue → process → success/retry/deadletter)
- Worker pool tests (start, process, stop)
- Retry policy tests (exponential, linear, fixed backoff)
- Error handling tests (timeout, network errors, executor failures)
- Integration tests (queue + executor + worker together)

Test coverage targets:
- Job creation and serialization: 3 tests
- Queue operations: 6 tests
- Retry policies: 5 tests
- Worker processing: 8 tests
- Worker pool management: 4 tests
- Integration: 3 tests
- Error handling: 4 tests

**Total: 33 new tests** → 127 + 33 = 160 tests passing

## Dependencies Required

Check if already in Cargo.toml:
- `uuid` ✅ (already used for event IDs)
- `chrono` ✅ (already used for timestamps)
- `tokio` ✅ (already used for async)
- `redis` ✅ (already used for dedup/cache)
- `futures` ✅ (already used for concurrent executor)
- `async_trait` ✅ (already used for traits)
- `serde_json` ✅ (already used for serialization)

**No new dependencies needed!**

## File Structure

```
src/queue/
├── mod.rs           (350 lines: Job, JobStatus, JobQueue trait, RetryPolicy traits)
├── redis.rs         (250 lines: RedisJobQueue implementation)
├── worker.rs        (350 lines: JobWorker, JobWorkerPool)
└── tests.rs         (150 lines: Comprehensive test suite)

Total: ~1,100 lines of new code
```

Module exports in `src/lib.rs`:
```rust
pub mod queue;

pub use queue::{
    Job, JobStatus, JobQueue, JobResult, QueueStats,
    RetryPolicy, ExponentialBackoffPolicy, LinearBackoffPolicy, FixedBackoffPolicy,
    JobWorker, JobWorkerPool,
};

#[cfg(feature = "queue")]
pub use queue::redis::RedisJobQueue;
```

Feature flag in `Cargo.toml`:
```toml
[features]
queue = []  # Job queue system (no dependencies)
phase8 = ["checkpoint", "dedup", "caching", "search", "queue", "metrics"]
```

## Success Criteria

✅ **Functional**:
- [ ] Job enqueueing and dequeueing works
- [ ] Retry policies calculate backoff correctly
- [ ] Worker pool processes jobs concurrently
- [ ] Dead letter queue captures failed jobs

✅ **Quality**:
- [ ] 160+ tests passing (33 new)
- [ ] 100% Clippy compliant
- [ ] Zero unsafe code
- [ ] All error paths tested

✅ **Performance**:
- [ ] Job enqueue < 5ms
- [ ] Job dequeue < 5ms
- [ ] Worker processes 100+ jobs/sec
- [ ] Minimal memory overhead per job

✅ **Reliability**:
- [ ] Retries work with exponential backoff
- [ ] Dead letter queue for unrecoverable failures
- [ ] Worker graceful shutdown
- [ ] No job loss on worker restart

## Testing Strategy

1. **Unit tests**: Each component (Job, Queue, RetryPolicy, Worker)
2. **Integration tests**: Queue + Executor + Worker together
3. **Chaos tests**: Simulate failures (network errors, timeouts, executor crashes)
4. **Performance tests**: Track job throughput and latency

## Estimated Time

- Job structures: 30 min
- Queue trait: 40 min
- Redis implementation: 60 min
- Retry policies: 40 min
- Worker pool: 60 min
- Integration: 30 min
- Tests: 90 min
- **Total: ~5 hours**

## Phase 8 Progress After Completion

```
Phase 8.6: Job Queue System ✅ Complete
Total Progress: 53.8% (7 of 13 subphases)
```

Ready for Phase 8.7: Prometheus Metrics 🚀
