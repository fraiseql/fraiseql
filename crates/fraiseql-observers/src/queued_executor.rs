//! Queued observer executor that asynchronously processes actions via job queue.
//!
//! This module implements a wrapper around `ObserverExecutor` that:
//! 1. Evaluates event matching and conditions (fast, in-memory)
//! 2. Queues actions as jobs instead of executing immediately
//! 3. Returns job IDs for status tracking
//! 4. Enables background job workers to execute actions with retry logic
//!
//! # Architecture
//!
//! ```text
//! Event arrives
//!     ↓
//! Match & condition evaluation (fast, synchronous)
//!     ↓
//! Create Job for each action (with retry config)
//!     ↓
//! Enqueue to job queue (Redis-backed, persistent)
//!     ↓
//! Return immediately with job IDs
//!     ↓
//! Background workers process jobs asynchronously
//! ```

use std::sync::Arc;
use uuid::Uuid;

#[cfg(feature = "metrics")]
use crate::metrics::MetricsRegistry;
#[cfg(feature = "queue")]
use crate::job_queue::{Job, JobQueue};
use crate::{
    executor::ExecutionSummary,
    event::EntityEvent,
    matcher::EventMatcher,
    condition::ConditionParser,
    error::Result,
};

#[cfg(feature = "queue")]
/// Queued observer executor for asynchronous action processing
pub struct QueuedObserverExecutor {
    /// Event-to-observer matcher
    matcher: Arc<EventMatcher>,

    /// Condition parser and evaluator
    condition_parser: Arc<ConditionParser>,

    /// Job queue for asynchronous action execution
    job_queue: Arc<dyn JobQueue>,

    /// Prometheus metrics registry
    #[cfg(feature = "metrics")]
    metrics: MetricsRegistry,
}

#[cfg(feature = "queue")]
impl QueuedObserverExecutor {
    /// Create a new queued executor
    ///
    /// # Arguments
    ///
    /// * `matcher` - Event matcher for finding matching observers
    /// * `job_queue` - Queue for enqueuing action jobs
    #[must_use]
    pub fn new(matcher: EventMatcher, job_queue: Arc<dyn JobQueue>) -> Self {
        Self {
            matcher: Arc::new(matcher),
            condition_parser: Arc::new(ConditionParser::new()),
            job_queue,
            #[cfg(feature = "metrics")]
            metrics: MetricsRegistry::global().unwrap_or_default(),
        }
    }

    /// Process an event by queuing matching actions as jobs
    ///
    /// Unlike the synchronous `ObserverExecutor::process_event()`, this:
    /// - Evaluates conditions synchronously (fast)
    /// - Queues actions asynchronously (fire-and-forget)
    /// - Returns immediately with job IDs for tracking
    ///
    /// # Arguments
    ///
    /// * `event` - The entity event to process
    ///
    /// # Returns
    ///
    /// An `ExecutionSummary` with:
    /// - `successful_actions`: Number of jobs successfully queued
    /// - `failed_actions`: Number of jobs that failed to queue
    /// - `job_ids`: List of queued job UUIDs (stored in ExecutionSummary errors for now)
    pub async fn process_event(&self, event: &EntityEvent) -> Result<QueuedExecutionSummary> {
        let mut summary = QueuedExecutionSummary::new();
        let matching_observers = self.matcher.find_matches(event);

        tracing::debug!(
            "Processing event {} for queuing (entity_type: {}, event_type: {:?})",
            event.id,
            event.entity_type,
            event.event_type
        );
        tracing::debug!(
            "Found {} matching observers for queuing",
            matching_observers.len()
        );

        for observer in matching_observers {
            // Skip if condition is not met
            if let Some(condition) = &observer.condition {
                match self.condition_parser.parse_and_evaluate(condition, event) {
                    Ok(true) => {
                        tracing::debug!("Condition passed, queuing actions");
                    },
                    Ok(false) => {
                        tracing::debug!("Condition failed, skipping observer");
                        summary.conditions_skipped += 1;
                        continue;
                    },
                    Err(e) => {
                        tracing::error!("Condition evaluation error: {}", e);
                        summary.errors.push(e.to_string());
                        continue;
                    },
                }
            }

            // Queue actions for this observer
            for action in &observer.actions {
                let job = Job::with_config(
                    event.id,
                    action.clone(),
                    observer.retry.max_attempts,
                    observer.retry.backoff_strategy,
                    observer.retry.initial_delay_ms,
                    observer.retry.max_delay_ms,
                );

                let job_id = job.id;
                let action_type_str = action.action_type();
                match self.job_queue.enqueue(job).await {
                    Ok(()) => {
                        tracing::debug!(
                            "Queued action {} (job_id: {}) for event {}",
                            action_type_str,
                            job_id,
                            event.id
                        );
                        #[cfg(feature = "metrics")]
                        self.metrics.job_queued();
                        summary.jobs_queued += 1;
                        summary.job_ids.push(job_id);
                    },
                    Err(e) => {
                        tracing::error!(
                            "Failed to queue action {} (job_id: {}): {e}",
                            action_type_str,
                            job_id,
                        );
                        summary.queueing_errors += 1;
                        summary.errors.push(format!("Failed to queue action: {e}"));
                    },
                }
            }
        }

        Ok(summary)
    }
}

/// Summary of queued event processing results
#[derive(Debug, Clone, Default)]
pub struct QueuedExecutionSummary {
    /// Number of actions successfully queued
    pub jobs_queued: usize,

    /// Number of actions that failed to queue
    pub queueing_errors: usize,

    /// Number of observers skipped due to condition
    pub conditions_skipped: usize,

    /// List of queued job IDs
    pub job_ids: Vec<Uuid>,

    /// Other errors encountered
    pub errors: Vec<String>,
}

impl QueuedExecutionSummary {
    /// Create a new empty summary
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if queuing was successful
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.queueing_errors == 0 && self.errors.is_empty()
    }

    /// Get total jobs processed
    #[must_use]
    pub const fn total_jobs(&self) -> usize {
        self.jobs_queued + self.queueing_errors
    }

    /// Convert to standard ExecutionSummary for compatibility
    #[must_use]
    pub fn to_execution_summary(&self) -> ExecutionSummary {
        ExecutionSummary {
            successful_actions: self.jobs_queued,
            failed_actions: self.queueing_errors,
            conditions_skipped: self.conditions_skipped,
            total_duration_ms: 0.0,
            dlq_errors: 0,
            errors: self.errors.clone(),
            duplicate_skipped: false,
            cache_hits: 0,
            cache_misses: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queued_summary_creation() {
        let summary = QueuedExecutionSummary::new();
        assert_eq!(summary.jobs_queued, 0);
        assert_eq!(summary.queueing_errors, 0);
        assert!(summary.is_success());
    }

    #[test]
    fn test_queued_summary_success() {
        let summary = QueuedExecutionSummary {
            jobs_queued: 5,
            queueing_errors: 0,
            conditions_skipped: 0,
            job_ids: vec![],
            errors: vec![],
        };
        assert!(summary.is_success());
        assert_eq!(summary.total_jobs(), 5);
    }

    #[test]
    fn test_queued_summary_with_errors() {
        let mut summary = QueuedExecutionSummary::new();
        summary.queueing_errors = 2;
        summary.errors.push("failed to connect".to_string());
        assert!(!summary.is_success());
        assert_eq!(summary.total_jobs(), 2);
    }

    #[test]
    fn test_to_execution_summary() {
        let mut summary = QueuedExecutionSummary::new();
        summary.jobs_queued = 5;
        summary.queueing_errors = 1;
        summary.conditions_skipped = 2;
        summary.errors.push("error1".to_string());

        let exec_summary = summary.to_execution_summary();
        assert_eq!(exec_summary.successful_actions, 5);
        assert_eq!(exec_summary.failed_actions, 1);
        assert_eq!(exec_summary.conditions_skipped, 2);
        assert_eq!(exec_summary.errors.len(), 1);
    }
}
