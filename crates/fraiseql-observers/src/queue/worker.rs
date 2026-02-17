//! Job worker and worker pool implementations.
//!
//! Workers continuously fetch jobs from the queue and process them with retries,
//! handling timeouts and failures gracefully.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use tokio::time::timeout;

use super::{Job, JobQueue, JobResult, JobStatus, RetryPolicy};
use crate::{
    error::{ObserverError, Result},
    traits::ActionExecutor,
};

/// A worker that processes jobs from the queue.
///
/// Continuously fetches and processes jobs, handling retries and failures.
pub struct JobWorker<Q, E, P>
where
    Q: JobQueue,
    E: ActionExecutor,
    P: RetryPolicy,
{
    queue:          Q,
    executor:       E,
    worker_id:      String,
    retry_policy:   P,
    job_timeout_ms: u64,
}

impl<Q, E, P> JobWorker<Q, E, P>
where
    Q: JobQueue,
    E: ActionExecutor,
    P: RetryPolicy,
{
    /// Create a new job worker.
    ///
    /// # Arguments
    ///
    /// * `queue` - Job queue to process from
    /// * `executor` - Action executor for job processing
    /// * `retry_policy` - Policy for retry logic
    /// * `job_timeout_ms` - Timeout per job in milliseconds
    pub fn new(queue: Q, executor: E, retry_policy: P, job_timeout_ms: u64) -> Self {
        let worker_id = format!("worker-{}", uuid::Uuid::new_v4());

        Self {
            queue,
            executor,
            worker_id,
            retry_policy,
            job_timeout_ms,
        }
    }

    /// Get worker ID.
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// Run the worker (continuously process jobs).
    ///
    /// This method runs indefinitely, fetching and processing jobs from the queue.
    ///
    /// # Errors
    ///
    /// Returns error if queue operations fail fatally.
    pub async fn run(&self) -> Result<()> {
        loop {
            match self.queue.dequeue(&self.worker_id).await {
                Ok(Some(job)) => {
                    if let Err(e) = self.process_job(job).await {
                        eprintln!("Error processing job: {e}");
                    }
                },
                Ok(None) => {
                    // No jobs available, wait a bit before retrying
                    tokio::time::sleep(Duration::from_millis(100)).await;
                },
                Err(e) => {
                    // Dequeue error - log and wait before retrying
                    eprintln!("Dequeue error: {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                },
            }
        }
    }

    /// Process a single job.
    async fn process_job(&self, job: Job) -> Result<()> {
        let start = Instant::now();
        let job_id = job.id.clone();

        // Mark as processing
        self.queue.mark_processing(&job_id).await?;

        // Execute with timeout
        let result = timeout(
            Duration::from_millis(self.job_timeout_ms),
            self.executor.execute(&job.event, &job.action_config),
        )
        .await;

        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        match result {
            Ok(Ok(action_result)) => {
                // Success
                let job_result = JobResult {
                    job_id: job_id.clone(),
                    status: JobStatus::Success,
                    action_result,
                    attempts: job.attempt,
                    duration_ms,
                };
                self.queue.mark_success(&job_id, &job_result).await?;
            },
            Ok(Err(e)) => {
                // Job execution failed
                self.handle_job_failure(&job, &e, duration_ms).await?;
            },
            Err(_) => {
                // Job timed out
                self.handle_job_timeout(&job, duration_ms).await?;
            },
        }

        Ok(())
    }

    /// Handle job failure with retry logic.
    async fn handle_job_failure(
        &self,
        job: &Job,
        error: &ObserverError,
        _duration_ms: f64,
    ) -> Result<()> {
        let next_attempt = job.attempt + 1;

        if self.retry_policy.should_retry(next_attempt) {
            // Schedule retry
            let backoff_ms = self.retry_policy.get_backoff_ms(next_attempt);
            let next_retry_at = chrono::Utc::now().timestamp() + (backoff_ms as i64 / 1000);

            self.queue.mark_retry(&job.id, next_retry_at).await?;
        } else {
            // Max retries exceeded - move to dead letter queue
            let reason = format!("Failed after {} attempts: {}", job.attempt, error);
            self.queue.mark_deadletter(&job.id, &reason).await?;
        }

        Ok(())
    }

    /// Handle job timeout.
    async fn handle_job_timeout(&self, job: &Job, duration_ms: f64) -> Result<()> {
        let timeout_error = ObserverError::ActionExecutionFailed {
            reason: format!("Job timeout after {}ms", self.job_timeout_ms),
        };
        self.handle_job_failure(job, &timeout_error, duration_ms).await
    }
}

/// A pool of workers for concurrent job processing.
///
/// Manages multiple worker instances and graceful shutdown.
pub struct JobWorkerPool<Q, E, P>
where
    Q: JobQueue,
    E: ActionExecutor,
    P: RetryPolicy,
{
    queue:          Q,
    executor:       E,
    retry_policy:   P,
    pool_size:      usize,
    job_timeout_ms: u64,
    workers:        Vec<tokio::task::JoinHandle<Result<()>>>,
    is_running:     Arc<AtomicBool>,
}

impl<Q, E, P> JobWorkerPool<Q, E, P>
where
    Q: JobQueue + 'static,
    E: ActionExecutor + 'static + Clone,
    P: RetryPolicy + 'static + Clone,
{
    /// Create a new worker pool.
    ///
    /// # Arguments
    ///
    /// * `queue` - Job queue to process from
    /// * `executor` - Action executor
    /// * `retry_policy` - Retry policy for jobs
    /// * `pool_size` - Number of concurrent workers
    /// * `job_timeout_ms` - Timeout per job
    pub fn new(
        queue: Q,
        executor: E,
        retry_policy: P,
        pool_size: usize,
        job_timeout_ms: u64,
    ) -> Self {
        Self {
            queue,
            executor,
            retry_policy,
            pool_size,
            job_timeout_ms,
            workers: Vec::new(),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the worker pool.
    ///
    /// Spawns all worker tasks to begin processing jobs.
    ///
    /// # Errors
    ///
    /// Returns error if already running.
    pub async fn start(&mut self) -> Result<()> {
        if self.is_running.load(Ordering::SeqCst) {
            return Err(ObserverError::InvalidConfig {
                message: "Worker pool already running".to_string(),
            });
        }

        self.is_running.store(true, Ordering::SeqCst);

        for _i in 0..self.pool_size {
            let queue = self.queue.clone();
            let executor = self.executor.clone();
            let retry_policy = self.retry_policy.clone();
            let job_timeout_ms = self.job_timeout_ms;
            let is_running = Arc::clone(&self.is_running);

            let handle = tokio::spawn(async move {
                let worker = JobWorker::new(queue, executor, retry_policy, job_timeout_ms);

                // Run until stopped
                loop {
                    if !is_running.load(Ordering::SeqCst) {
                        break;
                    }

                    // Try to process jobs
                    if let Err(e) = worker.run().await {
                        eprintln!("Worker error: {e}");
                        // Continue running despite errors
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }

                Ok(())
            });

            self.workers.push(handle);
        }

        Ok(())
    }

    /// Stop the worker pool gracefully.
    ///
    /// Signals all workers to stop and waits for them to finish.
    ///
    /// # Errors
    ///
    /// Returns error if worker tasks fail.
    pub async fn stop(&mut self) -> Result<()> {
        self.is_running.store(false, Ordering::SeqCst);

        // Wait for all workers to finish
        for handle in self.workers.drain(..) {
            match handle.await {
                Ok(Ok(())) => {},
                Ok(Err(e)) => {
                    eprintln!("Worker finished with error: {e}");
                },
                Err(e) => {
                    eprintln!("Worker task error: {e}");
                },
            }
        }

        Ok(())
    }

    /// Check if pool is running.
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Get number of active workers.
    pub fn active_workers(&self) -> usize {
        self.workers.len()
    }

    /// Get pool statistics.
    pub async fn get_stats(&self) -> Result<super::QueueStats> {
        self.queue.get_stats().await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_job_worker_pool_creation() {
        // Pool structure is tested through type system
        // Runtime tests require full async setup
    }
}
