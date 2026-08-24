//! #1046: inbound-webhook dedup is scoped per *route*, not per provider.
//!
//! Two `[webhooks.*]` routes may legitimately share a `provider` — two partners
//! signing with the generic `hmac-sha256` scheme under their own secrets, a
//! live/test pair, two accounts of one multi-tenant provider. Each sender mints
//! its own event ids, so the same id (`"1001"`) is expected to appear on both.
//!
//! Both dedup layers used to be keyed on the provider string alone:
//!
//! * the delivery ledger on `(provider, event_id)` — `webhooks.tb_inbound_delivery`;
//! * the durable spine on `(source, idempotency_key)`, where `source` is `webhook:<provider>`.
//!
//! so the second partner's genuine, correctly-signed delivery hit the first
//! partner's claim, was answered `200 {"status":"duplicate"}`, was never written to
//! the spine, and never fired `after:ingest`. The 200 tells the sender it
//! succeeded, so it never retries — a silent, permanent loss. It is also a
//! cross-sender suppression channel: a partner holding a valid secret for its own
//! route can pre-claim ids it predicts a co-tenant will use. This is the shape #775
//! fixed for email, which is why both layers are asserted here.
//!
//! Route segments are a sound dedup namespace only because boot now refuses two
//! routes resolving to one segment (#1048).
//!
//! Self-skips when `DATABASE_URL` is unset (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared ledger and spine on setup →
//! `--test-threads=1`.
#![cfg(feature = "inbound")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code.

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

/// The generic HMAC provider both routes use — the realistic instance of the
/// defect, because its senders mint their own sender-local event ids.
const PROVIDER: &str = "hmac-sha256";

const A_SECRET_ENV: &str = "FRAISEQL_TEST_PARTNER_A_WEBHOOK_SECRET";
const A_SECRET: &str = "whsec_partner_a";
const B_SECRET_ENV: &str = "FRAISEQL_TEST_PARTNER_B_WEBHOOK_SECRET";
const B_SECRET: &str = "whsec_partner_b";

/// Partner A's first event. Both partners number their own events from scratch,
/// so both bodies carry `"id":"1001"` — the collision the ledger key must not see.
const A_BODY: &[u8] = br#"{"id":"1001","type":"order.created","partner":"a"}"#;
/// Partner B's own, unrelated event number 1001 — different content, same id.
const B_BODY: &[u8] = br#"{"id":"1001","type":"order.created","partner":"b"}"#;

/// `hex(HMAC-SHA256(secret, body))` — the `hmac-sha256` verifier's `X-Signature` form.
fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// The real inbound router over a live pool, carrying **two** routes that share one
/// provider and differ only in path segment and signing secret.
fn router(pool: PgPool) -> Router {
    let route = |secret_env: &str| WebhookRouteConfig {
        secret_env: secret_env.to_string(),
        provider:   PROVIDER.to_string(),
        path:       None,
        public_url: None,
    };
    let mut routes = HashMap::new();
    routes.insert("partner-a".to_string(), route(A_SECRET_ENV));
    routes.insert("partner-b".to_string(), route(B_SECRET_ENV));

    let state = WebhookInboundState::new(pool, &routes, |name| match name {
        A_SECRET_ENV => Some(A_SECRET.to_string()),
        B_SECRET_ENV => Some(B_SECRET.to_string()),
        _ => None,
    });
    webhook_router(state)
}

/// Connect, create the ledger + spine, and truncate both so each test starts clean.
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
    sqlx::query("TRUNCATE _fraiseql_inbound_message RESTART IDENTITY")
        .execute(&pool)
        .await
        .unwrap();
    Some(pool)
}

/// POST a genuinely signed body to one route, returning the HTTP status and the
/// `status` field of the JSON response.
async fn post(
    router: &Router,
    segment: &str,
    secret: &str,
    body: &'static [u8],
) -> (StatusCode, String) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/webhooks/{segment}"))
        .header("X-Signature", sign(secret, body))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("expected JSON, got: {}", String::from_utf8_lossy(&bytes)));
    let disposition = json
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("expected a status field, got: {json}"))
        .to_string();
    (status, disposition)
}

async fn ledger_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM webhooks.tb_inbound_delivery")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn spine_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _fraiseql_inbound_message")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Which partners' payloads actually reached the spine, in insertion order — the
/// row count alone cannot tell "both partners landed" from "one landed twice".
async fn spine_partners(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT payload -> 'payload' ->> 'partner' FROM _fraiseql_inbound_message \
         ORDER BY pk_inbound_message",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

macro_rules! skip_if_no_db {
    ($name:literal) => {
        match setup().await {
            Some(pool) => pool,
            None => {
                eprintln!("skipping #1046 {}: DATABASE_URL not set", $name);
                return;
            },
        }
    };
}

/// The defect exactly as filed: two routes on one provider, each sender's own
/// event `1001`. Partner B's genuine delivery must not be swallowed by partner A's
/// claim — on either dedup layer.
#[tokio::test]
async fn two_routes_on_one_provider_do_not_share_a_dedup_namespace() {
    let pool = skip_if_no_db!("cross-route dedup scope");
    let router = router(pool.clone());

    let (a_status, a) = post(&router, "partner-a", A_SECRET, A_BODY).await;
    let (b_status, b) = post(&router, "partner-b", B_SECRET, B_BODY).await;

    assert_eq!(a_status, StatusCode::OK, "partner A's delivery is accepted");
    assert_eq!(a, "processed", "partner A's delivery is processed");
    assert_eq!(b_status, StatusCode::OK);
    assert_eq!(
        b, "processed",
        "#1046: partner B's own event 1001 is unrelated to partner A's and must be \
         processed. Answering `duplicate` loses it permanently — the 200 tells B it \
         succeeded, so it never retries",
    );
    assert_eq!(
        ledger_count(&pool).await,
        2,
        "#1046: the delivery ledger must claim both — its key is scoped by route, not \
         by the shared provider string",
    );
    assert_eq!(
        spine_count(&pool).await,
        2,
        "#1046: both messages must reach the durable spine. Scoping only the ledger \
         moves the silent drop here: the claim succeeds, the spine emit conflicts, and \
         the route still answers `processed`",
    );
    let partners = spine_partners(&pool).await;
    assert_eq!(
        partners,
        vec!["a".to_string(), "b".to_string()],
        "#1046: the two spine rows must be the two partners' distinct payloads, not one \
         partner's stored twice; got {partners:?}",
    );
}

/// The scoping must not go the other way: a genuine redelivery on the *same* route
/// still has to collapse, or the replay defence (#751) is gone.
#[tokio::test]
async fn a_redelivery_to_the_same_route_still_deduplicates() {
    let pool = skip_if_no_db!("same-route redelivery");
    let router = router(pool.clone());

    let (_, first) = post(&router, "partner-a", A_SECRET, A_BODY).await;
    let (second_status, second) = post(&router, "partner-a", A_SECRET, A_BODY).await;

    assert_eq!(first, "processed", "the genuine delivery is processed once");
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(
        second, "duplicate",
        "the provider's own retry of an already-processed delivery must still \
         deduplicate — route scoping narrows the namespace, it must not disable it",
    );
    assert_eq!(ledger_count(&pool).await, 1, "exactly one claimed delivery");
    assert_eq!(spine_count(&pool).await, 1, "exactly one spine row");
}
