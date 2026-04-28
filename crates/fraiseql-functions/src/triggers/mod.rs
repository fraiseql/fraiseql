//! Trigger system for serverless functions.
//!
//! Triggers enable functions to execute in response to specific events:
//! - `after:mutation`: Fire after mutation completes (async, non-blocking)
//! - `before:mutation`: Fire before mutation (sync, can abort)
//! - `after:storage`: Fire after storage operations
//! - `cron`: Fire on schedule
//! - `http`: Custom HTTP endpoints

pub mod cron;
pub mod http;
pub mod mutation;
pub mod storage;
#[cfg(test)]
mod tests;

pub use cron::{CronTrigger, CronSchedule, CronExecutionState};
pub use http::{HttpTriggerRoute, HttpTriggerPayload, HttpTriggerResponse, HttpTriggerMatcher};
pub use mutation::{AfterMutationTrigger, BeforeMutationTrigger};
pub use storage::{StorageTrigger, StorageOperation, StorageEventPayload};
