#![allow(missing_docs)] // Reason: integration test crate
#![cfg(feature = "axum-compat")]
#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable

use axum::{
    http::{HeaderValue, StatusCode},
    response::IntoResponse,
};
use fraiseql_error::{ErrorResponse, FileError, FraiseQLError};

// --- Status code tests ---

#[test]
fn parse_error_returns_400() {
    let err = FraiseQLError::parse("bad query");
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn validation_error_returns_400() {
    let err = FraiseQLError::validation("missing required field");
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn authentication_error_returns_401() {
    let err = FraiseQLError::auth_error("bad token");
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn auth_subsystem_error_returns_401() {
    // Smoke test of the boxed-payload Auth variant. A real subsystem error
    // type would `impl From<X> for FraiseQLError` in its own crate; here
    // we synthesize the boxed payload directly with a stand-in.
    let inner = FraiseQLError::auth_error("subsystem rejected token");
    let err = FraiseQLError::Auth(Box::new(inner));
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn authorization_error_returns_403() {
    let err = FraiseQLError::unauthorized("not allowed");
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[test]
fn not_found_returns_404() {
    let err = FraiseQLError::not_found("User", "42");
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[test]
fn rate_limited_returns_429() {
    let err = FraiseQLError::rate_limited_with_retry(60);
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(resp.headers().get("Retry-After").unwrap(), &HeaderValue::from_static("60"));
}

#[test]
fn service_unavailable_returns_503() {
    let err = FraiseQLError::ServiceUnavailable {
        message:     "maintenance".into(),
        retry_after: Some(120),
    };
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(resp.headers().get("Retry-After").unwrap(), &HeaderValue::from_static("120"));
}

#[test]
fn service_unavailable_without_retry_after_omits_header() {
    let err = FraiseQLError::ServiceUnavailable {
        message:     "down".into(),
        retry_after: None,
    };
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(resp.headers().get("Retry-After").is_none());
}

#[test]
fn database_error_returns_500() {
    let err = FraiseQLError::database("connection refused");
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn internal_error_returns_500() {
    let err = FraiseQLError::internal("oops");
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn observer_error_returns_500() {
    let inner = FraiseQLError::internal("observer fault");
    let err = FraiseQLError::Observer(Box::new(inner));
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn webhook_error_returns_400() {
    let inner = FraiseQLError::validation("bad signature");
    let err = FraiseQLError::Webhook(Box::new(inner));
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// --- File errors flow through FraiseQLError::File ---

#[test]
fn file_too_large_returns_400() {
    let err: FraiseQLError = FileError::TooLarge {
        size: 100,
        max:  50,
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn file_invalid_type_returns_400() {
    let err: FraiseQLError = FileError::InvalidType {
        got:     "exe".into(),
        allowed: vec!["png".into()],
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn file_not_found_returns_400() {
    // FileError::NotFound is a file-domain error; its HTTP shape is
    // controlled by `FraiseQLError::File(_)` which currently maps to 400.
    // Callers that need a 404 should construct `FraiseQLError::NotFound`
    // directly with the resource description.
    let err: FraiseQLError = FileError::NotFound { id: "abc".into() }.into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn file_quota_exceeded_returns_400() {
    let err: FraiseQLError = FileError::QuotaExceeded.into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn file_virus_detected_returns_400() {
    let err: FraiseQLError = FileError::VirusDetected {
        details: "eicar".into(),
    }
    .into();
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
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
