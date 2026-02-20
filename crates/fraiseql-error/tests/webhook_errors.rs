#![allow(missing_docs)]

use fraiseql_error::WebhookError;

#[test]
fn invalid_signature_error_code_and_display() {
    assert_eq!(WebhookError::InvalidSignature.error_code(), "webhook_invalid_signature");
    assert_eq!(WebhookError::InvalidSignature.to_string(), "Invalid signature");
}

#[test]
fn missing_signature_error_code_and_display() {
    let err = WebhookError::MissingSignature {
        header: "X-Hub-Signature".into(),
    };
    assert_eq!(err.error_code(), "webhook_missing_signature");
    assert_eq!(err.to_string(), "Missing signature header: X-Hub-Signature");
}

#[test]
fn timestamp_expired_error_code_and_display() {
    let err = WebhookError::TimestampExpired {
        age_seconds: 600,
        max_seconds: 300,
    };
    assert_eq!(err.error_code(), "webhook_timestamp_expired");
    assert_eq!(err.to_string(), "Timestamp too old: 600s (max: 300s)");
}

#[test]
fn timestamp_future_error_code_and_display() {
    let err = WebhookError::TimestampFuture {
        future_seconds: 120,
    };
    assert_eq!(err.error_code(), "webhook_timestamp_future");
    assert_eq!(err.to_string(), "Timestamp in future: 120s");
}

#[test]
fn duplicate_event_error_code_and_display() {
    let err = WebhookError::DuplicateEvent {
        event_id: "evt_123".into(),
    };
    assert_eq!(err.error_code(), "webhook_duplicate_event");
    assert_eq!(err.to_string(), "Duplicate event: evt_123");
}

#[test]
fn unknown_event_error_code_and_display() {
    let err = WebhookError::UnknownEvent {
        event_type: "user.deleted".into(),
    };
    assert_eq!(err.error_code(), "webhook_unknown_event");
    assert_eq!(err.to_string(), "Unknown event type: user.deleted");
}

#[test]
fn provider_not_configured_error_code_and_display() {
    let err = WebhookError::ProviderNotConfigured {
        provider: "stripe".into(),
    };
    assert_eq!(err.error_code(), "webhook_provider_not_configured");
    assert_eq!(err.to_string(), "Provider not configured: stripe");
}

#[test]
fn payload_error_code_and_display() {
    let err = WebhookError::PayloadError {
        message: "invalid JSON".into(),
    };
    assert_eq!(err.error_code(), "webhook_payload_error");
    assert_eq!(err.to_string(), "Payload parse error: invalid JSON");
}

#[test]
fn idempotency_error_code_and_display() {
    let err = WebhookError::IdempotencyError {
        message: "key conflict".into(),
    };
    assert_eq!(err.error_code(), "webhook_idempotency_error");
    assert_eq!(err.to_string(), "Idempotency check failed: key conflict");
}
