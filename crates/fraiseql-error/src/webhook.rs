#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Missing signature header: {header}")]
    MissingSignature { header: String },

    #[error("Timestamp too old: {age_seconds}s (max: {max_seconds}s)")]
    TimestampExpired { age_seconds: u64, max_seconds: u64 },

    #[error("Timestamp in future: {future_seconds}s")]
    TimestampFuture { future_seconds: u64 },

    #[error("Duplicate event: {event_id}")]
    DuplicateEvent { event_id: String },

    #[error("Unknown event type: {event_type}")]
    UnknownEvent { event_type: String },

    #[error("Provider not configured: {provider}")]
    ProviderNotConfigured { provider: String },

    #[error("Payload parse error: {message}")]
    PayloadError { message: String },

    #[error("Idempotency check failed: {message}")]
    IdempotencyError { message: String },
}

impl WebhookError {
    pub fn error_code(&self) -> &'static str {
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
