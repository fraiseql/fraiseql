#![allow(missing_docs)]

use std::time::Duration;

use fraiseql_error::NotificationError;

#[test]
fn configuration_error_code_and_display() {
    let err = NotificationError::Configuration {
        message: "missing API key".into(),
    };
    assert_eq!(err.error_code(), "notification_config_error");
    assert_eq!(err.to_string(), "Configuration error: missing API key");
}

#[test]
fn provider_error_code_and_display() {
    let err = NotificationError::Provider {
        provider: "sendgrid".into(),
        message:  "rate limited".into(),
    };
    assert_eq!(err.error_code(), "notification_provider_error");
    assert_eq!(
        err.to_string(),
        "Provider error: sendgrid - rate limited"
    );
}

#[test]
fn provider_unavailable_error_code_and_display() {
    let err = NotificationError::ProviderUnavailable {
        provider:    "twilio".into(),
        retry_after: Some(Duration::from_secs(30)),
    };
    assert_eq!(err.error_code(), "notification_provider_unavailable");
    assert_eq!(err.to_string(), "Provider unavailable: twilio");
}

#[test]
fn provider_unavailable_without_retry_after() {
    let err = NotificationError::ProviderUnavailable {
        provider:    "twilio".into(),
        retry_after: None,
    };
    assert_eq!(err.error_code(), "notification_provider_unavailable");
}

#[test]
fn invalid_input_error_code_and_display() {
    let err = NotificationError::InvalidInput {
        message: "empty recipient".into(),
    };
    assert_eq!(err.error_code(), "notification_invalid_input");
    assert_eq!(err.to_string(), "Invalid input: empty recipient");
}

#[test]
fn template_error_code_and_display() {
    let err = NotificationError::Template {
        message: "missing variable".into(),
    };
    assert_eq!(err.error_code(), "notification_template_error");
    assert_eq!(err.to_string(), "Template error: missing variable");
}

#[test]
fn provider_rate_limited_error_code_and_display() {
    let err = NotificationError::ProviderRateLimited {
        provider: "sns".into(),
        seconds:  60,
    };
    assert_eq!(err.error_code(), "notification_rate_limited");
    assert_eq!(
        err.to_string(),
        "Rate limited by provider: retry after 60 seconds"
    );
}

#[test]
fn circuit_open_error_code_and_display() {
    let err = NotificationError::CircuitOpen {
        provider:    "ses".into(),
        retry_after: Duration::from_secs(120),
    };
    assert_eq!(err.error_code(), "notification_circuit_open");
    assert_eq!(
        err.to_string(),
        "Circuit breaker open for provider: ses"
    );
}

#[test]
fn timeout_error_code_and_display() {
    assert_eq!(NotificationError::Timeout.error_code(), "notification_timeout");
    assert_eq!(
        NotificationError::Timeout.to_string(),
        "Timeout sending notification"
    );
}
