use std::time::Duration;

/// Errors that occur during notification delivery (email, SMS, push, etc.).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NotificationError {
    /// The notification provider is misconfigured (e.g. missing API key or
    /// invalid sender address).
    #[error("Configuration error: {message}")]
    Configuration {
        /// Description of the configuration problem.
        message: String,
    },

    /// The notification provider returned an unexpected error response.
    #[error("Provider error: {provider} - {message}")]
    Provider {
        /// Name of the notification provider (e.g. `"sendgrid"`, `"twilio"`).
        provider: String,
        /// Error message from the provider (kept server-side; not forwarded to clients).
        message:  String,
    },

    /// The notification provider is temporarily unreachable or returning
    /// 5xx responses.
    #[error("Provider unavailable: {provider}")]
    ProviderUnavailable {
        /// Name of the provider that is unavailable.
        provider:    String,
        /// How long to wait before retrying, if the provider indicated a backoff.
        retry_after: Option<Duration>,
    },

    /// The notification request contained invalid data (e.g. a malformed
    /// recipient address or an empty message body).
    #[error("Invalid input: {message}")]
    InvalidInput {
        /// Description of what was invalid.
        message: String,
    },

    /// An error occurred while rendering the notification template.
    #[error("Template error: {message}")]
    Template {
        /// Description of the template rendering failure.
        message: String,
    },

    /// The notification provider has rate-limited the sending account.
    #[error("Rate limited by provider: retry after {seconds} seconds")]
    ProviderRateLimited {
        /// Name of the provider that applied the rate limit.
        provider: String,
        /// Number of seconds to wait before retrying.
        seconds:  u64,
    },

    /// The circuit breaker for this provider is open because too many recent
    /// requests have failed.
    ///
    /// Requests will not be forwarded to the provider until `retry_after` has
    /// elapsed, giving the provider time to recover.
    #[error("Circuit breaker open for provider: {provider}")]
    CircuitOpen {
        /// Name of the provider whose circuit is open.
        provider:    String,
        /// How long to wait before the circuit transitions to half-open.
        retry_after: Duration,
    },

    /// The notification delivery attempt did not complete within the allowed
    /// time budget.
    #[error("Timeout sending notification")]
    Timeout,
}

impl NotificationError {
    /// Returns a short, stable error code string suitable for API responses and
    /// structured logging.
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
