/// Errors that occur when communicating with external integration services
/// such as search engines, caches, or message queues.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IntegrationError {
    /// A search provider (e.g. Elasticsearch, Typesense) returned an error.
    #[error("Search provider error: {provider} - {message}")]
    Search {
        /// Name of the search provider.
        provider: String,
        /// Error message from the provider.
        message:  String,
    },

    /// An error occurred while reading from or writing to an external cache
    /// (e.g. Redis).
    #[error("Cache error: {message}")]
    Cache {
        /// Description of the cache failure.
        message: String,
    },

    /// An error occurred while interacting with a message queue or broker
    /// (e.g. `RabbitMQ`, `NATS`).
    #[error("Queue error: {message}")]
    Queue {
        /// Description of the queue failure.
        message: String,
    },

    /// A network connection to an external service could not be established.
    #[error("Connection failed: {service}")]
    ConnectionFailed {
        /// Name or address of the service that could not be reached.
        service: String,
    },

    /// An operation against an external service did not complete within the
    /// allowed time budget.
    #[error("Timeout: {operation}")]
    Timeout {
        /// Name of the operation that timed out.
        operation: String,
    },
}

impl IntegrationError {
    /// Returns a short, stable error code string suitable for API responses and
    /// structured logging.
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::Search { .. } => "integration_search_error",
            Self::Cache { .. } => "integration_cache_error",
            Self::Queue { .. } => "integration_queue_error",
            Self::ConnectionFailed { .. } => "integration_connection_failed",
            Self::Timeout { .. } => "integration_timeout",
        }
    }
}
