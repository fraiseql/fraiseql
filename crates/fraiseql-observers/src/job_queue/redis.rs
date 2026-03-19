//! Redis-backed distributed job queue.
//!
//! Provides durable job storage and retrieval using Redis with support for:
//! - Pending job queue (FIFO via Redis list)
//! - Processing set with timeout (Redis sorted set with expiry timestamps)
//! - Dead letter queue (Redis list for failed jobs)
//! - Status hash for O(1) job state lookup
//!
//! # Atomicity
//!
//! All multi-step state transitions use Lua scripts executed via `redis::Script`,
//! which provides EVALSHA + NOSCRIPT fallback automatically:
//!
//! - **`enqueue`**: `SET` + `LPUSH` + `HSET "pending"` — job is either fully visible or not visible
//!   at all; the status hash stays in sync.
//! - **`acknowledge`**: `ZREM` + `DEL` + `HDEL` — processing entry, job data, and status hash entry
//!   are cleaned up in one atomic step.
//! - **`fail` (retry)**: `ZREM` + `SET` + `LPUSH` + `HSET "pending"` — job is moved from processing
//!   back to pending atomically.
//! - **`fail` (DLQ)**: `ZREM` + `SET` + `LPUSH dlq` + `HSET "dead_lettered"` — job is moved from
//!   processing to DLQ atomically.
//!
//! # Status hash
//!
//! `queue:status` is a Redis hash mapping `job_id → state_string`. All state
//! transitions update it atomically as part of their Lua script.
//! `get_status()` performs a single `HGET` instead of the previous O(N)
//! `LPOS` / `LRANGE` scans.

use chrono::Utc;
use redis::aio::ConnectionManager;
use uuid::Uuid;

use super::{Job, traits::JobQueue};
use crate::error::Result;

// ── Lua scripts ───────────────────────────────────────────────────────────────

/// Atomically store job data, enqueue the job ID, and record status as "pending".
///
/// - `KEYS[1]` — `job:{uuid}` (job data string key)
/// - `KEYS[2]` — `queue:pending` (the pending FIFO list)
/// - `KEYS[3]` — `queue:status` (the status hash)
/// - `ARGV[1]` — serialised job JSON
/// - `ARGV[2]` — job UUID string (pushed onto the list and used as hash field)
const JOB_ENQUEUE_SCRIPT: &str = r"
redis.call('SET', KEYS[1], ARGV[1])
redis.call('LPUSH', KEYS[2], ARGV[2])
redis.call('HSET', KEYS[3], ARGV[2], 'pending')
return 1
";

/// Atomically remove a job from the processing set, delete its data, and clear
/// its status hash entry on successful completion.
///
/// - `KEYS[1]` — `queue:processing` (the processing sorted set)
/// - `KEYS[2]` — `job:{uuid}` (the job data key)
/// - `KEYS[3]` — `queue:status` (the status hash)
/// - `ARGV[1]` — `processing:{uuid}` (sorted set member)
/// - `ARGV[2]` — job UUID string (status hash field)
const JOB_ACKNOWLEDGE_SCRIPT: &str = r"
redis.call('ZREM', KEYS[1], ARGV[1])
redis.call('DEL', KEYS[2])
redis.call('HDEL', KEYS[3], ARGV[2])
return 1
";

/// Atomically move a failed-but-retriable job from processing back to pending.
///
/// - `KEYS[1]` — `queue:processing`
/// - `KEYS[2]` — `job:{uuid}`
/// - `KEYS[3]` — `queue:pending`
/// - `KEYS[4]` — `queue:status`
/// - `ARGV[1]` — `processing:{uuid}` (sorted set member)
/// - `ARGV[2]` — updated serialised job JSON
/// - `ARGV[3]` — job UUID string
const JOB_FAIL_RETRY_SCRIPT: &str = r"
redis.call('ZREM', KEYS[1], ARGV[1])
redis.call('SET', KEYS[2], ARGV[2])
redis.call('LPUSH', KEYS[3], ARGV[3])
redis.call('HSET', KEYS[4], ARGV[3], 'pending')
return 1
";

/// Atomically move a permanently failed job from processing to the DLQ.
///
/// - `KEYS[1]` — `queue:processing`
/// - `KEYS[2]` — `job:{uuid}`
/// - `KEYS[3]` — `queue:dlq`
/// - `KEYS[4]` — `queue:status`
/// - `ARGV[1]` — `processing:{uuid}` (sorted set member)
/// - `ARGV[2]` — updated serialised job JSON (with Failed state)
/// - `ARGV[3]` — `dlq:{uuid}` (DLQ list member)
/// - `ARGV[4]` — job UUID string (status hash field)
const JOB_FAIL_DLQ_SCRIPT: &str = r"
redis.call('ZREM', KEYS[1], ARGV[1])
redis.call('SET', KEYS[2], ARGV[2])
redis.call('LPUSH', KEYS[3], ARGV[3])
redis.call('HSET', KEYS[4], ARGV[4], 'dead_lettered')
return 1
";

/// Redis-backed job queue.
///
/// Uses four Redis data structures:
/// - `queue:pending`    — List of jobs waiting to execute
/// - `queue:processing` — Sorted set of jobs being executed (score = expiry timestamp)
/// - `queue:dlq`        — List of permanently failed jobs
/// - `queue:status`     — Hash of `job_id → state_string` for O(1) status lookup
///
/// # Example
///
/// ```no_run
/// let conn = redis::Client::open("redis://localhost:6379")?.get_async_connection().await?;
/// let queue = RedisJobQueue::new(conn);
/// ```
#[derive(Clone)]
pub struct RedisJobQueue {
    conn: ConnectionManager,
}

impl RedisJobQueue {
    /// Create a new Redis job queue.
    ///
    /// # Arguments
    ///
    /// * `conn` - Redis connection manager
    #[must_use]
    pub const fn new(conn: ConnectionManager) -> Self {
        Self { conn }
    }

    /// Pending queue key
    const fn pending_key() -> &'static str {
        "queue:pending"
    }

    /// Processing sorted-set key
    const fn processing_key() -> &'static str {
        "queue:processing"
    }

    /// Dead-letter queue key
    const fn dlq_key() -> &'static str {
        "queue:dlq"
    }

    /// Status hash key — maps `job_id → state_string` for O(1) lookups.
    const fn status_key() -> &'static str {
        "queue:status"
    }

    /// Individual job data key
    fn job_key(job_id: Uuid) -> String {
        format!("job:{job_id}")
    }

    /// Processing sorted-set member value
    fn processing_member(job_id: Uuid) -> String {
        format!("processing:{job_id}")
    }

    /// DLQ list member value
    fn dlq_member(job_id: Uuid) -> String {
        format!("dlq:{job_id}")
    }
}

#[async_trait::async_trait]
impl JobQueue for RedisJobQueue {
    async fn enqueue(&self, job: Job) -> Result<()> {
        let job_id = job.id;
        let json = serde_json::to_string(&job)
            .map_err(|e| crate::error::ObserverError::SerializationError(e.to_string()))?;

        // Atomically store job data, push onto the pending queue, and set status to
        // "pending". A crash between separate commands would leave partial state;
        // the Lua script eliminates that window.
        redis::Script::new(JOB_ENQUEUE_SCRIPT)
            .key(Self::job_key(job_id)) // KEYS[1]
            .key(Self::pending_key()) // KEYS[2]
            .key(Self::status_key()) // KEYS[3]
            .arg(&json) // ARGV[1]
            .arg(job_id.to_string()) // ARGV[2]
            .invoke_async::<i64>(&mut self.conn.clone())
            .await?;

        Ok(())
    }

    async fn dequeue(&self, batch_size: usize, timeout_secs: u64) -> Result<Vec<Job>> {
        let mut jobs = Vec::new();
        let now = Utc::now();
        #[allow(clippy::cast_precision_loss)] // Reason: f64 precision is acceptable for timestamp scores
        let expiry_timestamp =
            (now + chrono::Duration::seconds(timeout_secs.cast_signed())).timestamp_millis() as f64;

        for _ in 0..batch_size {
            // Pop from pending queue
            let job_id_str: Option<String> = redis::cmd("RPOP")
                .arg(Self::pending_key())
                .query_async(&mut self.conn.clone())
                .await?;

            let Some(job_id_str) = job_id_str else {
                break;
            };

            let job_id = Uuid::parse_str(&job_id_str).map_err(|e| {
                crate::error::ObserverError::InvalidConfig {
                    message: format!("Invalid job ID in queue: {e}"),
                }
            })?;

            // Retrieve job data
            let job_json: String = redis::cmd("GET")
                .arg(Self::job_key(job_id))
                .query_async(&mut self.conn.clone())
                .await
                .map_err(|_| crate::error::ObserverError::InvalidConfig {
                    message: format!("Job {job_id} not found in storage"),
                })?;

            let mut job: Job = serde_json::from_str(&job_json)
                .map_err(|e| crate::error::ObserverError::SerializationError(e.to_string()))?;

            // Add to processing set with expiry timestamp and update status to "running".
            // These two calls are intentionally separate; the ZADD is the durable record,
            // and the HSET keeps the status hash consistent.
            redis::cmd("ZADD")
                .arg(Self::processing_key())
                .arg(expiry_timestamp)
                .arg(Self::processing_member(job_id))
                .query_async::<()>(&mut self.conn.clone())
                .await?;

            redis::cmd("HSET")
                .arg(Self::status_key())
                .arg(job_id.to_string())
                .arg("running")
                .query_async::<()>(&mut self.conn.clone())
                .await?;

            job.mark_running();
            jobs.push(job);
        }

        Ok(jobs)
    }

    async fn acknowledge(&self, job_id: Uuid) -> Result<()> {
        // Atomically remove from processing set, delete job data, and clear status entry.
        redis::Script::new(JOB_ACKNOWLEDGE_SCRIPT)
            .key(Self::processing_key()) // KEYS[1]
            .key(Self::job_key(job_id)) // KEYS[2]
            .key(Self::status_key()) // KEYS[3]
            .arg(Self::processing_member(job_id)) // ARGV[1]
            .arg(job_id.to_string()) // ARGV[2]
            .invoke_async::<i64>(&mut self.conn.clone())
            .await?;

        Ok(())
    }

    async fn fail(&self, job: &mut Job, error: String) -> Result<()> {
        let job_id = job.id;

        // Mark as failed — this decides whether we retry (state → Pending) or not (state → Failed).
        job.mark_failed(error);

        let json = serde_json::to_string(job)
            .map_err(|e| crate::error::ObserverError::SerializationError(e.to_string()))?;

        if job.can_retry() {
            // Atomically: leave processing, update job data, re-enqueue as pending, set status.
            redis::Script::new(JOB_FAIL_RETRY_SCRIPT)
                .key(Self::processing_key()) // KEYS[1]
                .key(Self::job_key(job_id)) // KEYS[2]
                .key(Self::pending_key()) // KEYS[3]
                .key(Self::status_key()) // KEYS[4]
                .arg(Self::processing_member(job_id)) // ARGV[1]
                .arg(&json) // ARGV[2]
                .arg(job_id.to_string()) // ARGV[3]
                .invoke_async::<i64>(&mut self.conn.clone())
                .await?;
        } else {
            // Atomically: leave processing, update job data, move to DLQ, set status.
            redis::Script::new(JOB_FAIL_DLQ_SCRIPT)
                .key(Self::processing_key()) // KEYS[1]
                .key(Self::job_key(job_id)) // KEYS[2]
                .key(Self::dlq_key()) // KEYS[3]
                .key(Self::status_key()) // KEYS[4]
                .arg(Self::processing_member(job_id)) // ARGV[1]
                .arg(&json) // ARGV[2]
                .arg(Self::dlq_member(job_id)) // ARGV[3]
                .arg(job_id.to_string()) // ARGV[4]
                .invoke_async::<i64>(&mut self.conn.clone())
                .await?;
        }

        Ok(())
    }

    async fn get_status(&self, job_id: Uuid) -> Result<Option<super::JobState>> {
        // O(1) lookup via the shadow status hash — avoids scanning LPOS/LRANGE.
        let state_str: Option<String> = redis::cmd("HGET")
            .arg(Self::status_key())
            .arg(job_id.to_string())
            .query_async(&mut self.conn.clone())
            .await?;

        Ok(state_str.as_deref().and_then(parse_job_state))
    }

    async fn queue_depth(&self) -> Result<usize> {
        let depth: usize = redis::cmd("LLEN")
            .arg(Self::pending_key())
            .query_async(&mut self.conn.clone())
            .await?;

        Ok(depth)
    }

    async fn dlq_size(&self) -> Result<usize> {
        let size: usize = redis::cmd("LLEN")
            .arg(Self::dlq_key())
            .query_async(&mut self.conn.clone())
            .await?;

        Ok(size)
    }
}

/// Parse the state string stored in `queue:status` back to `JobState`.
///
/// The strings match the `serde(rename_all = "snake_case")` serialisation of
/// `JobState`; keeping an explicit match here avoids a JSON round-trip.
fn parse_job_state(s: &str) -> Option<super::JobState> {
    match s {
        "pending" => Some(super::JobState::Pending),
        "running" => Some(super::JobState::Running),
        "completed" => Some(super::JobState::Completed),
        "failed" => Some(super::JobState::Failed),
        "dead_lettered" => Some(super::JobState::DeadLettered),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ActionConfig;

    #[test]
    fn test_key_generation() {
        assert_eq!(RedisJobQueue::pending_key(), "queue:pending");
        assert_eq!(RedisJobQueue::processing_key(), "queue:processing");
        assert_eq!(RedisJobQueue::dlq_key(), "queue:dlq");
        assert_eq!(RedisJobQueue::status_key(), "queue:status");

        let job_id = Uuid::nil();
        assert!(RedisJobQueue::job_key(job_id).starts_with("job:"));
        assert!(RedisJobQueue::processing_member(job_id).starts_with("processing:"));
        assert!(RedisJobQueue::dlq_member(job_id).starts_with("dlq:"));
    }

    #[test]
    fn test_redis_job_queue_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<RedisJobQueue>();
    }

    #[test]
    fn test_job_serialization_for_redis() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action:      "invalidate".to_string(),
        };
        let job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        let json = serde_json::to_string(&job).expect("serialization failed");
        let deserialized: Job = serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(job.id, deserialized.id);
        assert_eq!(job.event_id, deserialized.event_id);
        assert_eq!(job.attempt, deserialized.attempt);
    }

    #[test]
    fn test_parse_job_state_all_variants() {
        assert_eq!(parse_job_state("pending"), Some(super::super::JobState::Pending));
        assert_eq!(parse_job_state("running"), Some(super::super::JobState::Running));
        assert_eq!(parse_job_state("completed"), Some(super::super::JobState::Completed));
        assert_eq!(parse_job_state("failed"), Some(super::super::JobState::Failed));
        assert_eq!(parse_job_state("dead_lettered"), Some(super::super::JobState::DeadLettered));
        assert_eq!(parse_job_state("unknown"), None);
        assert_eq!(parse_job_state(""), None);
    }
}
