//! Execution summary for observer event processing.

/// Maximum number of condition-evaluation error strings retained per event.
///
/// Prevents `errors` from growing without bound when many observers hold a
/// condition string that fails at parse/evaluate time on every event.  Once
/// the cap is reached, additional errors are silently dropped (they are still
/// logged at ERROR level via the standard logging path in `mod.rs`).
pub(super) const MAX_ERROR_STRINGS: usize = 100;

/// Summary of event processing results
#[derive(Debug, Clone, Default)]
pub struct ExecutionSummary {
    /// Number of successful action executions
    pub successful_actions: usize,
    /// Number of failed action executions
    pub failed_actions:     usize,
    /// Number of observers skipped due to condition
    pub conditions_skipped: usize,
    /// Total execution time in milliseconds
    pub total_duration_ms:  f64,
    /// DLQ push errors
    pub dlq_errors:         usize,
    /// Other errors encountered
    pub errors:             Vec<String>,
    /// Whether this event was skipped due to deduplication
    pub duplicate_skipped:  bool,
    /// Whether this event was rejected due to a tenant scope violation
    pub tenant_rejected:    bool,
    /// Number of cache hits during action execution
    pub cache_hits:         usize,
    /// Number of cache misses during action execution
    pub cache_misses:       usize,
}

impl ExecutionSummary {
    /// Create a new empty summary
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if execution was successful
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.failed_actions == 0 && self.dlq_errors == 0 && self.errors.is_empty()
    }

    /// Get total actions processed
    #[must_use]
    pub const fn total_actions(&self) -> usize {
        self.successful_actions + self.failed_actions
    }
}
