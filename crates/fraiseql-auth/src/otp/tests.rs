#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::post,
};
use tower::ServiceExt as _;

use super::*;
use crate::session::InMemorySessionStore;

fn build_route_state() -> Arc<OtpRouteState> {
    Arc::new(OtpRouteState {
        otp_store:      Arc::new(InMemoryOtpStore::new()),
        email_delivery: Arc::new(NoopEmailDelivery),
        session_store:  Arc::new(InMemorySessionStore::new()),
    })
}

fn build_app(state: Arc<OtpRouteState>) -> Router {
    Router::new()
        .route("/auth/v1/otp", post(otp_send))
        .route("/auth/v1/verify", post(otp_verify))
        .with_state(state)
}

fn json_body(body: serde_json::Value) -> Body {
    Body::from(serde_json::to_vec(&body).unwrap())
}

// ── Cycle 3 tests — OTP ────────────────────────────────────────────────

#[tokio::test]
async fn test_otp_send_returns_200_with_message_id() {
    let state = build_route_state();
    let app = build_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/v1/otp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(json_body(serde_json::json!({"email": "alice@example.com"})))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["message_id"].as_str().is_some(),
        "response should contain a message_id field"
    );
}

#[tokio::test]
async fn test_otp_verify_valid_code_returns_session_token() {
    let otp_store = Arc::new(InMemoryOtpStore::new());
    let state = Arc::new(OtpRouteState {
        otp_store:      Arc::clone(&otp_store) as Arc<dyn OtpStore>,
        email_delivery: Arc::new(NoopEmailDelivery),
        session_store:  Arc::new(InMemorySessionStore::new()),
    });

    // Directly create an OTP so we know the code.
    let code = otp_store.create_otp("alice@example.com").await.unwrap();

    let app = build_app(state);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/v1/verify")
                .header(header::CONTENT_TYPE, "application/json")
                .body(json_body(serde_json::json!({"email": "alice@example.com", "code": code})))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK, "valid code should yield 200");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["access_token"].as_str().is_some(),
        "response should contain an access_token"
    );
    assert!(
        json["refresh_token"].as_str().is_some(),
        "response should contain a refresh_token"
    );
}

#[tokio::test]
async fn test_otp_verify_wrong_code_returns_422() {
    let otp_store = Arc::new(InMemoryOtpStore::new());
    let state = Arc::new(OtpRouteState {
        otp_store:      Arc::clone(&otp_store) as Arc<dyn OtpStore>,
        email_delivery: Arc::new(NoopEmailDelivery),
        session_store:  Arc::new(InMemorySessionStore::new()),
    });
    otp_store.create_otp("alice@example.com").await.unwrap();

    let app = build_app(state);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/v1/verify")
                .header(header::CONTENT_TYPE, "application/json")
                .body(json_body(
                    serde_json::json!({"email": "alice@example.com", "code": "000000"}),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "wrong code → 422");
}

#[tokio::test]
async fn test_otp_verify_no_pending_otp_returns_422() {
    let state = build_route_state();
    let app = build_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/v1/verify")
                .header(header::CONTENT_TYPE, "application/json")
                .body(json_body(
                    serde_json::json!({"email": "nobody@example.com", "code": "123456"}),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "no pending OTP → 422");
}

#[tokio::test]
async fn test_otp_is_single_use() {
    // After a successful verify the code must be invalidated.
    let otp_store = Arc::new(InMemoryOtpStore::new());
    let code = otp_store.create_otp("alice@example.com").await.unwrap();

    // First verify succeeds.
    otp_store.verify_otp("alice@example.com", &code).await.unwrap();

    // Second attempt with the same code must fail.
    let result = otp_store.verify_otp("alice@example.com", &code).await;
    assert!(result.is_err(), "OTP must be single-use");
}

#[tokio::test]
async fn test_otp_rate_limit_on_send() {
    let store = InMemoryOtpStore::new();

    // Exhaust the 3 sends allowed in the rate window.
    store.create_otp("alice@example.com").await.unwrap();
    store.create_otp("alice@example.com").await.unwrap();
    store.create_otp("alice@example.com").await.unwrap();

    // Fourth send should be rate-limited.
    let result = store.create_otp("alice@example.com").await;
    assert!(
        matches!(result, Err(AuthError::RateLimited { .. })),
        "4th OTP send should be rate limited, got: {result:?}"
    );
}

#[tokio::test]
async fn test_otp_blank_email_returns_422() {
    let state = build_route_state();
    let app = build_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/v1/otp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(json_body(serde_json::json!({"email": "   "})))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "blank email → 422");
}

#[tokio::test]
async fn test_otp_codes_are_six_digits() {
    let store = InMemoryOtpStore::new();
    for _ in 0..3 {
        let code = store.create_otp("alice@example.com").await.unwrap();
        assert_eq!(code.len(), 6, "OTP must be exactly 6 characters, got: {code}");
        assert!(code.chars().all(|c| c.is_ascii_digit()), "OTP must be decimal digits");
        // Reset for next iteration by using a different email
    }
}

#[tokio::test]
async fn test_otp_store_as_trait_object() {
    let store: Arc<dyn OtpStore> = Arc::new(InMemoryOtpStore::new());
    let code = store.create_otp("alice@example.com").await.unwrap();
    store.verify_otp("alice@example.com", &code).await.unwrap();
}

#[tokio::test]
async fn test_noop_email_delivery_returns_message_id() {
    let delivery = NoopEmailDelivery;
    let id = delivery.send_otp("alice@example.com", "123456").await.unwrap();
    assert!(!id.is_empty(), "message_id should not be empty");
}
