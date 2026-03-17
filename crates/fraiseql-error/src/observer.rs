/// Domain-level observer errors for `RuntimeError` aggregation.
///
/// These are the **client-facing** observer error variants. For operational
/// observer errors with structured OB-codes (used in logging and retry
/// decisions), see `fraiseql_observers::ObserverError`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ObserverError {
    /// The observer's trigger condition expression could not be parsed or
    /// evaluated.
    #[error("Invalid condition: {message}")]
    InvalidCondition {
        /// Description of why the condition is invalid.
        message: String,
    },

    /// An error occurred while rendering an observer action template (e.g.
    /// an email body or webhook payload template).
    #[error("Template error: {message}")]
    TemplateError {
        /// Description of the template rendering failure.
        message: String,
    },

    /// The configured action (e.g. HTTP call, notification dispatch) failed
    /// to execute.
    #[error("Action failed: {action} - {message}")]
    ActionFailed {
        /// Name or type of the action that failed.
        action:  String,
        /// Reason for the failure.
        message: String,
    },

    /// The observer definition contains an invalid or inconsistent
    /// configuration value.
    #[error("Invalid configuration: {message}")]
    InvalidConfig {
        /// Description of the configuration problem.
        message: String,
    },

    /// The event payload could not be processed (e.g. deserialization failed
    /// or required fields were missing).
    #[error("Event processing failed: {message}")]
    ProcessingFailed {
        /// Description of the processing failure.
        message: String,
    },

    /// The event has been retried the maximum number of times without
    /// succeeding and is being moved to the dead-letter queue.
    #[error("Max retries exceeded for event {event_id}")]
    MaxRetriesExceeded {
        /// Identifier of the event that exhausted its retry budget.
        event_id: String,
    },

    /// A database error occurred while recording observer state or audit logs.
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl ObserverError {
    /// Returns a short, stable error code string suitable for API responses and
    /// structured logging.
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidCondition { .. } => "observer_invalid_condition",
            Self::TemplateError { .. } => "observer_template_error",
            Self::ActionFailed { .. } => "observer_action_failed",
            Self::InvalidConfig { .. } => "observer_invalid_config",
            Self::ProcessingFailed { .. } => "observer_processing_failed",
            Self::MaxRetriesExceeded { .. } => "observer_max_retries",
            Self::Database(_) => "observer_database_error",
        }
    }
}
