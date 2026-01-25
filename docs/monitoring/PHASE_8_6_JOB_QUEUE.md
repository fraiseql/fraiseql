# Phase 8.6: Asynchronous Job Queue System

## Overview

Phase 8.6 introduces a **Redis-backed distributed job queue** for asynchronous action execution. Instead of blocking event processing on expensive operations like webhooks or emails, actions are queued and executed by background workers.

### Problem Solved

**Without job queue (synchronous execution)**:
```
Event arrives → Matcher → Conditions → Execute webhook (blocks!) → Return to client
                                             (5 seconds)
```

**With job queue (async execution)**:
```
Event arrives → Matcher → Conditions → Queue actions (fast!) → Return immediately
                                            (<10ms)
                                                ↓
                                        Background worker
                                        executes webhook
                                        (5 seconds, doesn't block)
```

### Key Benefits

- **Non-blocking event processing**: Returns in <10ms regardless of action complexity
- **Reliable retry logic**: Exponential, linear, or fixed backoff strategies
- **Dead letter queue**: Permanently failed jobs retained for inspection
- **Distributed**: Multiple workers can process same queue
- **Observable**: Full Prometheus metrics integration
- **Horizontally scalable**: Spawn workers across multiple machines

---

## Architecture

### Data Flow

```
┌─────────────────────────┐
│   Event Processing      │
│  (ObserverExecutor)     │
└────────────┬────────────┘
             │
             ├─ Match observers
             ├─ Evaluate conditions
             │
             └─ Queue Actions
                     ↓
        ┌────────────────────────┐
        │  QueuedObserverExecutor │
        │                         │
        │  For each action:       │
        │  - Create Job           │
        │  - job.enqueue()        │
        │  - Record metric        │
        │                         │
        │  RETURN IMMEDIATELY ✓   │
        └────────────┬────────────┘
                     │
                     ↓
        ┌────────────────────────┐
        │  Redis Job Queue       │
        │  (queue:pending)       │
        │                         │
        │  Persistent storage    │
        │  with acknowledgment   │
        └────────────┬────────────┘
                     │
                     ↓ (Workers dequeue)
        ┌────────────────────────┐
        │  JobExecutor Workers   │
        │  (one per machine)     │
        │                         │
        │  1. Dequeue batch      │
        │  2. Execute jobs       │
        │  3. Handle retries     │
        │  4. Record metrics     │
        │                         │
        │  On failure:           │
        │  ├─ Transient → retry  │
        │  │  (exponential wait) │
        │  └─ Permanent → DLQ    │
        └────────────────────────┘
```

### Components

#### 1. Job (Data Structure)
```rust
pub struct Job {
    pub id: Uuid,                      // Unique identifier
    pub event_id: Uuid,                // Link to originating event
    pub action: ActionConfig,          // Action to execute
    pub attempt: u32,                  // Current attempt (1-based)
    pub max_attempts: u32,             // Max retries
    pub backoff_strategy: BackoffStrategy,
    pub state: JobState,               // Pending, Running, Completed, Failed
    pub last_error: Option<String>,    // Error from last attempt
    pub retry_at: Option<DateTime>,    // When to retry (for scheduling)
}
```

#### 2. JobQueue Trait
```rust
pub trait JobQueue: Send + Sync {
    async fn enqueue(&self, job: Job) -> Result<()>;
    async fn dequeue(&self, batch_size: usize, timeout: u64) -> Result<Vec<Job>>;
    async fn acknowledge(&self, job_id: Uuid) -> Result<()>;
    async fn fail(&self, job: &mut Job, error: String) -> Result<()>;
    async fn get_status(&self, job_id: Uuid) -> Result<Option<JobState>>;
}
```

#### 3. QueuedObserverExecutor
Wraps `ObserverExecutor` to queue actions instead of executing them:
- Evaluates conditions (fast, in-memory)
- Creates Job for each action
- Enqueues to Redis
- Returns job IDs immediately

#### 4. JobExecutor Worker
Background worker that:
- Dequeues batches of jobs
- Executes in parallel with configurable concurrency
- Handles transient failures with backoff
- Moves permanent failures to DLQ
- Records metrics

---

## Configuration

### TOML Example

```toml
# Enable job queue feature at compile time
[features]
job_queue = true

# Runtime configuration
[job_queue]
url = "redis://localhost:6379"
batch_size = 100
batch_timeout_secs = 5
max_retries = 5
worker_concurrency = 10
poll_interval_ms = 1000
initial_delay_ms = 100
max_delay_ms = 30000
```

### Environment Variables

All configuration fields can be overridden via environment variables:

```bash
# Job queue backend
export FRAISEQL_JOB_QUEUE_URL="redis://redis.example.com:6379"

# Batch settings
export FRAISEQL_JOB_QUEUE_BATCH_SIZE=100
export FRAISEQL_JOB_QUEUE_BATCH_TIMEOUT_SECS=5

# Retry settings
export FRAISEQL_JOB_QUEUE_MAX_RETRIES=5
export FRAISEQL_JOB_QUEUE_INITIAL_DELAY_MS=100
export FRAISEQL_JOB_QUEUE_MAX_DELAY_MS=30000

# Worker settings
export FRAISEQL_JOB_QUEUE_WORKER_CONCURRENCY=10
export FRAISEQL_JOB_QUEUE_POLL_INTERVAL_MS=1000
```

### Configuration Fields

| Field | Default | Description |
|-------|---------|-------------|
| `url` | `redis://localhost:6379` | Redis connection URL |
| `batch_size` | `100` | Jobs to fetch per dequeue |
| `batch_timeout_secs` | `5` | Max wait before flushing partial batch |
| `max_retries` | `5` | Maximum retry attempts per job |
| `worker_concurrency` | `10` | Parallel jobs per worker |
| `poll_interval_ms` | `1000` | Sleep when queue empty (ms) |
| `initial_delay_ms` | `100` | Starting backoff delay (ms) |
| `max_delay_ms` | `30000` | Maximum backoff delay (ms) |

---

## Running Workers

### Single Worker (Development)

```rust
use fraiseql_observers::{
    config::ObserverRuntimeConfig,
    factory::ExecutorFactory,
    job_queue::executor::JobExecutor,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Load config
    let config = ObserverRuntimeConfig::load_from_file("config.toml")?
        .with_env_overrides();

    // Get job queue config
    let job_queue_config = config.job_queue.as_ref()
        .ok_or("job_queue config required")?;

    // Build job queue
    let job_queue = ExecutorFactory::build_job_queue(job_queue_config).await?;

    // Create executor (for running actions)
    let executor = Arc::new(ObserverExecutor::new(matcher, dlq));

    // Create and run worker
    let worker = JobExecutor::new(
        job_queue,
        executor,
        job_queue_config.worker_concurrency,
        job_queue_config.batch_size,
        60, // 60 second timeout per job
    );

    // Block forever (or until SIGTERM)
    worker.run().await?;
    Ok(())
}
```

### Multiple Workers (Production)

Use a process manager like Supervisor or systemd:

```ini
# /etc/supervisor/conf.d/fraiseql-job-worker.conf
[program:fraiseql-job-worker-1]
command=/usr/local/bin/fraiseql-job-worker
process_name=%(program_name)s-%(process_num)02d
numprocs=4
autostart=true
autorestart=true
environment=RUST_LOG=info,FRAISEQL_LOG_LEVEL=info
redirect_stderr=true
stdout_logfile=/var/log/fraiseql/job-worker.log
```

Or via Docker:

```dockerfile
# Dockerfile.worker
FROM rust:latest as builder
# ... build fraiseql-job-worker binary

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/fraiseql-job-worker /usr/local/bin/

ENTRYPOINT ["fraiseql-job-worker"]
```

```yaml
# docker-compose.yml
services:
  job-worker-1:
    build:
      context: .
      dockerfile: Dockerfile.worker
    environment:
      FRAISEQL_JOB_QUEUE_URL: redis://redis:6379
      FRAISEQL_JOB_QUEUE_WORKER_CONCURRENCY: 10
    depends_on:
      - redis
    restart: always

  job-worker-2:
    # Same as job-worker-1
    restart: always
```

---

## Monitoring & Observability

### Prometheus Metrics

All job queue metrics are automatically recorded when `metrics` feature is enabled.

#### Job Queuing

```promql
# Jobs queued per second
rate(fraiseql_observer_job_queued_total[1m])

# Total jobs queued
fraiseql_observer_job_queued_total
```

#### Job Execution

```promql
# Jobs executed per second by action type
sum(rate(fraiseql_observer_job_executed_total[1m])) by (action_type)

# 95th percentile execution time
histogram_quantile(0.95,
  sum(rate(fraiseql_observer_job_duration_seconds_bucket[5m])) by (le, action_type)
)

# Job success rate
(
  sum(rate(fraiseql_observer_job_executed_total[5m])) by (action_type)
) / (
  sum(rate(fraiseql_observer_job_executed_total[5m])) by (action_type) +
  sum(rate(fraiseql_observer_job_failed_total[5m])) by (action_type)
)
```

#### Job Failures

```promql
# Job failure rate by action type and error
sum(rate(fraiseql_observer_job_failed_total[5m])) by (action_type, error_type)

# Permanent failures (not retrying)
fraiseql_observer_job_failed_total{error_type="permanent_error"}

# Retries exhausted
fraiseql_observer_job_failed_total{error_type="retries_exhausted"}
```

#### Retry Tracking

```promql
# Retry attempts per second
rate(fraiseql_observer_job_retry_attempts_total[1m])

# Retry rate per action type
sum(rate(fraiseql_observer_job_retry_attempts_total[5m])) by (action_type)
```

#### Queue Health

```promql
# Current queue depth (jobs waiting)
fraiseql_observer_job_queue_depth

# Dead letter queue growth
rate(fraiseql_observer_job_dlq_items[5m])

# Alert if queue is backing up
fraiseql_observer_job_queue_depth > 1000

# Alert if DLQ is growing
rate(fraiseql_observer_job_dlq_items[1m]) > 0.1
```

### Grafana Dashboard

Create a Grafana dashboard with these panels:

1. **Job Throughput** (line chart)
   - Query: `rate(fraiseql_observer_job_executed_total[1m])`
   - Group by: action_type

2. **Execution Time** (heat map)
   - Query: `fraiseql_observer_job_duration_seconds_bucket`
   - Group by: action_type

3. **Success Rate** (gauge)
   - Query: Success rate PromQL query above

4. **Failure Rate** (bar chart)
   - Query: `rate(fraiseql_observer_job_failed_total[5m])`
   - Group by: action_type, error_type

5. **Queue Depth** (gauge)
   - Query: `fraiseql_observer_job_queue_depth`

6. **DLQ Items** (gauge)
   - Query: `fraiseql_observer_job_dlq_items`

7. **Retry Frequency** (line chart)
   - Query: `rate(fraiseql_observer_job_retry_attempts_total[5m])`
   - Group by: action_type

---

## Troubleshooting

### Queue Not Processing

**Symptom**: Jobs queue up but never execute.

**Diagnosis**:
```bash
# Check Redis connectivity
redis-cli -u redis://localhost:6379 PING

# Check queue depth
redis-cli -u redis://localhost:6379 LLEN queue:pending

# Check worker logs
tail -f /var/log/fraiseql/job-worker.log
```

**Fix**:
1. Ensure workers are running: `ps aux | grep fraiseql-job-worker`
2. Check Redis is accessible: `redis-cli PING`
3. Check worker configuration matches queue config
4. Restart workers: `systemctl restart fraiseql-job-worker`

### Jobs Keep Retrying

**Symptom**: Jobs retry endlessly, queue fills up.

**Diagnosis**:
```bash
# Check if error is transient or permanent
# Look at job_failed_total metric:
# - permanent_error = should move to DLQ
# - retries_exhausted = max retries reached

# View DLQ jobs:
redis-cli -u redis://localhost:6379 LLEN queue:dlq
redis-cli -u redis://localhost:6379 LRANGE queue:dlq 0 -1
```

**Fix**:
1. Identify root cause (network, auth, service down, etc.)
2. For transient errors:
   - Service issue? Fix underlying service
   - Network latency? Increase `batch_timeout_secs`
   - Rate limiting? Reduce `worker_concurrency`
3. For permanent errors:
   - Configuration issue? Fix observer/action config
   - Invalid data? Fix event data
   - Jobs remain in DLQ for manual retry

### Worker Running But Not Executing

**Symptom**: Worker process alive, but no jobs processed, metrics at zero.

**Diagnosis**:
```bash
# Check if worker has jobs to process
redis-cli -u redis://localhost:6379 LLEN queue:pending

# Check worker logs for errors
tail -100 /var/log/fraiseql/job-worker.log | grep -i error

# Check if executor is accessible
# (network issues, permission errors, etc.)
```

**Fix**:
1. Check observer executor is configured correctly
2. Check network connectivity to action targets (webhooks, email servers, etc.)
3. Increase log level: `RUST_LOG=debug`
4. Restart worker with fresh logs

### Memory Usage Growing

**Symptom**: Worker memory usage increases over time.

**Diagnosis**:
```bash
# Monitor worker memory
watch -n 5 'ps aux | grep fraiseql-job-worker'

# Check for stuck jobs in queue
redis-cli -u redis://localhost:6379 LLEN queue:processing
```

**Fix**:
1. Reduce `batch_size` (fewer jobs in memory at once)
2. Reduce `worker_concurrency` (fewer parallel jobs)
3. Check for jobs stuck in "processing" state (timeout, network hang)
4. Restart worker to clear state: `systemctl restart fraiseql-job-worker`

### High Latency

**Symptom**: Jobs take a long time to execute.

**Diagnosis**:
```bash
# Check execution time histogram
# Query: histogram_quantile(0.95, fraiseql_observer_job_duration_seconds_bucket)

# Check if it's the action or the job system
# - If action itself is slow, that's external (webhook slow, etc.)
# - If job system adds latency, check queue depth and worker concurrency
```

**Fix**:
1. Increase worker concurrency if queue is backing up
2. Add more workers if single worker is bottleneck
3. Reduce batch timeout if waiting unnecessarily
4. Profile action execution (webhook, email, etc.) - not job system

---

## Performance Tuning

### For High Throughput (>10k jobs/sec)

```toml
[job_queue]
batch_size = 500              # Larger batches
batch_timeout_secs = 2        # Shorter wait
worker_concurrency = 50       # More parallel execution
initial_delay_ms = 50         # Faster retries
max_delay_ms = 5000           # Cap retry backoff

# Run 4+ worker processes
# Use dedicated Redis instance
# Monitor queue depth and scale horizontally
```

### For Low Latency (<100ms)

```toml
[job_queue]
batch_size = 50               # Smaller batches
batch_timeout_secs = 1        # Immediate processing
worker_concurrency = 20       # Balanced concurrency
poll_interval_ms = 500        # Check queue more often
```

### For Resource Constrained (single worker)

```toml
[job_queue]
batch_size = 10               # Minimal memory
batch_timeout_secs = 10       # Batch slow jobs
worker_concurrency = 2        # Single action at a time
poll_interval_ms = 2000       # Sleep more
```

### For Reliability (mission-critical actions)

```toml
[job_queue]
batch_size = 50
batch_timeout_secs = 5
max_retries = 10              # More retries
worker_concurrency = 5        # Careful execution
initial_delay_ms = 200        # Safer backoff
max_delay_ms = 120000         # 2 minutes max wait
```

---

## Testing

### Unit Tests

```bash
cargo test --features "queue,metrics" job_queue
cargo test --features "queue,metrics" queued_executor
cargo test --features "queue,metrics" factory
```

### Integration Tests

```bash
# Requires Redis running
cargo test --features "queue,metrics,testing" --test job_queue_integration

# Load test
cargo test --features "queue,testing" -- --ignored stress_test_queue
```

### Manual Testing

```bash
# Terminal 1: Start worker
cargo run --example job_queue_example

# Terminal 2: Send events/jobs
redis-cli -u redis://localhost:6379 LPUSH queue:pending '{"id":"...","event_id":"..."}'

# Terminal 3: Monitor metrics
curl http://localhost:9090/metrics | grep fraiseql_observer_job
```

---

## Migration Guide

### From Synchronous to Asynchronous Execution

**Before (ObserverExecutor)**:
```rust
let executor = ObserverExecutor::new(matcher, dlq);
let summary = executor.process_event(&event).await?;
// Actions executed immediately, may block for seconds
```

**After (QueuedObserverExecutor)**:
```rust
let queued = ExecutorFactory::build_with_queue(&config, dlq).await?;
let summary = queued.process_event(&event).await?;
// Returns immediately with job IDs
// Workers execute in background
```

**Configuration**:
```toml
# Add to config.toml
[job_queue]
url = "redis://localhost:6379"
worker_concurrency = 10
```

**Worker Process**:
```bash
# Start in separate process/container
fraiseql-job-worker --config config.toml
```

---

## See Also

- Phase 8.7: Prometheus Metrics Integration
- Phase 8.5: Elasticsearch Integration (uses job queue for indexing)
- Phase 9: Arrow Flight (streaming analytics)
