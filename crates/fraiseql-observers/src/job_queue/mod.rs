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

// Re-export public types
pub use self::traits::{JobQueue, JobQueueError};
use crate::config::ActionConfig;

/// Job state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum JobState {
    /// Pending: waiting to execute
    Pending,
    /// Running: currently executing
    Running,
    /// Completed: successfully executed
    Completed,
    /// Failed: max retries exhausted
    Failed,
    /// `DeadLettered`: moved to DLQ after permanent failure
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
    pub const fn mark_running(&mut self) {
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
mod tests;
