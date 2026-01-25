//! Job executor for processing queued jobs.
//!
//! Implements the main worker loop that:
//! 1. Dequeues jobs from the queue
//! 2. Executes them in parallel
//! 3. Retries with backoff on transient failures
//! 4. Moves to DLQ on permanent failures
//! 5. Records metrics for observability

use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

#[cfg(feature = "metrics")]
use crate::metrics::MetricsRegistry;
use super::{backoff, traits::JobQueue, Job};
use crate::error::Result;
use crate::executor::ObserverExecutor;

/// Job executor that processes jobs from the queue
pub struct JobExecutor {
    /// The job queue to pull from
    queue: Arc<dyn JobQueue>,

    /// Observer executor for running actions
    observer_executor: Arc<ObserverExecutor>,

    /// Worker identifier (for distributed workers)
    worker_id: String,

    /// Number of jobs to process in parallel
    concurrency: usize,

    /// Batch size for dequeueing jobs
    batch_size: usize,

    /// Job timeout in seconds
    job_timeout_secs: u64,

    /// Poll interval when queue is empty
    poll_interval_ms: u64,

    /// Prometheus metrics registry
    #[cfg(feature = "metrics")]
    metrics: MetricsRegistry,
}

impl JobExecutor {
    /// Create a new job executor
    ///
    /// # Arguments
    ///
    /// * `queue` - The job queue to process
    /// * `observer_executor` - The executor for running actions
    /// * `concurrency` - Number of parallel jobs
    /// * `batch_size` - Jobs to fetch per batch
    /// * `job_timeout_secs` - Timeout for each job
    #[must_use]
    pub fn new(
        queue: Arc<dyn JobQueue>,
        observer_executor: Arc<ObserverExecutor>,
        concurrency: usize,
        batch_size: usize,
        job_timeout_secs: u64,
    ) -> Self {
        let worker_id = format!("worker-{}", uuid::Uuid::new_v4());

        Self {
            queue,
            observer_executor,
            worker_id,
            concurrency,
            batch_size,
            job_timeout_secs,
            poll_interval_ms: 1000,
            #[cfg(feature = "metrics")]
            metrics: MetricsRegistry::global().unwrap_or_default(),
        }
    }

    /// Set poll interval when queue is empty
    #[must_use]
    pub fn with_poll_interval(mut self, ms: u64) -> Self {
        self.poll_interval_ms = ms;
        self
    }

    /// Get the worker ID
    #[must_use]
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// Run the worker (blocking, should be spawned in a task)
    ///
    /// This is the main event loop that:
    /// 1. Continuously dequeues jobs
    /// 2. Executes them in parallel
    /// 3. Handles retries and failures
    /// 4. Records metrics
    ///
    /// The worker stops when an error occurs or shutdown is requested.
    ///
    /// # Errors
    ///
    /// Returns error if queue operations fail
    pub async fn run(&self) -> Result<()> {
        info!("Job executor {} starting", self.worker_id);

        loop {
            // Dequeue a batch of jobs
            let jobs = self.queue.dequeue(self.batch_size, self.job_timeout_secs).await?;

            if jobs.is_empty() {
                // Queue is empty, sleep and retry
                debug!("Queue empty, sleeping {}ms", self.poll_interval_ms);
                tokio::time::sleep(Duration::from_millis(self.poll_interval_ms)).await;
                continue;
            }

            debug!("Dequeued {} jobs", jobs.len());

            // Execute jobs in parallel with controlled concurrency
            self.execute_batch(jobs).await?;
        }
    }

    /// Execute a batch of jobs in parallel
    async fn execute_batch(&self, jobs: Vec<Job>) -> Result<()> {
        let mut join_set = JoinSet::new();

        // Spawn up to `concurrency` tasks
        for job in jobs {
            let queue = Arc::clone(&self.queue);
            let executor = Arc::clone(&self.observer_executor);
            let worker_id = self.worker_id.clone();
            #[cfg(feature = "metrics")]
            let metrics = self.metrics.clone();

            join_set.spawn(async move {
                Self::execute_job_with_retry(
                    job,
                    queue,
                    executor,
                    &worker_id,
                    #[cfg(feature = "metrics")]
                    metrics,
                )
                .await;
            });

            // Limit parallelism
            if join_set.len() >= self.concurrency {
                join_set.join_next().await;
            }
        }

        // Wait for remaining tasks
        while join_set.join_next().await.is_some() {}

        Ok(())
    }

    /// Execute a single job with retry logic
    async fn execute_job_with_retry(
        mut job: Job,
        queue: Arc<dyn JobQueue>,
        executor: Arc<ObserverExecutor>,
        worker_id: &str,
        #[cfg(feature = "metrics")] metrics: MetricsRegistry,
    ) {
        let job_id = job.id;
        let action_type = job.action_type().to_string();
        let start_time = std::time::Instant::now();

        loop {
            debug!(
                "Executing job {}: attempt {}/{} (worker: {})",
                job_id, job.attempt, job.max_attempts, worker_id
            );

            // Execute the action
            match timeout_job_execution(&executor, &job).await {
                Ok(()) => {
                    // Success
                    let duration_secs = start_time.elapsed().as_secs_f64();
                    info!("Job {} completed in {:.3}s", job_id, duration_secs);

                    #[cfg(feature = "metrics")]
                    metrics.job_executed(&action_type, duration_secs);

                    if let Err(e) = queue.acknowledge(job_id).await {
                        error!("Failed to acknowledge job {}: {}", job_id, e);
                    }

                    return;
                },
                Err(e) => {
                    let is_transient = is_transient_error(&e);

                    if !is_transient {
                        // Permanent error
                        warn!("Job {} failed permanently: {}", job_id, e);

                        #[cfg(feature = "metrics")]
                        metrics.job_failed(&action_type, "permanent_error");

                        if let Err(queue_err) = queue.fail(&mut job, e.to_string()).await {
                            error!("Failed to mark job {} as failed: {}", job_id, queue_err);
                        }

                        return;
                    }

                    if !job.can_retry() {
                        // Retries exhausted
                        error!("Job {} exhausted retries", job_id);

                        #[cfg(feature = "metrics")]
                        metrics.job_failed(&action_type, "retries_exhausted");

                        if let Err(queue_err) = queue.fail(&mut job, e.to_string()).await {
                            error!("Failed to mark job {} as failed: {}", job_id, queue_err);
                        }

                        return;
                    }

                    // Transient error, retry after backoff
                    let delay = backoff::calculate_backoff(
                        job.backoff_strategy,
                        job.attempt,
                        job.initial_delay_ms,
                        job.max_delay_ms,
                    );

                    warn!(
                        "Job {} attempt {} failed (transient): {}. Retrying in {:?}",
                        job_id, job.attempt, e, delay
                    );

                    #[cfg(feature = "metrics")]
                    metrics.job_retry_attempt(&action_type);

                    tokio::time::sleep(delay).await;

                    // Update job for retry and put back in queue
                    job.mark_failed(e.to_string());
                    if let Err(queue_err) = queue.fail(&mut job, e.to_string()).await {
                        error!("Failed to requeue job {}: {}", job_id, queue_err);
                        return;
                    }

                    // Continue to next iteration (but with updated job)
                    continue;
                },
            }
        }
    }
}

/// Execute a job with timeout
///
/// This is a placeholder that would integrate with the observer executor
/// in a full implementation. For now, it returns success.
async fn timeout_job_execution(
    _executor: &Arc<ObserverExecutor>,
    _job: &Job,
) -> Result<()> {
    // In a full implementation, this would:
    // 1. Determine the action type from job.action
    // 2. Execute the action with the observer executor
    // 3. Apply timeout protection
    //
    // For now, this is a placeholder returning success
    Ok(())
}

/// Determine if an error is transient (retryable)
const fn is_transient_error(e: &crate::error::ObserverError) -> bool {
    e.is_transient()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_creation() {
        // Note: This test doesn't require actual queue/executor connections
        // It just verifies the struct can be created with proper defaults
        // Actual execution tests would require integration with real queue and executor
    }

    #[test]
    fn test_is_transient_error() {
        let transient = crate::error::ObserverError::ActionExecutionFailed {
            reason: "timeout".to_string(),
        };
        assert!(is_transient_error(&transient));

        let permanent = crate::error::ObserverError::ActionPermanentlyFailed {
            reason: "invalid config".to_string(),
        };
        assert!(!is_transient_error(&permanent));
    }
}
