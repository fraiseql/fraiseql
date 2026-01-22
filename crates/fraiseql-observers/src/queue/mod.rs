//! Job queue system for async long-running action processing.
//!
//! This module provides a durable job queue with worker pools and resilient retry logic,
//! enabling long-running actions to be processed asynchronously without blocking events.
//!
//! # Problem Solved
//!
//! Without job queue:
//! - Long-running actions block event processing
//! - No way to process actions with long timeouts
//! - Failed actions don't retry automatically
//! - Worker capacity unconstrained
//!
//! With job queue:
//! - Actions enqueued asynchronously
//! - Worker pool processes jobs concurrently
//! - Exponential backoff retries (configurable)
//! - Bounded worker pools prevent resource exhaustion
//!
//! # Architecture
//!
//! ```text
//! EntityEvent
//!     ↓
//! Observer matched
//!     ↓
//! Action execution
//!     ├─ Quick actions → inline
//!     └─ Slow actions → Job Queue
//!         ↓
//! Job fetched by Worker
//!     ↓
//! Executed with retry logic
//!     ├─ Success → Complete
//!     └─ Failure → Retry or Dead Letter Queue
//! ```
//!
//! # Example
//!
//! ```ignore
//! // Enqueue a job
//! let job = Job {
//!     id: "job-123".to_string(),
//!     action_id: "send_batch_email".to_string(),
//!     event: entity_event,
//!     action_config: action,
//!     attempt: 1,
//!     created_at: now,
//!     next_retry_at: now,
//! };
//!
//! queue.enqueue(&job).await?;
//!
//! // Process with worker
//! let worker = JobWorker::new(queue, executor, concurrency);
//! worker.run().await?;
//! ```

#[cfg(feature = "queue")]
pub mod redis;
pub mod worker;

use crate::config::ActionConfig;
use crate::error::Result;
use crate::event::EntityEvent;
use crate::traits::ActionResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Status of a job in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    /// Job is waiting to be processed
    Pending,
    /// Job is currently being processed
    Processing,
    /// Job completed successfully
    Success,
    /// Job failed and cannot be retried
    Failed,
    /// Job failed and is waiting for retry
    Retrying,
    /// Job failed after max attempts (manual retry needed)
    Deadletter,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Processing => write!(f, "processing"),
            Self::Success => write!(f, "success"),
            Self::Failed => write!(f, "failed"),
            Self::Retrying => write!(f, "retrying"),
            Self::Deadletter => write!(f, "deadletter"),
        }
    }
}

/// A unit of work to be processed by the job queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique job identifier
    pub id: String,
    /// Action type identifier
    pub action_id: String,
    /// Event that triggered the job
    pub event: EntityEvent,
    /// Action configuration
    pub action_config: ActionConfig,
    /// Current attempt number (1-indexed)
    pub attempt: u32,
    /// Unix timestamp when job was created
    pub created_at: i64,
    /// Unix timestamp when job should be retried (if failed)
    pub next_retry_at: i64,
}

/// Result of processing a job.
#[derive(Debug, Clone)]
pub struct JobResult {
    /// Job ID
    pub job_id: String,
    /// Final status
    pub status: JobStatus,
    /// Action execution result
    pub action_result: ActionResult,
    /// Total attempts made
    pub attempts: u32,
    /// Total processing duration in milliseconds
    pub duration_ms: f64,
}

/// Statistics about queue health and performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    /// Number of jobs waiting to be processed
    pub pending_jobs: u64,
    /// Number of jobs currently being processed
    pub processing_jobs: u64,
    /// Number of jobs waiting for retry
    pub retry_jobs: u64,
    /// Number of successfully completed jobs
    pub successful_jobs: u64,
    /// Number of failed jobs in dead letter queue
    pub failed_jobs: u64,
    /// Average job processing time in milliseconds
    pub avg_processing_time_ms: f64,
}

/// Persistent job queue abstraction.
///
/// Implementations handle durable storage and retrieval of jobs.
/// This trait is object-safe and can be used as `Arc<dyn JobQueue>`.
#[async_trait]
pub trait JobQueue: Send + Sync + Clone {
    /// Enqueue a new job for processing.
    ///
    /// Returns the job ID if successful.
    ///
    /// # Errors
    ///
    /// Returns error if enqueuing fails.
    async fn enqueue(&self, job: &Job) -> Result<String>;

    /// Dequeue next job for processing.
    ///
    /// Returns None if no jobs are available.
    /// Atomically marks job as processing.
    ///
    /// # Arguments
    ///
    /// * `worker_id` - Identifier for the worker claiming the job
    ///
    /// # Errors
    ///
    /// Returns error if dequeuing fails.
    async fn dequeue(&self, worker_id: &str) -> Result<Option<Job>>;

    /// Mark a job as currently processing.
    ///
    /// # Errors
    ///
    /// Returns error if operation fails.
    async fn mark_processing(&self, job_id: &str) -> Result<()>;

    /// Mark a job as successfully completed.
    ///
    /// # Errors
    ///
    /// Returns error if operation fails.
    async fn mark_success(&self, job_id: &str, result: &JobResult) -> Result<()>;

    /// Mark a job for retry after a delay.
    ///
    /// # Arguments
    ///
    /// * `job_id` - Job to retry
    /// * `next_retry_at` - Unix timestamp when to retry
    ///
    /// # Errors
    ///
    /// Returns error if operation fails.
    async fn mark_retry(&self, job_id: &str, next_retry_at: i64) -> Result<()>;

    /// Move a job to the dead letter queue (manual retry needed).
    ///
    /// # Arguments
    ///
    /// * `job_id` - Job to move
    /// * `reason` - Error reason
    ///
    /// # Errors
    ///
    /// Returns error if operation fails.
    async fn mark_deadletter(&self, job_id: &str, reason: &str) -> Result<()>;

    /// Get queue statistics.
    ///
    /// # Errors
    ///
    /// Returns error if operation fails.
    async fn get_stats(&self) -> Result<QueueStats>;
}

/// Retry policy determines if and when a job should be retried.
pub trait RetryPolicy: Send + Sync + Clone {
    /// Check if job should be retried given the current attempt number.
    ///
    /// # Arguments
    ///
    /// * `attempt` - Current attempt number (1-indexed)
    fn should_retry(&self, attempt: u32) -> bool;

    /// Get backoff delay in milliseconds for the next retry.
    ///
    /// # Arguments
    ///
    /// * `attempt` - Current attempt number (1-indexed)
    fn get_backoff_ms(&self, attempt: u32) -> u64;
}

/// Exponential backoff retry policy.
///
/// Retries with exponentially increasing delay:
/// `delay = initial_delay * multiplier^attempt`, capped at `max_delay`.
#[derive(Debug, Clone)]
pub struct ExponentialBackoffPolicy {
    /// Maximum number of attempts
    pub max_attempts: u32,
    /// Initial delay in milliseconds
    pub initial_delay_ms: u64,
    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,
    /// Exponential multiplier (typically 2.0)
    pub multiplier: f64,
}

impl Default for ExponentialBackoffPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000,      // 1 second
            max_delay_ms: 60000,         // 60 seconds
            multiplier: 2.0,
        }
    }
}

impl RetryPolicy for ExponentialBackoffPolicy {
    fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }

    fn get_backoff_ms(&self, attempt: u32) -> u64 {
        let delay = (self.initial_delay_ms as f64 *
                    self.multiplier.powi((attempt - 1) as i32)) as u64;
        delay.min(self.max_delay_ms)
    }
}

/// Linear backoff retry policy.
///
/// Retries with linearly increasing delay:
/// `delay = delay_increment * attempt`, capped at `max_delay`.
#[derive(Debug, Clone)]
pub struct LinearBackoffPolicy {
    /// Maximum number of attempts
    pub max_attempts: u32,
    /// Delay increment in milliseconds per attempt
    pub delay_increment_ms: u64,
    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,
}

impl Default for LinearBackoffPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            delay_increment_ms: 5000,    // 5 seconds per attempt
            max_delay_ms: 30000,         // 30 seconds max
        }
    }
}

impl RetryPolicy for LinearBackoffPolicy {
    fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }

    fn get_backoff_ms(&self, attempt: u32) -> u64 {
        let delay = self.delay_increment_ms * attempt as u64;
        delay.min(self.max_delay_ms)
    }
}

/// Fixed backoff retry policy.
///
/// Retries with constant delay: `delay = delay_ms`.
#[derive(Debug, Clone)]
pub struct FixedBackoffPolicy {
    /// Maximum number of attempts
    pub max_attempts: u32,
    /// Fixed delay in milliseconds
    pub delay_ms: u64,
}

impl Default for FixedBackoffPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            delay_ms: 5000,              // 5 seconds
        }
    }
}

impl RetryPolicy for FixedBackoffPolicy {
    fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }

    fn get_backoff_ms(&self, _attempt: u32) -> u64 {
        self.delay_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_creation() {
        let job = Job {
            id: "job-1".to_string(),
            action_id: "send_email".to_string(),
            event: EntityEvent::new(
                crate::event::EventKind::Created,
                "Order".to_string(),
                uuid::Uuid::new_v4(),
                serde_json::json!({}),
            ),
            action_config: ActionConfig::Webhook {
                url: Some("http://localhost:8000".to_string()),
                url_env: None,
                headers: std::collections::HashMap::new(),
                body_template: None,
            },
            attempt: 1,
            created_at: chrono::Utc::now().timestamp(),
            next_retry_at: chrono::Utc::now().timestamp(),
        };

        assert_eq!(job.id, "job-1");
        assert_eq!(job.attempt, 1);
    }

    #[test]
    fn test_job_status_display() {
        assert_eq!(JobStatus::Pending.to_string(), "pending");
        assert_eq!(JobStatus::Processing.to_string(), "processing");
        assert_eq!(JobStatus::Success.to_string(), "success");
        assert_eq!(JobStatus::Failed.to_string(), "failed");
        assert_eq!(JobStatus::Retrying.to_string(), "retrying");
        assert_eq!(JobStatus::Deadletter.to_string(), "deadletter");
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        let policy = ExponentialBackoffPolicy {
            max_attempts: 5,
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            multiplier: 2.0,
        };

        // Attempt 1: 1000ms
        assert_eq!(policy.get_backoff_ms(1), 1000);
        // Attempt 2: 2000ms
        assert_eq!(policy.get_backoff_ms(2), 2000);
        // Attempt 3: 4000ms
        assert_eq!(policy.get_backoff_ms(3), 4000);
        // Attempt 4: 8000ms
        assert_eq!(policy.get_backoff_ms(4), 8000);
    }

    #[test]
    fn test_exponential_backoff_cap() {
        let policy = ExponentialBackoffPolicy {
            max_attempts: 10,
            initial_delay_ms: 1000,
            max_delay_ms: 10000,
            multiplier: 2.0,
        };

        // Normally would be 32000ms (1000 * 2^5), but capped at 10000ms
        assert_eq!(policy.get_backoff_ms(6), 10000);
        assert_eq!(policy.get_backoff_ms(7), 10000);
    }

    #[test]
    fn test_exponential_backoff_should_retry() {
        let policy = ExponentialBackoffPolicy {
            max_attempts: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            multiplier: 2.0,
        };

        // Attempts 1-2 should retry
        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
        // Attempt 3+ should not retry
        assert!(!policy.should_retry(3));
        assert!(!policy.should_retry(4));
    }

    #[test]
    fn test_linear_backoff_calculation() {
        let policy = LinearBackoffPolicy {
            max_attempts: 5,
            delay_increment_ms: 5000,
            max_delay_ms: 30000,
        };

        // Attempt 1: 5000ms
        assert_eq!(policy.get_backoff_ms(1), 5000);
        // Attempt 2: 10000ms
        assert_eq!(policy.get_backoff_ms(2), 10000);
        // Attempt 3: 15000ms
        assert_eq!(policy.get_backoff_ms(3), 15000);
    }

    #[test]
    fn test_linear_backoff_cap() {
        let policy = LinearBackoffPolicy {
            max_attempts: 10,
            delay_increment_ms: 5000,
            max_delay_ms: 30000,
        };

        // Normally would be 35000ms (5000 * 7), but capped at 30000ms
        assert_eq!(policy.get_backoff_ms(7), 30000);
        assert_eq!(policy.get_backoff_ms(8), 30000);
    }

    #[test]
    fn test_fixed_backoff_calculation() {
        let policy = FixedBackoffPolicy {
            max_attempts: 5,
            delay_ms: 5000,
        };

        // All attempts have same delay
        assert_eq!(policy.get_backoff_ms(1), 5000);
        assert_eq!(policy.get_backoff_ms(2), 5000);
        assert_eq!(policy.get_backoff_ms(3), 5000);
    }

    #[test]
    fn test_fixed_backoff_should_retry() {
        let policy = FixedBackoffPolicy {
            max_attempts: 3,
            delay_ms: 5000,
        };

        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
    }

    #[test]
    fn test_default_policies() {
        let exp = ExponentialBackoffPolicy::default();
        assert_eq!(exp.max_attempts, 3);

        let lin = LinearBackoffPolicy::default();
        assert_eq!(lin.max_attempts, 3);

        let fixed = FixedBackoffPolicy::default();
        assert_eq!(fixed.max_attempts, 3);
    }
}
