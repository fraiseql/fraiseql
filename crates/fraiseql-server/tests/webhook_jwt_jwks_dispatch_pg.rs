//! #1322 e2e: what a token-authenticated delivery actually writes.
//!
//! The scheme's own suite (`fraiseql_webhooks::signature::jwt_jwks::tests`) pins
//! what verification *reports*. This pins what the **route** does with that: the
//! durable spine row, the delivery-ledger row, and the fact that neither is
//! derived from the envelope the sender chose.
//!
//! It asserts the rows rather than the response. `{"status":"processed"}` is the
//! same string whichever event was dispatched, so a route that read the outer
//! JSON would answer it too — which is the #751 class, and the reason the
//! envelope here contradicts the token on every field.
//!
//! It is also the only place a `jwt-jwks` route meets a real
//! [`fraiseql_jwks::JwksSource`] over HTTP: the scheme's suite substitutes a local
//! key source, so the boot-time construction of a source from `jwks_uri`, the
//! loopback-`http` acceptance, and the fetch itself are exercised only here.
//!
//! Self-skips when no `DATABASE_URL` is set; belongs next to
//! `webhook_provider_matrix_pg` in the Dagger `integration: server` suite.
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
    inbound::{WebhookInboundState, webhook_router, webhook_routes_check},
};
use fraiseql_test_support::try_database_url;
use fraiseql_webhooks::PostgresIdempotencyStore;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt as _;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// A 2048-bit RSA private key, PKCS#8 PEM. Generated offline for tests only.
const RSA_PRIVATE_PEM: &str = include_str!("../test_data/idp/rs256-private.pem");

/// The matching modulus, base64url (the file ends with a newline; read it trimmed).
const RSA_N: &str = include_str!("../test_data/idp/rs256-modulus.txt");

const PUBLISHED_KID: &str = "hanko-tenant-key";
const JWKS_PATH: &str = "/.well-known/jwks.json";
const ROUTE: &str = "idp";

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

/// A local `IdP` publishing the fixture key over plain HTTP on loopback — which is
/// exactly what the scheme's `jwks_uri` rule permits and nothing else does.
async fn publishing_idp() -> MockServer {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "keys": [{
                "kty": "RSA", "kid": PUBLISHED_KID, "alg": "RS256", "use": "sig",
                "n": RSA_N.trim(), "e": "AQAB",
            }]
        })))
        .mount(&mock)
        .await;
    mock
}

/// Mount a `hanko` route pointed at `jwks_uri`, with **no** `secret_env`.
///
/// Going through `webhook_routes_check` is what makes this an e2e test of the
/// boot path too: it is the only way to a `WebhookRoutes`, it constructs the real
/// `JwksSource` from the URI, and it is where a route carrying a secret it cannot
/// use would have been refused.
fn router(pool: PgPool, jwks_uri: &str) -> Router {
    let mut routes = HashMap::new();
    routes.insert(
        ROUTE.to_string(),
        WebhookRouteConfig {
            provider: "hanko".to_string(),
            jwks_uri: Some(jwks_uri.to_string()),
            audience: Some("my-app".to_string()),
            ..Default::default()
        },
    );
    let built = webhook_routes_check(&routes, |_| None, true).unwrap_or_else(|error| {
        panic!("a hanko route needs no secret_env and must boot in production: {error}")
    });
    webhook_router(WebhookInboundState::new(pool, &built, |_| None))
}

fn now() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

/// Sign as the tenant's key would.
fn sign(claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(PUBLISHED_KID.to_string());
    let key = EncodingKey::from_rsa_pem(RSA_PRIVATE_PEM.as_bytes()).unwrap();
    jsonwebtoken::encode(&header, claims, &key).unwrap()
}

async fn send(router: &Router, body: String) -> (StatusCode, String) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/webhooks/{ROUTE}"))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// The one spine row for this delivery: its subject and its payload.
async fn spine_row(pool: &PgPool, idempotency_key: &str) -> Option<(Option<String>, Value)> {
    let row: Option<(Value,)> = sqlx::query_as(
        "SELECT payload FROM _fraiseql_inbound_message \
         WHERE source = 'webhook:hanko' AND idempotency_key = $1",
    )
    .bind(idempotency_key)
    .fetch_optional(pool)
    .await
    .unwrap();
    let (message,) = row?;
    let subject = message.get("subject").and_then(Value::as_str).map(str::to_string);
    let payload = message.get("payload").cloned().unwrap_or(Value::Null);
    Some((subject, payload))
}

/// The delivery-ledger row's `(event_id, event_type)`.
async fn ledger_row(pool: &PgPool) -> Option<(String, String)> {
    sqlx::query_as("SELECT event_id, event_type FROM webhooks.tb_inbound_delivery WHERE route = $1")
        .bind(ROUTE)
        .fetch_optional(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_hanko_delivery_dispatches_the_signed_event_and_not_the_envelope() {
    let Some(pool) = setup().await else {
        eprintln!(
            "skipping a_hanko_delivery_dispatches_the_signed_event_and_not_the_envelope: DATABASE_URL unset"
        );
        return;
    };
    let idp = publishing_idp().await;
    let router = router(pool.clone(), &format!("{}{JWKS_PATH}", idp.uri()));

    let issued = now();
    let signed_payload = json!({ "id": format!("user-{issued}"), "email": "signed@example.com" });
    let token = sign(&json!({
        "sub":  "hanko webhooks",
        "aud":  ["my-app"],
        "iat":  issued,
        "exp":  issued + 300,
        "evt":  "user.create",
        "data": signed_payload,
    }));
    // Every outer field contradicts the token. A route that read the envelope
    // would dispatch `user.delete` against the forged payload and still answer
    // `{"status":"processed"}` — which is why this asserts rows.
    let body = json!({
        "token": &token,
        "event": "user.delete",
        "evt":   "user.delete",
        "data":  { "id": "user-in-the-envelope", "email": "forged@example.com" },
        "id":    "id-in-the-envelope",
    })
    .to_string();

    let (status, response) = send(&router, body).await;
    assert_eq!(status, StatusCode::OK, "a genuine Hanko delivery must be accepted: {response}");
    assert!(response.contains("processed"), "expected processed, got: {response}");

    // The id is a digest of the verified token, because Hanko's tokens carry no
    // `jti` and no event id — the only signed, delivery-stable material there is.
    let signed_id = hex::encode(Sha256::digest(token.as_bytes()));
    let (event_id, event_type) =
        ledger_row(&pool).await.expect("the delivery ledger committed a row");
    assert_eq!(
        event_id, signed_id,
        "the ledger must key on the signed token's digest. Keying on anything the envelope \
         carries would let one captured delivery be replayed indefinitely under a fresh \
         outer field (#751)"
    );
    assert_eq!(event_type, "user.create", "and record the signed type");

    // #1046: the spine's key is `<route length>:<route>:<event id>`.
    let key = format!("{}:{ROUTE}:{signed_id}", ROUTE.len());
    let (subject, payload) = spine_row(&pool, &key).await.expect("the spine committed a row");
    assert_eq!(
        subject.as_deref(),
        Some("user.create"),
        "the durable row's subject is the signed `evt`, not the outer `event` — the envelope \
         said user.delete"
    );
    assert_eq!(
        payload, signed_payload,
        "and its payload is the signed `data`, not the outer object the sender chose"
    );
}

#[tokio::test]
async fn a_delivery_whose_token_is_signed_by_an_unpublished_key_writes_nothing() {
    let Some(pool) = setup().await else {
        eprintln!(
            "skipping a_delivery_whose_token_is_signed_by_an_unpublished_key_writes_nothing: DATABASE_URL unset"
        );
        return;
    };
    let idp = publishing_idp().await;
    let router = router(pool.clone(), &format!("{}{JWKS_PATH}", idp.uri()));

    let issued = now();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("a-kid-this-tenant-does-not-publish".to_string());
    let key = EncodingKey::from_rsa_pem(RSA_PRIVATE_PEM.as_bytes()).unwrap();
    let token = jsonwebtoken::encode(
        &header,
        &json!({
            "sub": "hanko webhooks", "aud": ["my-app"], "iat": issued, "exp": issued + 300,
            "evt": "user.create", "data": {},
        }),
        &key,
    )
    .unwrap();
    let body = json!({ "token": token }).to_string();

    let (status, response) = send(&router, body).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a signature the publisher cannot be held to is the sender's problem: {response}"
    );
    assert!(
        ledger_row(&pool).await.is_none(),
        "and no database work may happen at all — a forged delivery must be refused before \
         a connection is taken, which is why verification precedes the transaction"
    );
}
