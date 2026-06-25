//! Execution summary for observer event processing.

/// Maximum number of condition-evaluation error strings retained per event.
///
/// Prevents `errors` from growing without bound when many observers hold a
/// condition string that fails at parse/evaluate time on every event.  Once
/// the cap is reached, additional errors are silently dropped (they are still
/// logged at ERROR level via the standard logging path in `mod.rs`).
pub(super) const MAX_ERROR_STRINGS: usize = 100;

/// Per-action execution detail captured during event processing.
///
/// One entry is produced per action that ran (succeeded or failed), in
/// dispatch order. Embedders use these to populate a durable execution log
/// (`tb_observer_log`) with the action type, transport status code, and
/// response summary that the aggregate counters in [`ExecutionSummary`] would
/// otherwise discard (#468). It deliberately carries no request payload: the
/// triggering event is already available to the embedder, and payload capture
/// is a privacy-gated decision made at the persistence boundary.
#[derive(Debug, Clone)]
pub struct ActionExecutionDetail {
    /// Index of the action within its observer's action list.
    pub action_index:  usize,
    /// Action type that ran (`"webhook"`, `"slack"`, `"email"`, `"cache"`).
    pub action_type:   String,
    /// Whether the action ultimately succeeded.
    pub success:       bool,
    /// Transport status code for HTTP-backed actions, if any.
    pub status_code:   Option<u16>,
    /// Short outcome message (e.g. `"HTTP 200"` or a queued message id).
    pub message:       String,
    /// Failure detail when the action did not succeed.
    pub error_message: Option<String>,
    /// Wall-clock duration of the (final) attempt in milliseconds.
    pub duration_ms:   f64,
}

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
    /// Per-action execution details, in dispatch order (#468).
    ///
    /// Populated by [`process_event`](super::ObserverExecutor::process_event)
    /// so embedders can record per-action audit columns (status code, action
    /// type, response summary). Empty for skipped/deduplicated/rejected events.
    pub action_details:     Vec<ActionExecutionDetail>,
}

impl ExecutionSummary {
    /// Create a new empty summary
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if execution was successful
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.failed_actions == 0 && self.dlq_errors == 0 && self.errors.is_empty()
    }

    /// Get total actions processed
    #[must_use]
    pub const fn total_actions(&self) -> usize {
        self.successful_actions + self.failed_actions
    }
}
