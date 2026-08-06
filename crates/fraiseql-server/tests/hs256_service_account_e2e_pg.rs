//! #934: a service account must be able to authenticate under `[auth_hs256]`.
//!
//! The service-account seam (ADR-0018) resolves an `x-api-key` secret **inside the
//! GraphQL handler**. A service-account request carries that header and no
//! `Authorization` bearer. The HS256 middleware is attached as a `route_layer`, so
//! it runs first — and it returned 401 for any request without a valid bearer,
//! before the handler's seam was ever reached. Net: service accounts worked under
//! `[auth]` (OIDC) and were unreachable under `[auth_hs256]`.
//!
//! The fix must not weaken the layer, so this suite pins all four corners at once:
//! the deferral, and the three refusals that must survive it.
//!
//! Driven through `Server::serve_on_listener` over a real socket — the mount the
//! binary actually serves, not a hand-assembled router. A request that gets past
//! the auth layer answers with a GraphQL error, never 401; that status is the whole
//! assertion, so the query body deliberately need not resolve.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** binds an ephemeral port and sets a process-global env var for
//! the secrets → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::try_database_url;
use serde_json::json;

/// Env var naming the service account's secret, and the secret itself.
const SA_SECRET_ENV: &str = "FRAISEQL_TEST_P04_SA_SECRET";
const SA_SECRET: &str = "p04-service-account-secret";
/// Env var naming the HS256 signing key.
const HS256_SECRET_ENV: &str = "FRAISEQL_TEST_P04_HS256_SECRET";
const HS256_SECRET: &str = "p04-hs256-signing-key-at-least-32-bytes";

fn schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "version": "2.0.0",
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

/// Boot the real mount and return its base URL plus a shutdown handle.
async fn serve(url: &str) -> (String, tokio::sync::oneshot::Sender<()>) {
    let config = ServerConfig {
        // #874: production validate() refuses cors_enabled = true with no origins.
        cors_enabled: false,
        database_url: url.to_string(),
        auth_hs256: Some(fraiseql_server::server_config::Hs256Config {
            secret_env: HS256_SECRET_ENV.to_string(),
            issuer:     Some("fraiseql-test".to_string()),
            audience:   Some("fraiseql-test-api".to_string()),
        }),
        ..ServerConfig::default()
    };
    let adapter = Arc::new(PostgresAdapter::new(url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local addr").port();

    let server = Box::pin(Server::new(config, schema(), adapter, None))
        .await
        .expect("Server::new with [auth_hs256] and a declared service account");
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

#[tokio::test]
async fn service_accounts_are_reachable_under_hs256() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #934 HS256 service-account e2e: DATABASE_URL not set");
        return;
    };
    // Fixed valid values, set unconditionally: safe under the runner because this
    // suite is the only reader of these two names, and both are read at boot.
    std::env::set_var(SA_SECRET_ENV, SA_SECRET);
    std::env::set_var(HS256_SECRET_ENV, HS256_SECRET);

    let (base, shutdown) = serve(&url).await;

    // 1. The defect: x-api-key alone, no bearer — the documented service-account request shape.
    //    Must reach the handler's seam and authenticate.
    assert_ne!(
        post_status(&base, &[("x-api-key", SA_SECRET)]).await,
        401,
        "#934: a service account presenting its secret on x-api-key with no bearer must \
         authenticate under [auth_hs256], as it already does under [auth]. The HS256 \
         middleware ran first and 401'd before the handler's ADR-0018 seam was reached"
    );

    // 2. No credentials at all must still be refused — the deferral must not become an anonymous
    //    door into an authenticated transport.
    assert_eq!(
        post_status(&base, &[]).await,
        401,
        "a request with neither a bearer nor a service-account secret must still be refused"
    );

    // 3. An *invalid* bearer must still be refused at the layer. Only bearer-absent defers; a bad
    //    token is a bad token.
    assert_eq!(
        post_status(&base, &[("authorization", "Bearer not-a-valid-jwt")]).await,
        401,
        "an invalid bearer must still 401 — the deferral applies to absence, not to failure"
    );

    // 4. An unmatched secret is not a free pass into the handler.
    assert_eq!(
        post_status(&base, &[("x-api-key", "wrong-secret")]).await,
        401,
        "a service-account secret that matches no account must be refused, not treated as \
         an anonymous request"
    );

    let _ = shutdown.send(());
}
