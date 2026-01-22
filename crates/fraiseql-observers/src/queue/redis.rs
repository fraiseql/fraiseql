//! Redis-backed persistent job queue implementation.
//!
//! Uses Redis data structures for durable, distributed job storage:
//! - Pending jobs: Redis sorted set (score = priority/timestamp)
//! - Processing jobs: Redis hash (tracks worker, start time)
//! - Retry jobs: Redis sorted set (score = next retry timestamp)
//! - Job details: Redis hashes (serialized job data)
//! - Statistics: Redis counters

use super::{Job, JobQueue, JobResult, JobStatus, QueueStats};
use crate::error::{ObserverError, Result};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

/// Redis-backed persistent job queue.
#[derive(Clone)]
pub struct RedisJobQueue {
    conn: ConnectionManager,
    pending_key: String,
    processing_key: String,
    retry_key: String,
    deadletter_key: String,
}

impl RedisJobQueue {
    /// Create a new Redis-backed job queue.
    ///
    /// # Arguments
    ///
    /// * `conn` - Redis connection manager
    pub fn new(conn: ConnectionManager) -> Self {
        Self {
            conn,
            pending_key: "queue:v1:pending".to_string(),
            processing_key: "queue:v1:processing".to_string(),
            retry_key: "queue:v1:retry".to_string(),
            deadletter_key: "queue:v1:deadletter".to_string(),
        }
    }

    /// Get Redis key for job data storage.
    fn job_key(job_id: &str) -> String {
        format!("job:v1:{}", job_id)
    }

    /// Get Redis key for completed job data storage.
    fn completed_key(job_id: &str) -> String {
        format!("job:v1:completed:{}", job_id)
    }

    /// Serialize job to JSON for storage.
    fn serialize_job(job: &Job) -> Result<String> {
        serde_json::to_string(job)
            .map_err(|e| ObserverError::SerializationError(e.to_string()))
    }

    /// Deserialize job from JSON.
    fn deserialize_job(json: &str) -> Result<Job> {
        serde_json::from_str(json)
            .map_err(|e| ObserverError::SerializationError(e.to_string()))
    }

    /// Serialize job result metadata to JSON (excluding ActionResult which isn't Serializable).
    fn serialize_result(result: &JobResult) -> Result<String> {
        let metadata = serde_json::json!({
            "job_id": result.job_id,
            "status": result.status.to_string(),
            "attempts": result.attempts,
            "duration_ms": result.duration_ms,
            "action_type": result.action_result.action_type,
            "success": result.action_result.success,
            "message": result.action_result.message,
        });
        serde_json::to_string(&metadata)
            .map_err(|e| ObserverError::SerializationError(e.to_string()))
    }
}

#[async_trait::async_trait]
impl JobQueue for RedisJobQueue {
    async fn enqueue(&self, job: &Job) -> Result<String> {
        let job_id = job.id.clone();
        let job_json = Self::serialize_job(job)?;

        let mut conn = self.conn.clone();

        // Store job data
        conn.set_ex(
            Self::job_key(&job_id),
            &job_json,
            86400, // 24-hour expiration for job metadata
        )
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to store job: {}", e),
        })?;

        // Add to pending queue (score = current timestamp for FIFO)
        let now = chrono::Utc::now().timestamp() as f64;
        conn.zadd(&self.pending_key, &job_id, now)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to add to pending queue: {}", e),
            })?;

        Ok(job_id)
    }

    async fn dequeue(&self, worker_id: &str) -> Result<Option<Job>> {
        let mut conn = self.conn.clone();

        // BZPOPMIN on pending queue (blocking if empty)
        // Returns (key, member, score) - we need member (job_id)
        let result: Vec<(Vec<u8>, f64)> = conn
            .bzpop_min(&[&self.pending_key], 1.0)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to dequeue: {}", e),
            })?;

        if result.is_empty() {
            return Ok(None);
        }

        let job_id_bytes = &result[0].0;
        let job_id = String::from_utf8_lossy(job_id_bytes).to_string();

        // Get job data
        let job_json: String = conn
            .get(Self::job_key(&job_id))
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to get job data: {}", e),
            })?;

        let job = Self::deserialize_job(&job_json)?;

        // Mark as processing (add to processing set with worker info)
        let processing_info = format!("{}:{}", worker_id, chrono::Utc::now().timestamp());
        conn.hset(&self.processing_key, &job_id, &processing_info)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to mark as processing: {}", e),
            })?;

        Ok(Some(job))
    }

    async fn mark_processing(&self, job_id: &str) -> Result<()> {
        let mut conn = self.conn.clone();

        let processing_info = format!("{}:{}", "worker", chrono::Utc::now().timestamp());
        conn.hset(&self.processing_key, job_id, processing_info)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to mark as processing: {}", e),
            })?;

        Ok(())
    }

    async fn mark_success(&self, job_id: &str, result: &JobResult) -> Result<()> {
        let mut conn = self.conn.clone();

        let result_json = Self::serialize_result(result)?;

        // Store completed job data
        conn.set_ex(
            Self::completed_key(job_id),
            result_json,
            86400, // 24-hour retention
        )
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to store completed job: {}", e),
        })?;

        // Remove from processing
        conn.hdel(&self.processing_key, job_id)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to remove from processing: {}", e),
            })?;

        // Remove job data
        conn.del(Self::job_key(job_id))
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to delete job data: {}", e),
            })?;

        // Increment success counter
        conn.incr("queue:v1:stats:success", 1)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to update stats: {}", e),
            })?;

        Ok(())
    }

    async fn mark_retry(&self, job_id: &str, next_retry_at: i64) -> Result<()> {
        let mut conn = self.conn.clone();

        // Remove from processing
        conn.hdel(&self.processing_key, job_id)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to remove from processing: {}", e),
            })?;

        // Add to retry queue with next_retry_at as score
        conn.zadd(&self.retry_key, job_id, next_retry_at as f64)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to add to retry queue: {}", e),
            })?;

        // Increment retry counter
        conn.incr("queue:v1:stats:retries", 1)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to update stats: {}", e),
            })?;

        Ok(())
    }

    async fn mark_deadletter(&self, job_id: &str, reason: &str) -> Result<()> {
        let mut conn = self.conn.clone();

        // Remove from processing
        conn.hdel(&self.processing_key, job_id)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to remove from processing: {}", e),
            })?;

        // Store deadletter entry with reason
        let now = chrono::Utc::now().timestamp() as f64;
        let entry = format!("{}|{}", reason, chrono::Utc::now().timestamp());

        conn.zadd(&self.deadletter_key, &job_id, now)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to add to deadletter queue: {}", e),
            })?;

        // Store reason
        conn.set_ex(
            format!("job:v1:deadletter:reason:{}", job_id),
            entry,
            86400,
        )
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to store deadletter reason: {}", e),
        })?;

        // Increment failed counter
        conn.incr("queue:v1:stats:failed", 1)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to update stats: {}", e),
            })?;

        Ok(())
    }

    async fn get_stats(&self) -> Result<QueueStats> {
        let mut conn = self.conn.clone();

        // Get queue lengths
        let pending_jobs: u64 = conn
            .zcard(&self.pending_key)
            .await
            .unwrap_or(0);

        let processing_jobs: u64 = conn
            .hlen(&self.processing_key)
            .await
            .unwrap_or(0);

        let retry_jobs: u64 = conn
            .zcard(&self.retry_key)
            .await
            .unwrap_or(0);

        // Get counters
        let successful_jobs: u64 = conn
            .get("queue:v1:stats:success")
            .await
            .unwrap_or(0);

        let failed_jobs: u64 = conn
            .zcard(&self.deadletter_key)
            .await
            .unwrap_or(0);

        // Get average processing time (stored in Redis as float)
        let avg_processing_time_ms: f64 = conn
            .get("queue:v1:stats:avg_processing_ms")
            .await
            .unwrap_or(0.0);

        Ok(QueueStats {
            pending_jobs,
            processing_jobs,
            retry_jobs,
            successful_jobs,
            failed_jobs,
            avg_processing_time_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redis::Client;

    async fn setup_test_queue() -> RedisJobQueue {
        let client = Client::open("redis://localhost:6379").expect("Failed to create client");
        let conn = client
            .get_connection_manager()
            .await
            .expect("Failed to connect to Redis");

        // Clear test data
        let mut c = conn.clone();
        let _: () = redis::cmd("FLUSHDB")
            .query_async(&mut c)
            .await
            .expect("Failed to flush DB");

        RedisJobQueue::new(conn)
    }

    #[tokio::test]
    #[ignore] // Requires Redis running
    async fn test_redis_enqueue() {
        let queue = setup_test_queue().await;

        let job = Job {
            id: "job-1".to_string(),
            action_id: "send_email".to_string(),
            event: EntityEvent::new(
                crate::event::EventKind::Created,
                "Order".to_string(),
                uuid::Uuid::new_v4(),
                serde_json::json!({}),
            ),
            action_config: ActionConfig::Email {
                to: vec!["test@example.com".to_string()],
                to_env: None,
                subject: "Test".to_string(),
                body_template: None,
            },
            attempt: 1,
            created_at: chrono::Utc::now().timestamp(),
            next_retry_at: chrono::Utc::now().timestamp(),
        };

        let job_id = queue.enqueue(&job).await.expect("Failed to enqueue");
        assert_eq!(job_id, "job-1");
    }

    #[tokio::test]
    #[ignore] // Requires Redis running
    async fn test_redis_dequeue() {
        let queue = setup_test_queue().await;

        let job = Job {
            id: "job-1".to_string(),
            action_id: "send_email".to_string(),
            event: EntityEvent::new(
                crate::event::EventKind::Created,
                "Order".to_string(),
                uuid::Uuid::new_v4(),
                serde_json::json!({}),
            ),
            action_config: ActionConfig::Email {
                to: vec!["test@example.com".to_string()],
                to_env: None,
                subject: "Test".to_string(),
                body_template: None,
            },
            attempt: 1,
            created_at: chrono::Utc::now().timestamp(),
            next_retry_at: chrono::Utc::now().timestamp(),
        };

        queue.enqueue(&job).await.expect("Failed to enqueue");

        let dequeued = queue
            .dequeue("worker-1")
            .await
            .expect("Failed to dequeue")
            .expect("No job dequeued");

        assert_eq!(dequeued.id, "job-1");
    }

    #[tokio::test]
    #[ignore] // Requires Redis running
    async fn test_redis_mark_success() {
        let queue = setup_test_queue().await;

        let job = Job {
            id: "job-1".to_string(),
            action_id: "send_email".to_string(),
            event: EntityEvent::new(
                crate::event::EventKind::Created,
                "Order".to_string(),
                uuid::Uuid::new_v4(),
                serde_json::json!({}),
            ),
            action_config: ActionConfig::Email {
                to: vec!["test@example.com".to_string()],
                to_env: None,
                subject: "Test".to_string(),
                body_template: None,
            },
            attempt: 1,
            created_at: chrono::Utc::now().timestamp(),
            next_retry_at: chrono::Utc::now().timestamp(),
        };

        queue.enqueue(&job).await.expect("Failed to enqueue");

        let result = JobResult {
            job_id: "job-1".to_string(),
            status: JobStatus::Success,
            action_result: ActionResult {
                action_type: "send_email".to_string(),
                success: true,
                message: "Email sent".to_string(),
                duration_ms: 100.0,
            },
            attempts: 1,
            duration_ms: 100.0,
        };

        queue
            .mark_success("job-1", &result)
            .await
            .expect("Failed to mark success");

        let stats = queue.get_stats().await.expect("Failed to get stats");
        assert_eq!(stats.successful_jobs, 1);
    }

    #[tokio::test]
    #[ignore] // Requires Redis running
    async fn test_redis_mark_retry() {
        let queue = setup_test_queue().await;

        let job = Job {
            id: "job-1".to_string(),
            action_id: "send_email".to_string(),
            event: EntityEvent::new(
                crate::event::EventKind::Created,
                "Order".to_string(),
                uuid::Uuid::new_v4(),
                serde_json::json!({}),
            ),
            action_config: ActionConfig::Email {
                to: vec!["test@example.com".to_string()],
                to_env: None,
                subject: "Test".to_string(),
                body_template: None,
            },
            attempt: 1,
            created_at: chrono::Utc::now().timestamp(),
            next_retry_at: chrono::Utc::now().timestamp(),
        };

        queue.enqueue(&job).await.expect("Failed to enqueue");

        let next_retry = chrono::Utc::now().timestamp() + 5;
        queue
            .mark_retry("job-1", next_retry)
            .await
            .expect("Failed to mark retry");

        let stats = queue.get_stats().await.expect("Failed to get stats");
        assert_eq!(stats.retry_jobs, 1);
    }

    #[tokio::test]
    #[ignore] // Requires Redis running
    async fn test_redis_mark_deadletter() {
        let queue = setup_test_queue().await;

        let job = Job {
            id: "job-1".to_string(),
            action_id: "send_email".to_string(),
            event: EntityEvent::new(
                crate::event::EventKind::Created,
                "Order".to_string(),
                uuid::Uuid::new_v4(),
                serde_json::json!({}),
            ),
            action_config: ActionConfig::Email {
                to: vec!["test@example.com".to_string()],
                to_env: None,
                subject: "Test".to_string(),
                body_template: None,
            },
            attempt: 1,
            created_at: chrono::Utc::now().timestamp(),
            next_retry_at: chrono::Utc::now().timestamp(),
        };

        queue.enqueue(&job).await.expect("Failed to enqueue");

        queue
            .mark_deadletter("job-1", "Max retries exceeded")
            .await
            .expect("Failed to mark deadletter");

        let stats = queue.get_stats().await.expect("Failed to get stats");
        assert_eq!(stats.failed_jobs, 1);
    }

    #[tokio::test]
    #[ignore] // Requires Redis running
    async fn test_redis_get_stats() {
        let queue = setup_test_queue().await;

        let stats = queue.get_stats().await.expect("Failed to get stats");

        assert_eq!(stats.pending_jobs, 0);
        assert_eq!(stats.processing_jobs, 0);
        assert_eq!(stats.retry_jobs, 0);
    }
}
