#[derive(Debug, thiserror::Error)]
pub enum ObserverError {
    #[error("Invalid condition: {message}")]
    InvalidCondition { message: String },

    #[error("Template error: {message}")]
    TemplateError { message: String },

    #[error("Action failed: {action} - {message}")]
    ActionFailed { action: String, message: String },

    #[error("Invalid configuration: {message}")]
    InvalidConfig { message: String },

    #[error("Event processing failed: {message}")]
    ProcessingFailed { message: String },

    #[error("Max retries exceeded for event {event_id}")]
    MaxRetriesExceeded { event_id: String },

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl ObserverError {
    pub fn error_code(&self) -> &'static str {
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
