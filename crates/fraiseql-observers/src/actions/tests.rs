//! Tests for outbound webhook HMAC signing (#345).
//!
//! The acceptance gate is that the project's own verifier
//! (`fraiseql_webhooks::signature::stripe::StripeVerifier`) accepts our
//! signature **over the exact bytes transmitted on the wire** — not a
//! re-serialization. The `execute_*` tests capture the body wiremock actually
//! received and verify against that.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code; panics surface failures.

use std::collections::HashMap;

use fraiseql_webhooks::{SignatureVerifier, signature::stripe::StripeVerifier};
use uuid::Uuid;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

use super::{WEBHOOK_SIGNATURE_HEADER, WebhookAction, webhook_signature};
use crate::{
    event::{EntityEvent, EventKind},
    insecure_guard::ALLOW_INSECURE_ENV,
};

fn test_event() -> EntityEvent {
    EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        serde_json::json!({ "id": "ord_1", "amount": 4200, "nested": { "k": "v" } }),
    )
}

// ── Pure signer: round-trips with the project's own verifier ────────────────

#[test]
fn signature_round_trips_with_stripe_verifier() {
    let secret = "whsec_round_trip";
    let ts = chrono::Utc::now().timestamp();
    let body = br#"{"id":"ord_1","amount":4200}"#;

    let header = webhook_signature(secret, ts, body);
    assert!(header.starts_with("t="), "header carries the timestamp: {header}");
    assert!(header.contains(",v1="), "header carries the v1 hex signature: {header}");

    let verifier = StripeVerifier::new();
    assert!(
        verifier.verify(body, &header, secret, None, None).unwrap(),
        "the signature must verify with StripeVerifier"
    );
}

#[test]
fn tampered_body_fails_verification() {
    let secret = "whsec_tamper";
    let ts = chrono::Utc::now().timestamp();
    let header = webhook_signature(secret, ts, br#"{"amount":1}"#);

    let verifier = StripeVerifier::new();
    assert!(
        !verifier.verify(br#"{"amount":2}"#, &header, secret, None, None).unwrap(),
        "a tampered body must NOT verify"
    );
}

#[test]
fn signature_is_deterministic_for_fixed_inputs() {
    let body = br#"{"a":1}"#;
    assert_eq!(
        webhook_signature("k", 1_700_000_000, body),
        webhook_signature("k", 1_700_000_000, body),
    );
    assert_ne!(
        webhook_signature("k", 1_700_000_000, body),
        webhook_signature("k", 1_700_000_001, body),
        "a different timestamp yields a different signature",
    );
}

// ── Acceptance gate: signs the EXACT transmitted bytes ──────────────────────

#[tokio::test]
async fn execute_signs_the_exact_transmitted_bytes() {
    // wiremock binds to 127.0.0.1, which the SSRF guard blocks; allow the
    // insecure bypass for this loopback test only.
    temp_env::async_with_vars([(ALLOW_INSECURE_ENV, Some("true"))], async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let secret = "whsec_gate";
        WebhookAction::new()
            .execute(&server.uri(), &HashMap::new(), None, Some(secret), &test_event())
            .await
            .expect("webhook dispatch should succeed");

        let requests = server.received_requests().await.expect("recorded requests");
        assert_eq!(requests.len(), 1, "exactly one request should be sent");
        let req = &requests[0];

        let sig = req
            .headers
            .get(WEBHOOK_SIGNATURE_HEADER)
            .expect("signature header must be present")
            .to_str()
            .expect("signature header is valid ASCII");

        // The gate: verify over the bytes the server ACTUALLY received, not a
        // fresh re-serialization of the struct.
        let verifier = StripeVerifier::new();
        assert!(
            verifier.verify(&req.body, sig, secret, None, None).unwrap(),
            "signature must verify over the exact transmitted body bytes"
        );
    })
    .await;
}

#[tokio::test]
async fn execute_without_secret_sends_no_signature_header() {
    temp_env::async_with_vars([(ALLOW_INSECURE_ENV, Some("true"))], async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        WebhookAction::new()
            .execute(&server.uri(), &HashMap::new(), None, None, &test_event())
            .await
            .expect("unsigned webhook dispatch should succeed");

        let requests = server.received_requests().await.expect("recorded requests");
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].headers.get(WEBHOOK_SIGNATURE_HEADER).is_none(),
            "no signature header when signing is not configured"
        );
    })
    .await;
}
