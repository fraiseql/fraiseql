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
        otp_store:       Arc::new(InMemoryOtpStore::new()),
        email_delivery:  Arc::new(NoopEmailDelivery),
        session_store:   Arc::new(InMemorySessionStore::new()),
        account_store:   None,
        allow_gate:      None,
        session_cookies: None,
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
        otp_store:       Arc::clone(&otp_store) as Arc<dyn OtpStore>,
        email_delivery:  Arc::new(NoopEmailDelivery),
        session_store:   Arc::new(InMemorySessionStore::new()),
        account_store:   None,
        allow_gate:      None,
        session_cookies: None,
    });

    // Directly create an OTP so we know the code.
    let code = otp_store.create_otp("alice@example.com").await.unwrap().code;

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
        otp_store:       Arc::clone(&otp_store) as Arc<dyn OtpStore>,
        email_delivery:  Arc::new(NoopEmailDelivery),
        session_store:   Arc::new(InMemorySessionStore::new()),
        account_store:   None,
        allow_gate:      None,
        session_cookies: None,
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
    let code = otp_store.create_otp("alice@example.com").await.unwrap().code;

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
        let code = store.create_otp("alice@example.com").await.unwrap().code;
        assert_eq!(code.len(), 6, "OTP must be exactly 6 characters, got: {code}");
        assert!(code.chars().all(|c| c.is_ascii_digit()), "OTP must be decimal digits");
        // Reset for next iteration by using a different email
    }
}

#[tokio::test]
async fn test_otp_store_as_trait_object() {
    let store: Arc<dyn OtpStore> = Arc::new(InMemoryOtpStore::new());
    let code = store.create_otp("alice@example.com").await.unwrap().code;
    store.verify_otp("alice@example.com", &code).await.unwrap();
}

#[tokio::test]
async fn test_noop_email_delivery_returns_message_id() {
    let delivery = NoopEmailDelivery;
    let id = delivery.send_otp("alice@example.com", &OtpIssue::generate()).await.unwrap();
    assert!(!id.is_empty(), "message_id should not be empty");
}

// L-clock-failopen: a clock failure must treat the OTP code as expired (fail closed).
#[test]
fn otp_clock_failure_treats_code_as_expired() {
    assert!(
        super::OtpRecord::is_expired_at(u64::MAX, None),
        "clock failure must fail closed"
    );
    assert!(super::OtpRecord::is_expired_at(100, Some(100)), "now == expires → expired");
    assert!(!super::OtpRecord::is_expired_at(100, Some(99)), "before expiry → valid");
}

// ── guesses are bounded per address, not per code ───────────────

/// A resend issues a fresh code, and it used to issue a fresh attempt count with it: three
/// sends a window made nine guesses, not the three the module doc promises. The fourth
/// guess inside the window is refused even when it is the right code.
#[tokio::test]
async fn a_resend_does_not_buy_more_guesses_inside_the_window() {
    let store = InMemoryOtpStore::new();
    let email = "victim@example.com";

    store.create_otp(email).await.unwrap();
    for _ in 0..MAX_VERIFY_ATTEMPTS {
        let wrong = store.verify_otp(email, "not-a-code").await.unwrap_err();
        assert!(matches!(wrong, AuthError::InvalidToken { .. }), "got {wrong:?}");
    }

    let code = store.create_otp(email).await.unwrap().code;
    let fourth = store.verify_otp(email, &code).await.unwrap_err();
    assert!(matches!(fourth, AuthError::RateLimited { .. }), "got {fourth:?}");
}

// ── link tokens ─────────────────────────────────────────────────

#[test]
fn a_link_token_is_256_bits_of_base64url_and_debug_prints_neither_half() {
    let issue = OtpIssue::generate();
    assert_eq!(issue.link_token.len(), 43, "32 bytes, unpadded base64url");
    assert!(
        issue
            .link_token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "base64url alphabet"
    );
    assert_ne!(issue.link_token, OtpIssue::generate().link_token);
    let shown = format!("{issue:?}");
    assert!(!shown.contains(&issue.link_token) && !shown.contains(&issue.code), "{shown}");
}

#[tokio::test]
async fn a_link_token_redeems_once_and_names_its_address() {
    let store = InMemoryOtpStore::new();
    let issue = store.create_otp("alice@example.com").await.unwrap();
    assert_eq!(store.redeem_link(&issue.link_token).await.unwrap(), "alice@example.com");
    assert!(store.redeem_link(&issue.link_token).await.is_err(), "a link is single-use");
}

#[tokio::test]
async fn a_redeemed_link_spends_its_code_and_a_redeemed_code_spends_its_link() {
    let store = InMemoryOtpStore::new();
    let issue = store.create_otp("alice@example.com").await.unwrap();
    store.redeem_link(&issue.link_token).await.unwrap();
    assert!(store.verify_otp("alice@example.com", &issue.code).await.is_err());

    let issue = store.create_otp("bob@example.com").await.unwrap();
    store.verify_otp("bob@example.com", &issue.code).await.unwrap();
    assert!(store.redeem_link(&issue.link_token).await.is_err());
}

#[tokio::test]
async fn a_resend_replaces_the_link() {
    let store = InMemoryOtpStore::new();
    let first = store.create_otp("alice@example.com").await.unwrap();
    let second = store.create_otp("alice@example.com").await.unwrap();
    assert!(store.redeem_link(&first.link_token).await.is_err(), "replaced link refused");
    assert_eq!(store.redeem_link(&second.link_token).await.unwrap(), "alice@example.com");
}

#[tokio::test]
async fn an_expired_link_is_refused_and_spent() {
    let store = InMemoryOtpStore::new();
    let issue = store.create_otp("alice@example.com").await.unwrap();
    store.codes.get_mut("alice@example.com").unwrap().expires = 0;
    assert!(store.redeem_link(&issue.link_token).await.is_err(), "expired link refused");
    assert!(store.codes.is_empty(), "the expired row is removed with the attempt");
}

/// Each token names its own address, whatever order the store keeps addresses in.
#[tokio::test]
async fn a_link_token_of_another_address_never_matches() {
    let store = InMemoryOtpStore::new();
    let mut issued = Vec::new();
    for n in 0..16 {
        let email = format!("member{n}@example.com");
        let issue = store.create_otp(&email).await.unwrap();
        issued.push((email, issue));
    }
    assert!(store.redeem_link("not-a-link-token").await.is_err(), "unknown token refused");
    for (email, issue) in &issued {
        assert_eq!(&store.redeem_link(&issue.link_token).await.unwrap(), email);
    }
}
