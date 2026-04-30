//! Cross-cutting integration tests for Phase 13 advanced authentication flows.
//!
//! Tests the interactions between modules: multi-provider social login,
//! account linking, email OTP, SMS OTP, TOTP MFA, and anonymous sessions.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use axum::{Router, body::Body, http::Request, routing::post};
use tower::ServiceExt as _;

use crate::{
    account_linking::{InMemoryUserStore, UserStore},
    anonymous::{AnonAuthState, signup_anonymous},
    otp::{
        InMemoryEmailSender, InMemoryOtpStore, OtpAuthState, OtpStore, send_otp, verify_otp,
    },
    phone_otp::{InMemorySmsSender, SmsOtpAuthState, send_sms_otp, verify_sms_otp},
    session::{InMemorySessionStore, SessionStore},
    totp_mfa::{InMemoryMfaStore, MfaAuthState, MfaStore, generate_totp, mfa_enroll, mfa_verify},
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

async fn parse_json(resp: axum::http::Response<Body>) -> serde_json::Value {
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

// ---------------------------------------------------------------------------
// Test: email OTP and SMS OTP create separate users for different identifiers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_email_and_sms_otp_create_separate_users() {
    let user_store = Arc::new(InMemoryUserStore::new());
    let otp_store = Arc::new(InMemoryOtpStore::new());
    let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
    let email_sender = Arc::new(InMemoryEmailSender::new());
    let sms_sender = Arc::new(InMemorySmsSender::new());

    // Email OTP flow
    let email_state = Arc::new(OtpAuthState {
        otp_store:     otp_store.clone() as Arc<dyn OtpStore>,
        email_sender:  email_sender.clone(),
        session_store: session_store.clone(),
        user_store:    Some(user_store.clone() as Arc<dyn UserStore>),
    });
    let email_app = Router::new()
        .route("/auth/v1/otp", post(send_otp))
        .route("/auth/v1/verify", post(verify_otp))
        .with_state(email_state);

    let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "alice@example.com" }));
    email_app.clone().oneshot(req).await.unwrap();
    let code = email_sender.last_otp_for("alice@example.com").await.unwrap();

    let req = post_json(
        "/auth/v1/verify",
        serde_json::json!({ "email": "alice@example.com", "code": code }),
    );
    let resp = email_app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);

    // SMS OTP flow
    let sms_state = Arc::new(SmsOtpAuthState {
        otp_store:     otp_store as Arc<dyn OtpStore>,
        sms_sender:    sms_sender.clone(),
        session_store: session_store.clone(),
        user_store:    Some(user_store.clone() as Arc<dyn UserStore>),
    });
    let sms_app = Router::new()
        .route("/auth/v1/otp/sms", post(send_sms_otp))
        .route("/auth/v1/verify/sms", post(verify_sms_otp))
        .with_state(sms_state);

    let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+14155551234" }));
    sms_app.clone().oneshot(req).await.unwrap();
    let code = sms_sender.last_otp_for("+14155551234").await.unwrap();

    let req = post_json(
        "/auth/v1/verify/sms",
        serde_json::json!({ "phone": "+14155551234", "code": code }),
    );
    let resp = sms_app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);

    // Different identifiers → separate users
    assert_eq!(user_store.user_count().await, 2);
}

// ---------------------------------------------------------------------------
// Test: anonymous signup then email OTP verify creates separate users
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_anonymous_then_email_creates_separate_users() {
    let user_store = Arc::new(InMemoryUserStore::new());
    let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());

    // Anonymous signup
    let anon_state = Arc::new(
        AnonAuthState::new(session_store.clone())
            .with_user_store(user_store.clone() as Arc<dyn UserStore>),
    );
    let anon_app = Router::new()
        .route("/auth/v1/signup", post(signup_anonymous))
        .with_state(anon_state);

    let req = post_json("/auth/v1/signup", serde_json::json!({}));
    let resp = anon_app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let anon_json = parse_json(resp).await;
    assert_eq!(anon_json["is_anonymous"], true);

    // Email OTP
    let otp_store = Arc::new(InMemoryOtpStore::new());
    let email_sender = Arc::new(InMemoryEmailSender::new());
    let email_state = Arc::new(OtpAuthState {
        otp_store:     otp_store as Arc<dyn OtpStore>,
        email_sender:  email_sender.clone(),
        session_store,
        user_store:    Some(user_store.clone() as Arc<dyn UserStore>),
    });
    let email_app = Router::new()
        .route("/auth/v1/otp", post(send_otp))
        .route("/auth/v1/verify", post(verify_otp))
        .with_state(email_state);

    let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "alice@example.com" }));
    email_app.clone().oneshot(req).await.unwrap();
    let code = email_sender.last_otp_for("alice@example.com").await.unwrap();

    let req = post_json(
        "/auth/v1/verify",
        serde_json::json!({ "email": "alice@example.com", "code": code }),
    );
    let resp = email_app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);

    // anonymous + email = 2 separate users
    assert_eq!(user_store.user_count().await, 2);
}

// ---------------------------------------------------------------------------
// Test: TOTP MFA enroll → verify flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_mfa_enroll_then_verify_integration() {
    let mfa_store = Arc::new(InMemoryMfaStore::new());
    let mfa_state = Arc::new(MfaAuthState {
        mfa_store: mfa_store.clone(),
        issuer: "FraiseQL-Test".to_string(),
    });

    let app = Router::new()
        .route("/auth/v1/mfa/enroll", post(mfa_enroll))
        .route("/auth/v1/mfa/verify", post(mfa_verify))
        .with_state(mfa_state);

    // Enroll
    let req = post_json(
        "/auth/v1/mfa/enroll",
        serde_json::json!({ "user_id": "user-42" }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let enroll_json = parse_json(resp).await;
    let recovery_codes = enroll_json["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(recovery_codes.len(), 8);

    // Get the secret from the store to generate a valid TOTP code
    let enrollment = mfa_store.get_enrollment("user-42").await.unwrap().unwrap();
    let code = generate_totp(&enrollment.secret, unix_now());

    // Verify with TOTP code
    let req = post_json(
        "/auth/v1/mfa/verify",
        serde_json::json!({ "user_id": "user-42", "code": code }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);

    // Verify with recovery code
    let req = post_json(
        "/auth/v1/mfa/verify",
        serde_json::json!({ "user_id": "user-42", "code": recovery_codes[0] }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Test: duplicate anonymous signups all get unique identities
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_anonymous_signups_produce_unique_identities_integration() {
    let user_store = Arc::new(InMemoryUserStore::new());
    let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());

    let anon_state = Arc::new(
        AnonAuthState::new(session_store)
            .with_user_store(user_store.clone() as Arc<dyn UserStore>),
    );
    let app = Router::new()
        .route("/auth/v1/signup", post(signup_anonymous))
        .with_state(anon_state);

    let mut user_ids = Vec::new();
    for _ in 0..5 {
        let req = post_json("/auth/v1/signup", serde_json::json!({}));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let json = parse_json(resp).await;
        user_ids.push(json["user_id"].as_str().unwrap().to_string());
    }

    // All unique
    let mut unique_ids = user_ids.clone();
    unique_ids.sort();
    unique_ids.dedup();
    assert_eq!(unique_ids.len(), 5, "all anonymous signups must produce unique user IDs");
    assert_eq!(user_store.user_count().await, 5);
}

// ---------------------------------------------------------------------------
// Test: same email via email OTP twice → same user (idempotent)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_email_otp_same_email_returns_same_user() {
    let user_store = Arc::new(InMemoryUserStore::new());
    let otp_store = Arc::new(InMemoryOtpStore::new());
    let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
    let email_sender = Arc::new(InMemoryEmailSender::new());

    let state = Arc::new(OtpAuthState {
        otp_store:     otp_store as Arc<dyn OtpStore>,
        email_sender:  email_sender.clone(),
        session_store,
        user_store:    Some(user_store.clone() as Arc<dyn UserStore>),
    });
    let app = Router::new()
        .route("/auth/v1/otp", post(send_otp))
        .route("/auth/v1/verify", post(verify_otp))
        .with_state(state);

    // First login
    let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "bob@example.com" }));
    app.clone().oneshot(req).await.unwrap();
    let code = email_sender.last_otp_for("bob@example.com").await.unwrap();

    let req = post_json(
        "/auth/v1/verify",
        serde_json::json!({ "email": "bob@example.com", "code": code }),
    );
    app.clone().oneshot(req).await.unwrap();

    // Second login (same email)
    let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "bob@example.com" }));
    app.clone().oneshot(req).await.unwrap();
    let code = email_sender.last_otp_for("bob@example.com").await.unwrap();

    let req = post_json(
        "/auth/v1/verify",
        serde_json::json!({ "email": "bob@example.com", "code": code }),
    );
    app.oneshot(req).await.unwrap();

    // Same email → same user (only 1 user created)
    assert_eq!(user_store.user_count().await, 1, "same email must resolve to same user");
}

// ---------------------------------------------------------------------------
// Test: SMS OTP same phone twice → same user (idempotent)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sms_otp_same_phone_returns_same_user() {
    let user_store = Arc::new(InMemoryUserStore::new());
    let otp_store = Arc::new(InMemoryOtpStore::new());
    let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
    let sms_sender = Arc::new(InMemorySmsSender::new());

    let state = Arc::new(SmsOtpAuthState {
        otp_store:     otp_store as Arc<dyn OtpStore>,
        sms_sender:    sms_sender.clone(),
        session_store,
        user_store:    Some(user_store.clone() as Arc<dyn UserStore>),
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

    assert_eq!(user_store.user_count().await, 1, "same phone must resolve to same user");
}
