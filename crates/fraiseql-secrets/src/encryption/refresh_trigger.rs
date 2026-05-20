//! Automatic key refresh triggering with background job coordination,
//! TTL monitoring, and non-blocking refresh during operations.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

use chrono::{DateTime, Timelike, Utc};
use tracing;

/// Status of refresh job
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RefreshJobStatus {
    /// Job not started
    Idle,
    /// Job actively running
    Running,
    /// Job completed successfully
    Success,
    /// Job encountered error
    Failed,
    /// Job stopped/cancelled
    Stopped,
}

impl std::fmt::Display for RefreshJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Running => write!(f, "running"),
            Self::Success => write!(f, "success"),
            Self::Failed => write!(f, "failed"),
            Self::Stopped => write!(f, "stopped"),
        }
    }
}

/// Configuration for automatic refresh
#[derive(Debug, Clone)]
pub struct RefreshConfig {
    /// Enable automatic refresh (default: true)
    pub enabled: bool,
    /// Check interval in hours (default: 24)
    pub check_interval_hours: u32,
    /// TTL consumption threshold to trigger refresh (default: 80)
    pub refresh_threshold_percent: u32,
    /// Quiet hours start (0-23, None = disabled)
    pub quiet_hours_start: Option<u32>,
    /// Quiet hours end (0-23)
    pub quiet_hours_end: Option<u32>,
}

impl RefreshConfig {
    /// Create default refresh config (daily check, 80% threshold)
    #[must_use]
    pub const fn new() -> Self {
        Self {
            enabled: true,
            check_interval_hours: 24,
            refresh_threshold_percent: 80,
            quiet_hours_start: None,
            quiet_hours_end: None,
        }
    }

    /// Enable or disable automatic refresh
    #[must_use]
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set check interval in hours
    #[must_use]
    pub fn with_check_interval(mut self, hours: u32) -> Self {
        self.check_interval_hours = hours.max(1);
        self
    }

    /// Set refresh threshold percentage
    #[must_use]
    pub fn with_refresh_threshold(mut self, percent: u32) -> Self {
        self.refresh_threshold_percent = percent.min(99);
        self
    }

    /// Set quiet hours (e.g., 2 for 2am-4am)
    #[must_use]
    pub const fn with_quiet_hours(mut self, start_hour: u32, end_hour: u32) -> Self {
        if start_hour < 24 && end_hour < 24 {
            self.quiet_hours_start = Some(start_hour);
            self.quiet_hours_end = Some(end_hour);
        }
        self
    }
}

impl Default for RefreshConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Refresh trigger state and history
#[derive(Debug, Clone)]
pub struct RefreshTrigger {
    /// Refresh configuration
    config: Arc<RefreshConfig>,
    /// Last refresh check time
    // std::sync::Mutex is intentional: this lock is never held across .await.
    // Switch to tokio::sync::Mutex if that constraint ever changes.
    last_check: Arc<std::sync::Mutex<Option<DateTime<Utc>>>>,
    /// Last refresh completion time
    // std::sync::Mutex is intentional: this lock is never held across .await.
    // Switch to tokio::sync::Mutex if that constraint ever changes.
    last_refresh: Arc<std::sync::Mutex<Option<DateTime<Utc>>>>,
    /// Last refresh duration in milliseconds
    last_refresh_duration_ms: Arc<AtomicU64>,
    /// Total refreshes performed
    total_refreshes: Arc<AtomicU64>,
    /// Failed refreshes count
    failed_refreshes: Arc<AtomicU64>,
    /// Refresh pending flag
    refresh_pending: Arc<AtomicBool>,
}

impl RefreshTrigger {
    /// Create new refresh trigger
    #[must_use]
    pub fn new(config: RefreshConfig) -> Self {
        Self {
            config: Arc::new(config),
            last_check: Arc::new(std::sync::Mutex::new(None)),
            last_refresh: Arc::new(std::sync::Mutex::new(None)),
            last_refresh_duration_ms: Arc::new(AtomicU64::new(0)),
            total_refreshes: Arc::new(AtomicU64::new(0)),
            failed_refreshes: Arc::new(AtomicU64::new(0)),
            refresh_pending: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if refresh should be triggered
    #[must_use]
    pub fn should_trigger(&self, ttl_consumed_percent: u32) -> bool {
        if !self.config.enabled {
            return false;
        }

        // Check if refresh pending (already triggered for this version)
        if self.refresh_pending.load(Ordering::Relaxed) {
            return false;
        }

        // Check if TTL threshold reached
        if ttl_consumed_percent < self.config.refresh_threshold_percent {
            return false;
        }

        // Check quiet hours if configured
        if let (Some(start), Some(end)) =
            (self.config.quiet_hours_start, self.config.quiet_hours_end)
        {
            let now = Utc::now();
            let hour = now.hour();

            if start < end {
                // Normal case: 2am-4am
                if hour >= start && hour < end {
                    return false;
                }
            } else {
                // Wrap case: 22pm-2am
                if hour >= start || hour < end {
                    return false;
                }
            }
        }

        true
    }

    /// Mark refresh as pending
    pub fn mark_pending(&self) {
        self.refresh_pending.store(true, Ordering::Relaxed);
    }

    /// Clear pending flag after refresh completes
    pub fn clear_pending(&self) {
        self.refresh_pending.store(false, Ordering::Relaxed);
    }

    /// Record successful refresh
    pub fn record_success(&self, duration_ms: u64) {
        self.total_refreshes.fetch_add(1, Ordering::Relaxed);
        self.last_refresh_duration_ms.store(duration_ms, Ordering::Relaxed);
        if let Ok(mut last) = self.last_refresh.lock() {
            *last = Some(Utc::now());
        }
    }

    /// Record failed refresh
    pub fn record_failure(&self) {
        self.failed_refreshes.fetch_add(1, Ordering::Relaxed);
    }

    /// Record check attempt
    pub fn record_check(&self) {
        if let Ok(mut last) = self.last_check.lock() {
            *last = Some(Utc::now());
        }
    }

    /// Get last check time
    #[must_use]
    pub fn last_check_time(&self) -> Option<DateTime<Utc>> {
        if let Ok(last) = self.last_check.lock() {
            *last
        } else {
            None
        }
    }

    /// Get last refresh time
    #[must_use]
    pub fn last_refresh_time(&self) -> Option<DateTime<Utc>> {
        if let Ok(last) = self.last_refresh.lock() {
            *last
        } else {
            None
        }
    }

    /// Get total refreshes count
    #[must_use]
    pub fn total_refreshes(&self) -> u64 {
        self.total_refreshes.load(Ordering::Relaxed)
    }

    /// Get failed refreshes count
    #[must_use]
    pub fn failed_refreshes(&self) -> u64 {
        self.failed_refreshes.load(Ordering::Relaxed)
    }

    /// Get success rate percentage
    #[must_use]
    pub fn success_rate_percent(&self) -> u32 {
        let total = self.total_refreshes();
        if total == 0 {
            100
        } else {
            let failed = self.failed_refreshes();
            let successful = total - failed;
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            // Reason: computing a percentage (0–100) from counters; precision loss and truncation
            // are acceptable for a human-readable metric
            {
                ((successful as f64 / total as f64) * 100.0) as u32
            }
        }
    }

    /// Check if refresh is pending
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.refresh_pending.load(Ordering::Relaxed)
    }

    /// Check if refresh enabled
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

impl Default for RefreshTrigger {
    fn default() -> Self {
        Self::new(RefreshConfig::default())
    }
}

/// Refresh job state and coordination
#[derive(Debug, Clone)]
pub struct RefreshJob {
    /// Job status
    // std::sync::Mutex is intentional: this lock is never held across .await.
    // Switch to tokio::sync::Mutex if that constraint ever changes.
    status: Arc<std::sync::Mutex<RefreshJobStatus>>,
    /// Job start time
    // std::sync::Mutex is intentional: this lock is never held across .await.
    // Switch to tokio::sync::Mutex if that constraint ever changes.
    start_time: Arc<std::sync::Mutex<Option<Instant>>>,
    /// Job last error message
    // std::sync::Mutex is intentional: this lock is never held across .await.
    // Switch to tokio::sync::Mutex if that constraint ever changes.
    last_error: Arc<std::sync::Mutex<Option<String>>>,
    /// Job is shutting down
    shutdown_requested: Arc<AtomicBool>,
}

impl RefreshJob {
    /// Create new refresh job
    #[must_use]
    pub fn new() -> Self {
        Self {
            status: Arc::new(std::sync::Mutex::new(RefreshJobStatus::Idle)),
            start_time: Arc::new(std::sync::Mutex::new(None)),
            last_error: Arc::new(std::sync::Mutex::new(None)),
            shutdown_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Mark job as running
    ///
    /// # Errors
    ///
    /// Returns an error string if the job is not idle or if a mutex is poisoned.
    pub fn start(&self) -> Result<(), String> {
        let mut status = self.status.lock().map_err(|e| format!("Failed to lock status: {}", e))?;

        if *status != RefreshJobStatus::Idle {
            return Err(format!("Job already running: {}", status));
        }

        *status = RefreshJobStatus::Running;
        let mut start = self
            .start_time
            .lock()
            .map_err(|e| format!("Failed to lock start time: {}", e))?;
        *start = Some(Instant::now());
        Ok(())
    }

    /// Mark job as succeeded
    ///
    /// # Errors
    ///
    /// Returns an error string if the status mutex is poisoned.
    pub fn complete_success(&self) -> Result<(), String> {
        let mut status = self.status.lock().map_err(|e| format!("Failed to lock status: {}", e))?;
        *status = RefreshJobStatus::Success;
        Ok(())
    }

    /// Mark job as failed with error
    ///
    /// # Errors
    ///
    /// Returns an error string if the status or error mutexes are poisoned.
    pub fn complete_failure(&self, error: impl Into<String>) -> Result<(), String> {
        let mut status = self.status.lock().map_err(|e| format!("Failed to lock status: {}", e))?;
        *status = RefreshJobStatus::Failed;

        let mut last_error =
            self.last_error.lock().map_err(|e| format!("Failed to lock error: {}", e))?;
        *last_error = Some(error.into());

        Ok(())
    }

    /// Request job shutdown
    pub fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Relaxed);
    }

    /// Check if shutdown was requested
    #[must_use]
    pub fn should_shutdown(&self) -> bool {
        self.shutdown_requested.load(Ordering::Relaxed)
    }

    /// Get current job status
    ///
    /// # Errors
    ///
    /// Returns an error string if the status mutex is poisoned.
    pub fn status(&self) -> Result<RefreshJobStatus, String> {
        let status = self.status.lock().map_err(|e| format!("Failed to lock status: {}", e))?;
        Ok(*status)
    }

    /// Get job duration if running or completed
    ///
    /// # Errors
    ///
    /// Returns an error string if the start-time mutex is poisoned.
    pub fn duration(&self) -> Result<Option<std::time::Duration>, String> {
        let start = self
            .start_time
            .lock()
            .map_err(|e| format!("Failed to lock start time: {}", e))?;

        Ok(start.map(|s| s.elapsed()))
    }

    /// Get last error message
    ///
    /// # Errors
    ///
    /// Returns an error string if the error mutex is poisoned.
    pub fn last_error(&self) -> Result<Option<String>, String> {
        let error = self.last_error.lock().map_err(|e| format!("Failed to lock error: {}", e))?;
        Ok(error.clone())
    }
}

impl Default for RefreshJob {
    fn default() -> Self {
        Self::new()
    }
}

/// Refresh manager combining trigger and job coordination
#[derive(Debug, Clone)]
pub struct RefreshManager {
    /// Refresh trigger
    trigger: Arc<RefreshTrigger>,
    /// Refresh job
    job: Arc<RefreshJob>,
}

impl RefreshManager {
    /// Create new refresh manager
    #[must_use]
    pub fn new(config: RefreshConfig) -> Self {
        Self {
            trigger: Arc::new(RefreshTrigger::new(config)),
            job: Arc::new(RefreshJob::new()),
        }
    }

    /// Check if refresh should trigger and mark pending
    #[must_use]
    pub fn check_and_trigger(&self, ttl_consumed_percent: u32) -> bool {
        self.trigger.record_check();
        if self.trigger.should_trigger(ttl_consumed_percent) {
            self.trigger.mark_pending();
            true
        } else {
            false
        }
    }

    /// Start refresh job
    ///
    /// # Errors
    ///
    /// Returns an error string if the job is not idle or if a mutex is poisoned.
    pub fn start_job(&self) -> Result<(), String> {
        self.job.start()
    }

    /// Complete refresh job successfully
    ///
    /// # Errors
    ///
    /// Returns an error string if a mutex is poisoned.
    pub fn complete_job_success(&self) -> Result<(), String> {
        self.trigger.clear_pending();
        self.job.complete_success()
    }

    /// Complete refresh job with failure
    ///
    /// # Errors
    ///
    /// Returns an error string if a mutex is poisoned.
    pub fn complete_job_failure(&self, error: impl Into<String>) -> Result<(), String> {
        // Don't clear pending - allow retry
        self.job.complete_failure(error)
    }

    /// Get refresh trigger
    #[must_use]
    pub fn trigger(&self) -> Arc<RefreshTrigger> {
        Arc::clone(&self.trigger)
    }

    /// Get refresh job
    #[must_use]
    pub fn job(&self) -> Arc<RefreshJob> {
        Arc::clone(&self.job)
    }

    /// Check if refresh is needed and pending
    #[must_use]
    pub fn refresh_pending(&self) -> bool {
        self.trigger.is_pending()
    }

    /// Check if automatic refresh enabled
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.trigger.is_enabled()
    }

    /// Manually trigger refresh (bypass TTL check)
    ///
    /// # Errors
    ///
    /// Returns an error string if a refresh is already pending.
    pub fn trigger_manual(&self) -> Result<(), String> {
        if self.trigger.is_pending() {
            Err("Refresh already pending".to_string())
        } else {
            self.trigger.mark_pending();
            Ok(())
        }
    }

    /// Request job shutdown
    pub fn request_shutdown(&self) {
        self.job.request_shutdown();
    }

    /// Get time since last check.
    ///
    /// Returns `None` if no check has occurred yet.  Returns `Duration::ZERO`
    /// and logs a warning if the system clock appears to have gone backwards,
    /// which would otherwise cause the refresh timer to stall indefinitely.
    #[must_use]
    pub fn time_since_last_check(&self) -> Option<std::time::Duration> {
        self.trigger.last_check_time().map(|last| {
            let delta = Utc::now() - last;
            delta.to_std().unwrap_or_else(|_| {
                tracing::warn!(
                    delta_ns = delta.num_nanoseconds().unwrap_or(i64::MIN),
                    "Clock appears to have gone backwards since last refresh check; \
                     forcing immediate re-check"
                );
                std::time::Duration::ZERO
            })
        })
    }

    /// Get time since last refresh.
    ///
    /// Returns `None` if no refresh has occurred yet.  Returns `Duration::ZERO`
    /// and logs a warning on clock regression (same rationale as
    /// [`Self::time_since_last_check`]).
    #[must_use]
    pub fn time_since_last_refresh(&self) -> Option<std::time::Duration> {
        self.trigger.last_refresh_time().map(|last| {
            let delta = Utc::now() - last;
            delta.to_std().unwrap_or_else(|_| {
                tracing::warn!(
                    delta_ns = delta.num_nanoseconds().unwrap_or(i64::MIN),
                    "Clock appears to have gone backwards since last refresh; \
                     forcing immediate re-refresh"
                );
                std::time::Duration::ZERO
            })
        })
    }

    /// Check if job is currently running
    #[must_use]
    pub fn job_running(&self) -> bool {
        self.job.status().map(|s| s == RefreshJobStatus::Running).unwrap_or(false)
    }

    /// Get job success rate percentage
    #[must_use]
    pub fn job_success_rate_percent(&self) -> u32 {
        self.trigger.success_rate_percent()
    }

    /// Get health status of refresh system
    #[must_use]
    pub fn health_status(&self) -> RefreshHealthStatus {
        let job_status = self.job.status().unwrap_or(RefreshJobStatus::Failed);

        if !self.is_enabled() {
            RefreshHealthStatus::Disabled
        } else if self.job_running() {
            RefreshHealthStatus::Running
        } else if self.refresh_pending() {
            RefreshHealthStatus::Pending
        } else if job_status == RefreshJobStatus::Failed && self.trigger.failed_refreshes() > 2 {
            RefreshHealthStatus::Degraded
        } else {
            RefreshHealthStatus::Healthy
        }
    }

    /// Check if should retry refresh (has pending but not max retries)
    #[must_use]
    pub fn should_retry_refresh(&self) -> bool {
        self.refresh_pending() && self.trigger.failed_refreshes() < 5
    }

    /// Reset refresh state for retry
    pub fn reset_for_retry(&self) {
        // Clear pending so next check can trigger again
        self.trigger.clear_pending();
    }
}

/// Health status of refresh system
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RefreshHealthStatus {
    /// Automatic refresh disabled
    Disabled,
    /// Refresh running
    Running,
    /// Refresh waiting to execute
    Pending,
    /// Refresh working but with failures
    Degraded,
    /// All systems healthy
    Healthy,
}

impl Default for RefreshManager {
    fn default() -> Self {
        Self::new(RefreshConfig::default())
    }
}

#[cfg(test)]
mod tests;
