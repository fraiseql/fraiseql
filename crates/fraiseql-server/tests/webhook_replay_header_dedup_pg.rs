//! #751 regression: an inbound webhook replay cannot mint a fresh idempotency key
//! by varying an unsigned request header.
//!
//! Every supported provider signs the **body** only — `GitHub`'s `X-Hub-Signature-256`
//! is `HMAC-SHA256(secret, body)` and covers no header. While the dedup key was read
//! from `webhook-id` / `x-github-delivery`, anyone holding one captured signed
//! delivery could POST it repeatedly under a fresh delivery header: signature
//! verification passed each time, each replay claimed a new
//! `webhooks.tb_inbound_delivery` row, and `after:ingest` re-fired per replay.
//!
//! The unit tests in `src/inbound/webhook/tests.rs` pin the key-derivation function;
//! only this test proves the property that actually matters — that the *handler*,
//! against a real database and the real idempotency claim, collapses the replay onto
//! one delivery. That is the seam the audit found unguarded.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
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
use fraiseql_server::{
    config::WebhookRouteConfig,
    inbound::{WebhookInboundState, webhook_router},
};
use fraiseql_test_support::try_database_url;
use fraiseql_webhooks::PostgresIdempotencyStore;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt as _;

const SECRET_ENV: &str = "FRAISEQL_TEST_GITHUB_WEBHOOK_SECRET";
const SECRET: &str = "whsec_751";

/// The body of one captured, genuinely signed `GitHub` delivery. It deliberately has
/// no top-level `id`: `GitHub` bodies carry none, so this exercises the body-hash
/// branch — the branch the delivery header used to shadow.
const CAPTURED_BODY: &[u8] =
    br#"{"action":"opened","number":1,"repository":{"full_name":"acme/widgets"}}"#;

/// `HMAC-SHA256(secret, body)` in `GitHub`'s `sha256=<hex>` form — the one signature
/// the attacker captured along with the body.
fn github_signature(body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// Build the real inbound router over a live pool, with one configured `GitHub` route.
fn router(pool: PgPool) -> Router {
    let mut routes = HashMap::new();
    routes.insert(
        "github".to_string(),
        WebhookRouteConfig {
            secret_env: SECRET_ENV.to_string(),
            provider:   "github".to_string(),
            path:       None,
            public_url: None,
        },
    );
    let state = WebhookInboundState::new(pool, &routes, |name| {
        (name == SECRET_ENV).then(|| SECRET.to_string())
    });
    webhook_router(state)
}

/// Connect, create the ledger + spine, and truncate so each test starts clean.
/// Returns `None` (skip) when `DATABASE_URL` is unset.
async fn setup() -> Option<PgPool> {
    let url = try_database_url()?;
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    PostgresIdempotencyStore::new(pool.clone()).init().await.unwrap();
    WebhookInboundState::init_spine(&pool).await.unwrap();
    sqlx::query("TRUNCATE webhooks.tb_inbound_delivery RESTART IDENTITY")
        .execute(&pool)
        .await
        .unwrap();
    // Both layers, because every test here posts the SAME `CAPTURED_BODY` and the two
    // dedup keys derive from it. Truncating only the ledger left the previous test's
    // committed spine row owning this body, so the next test's genuine first delivery
    // was refused by the spine. That was invisible until #1176 stopped reporting a
    // spine refusal as `processed`; the sibling suite
    // (`webhook_route_dedup_scope_pg.rs`) has always truncated both.
    sqlx::query("TRUNCATE _fraiseql_inbound_message RESTART IDENTITY")
        .execute(&pool)
        .await
        .unwrap();
    Some(pool)
}

/// POST the captured body under an attacker-chosen `X-GitHub-Delivery`, returning
/// the HTTP status and the JSON status string.
async fn post_replay(router: &Router, delivery_id: &str) -> (StatusCode, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/github")
        .header("X-Hub-Signature-256", github_signature(CAPTURED_BODY))
        .header("X-GitHub-Delivery", delivery_id)
        .header("X-GitHub-Event", "pull_request")
        .header("content-type", "application/json")
        .body(Body::from(CAPTURED_BODY))
        .unwrap();

    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let disposition = json
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("expected a status field, got: {json}"))
        .to_string();
    (status, disposition)
}

async fn delivery_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM webhooks.tb_inbound_delivery")
        .fetch_one(pool)
        .await
        .unwrap()
}

macro_rules! skip_if_no_db {
    () => {
        match setup().await {
            Some(pool) => pool,
            None => {
                eprintln!("skipping #751 webhook replay test: DATABASE_URL not set");
                return;
            },
        }
    };
}

/// The exact attack from #751: one captured signed body, replayed under a second,
/// attacker-chosen `X-GitHub-Delivery`. Before the fix both replays were
/// `processed`; now the second deduplicates against the first.
#[tokio::test]
async fn replay_under_a_fresh_delivery_header_is_a_duplicate() {
    let pool = skip_if_no_db!();
    let router = router(pool.clone());

    let (first_status, first) = post_replay(&router, "11111111-1111-1111-1111-111111111111").await;
    let (second_status, second) =
        post_replay(&router, "22222222-2222-2222-2222-222222222222").await;

    assert_eq!(first_status, StatusCode::OK, "the genuine delivery is accepted");
    assert_eq!(first, "processed", "the genuine delivery is processed once");
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(
        second, "duplicate",
        "#751: a replay of the same signed body must deduplicate even though the sender \
         chose a fresh X-GitHub-Delivery — the signature covers the body only, so the header \
         must not key the replay defence",
    );
    assert_eq!(
        delivery_count(&pool).await,
        1,
        "#751: the replay must claim no second idempotency row (a second row is what let \
         after:ingest re-fire per replay)",
    );
}

/// The same body with no delivery header at all must land on the same key as the
/// header-bearing replays — proving the header is not an input on any path, rather
/// than merely being overridden by something else.
#[tokio::test]
async fn a_replay_with_no_delivery_header_hits_the_same_key() {
    let pool = skip_if_no_db!();
    let router = router(pool.clone());

    let (_, first) = post_replay(&router, "33333333-3333-3333-3333-333333333333").await;

    let bare = Request::builder()
        .method("POST")
        .uri("/webhooks/github")
        .header("X-Hub-Signature-256", github_signature(CAPTURED_BODY))
        .header("X-GitHub-Event", "pull_request")
        .header("content-type", "application/json")
        .body(Body::from(CAPTURED_BODY))
        .unwrap();
    let response = router.clone().oneshot(bare).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(first, "processed");
    assert_eq!(
        json.get("status").and_then(serde_json::Value::as_str),
        Some("duplicate"),
        "the key is a pure function of the signed body, so presence or absence of the \
         delivery header cannot change it",
    );
    assert_eq!(delivery_count(&pool).await, 1, "still exactly one claimed delivery");
}

/// A genuinely different delivery still gets through — the fix must not collapse
/// distinct events onto one key (which would silently drop real webhooks).
#[tokio::test]
async fn distinct_bodies_are_still_processed_independently() {
    let pool = skip_if_no_db!();
    let router = router(pool.clone());

    let (_, first) = post_replay(&router, "44444444-4444-4444-4444-444444444444").await;

    let other_body = br#"{"action":"closed","number":2,"repository":{"full_name":"acme/widgets"}}"#;
    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/github")
        .header("X-Hub-Signature-256", github_signature(other_body))
        .header("X-GitHub-Delivery", "44444444-4444-4444-4444-444444444444")
        .header("X-GitHub-Event", "pull_request")
        .header("content-type", "application/json")
        .body(Body::from(other_body.to_vec()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(first, "processed");
    assert_eq!(
        json.get("status").and_then(serde_json::Value::as_str),
        Some("processed"),
        "a different signed body is a different event and must still be processed — even \
         when it reuses the previous delivery header",
    );
    assert_eq!(delivery_count(&pool).await, 2, "two genuinely distinct deliveries were claimed");
}
