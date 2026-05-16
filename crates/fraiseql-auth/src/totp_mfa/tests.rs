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

/// Build a store, enroll a user, confirm enrollment, and return (store, user_id,
/// recovery_codes).
async fn setup_enrolled_user() -> (Arc<InMemoryMfaStore>, String, Vec<String>) {
    let store = Arc::new(InMemoryMfaStore::new());
    let user_id = "user_test_mfa_001";

    let resp = store.begin_enrollment(user_id, "FraiseQL", "alice@example.com").await.unwrap();

    // Generate a valid TOTP code to confirm enrollment.
    let totp = build_totp(&resp.secret_base32, None, "").unwrap();
    let code = totp.generate_current().unwrap();
    store.confirm_enrollment(user_id, &code).await.unwrap();

    (store, user_id.to_string(), resp.recovery_codes)
}

fn build_app(session_store: Arc<InMemorySessionStore>) -> Router {
    let mfa_store = Arc::new(InMemoryMfaStore::new());
    let state = Arc::new(MfaRouteState {
        mfa_store:     mfa_store as Arc<dyn MfaStore>,
        session_store: session_store as Arc<dyn SessionStore>,
        issuer:        "FraiseQL".to_string(),
    });
    Router::new()
        .route("/auth/v1/mfa/enroll", post(mfa_enroll))
        .route("/auth/v1/mfa/challenge", post(mfa_challenge))
        .route("/auth/v1/mfa/verify", post(mfa_verify))
        .route("/auth/v1/mfa/unenroll", post(mfa_unenroll))
        .with_state(state)
}

fn json_body(body: serde_json::Value) -> Body {
    Body::from(serde_json::to_vec(&body).unwrap())
}

// ── Cycle 4 tests — TOTP MFA ───────────────────────────────────────────

#[tokio::test]
async fn test_enroll_returns_otpauth_uri_and_recovery_codes() {
    let store = InMemoryMfaStore::new();
    let resp = store
        .begin_enrollment("user_001", "FraiseQL", "alice@example.com")
        .await
        .unwrap();

    assert!(
        resp.otpauth_uri.starts_with("otpauth://"),
        "should return an otpauth:// URI, got: {}",
        resp.otpauth_uri
    );
    assert_eq!(
        resp.recovery_codes.len(),
        RECOVERY_CODE_COUNT,
        "should return {RECOVERY_CODE_COUNT} recovery codes"
    );
    // Recovery codes must be hex strings of the right length.
    for code in &resp.recovery_codes {
        assert_eq!(code.len(), RECOVERY_CODE_HEX_LEN, "recovery code length wrong: {code}");
    }
    // Enrollment is pending (not confirmed) until a TOTP code is verified.
    assert!(!store.is_enrolled("user_001").await);
}

#[tokio::test]
async fn test_confirm_enrollment_with_valid_totp() {
    let store = InMemoryMfaStore::new();
    let resp = store
        .begin_enrollment("user_001", "FraiseQL", "alice@example.com")
        .await
        .unwrap();

    let totp = build_totp(&resp.secret_base32, None, "").unwrap();
    let code = totp.generate_current().unwrap();

    store.confirm_enrollment("user_001", &code).await.unwrap();
    assert!(store.is_enrolled("user_001").await, "should be enrolled after confirmation");
}

#[tokio::test]
async fn test_confirm_enrollment_wrong_code_fails() {
    let store = InMemoryMfaStore::new();
    store
        .begin_enrollment("user_001", "FraiseQL", "alice@example.com")
        .await
        .unwrap();

    let err = store.confirm_enrollment("user_001", "000000").await.unwrap_err();
    assert!(
        matches!(err, AuthError::InvalidToken { .. }),
        "wrong TOTP code should fail, got: {err:?}"
    );
    assert!(!store.is_enrolled("user_001").await);
}

#[tokio::test]
async fn test_challenge_verify_with_valid_totp() {
    let (store, user_id, _) = setup_enrolled_user().await;

    // Create a challenge.
    let challenge_token = store.create_challenge(&user_id).await.unwrap();

    // Generate a valid TOTP code.
    let enrollment = store.enrollments.get(&user_id).unwrap();
    let totp = build_totp(&enrollment.secret_base32, None, "").unwrap();
    let code = totp.generate_current().unwrap();
    drop(enrollment);

    let verified_user_id = store.verify_challenge(&challenge_token, &code).await.unwrap();
    assert_eq!(verified_user_id, user_id);
}

#[tokio::test]
async fn test_challenge_verify_with_recovery_code() {
    let (store, user_id, recovery_codes) = setup_enrolled_user().await;

    let challenge_token = store.create_challenge(&user_id).await.unwrap();
    let verified_user_id =
        store.verify_challenge(&challenge_token, &recovery_codes[0]).await.unwrap();

    assert_eq!(verified_user_id, user_id, "recovery code should yield correct user_id");

    // The used recovery code must be consumed (cannot be reused).
    let challenge_token2 = store.create_challenge(&user_id).await.unwrap();
    let result = store.verify_challenge(&challenge_token2, &recovery_codes[0]).await;
    assert!(result.is_err(), "recovery code should be single-use");
}

#[tokio::test]
async fn test_challenge_verify_invalid_code_fails() {
    let (store, user_id, _) = setup_enrolled_user().await;
    let challenge_token = store.create_challenge(&user_id).await.unwrap();

    let result = store.verify_challenge(&challenge_token, "000000").await;
    assert!(
        matches!(result, Err(AuthError::InvalidToken { .. })),
        "invalid code should fail, got: {result:?}"
    );
}

#[tokio::test]
async fn test_unenroll_with_valid_totp() {
    let (store, user_id, _) = setup_enrolled_user().await;

    let enrollment = store.enrollments.get(&user_id).unwrap();
    let totp = build_totp(&enrollment.secret_base32, None, "").unwrap();
    let code = totp.generate_current().unwrap();
    drop(enrollment);

    store.unenroll(&user_id, &code).await.unwrap();
    assert!(!store.is_enrolled(&user_id).await, "should not be enrolled after unenroll");
}

#[tokio::test]
async fn test_unenroll_with_recovery_code() {
    let (store, user_id, recovery_codes) = setup_enrolled_user().await;
    store.unenroll(&user_id, &recovery_codes[0]).await.unwrap();
    assert!(!store.is_enrolled(&user_id).await);
}

#[tokio::test]
async fn test_unenroll_wrong_code_fails() {
    let (store, user_id, _) = setup_enrolled_user().await;
    let result = store.unenroll(&user_id, "wrong").await;
    assert!(result.is_err());
    // Should still be enrolled.
    assert!(store.is_enrolled(&user_id).await);
}

#[tokio::test]
async fn test_recovery_codes_are_unique() {
    let store = InMemoryMfaStore::new();
    let resp = store
        .begin_enrollment("user_001", "FraiseQL", "alice@example.com")
        .await
        .unwrap();

    let unique: std::collections::HashSet<&String> = resp.recovery_codes.iter().collect();
    assert_eq!(unique.len(), RECOVERY_CODE_COUNT, "all recovery codes should be unique");
}

#[tokio::test]
async fn test_mfa_enroll_http_returns_200() {
    let app = build_app(Arc::new(InMemorySessionStore::new()));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/v1/mfa/enroll")
                .header(header::CONTENT_TYPE, "application/json")
                .body(json_body(serde_json::json!({
                    "user_id":      "user_001",
                    "account_name": "alice@example.com"
                })))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["otpauth_uri"].as_str().is_some());
    assert_eq!(json["recovery_codes"].as_array().unwrap().len(), 8);
}

#[tokio::test]
async fn test_mfa_challenge_not_enrolled_returns_404() {
    let app = build_app(Arc::new(InMemorySessionStore::new()));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/v1/mfa/challenge")
                .header(header::CONTENT_TYPE, "application/json")
                .body(json_body(serde_json::json!({"user_id": "user_not_enrolled"})))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
