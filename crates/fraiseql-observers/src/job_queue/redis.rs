//! Redis-backed distributed job queue.
//!
//! Provides durable job storage and retrieval using Redis with support for:
//! - Pending job queue (FIFO via Redis list)
//! - Processing set with timeout (Redis sorted set with expiry timestamps)
//! - Dead letter queue (Redis list for failed jobs)

use chrono::Utc;
use redis::aio::ConnectionManager;
use uuid::Uuid;

use super::traits::JobQueue;
use super::Job;
use crate::error::Result;

/// Redis-backed job queue.
///
/// Uses three Redis data structures:
/// - `queue:pending` - List of jobs waiting to execute
/// - `queue:processing` - Sorted set of jobs being executed (with expiry timestamp as score)
/// - `queue:dlq` - List of permanently failed jobs
///
/// # Example
///
/// ```ignore
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

    /// Generate pending queue key
    const fn pending_key() -> &'static str {
        "queue:pending"
    }

    /// Generate processing set key
    const fn processing_key() -> &'static str {
        "queue:processing"
    }

    /// Generate DLQ key
    const fn dlq_key() -> &'static str {
        "queue:dlq"
    }

    /// Generate individual job key
    fn job_key(job_id: Uuid) -> String {
        format!("job:{job_id}")
    }

    /// Generate processing job key (for sorted set member)
    fn processing_member(job_id: Uuid) -> String {
        format!("processing:{job_id}")
    }

    /// Generate DLQ job key (for list member)
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

        // Store job data
        redis::cmd("SET")
            .arg(Self::job_key(job_id))
            .arg(&json)
            .query_async::<_, ()>(&mut self.conn.clone())
            .await?;

        // Add to pending queue
        redis::cmd("LPUSH")
            .arg(Self::pending_key())
            .arg(job_id.to_string())
            .query_async::<_, ()>(&mut self.conn.clone())
            .await?;

        Ok(())
    }

    async fn dequeue(&self, batch_size: usize, timeout_secs: u64) -> Result<Vec<Job>> {
        let mut jobs = Vec::new();
        let now = Utc::now();
        let expiry_timestamp = (now + chrono::Duration::seconds(timeout_secs as i64))
            .timestamp_millis() as f64;

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

            // Mark as processing with expiry timestamp
            redis::cmd("ZADD")
                .arg(Self::processing_key())
                .arg(expiry_timestamp)
                .arg(Self::processing_member(job_id))
                .query_async::<_, ()>(&mut self.conn.clone())
                .await?;

            job.mark_running();
            jobs.push(job);
        }

        Ok(jobs)
    }

    async fn acknowledge(&self, job_id: Uuid) -> Result<()> {
        // Remove from processing set
        redis::cmd("ZREM")
            .arg(Self::processing_key())
            .arg(Self::processing_member(job_id))
            .query_async::<_, ()>(&mut self.conn.clone())
            .await?;

        // Remove job data
        redis::cmd("DEL")
            .arg(Self::job_key(job_id))
            .query_async::<_, ()>(&mut self.conn.clone())
            .await?;

        Ok(())
    }

    async fn fail(&self, job: &mut Job, error: String) -> Result<()> {
        let job_id = job.id;

        // Remove from processing set
        redis::cmd("ZREM")
            .arg(Self::processing_key())
            .arg(Self::processing_member(job_id))
            .query_async::<_, ()>(&mut self.conn.clone())
            .await?;

        // Mark as failed and decide next action
        job.mark_failed(error);

        if job.can_retry() {
            // Put back in pending queue
            let json = serde_json::to_string(job)
                .map_err(|e| crate::error::ObserverError::SerializationError(e.to_string()))?;

            redis::cmd("SET")
                .arg(Self::job_key(job_id))
                .arg(&json)
                .query_async::<_, ()>(&mut self.conn.clone())
                .await?;

            redis::cmd("LPUSH")
                .arg(Self::pending_key())
                .arg(job_id.to_string())
                .query_async::<_, ()>(&mut self.conn.clone())
                .await?;
        } else {
            // Move to DLQ
            let json = serde_json::to_string(job)
                .map_err(|e| crate::error::ObserverError::SerializationError(e.to_string()))?;

            redis::cmd("SET")
                .arg(Self::job_key(job_id))
                .arg(&json)
                .query_async::<_, ()>(&mut self.conn.clone())
                .await?;

            redis::cmd("LPUSH")
                .arg(Self::dlq_key())
                .arg(Self::dlq_member(job_id))
                .query_async::<_, ()>(&mut self.conn.clone())
                .await?;
        }

        Ok(())
    }

    async fn get_status(&self, job_id: Uuid) -> Result<Option<super::JobState>> {
        // Check processing set
        let in_processing: bool = redis::cmd("ZSCORE")
            .arg(Self::processing_key())
            .arg(Self::processing_member(job_id))
            .query_async(&mut self.conn.clone())
            .await
            .ok()
            .and_then(|score: Option<f64>| score)
            .is_some();

        if in_processing {
            return Ok(Some(super::JobState::Running));
        }

        // Check DLQ
        let in_dlq: bool = redis::cmd("LPOS")
            .arg(Self::dlq_key())
            .arg(Self::dlq_member(job_id))
            .query_async(&mut self.conn.clone())
            .await
            .ok()
            .and_then(|pos: Option<i64>| pos)
            .is_some();

        if in_dlq {
            // Fetch the job to determine if Failed or DeadLettered
            if let Ok(Some(job_json)) = self.get_job_data(job_id).await {
                if let Ok(job) = serde_json::from_str::<Job>(&job_json) {
                    return Ok(Some(job.state));
                }
            }
            return Ok(Some(super::JobState::DeadLettered));
        }

        // Check pending queue - need to scan the list
        let pending_jobs: Vec<String> = redis::cmd("LRANGE")
            .arg(Self::pending_key())
            .arg(0)
            .arg(-1)
            .query_async(&mut self.conn.clone())
            .await
            .unwrap_or_default();

        if pending_jobs.iter().any(|id| id == &job_id.to_string()) {
            return Ok(Some(super::JobState::Pending));
        }

        // Check if job exists in storage (completed or in between states)
        if let Ok(Some(job_json)) = self.get_job_data(job_id).await {
            if let Ok(job) = serde_json::from_str::<Job>(&job_json) {
                return Ok(Some(job.state));
            }
        }

        Ok(None)
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

impl RedisJobQueue {
    /// Get job data from storage (internal helper)
    async fn get_job_data(&self, job_id: Uuid) -> Result<Option<String>> {
        let json: Option<String> = redis::cmd("GET")
            .arg(Self::job_key(job_id))
            .query_async(&mut self.conn.clone())
            .await?;

        Ok(json)
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

        let job_id = Uuid::nil();
        assert!(RedisJobQueue::job_key(job_id).starts_with("job:"));
        assert!(RedisJobQueue::processing_member(job_id).starts_with("processing:"));
        assert!(RedisJobQueue::dlq_member(job_id).starts_with("dlq:"));
    }

    #[test]
    fn test_redis_job_queue_clone() {
        // Ensure RedisJobQueue is Clone-able
        fn assert_clone<T: Clone>() {}
        assert_clone::<RedisJobQueue>();
    }

    #[test]
    fn test_job_serialization_for_redis() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };
        let job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        let json = serde_json::to_string(&job).expect("serialization failed");
        let deserialized: Job = serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(job.id, deserialized.id);
        assert_eq!(job.event_id, deserialized.event_id);
        assert_eq!(job.attempt, deserialized.attempt);
    }
}
