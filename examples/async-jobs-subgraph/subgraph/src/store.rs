//! In-memory job store for the async-jobs subgraph.
//!
//! **Dev only.** This store lives in process memory: it is lost on restart and
//! is not shared across instances. A production deployment must replace it with
//! a durable, shared backend (Redis, SQS, a database table, ...). The trait
//! boundary here is deliberately tiny so that swap is a localized change.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Lifecycle states for an async job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, async_graphql::Enum)]
pub enum JobStatus {
    /// Accepted, not yet picked up by a worker.
    Pending,
    /// A worker is processing the job.
    Running,
    /// Completed successfully; `result` is populated.
    Succeeded,
    /// Terminated with an error.
    Failed,
}

/// A handle to a single async job.
///
/// Returned immediately from `enqueueJob` and polled via `jobStatus`. This type
/// is a federation entity keyed on `id`, so a router can resolve it across
/// subgraph boundaries.
#[derive(Debug, Clone, async_graphql::SimpleObject)]
pub struct JobHandle {
    /// Stable identifier for polling.
    pub id:     async_graphql::ID,
    /// Current lifecycle state.
    pub status: JobStatus,
    /// Output payload, populated only once `status` is `SUCCEEDED`.
    pub result: Option<String>,
}

/// Process-local, thread-safe job store.
#[derive(Clone, Default)]
pub struct JobStore {
    inner:   Arc<Mutex<HashMap<String, JobHandle>>>,
    next_id: Arc<AtomicU64>,
}

impl JobStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a new job and spawn the (toy) worker that completes it.
    ///
    /// The worker sleeps for two seconds, then marks the job `SUCCEEDED` with
    /// the uppercase of `input`. Swap this body for a real HTTP call, ML
    /// inference, or payment-API request — the GraphQL surface stays identical.
    pub fn enqueue(&self, input: String) -> JobHandle {
        let id = format!("job-{}", self.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        let handle = JobHandle {
            id:     async_graphql::ID(id.clone()),
            status: JobStatus::Pending,
            result: None,
        };
        self.insert(handle.clone());

        let store = self.clone();
        let job_id = id;
        tokio::spawn(async move {
            store.set_status(&job_id, JobStatus::Running, None);
            tokio::time::sleep(Duration::from_secs(2)).await;
            store.set_status(&job_id, JobStatus::Succeeded, Some(input.to_uppercase()));
        });

        handle
    }

    /// Look up a job by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<JobHandle> {
        self.lock().get(id).cloned()
    }

    fn insert(&self, handle: JobHandle) {
        self.lock().insert(handle.id.to_string(), handle);
    }

    fn set_status(&self, id: &str, status: JobStatus, result: Option<String>) {
        if let Some(job) = self.lock().get_mut(id) {
            job.status = status;
            job.result = result;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, JobHandle>> {
        // Reason: the only panics under this lock are unreachable (no nested
        // locking, no user code), so a poisoned mutex would indicate an
        // already-aborting process; recovering the guard is the safe choice.
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
