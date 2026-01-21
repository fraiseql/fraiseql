#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    #[error("Search provider error: {provider} - {message}")]
    Search { provider: String, message: String },

    #[error("Cache error: {message}")]
    Cache { message: String },

    #[error("Queue error: {message}")]
    Queue { message: String },

    #[error("Connection failed: {service}")]
    ConnectionFailed { service: String },

    #[error("Timeout: {operation}")]
    Timeout { operation: String },
}

impl IntegrationError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Search { .. } => "integration_search_error",
            Self::Cache { .. } => "integration_cache_error",
            Self::Queue { .. } => "integration_queue_error",
            Self::ConnectionFailed { .. } => "integration_connection_failed",
            Self::Timeout { .. } => "integration_timeout",
        }
    }
}
