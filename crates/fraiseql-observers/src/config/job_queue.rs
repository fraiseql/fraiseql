//! Job queue configuration for asynchronous action execution.

use std::env;

use serde::{Deserialize, Serialize};

use crate::error::{ObserverError, Result};

/// Job queue configuration for asynchronous action execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobQueueConfig {
    /// Redis URL for job queue backend (e.g., "redis://localhost:6379")
    /// If not specified, uses the main redis config URL
    #[serde(default = "default_job_queue_url")]
    pub url: String,

    /// Number of jobs to fetch per batch (default: 100)
    #[serde(default = "default_job_queue_batch_size")]
    pub batch_size: usize,

    /// Batch timeout in seconds (how long to wait before flushing partial batch)
    #[serde(default = "default_job_queue_batch_timeout_secs")]
    pub batch_timeout_secs: u64,

    /// Maximum number of retry attempts per job (default: 5)
    #[serde(default = "default_job_queue_max_retries")]
    pub max_retries: u32,

    /// Worker concurrency (number of jobs to execute in parallel)
    #[serde(default = "default_job_queue_worker_concurrency")]
    pub worker_concurrency: usize,

    /// Poll interval when queue is empty, in milliseconds (default: 1000)
    #[serde(default = "default_job_queue_poll_interval_ms")]
    pub poll_interval_ms: u64,

    /// Initial retry delay in milliseconds (default: 100)
    #[serde(default = "default_job_queue_initial_delay_ms")]
    pub initial_delay_ms: u64,

    /// Maximum retry delay in milliseconds (default: 30000)
    #[serde(default = "default_job_queue_max_delay_ms")]
    pub max_delay_ms: u64,
}

fn default_job_queue_url() -> String {
    env::var("FRAISEQL_JOB_QUEUE_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

const fn default_job_queue_batch_size() -> usize {
    100
}

const fn default_job_queue_batch_timeout_secs() -> u64 {
    5
}

const fn default_job_queue_max_retries() -> u32 {
    5
}

const fn default_job_queue_worker_concurrency() -> usize {
    10
}

const fn default_job_queue_poll_interval_ms() -> u64 {
    1000
}

const fn default_job_queue_initial_delay_ms() -> u64 {
    100
}

const fn default_job_queue_max_delay_ms() -> u64 {
    30000
}

impl Default for JobQueueConfig {
    fn default() -> Self {
        Self {
            url:                default_job_queue_url(),
            batch_size:         default_job_queue_batch_size(),
            batch_timeout_secs: default_job_queue_batch_timeout_secs(),
            max_retries:        default_job_queue_max_retries(),
            worker_concurrency: default_job_queue_worker_concurrency(),
            poll_interval_ms:   default_job_queue_poll_interval_ms(),
            initial_delay_ms:   default_job_queue_initial_delay_ms(),
            max_delay_ms:       default_job_queue_max_delay_ms(),
        }
    }
}

impl JobQueueConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(url) = env::var("FRAISEQL_JOB_QUEUE_URL") {
            self.url = url;
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_BATCH_SIZE") {
            if let Ok(size) = v.parse() {
                self.batch_size = size;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_BATCH_TIMEOUT_SECS") {
            if let Ok(secs) = v.parse() {
                self.batch_timeout_secs = secs;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_MAX_RETRIES") {
            if let Ok(retries) = v.parse() {
                self.max_retries = retries;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_WORKER_CONCURRENCY") {
            if let Ok(concurrency) = v.parse() {
                self.worker_concurrency = concurrency;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_POLL_INTERVAL_MS") {
            if let Ok(ms) = v.parse() {
                self.poll_interval_ms = ms;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_INITIAL_DELAY_MS") {
            if let Ok(ms) = v.parse() {
                self.initial_delay_ms = ms;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_MAX_DELAY_MS") {
            if let Ok(ms) = v.parse() {
                self.max_delay_ms = ms;
            }
        }
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.url.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "job_queue.url cannot be empty".to_string(),
            });
        }
        if self.batch_size == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "job_queue.batch_size must be > 0".to_string(),
            });
        }
        if self.batch_timeout_secs == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "job_queue.batch_timeout_secs must be > 0".to_string(),
            });
        }
        if self.max_retries == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "job_queue.max_retries must be > 0".to_string(),
            });
        }
        if self.worker_concurrency == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "job_queue.worker_concurrency must be > 0".to_string(),
            });
        }
        Ok(())
    }
}
