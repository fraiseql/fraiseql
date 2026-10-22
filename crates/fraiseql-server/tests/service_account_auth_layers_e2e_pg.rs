//! A service account is admitted as itself by every auth layer (#934, #1650).
//!
//! A service account presents its secret on `x-api-key` and no `Authorization` bearer
//! (ADR-0018). The auth layer is a `route_layer`, so it runs first:
//!
//! - under `[auth_hs256]` it 401'd that request before anything resolved the secret (#934). The fix
//!   let it through but left the resolution to `/graphql` and `/ws`, so every other transport
//!   behind the layer served the account as **anonymous**;
//! - under `[auth]` (OIDC) with `required = true` it still 401'd it (#1650).
//!
//! The layer now resolves the secret and inserts the principal. This suite pins it on
//! both layers, through `Server::serve_on_listener` over a real socket: the mount the
//! binary actually serves, not a hand-assembled router.
//!
//! - `/graphql` admits the account. The query body deliberately need not resolve: a request that
//!   gets past the layer answers with a GraphQL error, never 401.
//! - `/operations/v1` (async operations) is a second transport behind the same layer, and it
//!   refuses a request with no principal by name. An admitted account gets past that refusal to the
//!   allowlist's 404, which is what shows it arrived as itself.
//! - `/ws` upgrades for the account, where it is mounted (behind OIDC, or open).
//! - No credentials, an invalid bearer, an unmatched secret, and a bearer beside a secret are all
//!   refused at the layer.
//!
//! A third run boots with no JWT authentication at all: the service account is then the only
//! credential, and the layer that resolves it is mounted on its own.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** binds an ephemeral port and sets a process-global env var for
//! the secrets → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema, security::OidcConfig};
use fraiseql_server::{
    Server,
    server_config::{AsyncOperationsConfig, Hs256Config, ServerConfig},
};
use fraiseql_test_support::try_database_url;
use serde_json::json;

/// Env var naming the service account's secret, and the secret itself.
const SA_SECRET_ENV: &str = "FRAISEQL_TEST_P04_SA_SECRET";
const SA_SECRET: &str = "p04-service-account-secret";
/// Env var naming the HS256 signing key.
const HS256_SECRET_ENV: &str = "FRAISEQL_TEST_P04_HS256_SECRET";
const HS256_SECRET: &str = "p04-hs256-signing-key-at-least-32-bytes";
/// Held across `Server::new`, so no two of the suite's servers run startup DDL at once.
static BOOT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The auth layer a run boots with.
#[derive(Clone, Copy, Debug)]
enum Layer {
    Hs256,
    /// `required = true`, with the JWKS URI pinned so boot performs no discovery. The
    /// address is unroutable: a request that reached token validation would fail there.
    Oidc,
    /// No JWT authentication: the service account is the only credential there is.
    None,
}

fn schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
        "security": {
            "service_accounts": {
                "reconciler": {
                    "secret_env": SA_SECRET_ENV,
                    "roles": ["ledger:read"],
                    "scopes": [],
                    "static_enriched": { "user_id": "svc-reconciler" }
                }
            }
        },
    }))
    .expect("compiled schema")
}

/// Boot the real mount under `layer` and return its base URL plus a shutdown handle.
async fn serve(url: &str, layer: Layer) -> (String, tokio::sync::oneshot::Sender<()>) {
    let (auth, auth_hs256) = match layer {
        Layer::Hs256 => (
            None,
            Some(Hs256Config {
                secret_env: HS256_SECRET_ENV.to_string(),
                issuer:     Some("fraiseql-test".to_string()),
                audience:   Some("fraiseql-test-api".to_string()),
            }),
        ),
        Layer::Oidc => (
            Some(OidcConfig {
                issuer: None,
                audience: Some("fraiseql-test-api".to_string()),
                jwks_uri: Some("https://192.0.2.1/.well-known/jwks.json".to_string()),
                required: true,
                ..OidcConfig::default()
            }),
            None,
        ),
        Layer::None => (None, None),
    };
    let config = ServerConfig {
        // #874: production validate() refuses cors_enabled = true with no origins.
        cors_enabled: false,
        database_url: url.to_string(),
        auth,
        auth_hs256,
        async_operations: Some(AsyncOperationsConfig {
            operations: vec!["reconcileLedger".to_string()],
            ..AsyncOperationsConfig::default()
        }),
        // `/ws` is mounted behind OIDC when it requires auth, and open when it does not;
        // under HS256 alone it is not mounted.
        subscriptions_enabled: true,
        subscription_require_auth: Some(matches!(layer, Layer::Oidc)),
        ..ServerConfig::default()
    };
    let adapter = Arc::new(PostgresAdapter::new(url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local addr").port();

    // `[async_operations]` keeps its queue in the database, so it needs the pool.
    let pool = sqlx::PgPool::connect(url).await.expect("PgPool::connect");
    // Its startup DDL is not yet serialised (#1677), so the suite's servers boot one at a time.
    let boot = BOOT.lock().await;
    let server = Box::pin(Server::new(config, schema(), adapter, Some(pool)))
        .await
        .expect("Server::new with an auth layer and a declared service account");
    drop(boot);
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    (format!("http://127.0.0.1:{port}"), tx)
}

/// POST a trivial GraphQL document with the given headers, returning the status.
async fn post_status(base: &str, headers: &[(&str, &str)]) -> u16 {
    let client = reqwest::Client::new();
    let mut req = client
        .post(format!("{base}/graphql"))
        .json(&json!({ "query": "{ __typename }" }));
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    req.send().await.expect("request").status().as_u16()
}

/// POST `body` to `path` with the given headers, returning the status and body.
async fn post(
    base: &str,
    path: &str,
    body: &serde_json::Value,
    headers: &[(&str, &str)],
) -> (u16, String) {
    let mut req = reqwest::Client::new().post(format!("{base}{path}")).json(body);
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let response = req.send().await.expect("request");
    let status = response.status().as_u16();
    (status, response.text().await.expect("body"))
}

/// Submit an async operation the allowlist does not name. A caller with no principal
/// is refused before the allowlist is consulted; an admitted one reaches it.
async fn submit_status(base: &str, headers: &[(&str, &str)]) -> (u16, String) {
    post(
        base,
        "/operations/v1/notAllowlisted",
        &json!({ "query": "{ notAllowlisted }" }),
        headers,
    )
    .await
}

/// The status a `/ws` upgrade with the given headers is answered with: `101` when it
/// is accepted.
async fn upgrade_status(base: &str, headers: &[(&str, &str)]) -> u16 {
    use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

    let mut request =
        format!("{}/ws", base.replacen("http", "ws", 1)).into_client_request().unwrap();
    for (name, value) in headers {
        request.headers_mut().insert(
            tungstenite::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            tungstenite::http::HeaderValue::from_str(value).unwrap(),
        );
    }
    match tokio_tungstenite::connect_async(request).await {
        Ok(_) => 101,
        Err(tungstenite::Error::Http(response)) => response.status().as_u16(),
        Err(error) => panic!("the upgrade failed below HTTP: {error}"),
    }
}

async fn a_service_account_is_admitted_as_itself(layer: Layer) {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #1650 service-account e2e under {layer:?}: DATABASE_URL not set");
        return;
    };
    // Fixed valid values, set unconditionally: safe under the runner because this
    // suite is the only reader of these two names, and both are read at boot.
    std::env::set_var(SA_SECRET_ENV, SA_SECRET);
    std::env::set_var(HS256_SECRET_ENV, HS256_SECRET);

    let (base, shutdown) = serve(&url, layer).await;

    // 1. x-api-key alone, no bearer: the documented service-account request shape.
    assert_ne!(
        post_status(&base, &[("x-api-key", SA_SECRET)]).await,
        401,
        "{layer:?}: a service account presenting its secret on x-api-key with no bearer \
         must be admitted"
    );

    // 2. ...and admitted as itself, on a second transport behind the same layer. Served as
    //    anonymous it is refused for having no principal.
    let (status, body) = submit_status(&base, &[("x-api-key", SA_SECRET)]).await;
    assert_eq!(
        status, 404,
        "{layer:?}: the async-operations transport must see the service account, reaching \
         the allowlist rather than the no-principal refusal: {body}"
    );
    assert!(body.contains("allowlist"), "{layer:?}: {body}");

    // 3. `/ws`, where it is mounted, upgrades for the account and refuses a secret that matches
    //    none, rather than serving it anonymously.
    if !matches!(layer, Layer::Hs256) {
        assert_eq!(
            upgrade_status(&base, &[("x-api-key", SA_SECRET)]).await,
            101,
            "{layer:?}: the /ws upgrade must admit the service account"
        );
        assert_eq!(
            upgrade_status(&base, &[("x-api-key", "wrong-secret")]).await,
            401,
            "{layer:?}: an unmatched secret must not upgrade"
        );
    }

    if matches!(layer, Layer::None) {
        // With no JWT authentication, a request without credentials is anonymous by
        // design, and there is no bearer to be ambiguous with.
        let _ = shutdown.send(());
        return;
    }

    // 4. No credentials at all are still refused: the admission is not an anonymous door.
    assert_eq!(
        post_status(&base, &[]).await,
        401,
        "{layer:?}: a request with neither a bearer nor a service-account secret must be \
         refused"
    );

    // 5. An invalid bearer is still refused at the layer: a bad token is a bad token.
    assert_eq!(
        post_status(&base, &[("authorization", "Bearer not-a-valid-jwt")]).await,
        401,
        "{layer:?}: an invalid bearer must still 401"
    );

    // 6. An unmatched secret is not a free pass.
    assert_eq!(
        post_status(&base, &[("x-api-key", "wrong-secret")]).await,
        401,
        "{layer:?}: a secret that matches no account must be refused, not served as \
         anonymous"
    );

    // 7. A bearer beside a secret is one request with two identities: refused as such.
    let (status, body) = submit_status(
        &base,
        &[
            ("authorization", "Bearer not-a-valid-jwt"),
            ("x-api-key", SA_SECRET),
        ],
    )
    .await;
    assert_eq!(status, 401, "{layer:?}: {body}");
    assert!(body.contains("Ambiguous credentials"), "{layer:?}: {body}");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn a_service_account_is_admitted_as_itself_under_hs256() {
    a_service_account_is_admitted_as_itself(Layer::Hs256).await;
}

#[tokio::test]
async fn a_service_account_is_admitted_as_itself_under_required_oidc() {
    a_service_account_is_admitted_as_itself(Layer::Oidc).await;
}

#[tokio::test]
async fn a_service_account_is_admitted_as_itself_without_jwt_auth() {
    a_service_account_is_admitted_as_itself(Layer::None).await;
}

/// The document this suite serves loads, checked with no database.
///
/// Every other test here reaches the document only after it has found a database, so in
/// a run without one they skip before it is built and a load-time refusal of it reports
/// as a pass (`2b843cd27`: 12 tests red for a session under a green preflight).
/// This one needs nothing but the loader, so that refusal cannot hide.
///
/// This document is deserialised with `from_value` and never meets `finish_load`, so
/// what can refuse it is deserialisation: an unknown key, a missing field, a type.
#[test]
fn the_document_loads_without_a_database() {
    schema();
}
