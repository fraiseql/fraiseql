//! Redis-backed persistent job queue implementation.
//!
//! Uses Redis data structures for durable, distributed job storage:
//! - Pending jobs: Redis sorted set (score = priority/timestamp)
//! - Processing jobs: Redis hash (tracks worker, start time)
//! - Retry jobs: Redis sorted set (score = next retry timestamp)
//! - Job details: Redis hashes (serialized job data)
//! - Statistics: Redis counters

use redis::{AsyncCommands, aio::ConnectionManager};

use super::{Job, JobQueue, JobResult, QueueStats};
use crate::error::{ObserverError, Result};

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
    #[must_use]
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
        format!("job:v1:{job_id}")
    }

    /// Get Redis key for completed job data storage.
    fn completed_key(job_id: &str) -> String {
        format!("job:v1:completed:{job_id}")
    }

    /// Serialize job to JSON for storage.
    fn serialize_job(job: &Job) -> Result<String> {
        serde_json::to_string(job).map_err(|e| ObserverError::SerializationError(e.to_string()))
    }

    /// Deserialize job from JSON.
    fn deserialize_job(json: &str) -> Result<Job> {
        serde_json::from_str(json).map_err(|e| ObserverError::SerializationError(e.to_string()))
    }

    /// Serialize job result metadata to JSON (excluding `ActionResult` which isn't Serializable).
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
        conn.set_ex::<_, _, ()>(
            Self::job_key(&job_id),
            &job_json,
            86400, // 24-hour expiration for job metadata
        )
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to store job: {e}"),
        })?;

        // Add to pending queue (score = current timestamp for FIFO)
        #[allow(clippy::cast_precision_loss)]
        // Reason: f64 precision is acceptable for Redis sorted set scores
        let now = chrono::Utc::now().timestamp() as f64;
        conn.zadd::<_, _, _, ()>(&self.pending_key, &job_id, now).await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Failed to add to pending queue: {e}"),
            }
        })?;

        Ok(job_id)
    }

    async fn dequeue(&self, worker_id: &str) -> Result<Option<Job>> {
        let mut conn = self.conn.clone();

        // ZPOPMIN on pending queue (non-blocking, gets first element)
        // Returns Vec<(member, score)>
        let result: Vec<(String, f64)> =
            conn.zpopmin(&self.pending_key, 1)
                .await
                .map_err(|e| ObserverError::DatabaseError {
                    reason: format!("Failed to dequeue: {e}"),
                })?;

        if result.is_empty() {
            return Ok(None);
        }

        let job_id = &result[0].0;

        // Get job data
        let job_json: String =
            conn.get(Self::job_key(job_id))
                .await
                .map_err(|e| ObserverError::DatabaseError {
                    reason: format!("Failed to get job data: {e}"),
                })?;

        let job = Self::deserialize_job(&job_json)?;

        // Mark as processing (add to processing set with worker info)
        let processing_info = format!("{}:{}", worker_id, chrono::Utc::now().timestamp());
        conn.hset::<_, _, _, ()>(&self.processing_key, job_id, &processing_info)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to mark as processing: {e}"),
            })?;

        Ok(Some(job))
    }

    async fn mark_processing(&self, job_id: &str) -> Result<()> {
        let mut conn = self.conn.clone();

        let processing_info = format!("{}:{}", "worker", chrono::Utc::now().timestamp());
        conn.hset::<_, _, _, ()>(&self.processing_key, job_id, processing_info)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to mark as processing: {e}"),
            })?;

        Ok(())
    }

    async fn mark_success(&self, job_id: &str, result: &JobResult) -> Result<()> {
        let mut conn = self.conn.clone();

        let result_json = Self::serialize_result(result)?;

        // Store completed job data
        conn.set_ex::<_, _, ()>(
            Self::completed_key(job_id),
            result_json,
            86400, // 24-hour retention
        )
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to store completed job: {e}"),
        })?;

        // Remove from processing
        conn.hdel::<_, _, ()>(&self.processing_key, job_id).await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Failed to remove from processing: {e}"),
            }
        })?;

        // Remove job data
        conn.del::<_, ()>(Self::job_key(job_id)).await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Failed to delete job data: {e}"),
            }
        })?;

        // Increment success counter
        conn.incr::<_, _, ()>("queue:v1:stats:success", 1).await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Failed to update stats: {e}"),
            }
        })?;

        Ok(())
    }

    async fn mark_retry(&self, job_id: &str, next_retry_at: i64) -> Result<()> {
        let mut conn = self.conn.clone();

        // Remove from processing
        conn.hdel::<_, _, ()>(&self.processing_key, job_id).await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Failed to remove from processing: {e}"),
            }
        })?;

        // Add to retry queue with next_retry_at as score
        #[allow(clippy::cast_precision_loss)]
        // Reason: f64 precision is acceptable for Redis sorted set scores
        conn.zadd::<_, _, _, ()>(&self.retry_key, job_id, next_retry_at as f64)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to add to retry queue: {e}"),
            })?;

        // Increment retry counter
        conn.incr::<_, _, ()>("queue:v1:stats:retries", 1).await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Failed to update stats: {e}"),
            }
        })?;

        Ok(())
    }

    async fn mark_deadletter(&self, job_id: &str, reason: &str) -> Result<()> {
        let mut conn = self.conn.clone();

        // Remove from processing
        conn.hdel::<_, _, ()>(&self.processing_key, job_id).await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Failed to remove from processing: {e}"),
            }
        })?;

        // Store deadletter entry with reason
        #[allow(clippy::cast_precision_loss)]
        // Reason: f64 precision is acceptable for Redis sorted set scores
        let now = chrono::Utc::now().timestamp() as f64;
        let entry = format!("{}|{}", reason, chrono::Utc::now().timestamp());

        conn.zadd::<_, _, _, ()>(&self.deadletter_key, &job_id, now)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to add to deadletter queue: {e}"),
            })?;

        // Store reason
        conn.set_ex::<_, _, ()>(format!("job:v1:deadletter:reason:{job_id}"), entry, 86400)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to store deadletter reason: {e}"),
            })?;

        // Increment failed counter
        conn.incr::<_, _, ()>("queue:v1:stats:failed", 1).await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Failed to update stats: {e}"),
            }
        })?;

        Ok(())
    }

    async fn get_stats(&self) -> Result<QueueStats> {
        let mut conn = self.conn.clone();

        // Get queue lengths — log a warning on Redis errors so monitoring systems
        // can distinguish "empty queue" from "metrics unavailable".
        let pending_jobs: u64 = conn.zcard(&self.pending_key).await.unwrap_or_else(|e| {
            tracing::warn!(error = %e, key = %self.pending_key, "get_stats: zcard failed; pending count may be stale");
            0
        });

        let processing_jobs: u64 = conn.hlen(&self.processing_key).await.unwrap_or_else(|e| {
            tracing::warn!(error = %e, key = %self.processing_key, "get_stats: hlen failed; processing count may be stale");
            0
        });

        let retry_jobs: u64 = conn.zcard(&self.retry_key).await.unwrap_or_else(|e| {
            tracing::warn!(error = %e, key = %self.retry_key, "get_stats: zcard failed; retry count may be stale");
            0
        });

        // Get counters
        let successful_jobs: u64 = conn.get("queue:v1:stats:success").await.unwrap_or_else(|e| {
            tracing::warn!(error = %e, "get_stats: get failed; successful count may be stale");
            0
        });

        let failed_jobs: u64 = conn.zcard(&self.deadletter_key).await.unwrap_or_else(|e| {
            tracing::warn!(error = %e, key = %self.deadletter_key, "get_stats: zcard failed; failed count may be stale");
            0
        });

        // Get average processing time (stored in Redis as float)
        let avg_processing_time_ms: f64 =
            conn.get("queue:v1:stats:avg_processing_ms").await.unwrap_or_else(|e| {
                tracing::warn!(error = %e, "get_stats: get failed; avg_processing_ms may be stale");
                0.0
            });

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
