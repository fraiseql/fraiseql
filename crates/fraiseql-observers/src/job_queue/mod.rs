//! Job queue system for asynchronous action execution.
//!
//! This module implements a Redis-backed distributed job queue that enables:
//! - Non-blocking action execution (fire-and-forget)
//! - Reliable retry logic with exponential backoff
//! - Dead letter queue for permanently failed jobs
//! - Job status tracking and monitoring
//!
//! # Architecture
//!
//! Jobs flow through three states:
//! 1. **Pending**: Waiting to execute (stored in Redis queue)
//! 2. **Running**: Currently executing (stored in Redis processing set with timeout)
//! 3. **Completed/Failed**: Terminal states
//!
//! Failed jobs are moved to the Dead Letter Queue (DLQ) after max retries.

pub mod traits;

#[cfg(feature = "queue")]
pub mod redis;

#[cfg(feature = "queue")]
pub mod dlq;

#[cfg(feature = "queue")]
pub mod executor;

pub mod backoff;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::ActionConfig;

// Re-export public types
pub use self::traits::{JobQueue, JobQueueError};

/// Job state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// Pending: waiting to execute
    Pending,
    /// Running: currently executing
    Running,
    /// Completed: successfully executed
    Completed,
    /// Failed: max retries exhausted
    Failed,
    /// DeadLettered: moved to DLQ after permanent failure
    DeadLettered,
}

impl JobState {
    /// Returns true if this is a terminal state
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, JobState::Completed | JobState::Failed | JobState::DeadLettered)
    }

    /// Returns true if this state indicates the job is still active
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, JobState::Pending | JobState::Running)
    }
}

/// Job execution attempt information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobAttempt {
    /// Attempt number (1-based)
    pub attempt: u32,
    /// When this attempt started
    pub started_at: DateTime<Utc>,
    /// Whether this attempt succeeded
    pub success: bool,
    /// Error message if it failed
    pub error: Option<String>,
}

/// Job to be executed asynchronously
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique job ID (for tracking)
    pub id: Uuid,

    /// Link to the originating event
    pub event_id: Uuid,

    /// Action to execute
    pub action: ActionConfig,

    /// When this job was created
    pub created_at: DateTime<Utc>,

    /// Current attempt number (1-based)
    pub attempt: u32,

    /// Maximum number of retry attempts
    pub max_attempts: u32,

    /// Backoff strategy for retries
    pub backoff_strategy: crate::config::BackoffStrategy,

    /// Initial delay in milliseconds for backoff calculation
    pub initial_delay_ms: u64,

    /// Maximum delay in milliseconds for backoff
    pub max_delay_ms: u64,

    /// Current state
    pub state: JobState,

    /// Error message from last failure (if any)
    pub last_error: Option<String>,

    /// Timestamp of when the job will be ready for next execution (for retries)
    pub retry_at: Option<DateTime<Utc>>,

    /// History of all attempts
    #[serde(default)]
    pub attempts: Vec<JobAttempt>,
}

impl Job {
    /// Create a new job with default retry configuration
    #[must_use]
    pub fn new(
        event_id: Uuid,
        action: ActionConfig,
        max_attempts: u32,
        backoff_strategy: crate::config::BackoffStrategy,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_id,
            action,
            created_at: Utc::now(),
            attempt: 1,
            max_attempts,
            backoff_strategy,
            initial_delay_ms: 100,
            max_delay_ms: 30000,
            state: JobState::Pending,
            last_error: None,
            retry_at: None,
            attempts: Vec::new(),
        }
    }

    /// Create a new job with explicit retry configuration
    #[must_use]
    pub fn with_config(
        event_id: Uuid,
        action: ActionConfig,
        max_attempts: u32,
        backoff_strategy: crate::config::BackoffStrategy,
        initial_delay_ms: u64,
        max_delay_ms: u64,
    ) -> Self {
        let mut job = Self::new(event_id, action, max_attempts, backoff_strategy);
        job.initial_delay_ms = initial_delay_ms;
        job.max_delay_ms = max_delay_ms;
        job
    }

    /// Check if this job can be retried
    #[must_use]
    pub const fn can_retry(&self) -> bool {
        self.attempt < self.max_attempts
    }

    /// Mark the job as completed
    pub fn mark_completed(&mut self) {
        self.state = JobState::Completed;
        self.attempts.push(JobAttempt {
            attempt: self.attempt,
            started_at: Utc::now(),
            success: true,
            error: None,
        });
    }

    /// Mark the job as failed with an error message
    pub fn mark_failed(&mut self, error: String) {
        self.last_error = Some(error.clone());
        self.attempts.push(JobAttempt {
            attempt: self.attempt,
            started_at: Utc::now(),
            success: false,
            error: Some(error),
        });

        if self.can_retry() {
            self.state = JobState::Pending;
            self.attempt += 1;
        } else {
            self.state = JobState::Failed;
        }
    }

    /// Mark the job as dead lettered
    pub fn mark_dead_lettered(&mut self, reason: String) {
        self.state = JobState::DeadLettered;
        self.last_error = Some(reason);
    }

    /// Mark the job as running
    pub fn mark_running(&mut self) {
        self.state = JobState::Running;
    }

    /// Get the action type for this job
    #[must_use]
    pub const fn action_type(&self) -> &str {
        match &self.action {
            ActionConfig::Webhook { .. } => "webhook",
            ActionConfig::Slack { .. } => "slack",
            ActionConfig::Email { .. } => "email",
            ActionConfig::Sms { .. } => "sms",
            ActionConfig::Push { .. } => "push",
            ActionConfig::Search { .. } => "search",
            ActionConfig::Cache { .. } => "cache",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_state_is_terminal() {
        assert!(JobState::Completed.is_terminal());
        assert!(JobState::Failed.is_terminal());
        assert!(JobState::DeadLettered.is_terminal());
        assert!(!JobState::Pending.is_terminal());
        assert!(!JobState::Running.is_terminal());
    }

    #[test]
    fn test_job_state_is_active() {
        assert!(JobState::Pending.is_active());
        assert!(JobState::Running.is_active());
        assert!(!JobState::Completed.is_active());
        assert!(!JobState::Failed.is_active());
        assert!(!JobState::DeadLettered.is_active());
    }

    #[test]
    fn test_job_creation() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        assert_eq!(job.event_id, event_id);
        assert_eq!(job.attempt, 1);
        assert_eq!(job.max_attempts, 3);
        assert_eq!(job.state, JobState::Pending);
        assert!(job.last_error.is_none());
        assert!(job.attempts.is_empty());
    }

    #[test]
    fn test_job_can_retry() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        assert!(job.can_retry());

        let mut job = job;
        job.attempt = 3;
        assert!(!job.can_retry());
    }

    #[test]
    fn test_job_mark_completed() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let mut job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        job.mark_completed();

        assert_eq!(job.state, JobState::Completed);
        assert_eq!(job.attempts.len(), 1);
        assert!(job.attempts[0].success);
        assert!(job.attempts[0].error.is_none());
    }

    #[test]
    fn test_job_mark_failed_with_retry() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let mut job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        job.mark_failed("connection timeout".to_string());

        assert_eq!(job.state, JobState::Pending); // Can retry
        assert_eq!(job.attempt, 2); // Incremented
        assert_eq!(job.last_error.as_ref().unwrap(), "connection timeout");
        assert_eq!(job.attempts.len(), 1);
        assert!(!job.attempts[0].success);
    }

    #[test]
    fn test_job_mark_failed_exhausted() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let mut job = Job::new(event_id, action, 2, crate::config::BackoffStrategy::Exponential);
        job.attempt = 2;

        job.mark_failed("connection timeout".to_string());

        assert_eq!(job.state, JobState::Failed); // Cannot retry anymore
        assert_eq!(job.last_error.as_ref().unwrap(), "connection timeout");
    }

    #[test]
    fn test_job_mark_dead_lettered() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let mut job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        job.mark_dead_lettered("invalid configuration".to_string());

        assert_eq!(job.state, JobState::DeadLettered);
        assert_eq!(job.last_error.as_ref().unwrap(), "invalid configuration");
    }

    #[test]
    fn test_job_serialization() {
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
        assert_eq!(job.state, deserialized.state);
    }

    #[test]
    fn test_job_action_type() {
        let event_id = Uuid::new_v4();

        let job_cache = Job::new(
            event_id,
            ActionConfig::Cache {
                key_pattern: "test:*".to_string(),
                action: "invalidate".to_string(),
            },
            3,
            crate::config::BackoffStrategy::Exponential,
        );
        assert_eq!(job_cache.action_type(), "cache");

        let job_webhook = Job::new(
            event_id,
            ActionConfig::Webhook {
                url: Some("http://example.com".to_string()),
                url_env: None,
                headers: Default::default(),
                body_template: None,
            },
            3,
            crate::config::BackoffStrategy::Exponential,
        );
        assert_eq!(job_webhook.action_type(), "webhook");
    }
}
