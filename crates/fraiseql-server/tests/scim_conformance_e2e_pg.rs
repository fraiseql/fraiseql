//! #946: the SCIM surface, driven by a **third-party** conformance client.
//!
//! The issue's verification gate is explicit about why this exists: "the SCIM surface must be
//! exercised against a real provisioning client, not a hand-written request set … a
//! hand-rolled test suite passes on the request shapes we thought of, which is the failure
//! mode this issue is trying to avoid."
//!
//! Okta's validator and the Entra provisioning agent are hosted services needing a public URL
//! and a vendor tenant, so neither can run in CI. `scim2-tester` can: an independent SCIM 2.0
//! client that discovers the server through `/ServiceProviderConfig`, `/ResourceTypes` and
//! `/Schemas`, then exercises what it finds with request shapes nobody in this repository
//! chose. Vendor validation stays a manual pre-release step; this is the gate that runs on
//! every push.
//!
//! Self-skips when `DATABASE_URL` or `FRAISEQL_SCIM_TESTER_PYTHON` is unset, so it is inert
//! outside the Dagger `saml` leg — which installs the client and points that variable at it.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` +
//! `FRAISEQL_SCIM_TESTER_PYTHON` · **Parallelism:** owns its own database → `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::print_stdout)] // Reason: test code
#![allow(clippy::doc_markdown)] // Reason: technical terms (IdP, SCIM) throughout the docs

use std::sync::Arc;

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{
    Server,
    server_config::{ServerConfig, hs256::Hs256Config, scim::ScimServerConfig},
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};
use sqlx::PgPool;

const HS256_SECRET: &str = "p16-scim-conf-hs256-secret-32byt";
const SECRET_ENV: &str = "FRAISEQL_TEST_P16_SCIM_CONF_SECRET";
const ADMIN_TOKEN: &str = "p16-scim-conformance-admin-token";
/// Interpreter with `scim2-tester` installed. The Dagger leg sets it; locally,
/// `uv venv .scim && uv pip install --python .scim/bin/python scim2-tester httpx`.
const TESTER_PYTHON_ENV: &str = "FRAISEQL_SCIM_TESTER_PYTHON";
const DB: &str = "fraiseql_p16_scim_conformance";

#[tokio::test]
async fn third_party_scim_client_finds_the_surface_conformant() {
    let Some(url) = try_database_url() else {
        eprintln!("SKIP scim conformance: DATABASE_URL not set");
        return;
    };
    let Ok(python) = std::env::var(TESTER_PYTHON_ENV) else {
        eprintln!("SKIP scim conformance: {TESTER_PYTHON_ENV} not set (scim2-tester absent)");
        return;
    };

    std::env::set_var(SECRET_ENV, HS256_SECRET);
    let (base, _url) = url.rsplit_once('/').expect("database URL has a path component");
    let scratch_url = format!("{base}/{DB}");

    let admin = PgPool::connect(&url).await.expect("connect to admin database");
    sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {DB} WITH (FORCE)"))
        .execute(&admin)
        .await
        .expect("drop scratch database");
    sqlx::raw_sql(&format!("CREATE DATABASE {DB}"))
        .execute(&admin)
        .await
        .expect("create scratch database");
    admin.close().await;
    let pool = PgPool::connect(&scratch_url).await.expect("connect to scratch database");

    let config = ServerConfig {
        cors_enabled: false,
        admin_token: Some(ADMIN_TOKEN.to_string()),
        database_url: scratch_url.clone(),
        scim: Some(ScimServerConfig {
            enabled:  true,
            base_url: "/scim/v2".to_string(),
        }),
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some("https://api.example.com".to_string()),
            audience:   Some("fraiseql".to_string()),
        }),
        ..ServerConfig::default()
    };

    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let schema: CompiledSchema = serde_json::from_value(json!({
        "version": "2.0.0", "types": [], "queries": [], "mutations": [],
    }))
    .expect("compiled schema");

    let server = Box::pin(Server::new(config, schema, adapter, Some(pool.clone())))
        .await
        .expect("[scim] must boot");
    let (stop, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    let base_url = format!("http://127.0.0.1:{port}/scim/v2");
    let client = reqwest::Client::new();
    let minted: Value = client
        .post(format!("http://127.0.0.1:{port}/api/scim/tokens"))
        .bearer_auth(ADMIN_TOKEN)
        .json(&json!({ "idp_name": "conformance" }))
        .send()
        .await
        .expect("mint provisioning token")
        .json()
        .await
        .expect("mint token json");
    let token = minted["token"].as_str().expect("token").to_string();

    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/scim-conformance.py");
    // Blocking `std::process` on a dedicated thread: tokio's `process` feature is not
    // enabled in this workspace, and the server keeps serving on the runtime meanwhile.
    let output = tokio::task::spawn_blocking({
        let python = python.clone();
        let base_url = base_url.clone();
        let token = token.clone();
        move || {
            std::process::Command::new(&python)
                .arg(script)
                .arg(&base_url)
                .arg(&token)
                .output()
                .expect("run the conformance client")
        }
    })
    .await
    .expect("conformance client task");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("{stdout}");
    if !output.status.success() {
        eprintln!("{stderr}");
    }

    let _ = stop.send(());
    let _ = handle.await;
    let Ok(admin) = PgPool::connect(&url).await else {
        return;
    };
    let _ = sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {DB} WITH (FORCE)"))
        .execute(&admin)
        .await;

    assert!(
        output.status.success(),
        "the third-party SCIM client found the surface non-conformant:\n{stdout}\n{stderr}"
    );
}
