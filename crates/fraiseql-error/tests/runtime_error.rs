#![allow(missing_docs)]

use fraiseql_error::{
    AuthError, ConfigError, FileError, IntegrationError, NotificationError, ObserverError,
    RuntimeError, WebhookError,
};

#[test]
fn from_auth_error() {
    let inner = AuthError::TokenExpired;
    let err: RuntimeError = inner.into();
    assert_eq!(err.error_code(), "token_expired");
    assert_eq!(err.to_string(), "Token expired");
}

#[test]
fn from_config_error() {
    let inner = ConfigError::NotFound;
    let err: RuntimeError = inner.into();
    assert_eq!(err.error_code(), "config_not_found");
}

#[test]
fn from_webhook_error() {
    let inner = WebhookError::InvalidSignature;
    let err: RuntimeError = inner.into();
    assert_eq!(err.error_code(), "webhook_invalid_signature");
}

#[test]
fn from_file_error() {
    let inner = FileError::QuotaExceeded;
    let err: RuntimeError = inner.into();
    assert_eq!(err.error_code(), "file_quota_exceeded");
}

#[test]
fn from_notification_error() {
    let inner = NotificationError::Timeout;
    let err: RuntimeError = inner.into();
    assert_eq!(err.error_code(), "notification_timeout");
}

#[test]
fn from_observer_error() {
    let inner = ObserverError::InvalidCondition {
        message: "bad".into(),
    };
    let err: RuntimeError = inner.into();
    assert_eq!(err.error_code(), "observer_invalid_condition");
}

#[test]
fn from_integration_error() {
    let inner = IntegrationError::Timeout {
        operation: "query".into(),
    };
    let err: RuntimeError = inner.into();
    assert_eq!(err.error_code(), "integration_timeout");
}

#[test]
fn from_sqlx_error() {
    let err: RuntimeError = sqlx::Error::RowNotFound.into();
    assert_eq!(err.error_code(), "database_error");
    assert_eq!(
        err.to_string(),
        "Database error: no rows returned by a query that expected to return at least one row"
    );
}

#[test]
fn rate_limited_error_code() {
    let err = RuntimeError::RateLimited {
        retry_after: Some(60),
    };
    assert_eq!(err.error_code(), "rate_limited");
    assert_eq!(err.to_string(), "Rate limit exceeded");
}

#[test]
fn service_unavailable_error_code_and_display() {
    let err = RuntimeError::ServiceUnavailable {
        reason: "maintenance".into(),
        retry_after: Some(300),
    };
    assert_eq!(err.error_code(), "service_unavailable");
    assert_eq!(err.to_string(), "Service unavailable: maintenance");
}

#[test]
fn not_found_error_code_and_display() {
    let err = RuntimeError::NotFound {
        resource: "user/42".into(),
    };
    assert_eq!(err.error_code(), "not_found");
    assert_eq!(err.to_string(), "Resource not found: user/42");
}

#[test]
fn internal_error_without_source() {
    let err = RuntimeError::Internal {
        message: "unexpected state".into(),
        source: None,
    };
    assert_eq!(err.error_code(), "internal_error");
    assert_eq!(err.to_string(), "Internal error: unexpected state");
    assert!(std::error::Error::source(&err).is_none());
}

#[test]
fn internal_error_with_source() {
    let io_err = std::io::Error::other("disk failure");
    let err = RuntimeError::Internal {
        message: "storage failed".into(),
        source: Some(Box::new(io_err)),
    };
    assert_eq!(err.error_code(), "internal_error");
    assert!(std::error::Error::source(&err).is_some());
}

#[test]
fn docs_url_format() {
    let err = RuntimeError::RateLimited { retry_after: None };
    assert_eq!(err.docs_url(), "https://docs.fraiseql.dev/errors#rate_limited");
}

#[test]
fn docs_url_delegates_to_inner() {
    let err: RuntimeError = AuthError::TokenExpired.into();
    assert_eq!(err.docs_url(), "https://docs.fraiseql.dev/errors#token_expired");
}
