#![allow(clippy::unwrap_used)]  // Reason: test/bench code, panics are acceptable
#![allow(missing_docs)]

use axum::{
    http::{HeaderValue, StatusCode},
    response::IntoResponse,
};
use fraiseql_error::{
    AuthError, ErrorResponse, FileError, IntegrationError, NotificationError, RuntimeError,
    WebhookError,
};

// --- Status code tests ---

#[test]
fn config_error_returns_500() {
    let err: RuntimeError = fraiseql_error::ConfigError::NotFound.into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn auth_error_returns_401() {
    let err: RuntimeError = AuthError::InvalidCredentials.into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn auth_token_expired_returns_401() {
    let err: RuntimeError = AuthError::TokenExpired.into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn auth_insufficient_permissions_returns_403() {
    let err: RuntimeError = AuthError::InsufficientPermissions {
        required: "admin".into(),
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[test]
fn auth_account_locked_returns_403() {
    let err: RuntimeError = AuthError::AccountLocked {
        reason: "brute force".into(),
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[test]
fn webhook_invalid_signature_returns_401() {
    let err: RuntimeError = WebhookError::InvalidSignature.into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn webhook_duplicate_event_returns_200() {
    let err: RuntimeError = WebhookError::DuplicateEvent {
        event_id: "evt_1".into(),
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn webhook_payload_error_returns_400() {
    let err: RuntimeError = WebhookError::PayloadError {
        message: "bad json".into(),
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn file_too_large_returns_413() {
    let err: RuntimeError = FileError::TooLarge {
        size: 100,
        max:  50,
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[test]
fn file_invalid_type_returns_415() {
    let err: RuntimeError = FileError::InvalidType {
        got:     "exe".into(),
        allowed: vec!["png".into()],
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[test]
fn file_not_found_returns_404() {
    let err: RuntimeError = FileError::NotFound { id: "x".into() }.into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[test]
fn file_virus_detected_returns_422() {
    let err: RuntimeError = FileError::VirusDetected {
        details: "eicar".into(),
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn file_quota_exceeded_returns_507() {
    let err: RuntimeError = FileError::QuotaExceeded.into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::INSUFFICIENT_STORAGE);
}

#[test]
fn file_storage_returns_400() {
    let err: RuntimeError = FileError::Storage {
        message: "err".into(),
        source:  None,
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn notification_circuit_open_returns_503() {
    let err: RuntimeError = NotificationError::CircuitOpen {
        provider:    "ses".into(),
        retry_after: std::time::Duration::from_secs(60),
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn notification_rate_limited_returns_429() {
    let err: RuntimeError = NotificationError::ProviderRateLimited {
        provider: "sns".into(),
        seconds:  30,
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[test]
fn notification_invalid_input_returns_400() {
    let err: RuntimeError = NotificationError::InvalidInput {
        message: "bad".into(),
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn notification_provider_returns_500() {
    let err: RuntimeError = NotificationError::Provider {
        provider: "x".into(),
        message:  "y".into(),
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// --- Rate limited / service unavailable ---

#[test]
fn rate_limited_returns_429() {
    let err = RuntimeError::RateLimited { retry_after: None };
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[test]
fn rate_limited_with_retry_after_header() {
    let err = RuntimeError::RateLimited {
        retry_after: Some(60),
    };
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(resp.headers().get("Retry-After").unwrap(), &HeaderValue::from_static("60"));
}

#[test]
fn service_unavailable_returns_503() {
    let err = RuntimeError::ServiceUnavailable {
        reason:      "maintenance".into(),
        retry_after: None,
    };
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn service_unavailable_with_retry_after_header() {
    let err = RuntimeError::ServiceUnavailable {
        reason:      "deploying".into(),
        retry_after: Some(120),
    };
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(resp.headers().get("Retry-After").unwrap(), &HeaderValue::from_static("120"));
}

#[test]
fn not_found_returns_404() {
    let err = RuntimeError::NotFound {
        resource: "user".into(),
    };
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[test]
fn database_error_returns_500() {
    let err: RuntimeError = sqlx::Error::RowNotFound.into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn internal_error_returns_500() {
    let err = RuntimeError::Internal {
        message: "oops".into(),
        source:  None,
    };
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn integration_error_returns_500() {
    let err: RuntimeError = IntegrationError::Cache {
        message: "down".into(),
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// --- ErrorResponse serialization ---

#[test]
fn error_response_serialization_basic() {
    let resp = ErrorResponse::new("test_error", "Something failed", "test_code");
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["error"], "test_error");
    assert_eq!(json["error_description"], "Something failed");
    assert_eq!(json["error_code"], "test_code");
    assert_eq!(json["error_uri"], "https://docs.fraiseql.dev/errors#test_code");
    assert!(json.get("details").is_none());
    assert!(json.get("retry_after").is_none());
}

#[test]
fn error_response_with_details() {
    let resp = ErrorResponse::new("err", "desc", "code")
        .with_details(serde_json::json!({"field": "name"}));
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["details"]["field"], "name");
}

#[test]
fn error_response_with_retry_after() {
    let resp = ErrorResponse::new("err", "desc", "code").with_retry_after(30);
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["retry_after"], 30);
}
