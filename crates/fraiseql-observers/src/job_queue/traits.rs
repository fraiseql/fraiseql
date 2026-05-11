//! Job queue trait definitions and error types.

use std::{fmt, sync::Arc};

use async_trait::async_trait;
use uuid::Uuid;

use super::Job;
use crate::error::Result as ObserverResult;

/// Job queue error type
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum JobQueueError {
    /// Job not found
    JobNotFound(Uuid),
    /// Failed to enqueue job
    EnqueueFailed(String),
    /// Failed to dequeue jobs
    DequeueFailed(String),
    /// Failed to acknowledge job
    AcknowledgeFailed(String),
    /// Failed to mark job as failed
    FailedFailed(String),
    /// Failed to query job status
    StatusQueryFailed(String),
    /// Queue is unavailable
    QueueUnavailable(String),
}

impl fmt::Display for JobQueueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobQueueError::JobNotFound(id) => write!(f, "Job not found: {id}"),
            JobQueueError::EnqueueFailed(reason) => write!(f, "Failed to enqueue job: {reason}"),
            JobQueueError::DequeueFailed(reason) => write!(f, "Failed to dequeue jobs: {reason}"),
            JobQueueError::AcknowledgeFailed(reason) => {
                write!(f, "Failed to acknowledge job: {reason}")
            },
            JobQueueError::FailedFailed(reason) => {
                write!(f, "Failed to mark job as failed: {reason}")
            },
            JobQueueError::StatusQueryFailed(reason) => {
                write!(f, "Failed to query job status: {reason}")
            },
            JobQueueError::QueueUnavailable(reason) => {
                write!(f, "Queue is unavailable: {reason}")
            },
        }
    }
}

impl std::error::Error for JobQueueError {}

/// Job queue trait for asynchronous job execution
// Reason: used as dyn Trait (Arc<dyn JobQueue>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait JobQueue: Send + Sync {
    /// Enqueue a job for execution
    ///
    /// # Arguments
    ///
    /// * `job` - The job to enqueue
    ///
    /// # Errors
    ///
    /// Returns error if enqueueing fails (e.g., Redis connection error)
    async fn enqueue(&self, job: Job) -> ObserverResult<()>;

    /// Dequeue jobs for execution
    ///
    /// # Arguments
    ///
    /// * `batch_size` - Maximum number of jobs to dequeue
    /// * `timeout_secs` - How long these jobs can run before timeout
    ///
    /// # Errors
    ///
    /// Returns error if dequeueing fails
    async fn dequeue(&self, batch_size: usize, timeout_secs: u64) -> ObserverResult<Vec<Job>>;

    /// Acknowledge successful job completion
    ///
    /// # Arguments
    ///
    /// * `job_id` - ID of the completed job
    ///
    /// # Errors
    ///
    /// Returns error if acknowledgement fails
    async fn acknowledge(&self, job_id: Uuid) -> ObserverResult<()>;

    /// Mark a job as failed
    ///
    /// Determines whether to retry or move to DLQ based on the job state.
    ///
    /// # Arguments
    ///
    /// * `job` - The failed job
    /// * `error` - Error message
    ///
    /// # Errors
    ///
    /// Returns error if marking as failed fails
    async fn fail(&self, job: &mut Job, error: String) -> ObserverResult<()>;

    /// Get the current status of a job
    ///
    /// # Arguments
    ///
    /// * `job_id` - ID of the job to query
    ///
    /// # Errors
    ///
    /// Returns error if status query fails
    async fn get_status(&self, job_id: Uuid) -> ObserverResult<Option<super::JobState>>;

    /// Get queue depth (number of pending jobs)
    ///
    /// # Errors
    ///
    /// Returns error if query fails
    async fn queue_depth(&self) -> ObserverResult<usize>;

    /// Get DLQ size (number of dead lettered jobs)
    ///
    /// # Errors
    ///
    /// Returns error if query fails
    async fn dlq_size(&self) -> ObserverResult<usize>;
}

/// Mock job queue for testing
pub struct MockJobQueue {
    jobs:    Arc<dashmap::DashMap<Uuid, Job>>,
    pending: Arc<dashmap::DashMap<Uuid, ()>>,
    dlq:     Arc<dashmap::DashMap<Uuid, Job>>,
}

impl MockJobQueue {
    /// Create a new mock queue
    #[must_use]
    pub fn new() -> Self {
        Self {
            jobs:    Arc::new(dashmap::DashMap::new()),
            pending: Arc::new(dashmap::DashMap::new()),
            dlq:     Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Get all jobs (for testing)
    #[must_use]
    pub fn all_jobs(&self) -> Vec<Job> {
        self.jobs.iter().map(|entry| entry.value().clone()).collect()
    }

    /// Get all DLQ jobs (for testing)
    #[must_use]
    pub fn dlq_jobs(&self) -> Vec<Job> {
        self.dlq.iter().map(|entry| entry.value().clone()).collect()
    }
}

impl Default for MockJobQueue {
    fn default() -> Self {
        Self::new()
    }
}

// Reason: JobQueue is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl JobQueue for MockJobQueue {
    async fn enqueue(&self, job: Job) -> ObserverResult<()> {
        let job_id = job.id;
        self.jobs.insert(job_id, job);
        self.pending.insert(job_id, ());
        Ok(())
    }

    async fn dequeue(&self, batch_size: usize, _timeout_secs: u64) -> ObserverResult<Vec<Job>> {
        let mut jobs = Vec::new();
        let mut to_remove = Vec::new();

        for entry in self.pending.iter().take(batch_size) {
            let job_id = *entry.key();
            to_remove.push(job_id);

            if let Some(mut job) = self.jobs.get_mut(&job_id) {
                job.mark_running();
                jobs.push(job.clone());
            }
        }

        for job_id in to_remove {
            self.pending.remove(&job_id);
        }

        Ok(jobs)
    }

    async fn acknowledge(&self, job_id: Uuid) -> ObserverResult<()> {
        if let Some(mut job) = self.jobs.get_mut(&job_id) {
            job.mark_completed();
            Ok(())
        } else {
            Err(crate::error::ObserverError::InvalidConfig {
                message: format!("Job not found: {job_id}"),
            })
        }
    }

    async fn fail(&self, job: &mut Job, error: String) -> ObserverResult<()> {
        job.mark_failed(error);

        if job.can_retry() {
            self.pending.insert(job.id, ());
        } else {
            self.dlq.insert(job.id, job.clone());
        }
        self.jobs.insert(job.id, job.clone());

        Ok(())
    }

    async fn get_status(&self, job_id: Uuid) -> ObserverResult<Option<super::JobState>> {
        Ok(self.jobs.get(&job_id).map(|job| job.state))
    }

    async fn queue_depth(&self) -> ObserverResult<usize> {
        Ok(self.pending.len())
    }

    async fn dlq_size(&self) -> ObserverResult<usize> {
        Ok(self.dlq.len())
    }
}

