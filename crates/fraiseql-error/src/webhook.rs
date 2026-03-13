/// Errors that occur while receiving and validating inbound webhook requests.
#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    /// The HMAC signature on the webhook payload does not match the expected
    /// value for the shared secret.
    ///
    /// This can indicate an invalid secret, payload tampering, or a replay
    /// from a different provider.
    #[error("Invalid signature")]
    InvalidSignature,

    /// The expected signature header was absent from the incoming request.
    #[error("Missing signature header: {header}")]
    MissingSignature {
        /// Name of the HTTP header that was expected but not present.
        header: String,
    },

    /// The webhook timestamp is older than the configured replay-window,
    /// indicating a replay attack or severe clock skew.
    #[error("Timestamp too old: {age_seconds}s (max: {max_seconds}s)")]
    TimestampExpired {
        /// Age of the webhook event in seconds.
        age_seconds: u64,
        /// Maximum allowed age in seconds.
        max_seconds: u64,
    },

    /// The webhook timestamp is further in the future than clock skew allows,
    /// suggesting a pre-generated or tampered request.
    #[error("Timestamp in future: {future_seconds}s")]
    TimestampFuture {
        /// Number of seconds the timestamp is ahead of the server clock.
        future_seconds: u64,
    },

    /// An event with this identifier has already been successfully processed.
    ///
    /// The event should be acknowledged (2xx) and then discarded.
    #[error("Duplicate event: {event_id}")]
    DuplicateEvent {
        /// Identifier of the duplicate event.
        event_id: String,
    },

    /// The event's `type` field does not correspond to any registered handler.
    #[error("Unknown event type: {event_type}")]
    UnknownEvent {
        /// The unrecognised event type string.
        event_type: String,
    },

    /// A webhook was received from a provider that has not been configured
    /// in `fraiseql.toml`.
    #[error("Provider not configured: {provider}")]
    ProviderNotConfigured {
        /// Name of the unconfigured provider.
        provider: String,
    },

    /// The webhook request body could not be parsed (invalid JSON, unexpected
    /// schema, etc.).
    ///
    /// The raw error message is kept server-side and a generic response is
    /// returned to the caller.
    #[error("Payload parse error: {message}")]
    PayloadError {
        /// Description of the parse failure (server-side only).
        message: String,
    },

    /// The idempotency check (deduplication store lookup or write) failed.
    #[error("Idempotency check failed: {message}")]
    IdempotencyError {
        /// Description of the idempotency failure.
        message: String,
    },
}

impl WebhookError {
    /// Returns a short, stable error code string suitable for API responses and
    /// structured logging.
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidSignature => "webhook_invalid_signature",
            Self::MissingSignature { .. } => "webhook_missing_signature",
            Self::TimestampExpired { .. } => "webhook_timestamp_expired",
            Self::TimestampFuture { .. } => "webhook_timestamp_future",
            Self::DuplicateEvent { .. } => "webhook_duplicate_event",
            Self::UnknownEvent { .. } => "webhook_unknown_event",
            Self::ProviderNotConfigured { .. } => "webhook_provider_not_configured",
            Self::PayloadError { .. } => "webhook_payload_error",
            Self::IdempotencyError { .. } => "webhook_idempotency_error",
        }
    }
}
