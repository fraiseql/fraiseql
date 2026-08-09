//! #781 e2e: genuine provider deliveries are accepted by the real inbound route.
//!
//! The verifiers were individually correct for Slack/Discord/SendGrid/Twilio,
//! but the route never threaded the timestamp header or the request URL into
//! them (`Delivery { timestamp: None, url: None }`), so every genuine delivery
//! from those providers answered 401 — and Lemon Squeezy's verifier compared the
//! wrong encoding. Unit fixtures (`fraiseql-webhooks::signature::tests`) pin the
//! verifiers; this suite pins the **route**: each request below is built exactly
//! as the provider would send it, sent to the mounted router over a live
//! database, and must land as `processed`.
//!
//! Self-skips when no `DATABASE_URL` is set; wired into the Dagger
//! `integration: server` suite next to `webhook_replay_header_dedup_pg`.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `webhooks` ledger on setup → run
//! `--test-threads=1`.
#![cfg(feature = "inbound")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::collections::HashMap;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use fraiseql_server::{
    config::WebhookRouteConfig,
    inbound::{WebhookInboundState, webhook_router, webhook_routes_check},
};
use fraiseql_test_support::try_database_url;
use fraiseql_webhooks::PostgresIdempotencyStore;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use sha2::{Digest as _, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt as _;

const SLACK_SECRET_ENV: &str = "FRAISEQL_TEST_SLACK_SIGNING_SECRET";
const TWILIO_SECRET_ENV: &str = "FRAISEQL_TEST_TWILIO_AUTH_TOKEN";
const LEMON_SECRET_ENV: &str = "FRAISEQL_TEST_LEMONSQUEEZY_SECRET";
const SECRET: &str = "whsec_781";
const TWILIO_PUBLIC_URL: &str = "https://hooks.example.com/webhooks/twilio";

fn hmac_sha256(message: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

fn routes() -> HashMap<String, WebhookRouteConfig> {
    let mut routes = HashMap::new();
    routes.insert(
        "slack".to_string(),
        WebhookRouteConfig {
            secret_env: SLACK_SECRET_ENV.to_string(),
            provider:   "slack".to_string(),
            path:       None,
            public_url: None,
        },
    );
    routes.insert(
        "twilio".to_string(),
        WebhookRouteConfig {
            secret_env: TWILIO_SECRET_ENV.to_string(),
            provider:   "twilio".to_string(),
            path:       None,
            public_url: Some(TWILIO_PUBLIC_URL.to_string()),
        },
    );
    routes.insert(
        "lemonsqueezy".to_string(),
        WebhookRouteConfig {
            secret_env: LEMON_SECRET_ENV.to_string(),
            provider:   "lemonsqueezy".to_string(),
            path:       None,
            public_url: None,
        },
    );
    routes
}

fn router(pool: PgPool) -> Router {
    let state = WebhookInboundState::new(pool, &routes(), |_| Some(SECRET.to_string()));
    webhook_router(state)
}

/// Connect, create the ledger + spine, and truncate so each test starts clean.
async fn setup() -> Option<PgPool> {
    let url = try_database_url()?;
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    PostgresIdempotencyStore::new(pool.clone()).init().await.unwrap();
    WebhookInboundState::init_spine(&pool).await.unwrap();
    sqlx::query("TRUNCATE webhooks.tb_inbound_delivery RESTART IDENTITY")
        .execute(&pool)
        .await
        .unwrap();
    Some(pool)
}

async fn send(router: &Router, request: Request<Body>) -> (StatusCode, String) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string()
}

#[tokio::test]
async fn a_genuine_slack_delivery_is_processed() {
    let Some(pool) = setup().await else {
        eprintln!("skipping a_genuine_slack_delivery_is_processed: DATABASE_URL unset");
        return;
    };
    let router = router(pool);

    let body = format!(r#"{{"id":"slack-{}","type":"event_callback"}}"#, now());
    let ts = now();
    let base = format!("v0:{ts}:{body}");
    let signature = format!("v0={}", hex::encode(hmac_sha256(base.as_bytes())));

    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/slack")
        .header("X-Slack-Signature", signature)
        .header("X-Slack-Request-Timestamp", &ts)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a genuine Slack delivery must be accepted — before #781 the route never \
         read X-Slack-Request-Timestamp and every delivery got 401; body: {response}"
    );
    assert!(response.contains("processed"), "expected processed, got: {response}");
}

#[tokio::test]
async fn a_genuine_twilio_delivery_is_processed() {
    let Some(pool) = setup().await else {
        eprintln!("skipping a_genuine_twilio_delivery_is_processed: DATABASE_URL unset");
        return;
    };
    let router = router(pool);

    // Twilio posts JSON here (the route requires a JSON body). For a non-form body its
    // scheme appends `bodySHA256=<hex>` to the request URI and signs the URI *including*
    // that parameter (#1069) — this request is built the way Twilio actually sends one.
    // Before #1069 the test signed `TWILIO_PUBLIC_URL` bare, which is why it passed
    // against a verifier whose MAC covered no body material at all.
    let body = format!(r#"{{"id":"twilio-{}","type":"sms"}}"#, now());
    let body_hash = hex::encode(Sha256::digest(body.as_bytes()));
    let signed_url = format!("{TWILIO_PUBLIC_URL}?bodySHA256={body_hash}");
    let mut mac = Hmac::<Sha1>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(signed_url.as_bytes());
    let signature = BASE64.encode(mac.finalize().into_bytes());

    let request = Request::builder()
        .method("POST")
        .uri(format!("/webhooks/twilio?bodySHA256={body_hash}"))
        .header("X-Twilio-Signature", signature.clone())
        .header("content-type", "application/json")
        .body(Body::from(body.clone()))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a genuine Twilio delivery must be accepted — before #781 the route never \
         passed the configured public_url, so verification always errored; body: {response}"
    );
    assert!(response.contains("processed"), "expected processed, got: {response}");
}

/// The #1069 exploit, at the route: the same captured `X-Twilio-Signature`, a body the
/// attacker wrote. It used to reach `emit_in_tx` and fire `after:ingest`; the route must
/// now answer 401.
#[tokio::test]
async fn a_forged_twilio_body_under_a_captured_signature_is_refused() {
    let Some(pool) = setup().await else {
        eprintln!(
            "skipping a_forged_twilio_body_under_a_captured_signature_is_refused: DATABASE_URL unset"
        );
        return;
    };
    let router = router(pool);

    let genuine = format!(r#"{{"id":"twilio-{}","type":"sms"}}"#, now());
    let body_hash = hex::encode(Sha256::digest(genuine.as_bytes()));
    let signed_url = format!("{TWILIO_PUBLIC_URL}?bodySHA256={body_hash}");
    let mut mac = Hmac::<Sha1>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(signed_url.as_bytes());
    let captured = BASE64.encode(mac.finalize().into_bytes());

    // Same header, same query string, attacker-chosen payload.
    let forged = r#"{"id":"forged-1","type":"anything","amount":999}"#;
    let request = Request::builder()
        .method("POST")
        .uri(format!("/webhooks/twilio?bodySHA256={body_hash}"))
        .header("X-Twilio-Signature", captured)
        .header("content-type", "application/json")
        .body(Body::from(forged))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a body that is not the one the signature covers must be refused; got: {response}"
    );
}

/// A JSON delivery carrying no body hash at all cannot verify against the pre-#1069
/// constant `HMAC(public_url)` — the value that, once captured anywhere, authorised
/// arbitrary bodies forever.
#[tokio::test]
async fn the_legacy_body_free_twilio_signature_is_refused() {
    let Some(pool) = setup().await else {
        eprintln!("skipping the_legacy_body_free_twilio_signature_is_refused: DATABASE_URL unset");
        return;
    };
    let router = router(pool);

    let mut mac = Hmac::<Sha1>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(TWILIO_PUBLIC_URL.as_bytes());
    let legacy = BASE64.encode(mac.finalize().into_bytes());

    let body = format!(r#"{{"id":"twilio-{}","type":"sms"}}"#, now());
    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/twilio")
        .header("X-Twilio-Signature", legacy)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "the body-free signature must no longer authorise anything; got: {response}"
    );
}

#[tokio::test]
async fn a_genuine_lemonsqueezy_delivery_is_processed() {
    let Some(pool) = setup().await else {
        eprintln!("skipping a_genuine_lemonsqueezy_delivery_is_processed: DATABASE_URL unset");
        return;
    };
    let router = router(pool);

    let body = format!(r#"{{"id":"ls-{}","type":"order_created"}}"#, now());
    // hash_hmac('sha256', $payload, $secret) — hex, as Lemon Squeezy sends it.
    let signature = hex::encode(hmac_sha256(body.as_bytes()));

    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/lemonsqueezy")
        .header("X-Signature", signature)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a genuine (hex-signed) Lemon Squeezy delivery must be accepted (#781); body: {response}"
    );
    assert!(response.contains("processed"), "expected processed, got: {response}");
}

#[tokio::test]
async fn a_tampered_slack_delivery_is_rejected() {
    let Some(pool) = setup().await else {
        eprintln!("skipping a_tampered_slack_delivery_is_rejected: DATABASE_URL unset");
        return;
    };
    let router = router(pool);

    let body = format!(r#"{{"id":"slack-tampered-{}","type":"event_callback"}}"#, now());
    let ts = now();
    let base = format!("v0:{ts}:{body}");
    let signature = format!("v0={}", hex::encode(hmac_sha256(base.as_bytes())));

    // Same signature, body altered after signing.
    let tampered = body.replace("event_callback", "event_tampered");
    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/slack")
        .header("X-Slack-Signature", signature)
        .header("X-Slack-Request-Timestamp", &ts)
        .header("content-type", "application/json")
        .body(Body::from(tampered))
        .unwrap();
    let (status, _) = send(&router, request).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED, "a tampered Slack delivery must 401");
}

// ── #787: boot-time route validation ─────────────────────────────────────────

#[test]
fn a_route_with_unset_secret_refuses_to_boot_in_production() {
    let err = webhook_routes_check(&routes(), |_| None, true)
        .expect_err("an unset signing-secret env must refuse to boot in production");
    assert!(err.to_string().contains("secret_env"), "must name the knob; got: {err}");
}

#[test]
fn a_route_with_unset_secret_is_skipped_with_a_warning_in_development() {
    assert!(webhook_routes_check(&routes(), |_| None, false).is_ok());
}

#[test]
fn an_unknown_provider_refuses_to_boot_in_every_environment() {
    let mut routes = routes();
    routes.get_mut("slack").unwrap().provider = "slak".to_string();
    for is_production in [true, false] {
        let err = webhook_routes_check(&routes, |_| Some(SECRET.to_string()), is_production)
            .expect_err("a provider the registry does not know can never verify anything");
        assert!(err.to_string().contains("slak"), "must name the bad value; got: {err}");
    }
}

#[test]
fn a_url_signing_provider_without_public_url_refuses_to_boot() {
    let mut routes = routes();
    routes.get_mut("twilio").unwrap().public_url = None;
    for is_production in [true, false] {
        let err = webhook_routes_check(&routes, |_| Some(SECRET.to_string()), is_production)
            .expect_err("Twilio signs the URL; a route without public_url cannot verify");
        assert!(err.to_string().contains("public_url"), "must name the knob; got: {err}");
    }
}

#[test]
fn a_fully_configured_route_set_boots() {
    assert!(webhook_routes_check(&routes(), |_| Some(SECRET.to_string()), true).is_ok());
}
