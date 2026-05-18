//! Cross-cutting integration tests for advanced authentication flows.
//!
//! Tests the interactions between modules: email OTP, SMS OTP, TOTP MFA,
//! and account linking.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use axum::{Router, body::Body, http::Request, routing::post};
use tower::ServiceExt as _;

use crate::{
    account_linking::{AccountStore, InMemoryAccountStore},
    otp::{InMemoryOtpStore, OtpStore},
    phone_otp::{InMemorySmsSender, SmsOtpAuthState, send_sms_otp, verify_sms_otp},
    session::{InMemorySessionStore, SessionStore},
    totp_mfa::{InMemoryMfaStore, MfaStore},
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Test: TOTP MFA enroll -> confirm -> challenge -> verify flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_mfa_enroll_then_challenge_verify_integration() {
    let mfa_store = Arc::new(InMemoryMfaStore::new());

    // Enroll and confirm via store API
    let enrollment = mfa_store
        .begin_enrollment("user-42", "FraiseQL-Test", "user42@example.com")
        .await
        .unwrap();

    assert!(!enrollment.secret_base32.is_empty());
    assert!(enrollment.otpauth_uri.starts_with("otpauth://"));
    assert_eq!(enrollment.recovery_codes.len(), 8);

    // Generate a valid TOTP to confirm enrollment
    let totp = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        totp_rs::Secret::Encoded(enrollment.secret_base32.clone()).to_bytes().unwrap(),
        None,
        String::new(),
    )
    .unwrap();
    let code = totp.generate_current().unwrap();
    mfa_store.confirm_enrollment("user-42", &code).await.unwrap();
    assert!(mfa_store.is_enrolled("user-42").await);

    // Create challenge and verify with TOTP code
    let challenge_token = mfa_store.create_challenge("user-42").await.unwrap();
    let verify_code = totp.generate_current().unwrap();
    let user_id = mfa_store.verify_challenge(&challenge_token, &verify_code).await.unwrap();
    assert_eq!(user_id, "user-42");

    // Verify with recovery code via a new challenge
    let challenge_token2 = mfa_store.create_challenge("user-42").await.unwrap();
    let result = mfa_store
        .verify_challenge(&challenge_token2, &enrollment.recovery_codes[0])
        .await;
    assert!(result.is_ok(), "recovery code should work");
}

// ---------------------------------------------------------------------------
// Test: SMS OTP same phone twice -> same user (idempotent via AccountStore)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sms_otp_same_phone_returns_same_user() {
    let account_store = Arc::new(InMemoryAccountStore::new());
    let otp_store = Arc::new(InMemoryOtpStore::new());
    let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
    let sms_sender = Arc::new(InMemorySmsSender::new());

    let state = Arc::new(SmsOtpAuthState {
        otp_store: otp_store as Arc<dyn OtpStore>,
        sms_sender: sms_sender.clone(),
        session_store,
        user_store: Some(account_store.clone() as Arc<dyn AccountStore>),
    });
    let app = Router::new()
        .route("/auth/v1/otp/sms", post(send_sms_otp))
        .route("/auth/v1/verify/sms", post(verify_sms_otp))
        .with_state(state);

    for _ in 0..2 {
        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+33612345678" }));
        app.clone().oneshot(req).await.unwrap();
        let code = sms_sender.last_otp_for("+33612345678").await.unwrap();

        let req = post_json(
            "/auth/v1/verify/sms",
            serde_json::json!({ "phone": "+33612345678", "code": code }),
        );
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
    }

    assert_eq!(account_store.len(), 1, "same phone must resolve to same user");
}

// ---------------------------------------------------------------------------
// Test: account linking via InMemoryAccountStore
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_account_linking_same_email_across_providers() {
    let store = InMemoryAccountStore::new();

    let r1 = store
        .link_or_create_user("alice@example.com", "github", "gh-123")
        .await
        .unwrap();
    assert!(r1.is_new);

    let r2 = store
        .link_or_create_user("alice@example.com", "google", "gg-456")
        .await
        .unwrap();
    assert!(!r2.is_new);
    assert!(r2.linked);
    assert_eq!(r1.user_id, r2.user_id, "same email must link to same user");

    let account = store.get_account(&r1.user_id).await.unwrap();
    assert_eq!(account.providers.len(), 2);
}
