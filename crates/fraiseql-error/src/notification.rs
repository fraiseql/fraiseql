use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    #[error("Provider error: {provider} - {message}")]
    Provider { provider: String, message: String },

    #[error("Provider unavailable: {provider}")]
    ProviderUnavailable { provider: String, retry_after: Option<Duration> },

    #[error("Invalid input: {message}")]
    InvalidInput { message: String },

    #[error("Template error: {message}")]
    Template { message: String },

    #[error("Rate limited by provider: retry after {seconds} seconds")]
    ProviderRateLimited { provider: String, seconds: u64 },

    #[error("Circuit breaker open for provider: {provider}")]
    CircuitOpen { provider: String, retry_after: Duration },

    #[error("Timeout sending notification")]
    Timeout,
}

impl NotificationError {
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::Configuration { .. } => "notification_config_error",
            Self::Provider { .. } => "notification_provider_error",
            Self::ProviderUnavailable { .. } => "notification_provider_unavailable",
            Self::InvalidInput { .. } => "notification_invalid_input",
            Self::Template { .. } => "notification_template_error",
            Self::ProviderRateLimited { .. } => "notification_rate_limited",
            Self::CircuitOpen { .. } => "notification_circuit_open",
            Self::Timeout => "notification_timeout",
        }
    }
}
