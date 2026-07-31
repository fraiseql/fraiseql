//! Job executor for processing queued jobs.
//!
//! Implements the main worker loop that:
//! 1. Dequeues jobs from the queue
//! 2. Executes them in parallel
//! 3. Retries with backoff on transient failures
//! 4. Moves to DLQ on permanent failures
//! 5. Records metrics for observability

use std::{sync::Arc, time::Duration};

use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

use super::{Job, backoff, traits::JobQueue};
#[cfg(feature = "metrics")]
use crate::metrics::MetricsRegistry;
use crate::{error::Result, executor::ObserverExecutor};

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
    pub const fn with_poll_interval(mut self, ms: u64) -> Self {
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
            let job_timeout_secs = self.job_timeout_secs;
            #[cfg(feature = "metrics")]
            let metrics = self.metrics.clone();

            join_set.spawn(async move {
                Self::execute_job_with_retry(
                    job,
                    queue,
                    executor,
                    &worker_id,
                    job_timeout_secs,
                    #[cfg(feature = "metrics")]
                    metrics,
                )
                .await;
            });

            // Limit parallelism
            if join_set.len() >= self.concurrency {
                if let Some(res) = join_set.join_next().await {
                    self.handle_join_outcome(res);
                }
            }
        }

        // Wait for remaining tasks
        while let Some(res) = join_set.join_next().await {
            self.handle_join_outcome(res);
        }

        Ok(())
    }

    /// Log a panic / cancellation outcome from a spawned worker task (F014).
    ///
    /// `execute_job_with_retry` returns `()`, so the inner result is `Ok(())`
    /// on every successful completion; only `JoinError`s carry information.
    /// Panics are routed through the prometheus `job_failed` counter (with the
    /// `panic` error label) when the `metrics` feature is enabled.
    fn handle_join_outcome(&self, res: std::result::Result<(), tokio::task::JoinError>) {
        match res {
            Ok(()) => {},
            Err(je) if je.is_panic() => {
                error!(
                    worker = %self.worker_id,
                    error = %je,
                    "job worker task panicked"
                );
                #[cfg(feature = "metrics")]
                self.metrics.job_failed("unknown", "panic");
            },
            Err(je) => {
                warn!(
                    worker = %self.worker_id,
                    error = %je,
                    "job worker task cancelled or otherwise failed to join"
                );
            },
        }
    }

    /// Execute a single dequeued job: one real dispatch attempt, then a
    /// terminal or retry outcome recorded in the queue (#844).
    ///
    /// The job is only [`JobQueue::acknowledge`]d — which destroys the payload —
    /// after its action genuinely dispatched. Every failure goes through
    /// [`JobQueue::fail`], which either re-enqueues the job (transient error,
    /// retry budget left; `fail` itself records the attempt and increments the
    /// counter — calling `mark_failed` here too would double-count, the old
    /// `#844` secondary bug) or moves it to the DLQ. Retries re-enter through
    /// the queue rather than looping in-process, so the attempt count stays
    /// durable and a crashed worker never strands an invisible in-memory retry.
    async fn execute_job_with_retry(
        mut job: Job,
        queue: Arc<dyn JobQueue>,
        executor: Arc<ObserverExecutor>,
        worker_id: &str,
        job_timeout_secs: u64,
        #[cfg(feature = "metrics")] metrics: MetricsRegistry,
    ) {
        let job_id = job.id;
        #[cfg(feature = "metrics")]
        let action_type = job.action_type().to_string();
        let start_time = std::time::Instant::now();

        debug!(
            "Executing job {}: attempt {}/{} (worker: {})",
            job_id, job.attempt, job.max_attempts, worker_id
        );

        match timeout_job_execution(&executor, &job, job_timeout_secs).await {
            Ok(()) => {
                let duration_secs = start_time.elapsed().as_secs_f64();
                info!("Job {} completed in {:.3}s", job_id, duration_secs);

                #[cfg(feature = "metrics")]
                metrics.job_executed(&action_type, duration_secs);

                if let Err(e) = queue.acknowledge(job_id).await {
                    error!("Failed to acknowledge job {}: {}", job_id, e);
                }
            },
            Err(e) => {
                let is_transient = is_transient_error(&e);

                if is_transient && job.can_retry() {
                    // Transient failure with retry budget left: wait out the
                    // backoff, then hand the job back to the queue. `fail`
                    // records the attempt and re-enqueues atomically.
                    let delay = backoff::calculate_backoff(
                        job.backoff_strategy,
                        job.attempt,
                        job.initial_delay_ms,
                        job.max_delay_ms,
                    );

                    warn!(
                        "Job {} attempt {}/{} failed (transient): {}. Re-queueing after {:?}",
                        job_id, job.attempt, job.max_attempts, e, delay
                    );

                    #[cfg(feature = "metrics")]
                    metrics.job_retry_attempt(&action_type);

                    tokio::time::sleep(delay).await;

                    if let Err(queue_err) = queue.fail(&mut job, e.to_string()).await {
                        error!("Failed to requeue job {}: {}", job_id, queue_err);
                    }
                    return;
                }

                // Terminal failure: a permanent error (retrying would fail
                // identically) or an exhausted retry budget. Route to the DLQ —
                // recorded, never destroyed.
                if is_transient {
                    error!("Job {} exhausted retries: {}", job_id, e);
                    #[cfg(feature = "metrics")]
                    metrics.job_failed(&action_type, "retries_exhausted");
                } else {
                    warn!("Job {} failed permanently: {}", job_id, e);
                    job.exhaust_retry_budget();
                    #[cfg(feature = "metrics")]
                    metrics.job_failed(&action_type, "permanent_error");
                }

                if let Err(queue_err) = queue.fail(&mut job, e.to_string()).await {
                    error!("Failed to dead-letter job {}: {}", job_id, queue_err);
                }
            },
        }
    }
}

/// Dispatch a job's action against its event, bounded by the worker's job
/// timeout (#844).
///
/// A timeout maps to a transient [`ObserverError::ActionExecutionFailed`] so
/// the job is retried per its policy rather than destroyed or — as the
/// pre-#844 placeholder did — reported executed without any dispatch.
async fn timeout_job_execution(
    executor: &Arc<ObserverExecutor>,
    job: &Job,
    timeout_secs: u64,
) -> Result<()> {
    match tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        executor.execute_action_internal(&job.action, &job.event),
    )
    .await
    {
        Ok(Ok(_result)) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => Err(crate::error::ObserverError::ActionExecutionFailed {
            reason: format!(
                "job {} ({}) timed out after {timeout_secs}s",
                job.id,
                job.action_type()
            ),
        }),
    }
}

/// Determine if an error is transient (retryable)
pub(super) const fn is_transient_error(e: &crate::error::ObserverError) -> bool {
    e.is_transient()
}
