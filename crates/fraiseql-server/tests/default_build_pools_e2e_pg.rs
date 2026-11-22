//! #2098: the surfaces that run on the application database pool, served by the binary's
//! own boot path on a build with the default features.
//!
//! The binary opened its PostgreSQL pool only in builds with the `observers` feature, so
//! the default build refused `[auth.local]`, mounted neither SCIM nor the RBAC API, and
//! built no tenant admin token store. Every suite that covered those surfaces ran with
//! `observers` on, which is why none of them saw it. This suite must run with the default
//! features: its CI line passes no `--features`.
//!
//! Each test boots through `boot::build_postgres_server`, where the binary opens its
//! pools, then serves the result on a local listener.
//!
//! Self-skips when no `DATABASE_URL` is set.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `fraiseql_default_pools_*` databases →
//! run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics/skips are fine
#![allow(clippy::doc_markdown)] // Reason: technical terms (SCIM, RBAC) throughout the docs

mod common;

use common::first_party::FirstPartyAuth;
use fraiseql_core::schema::{
    AuthClientConfig, AuthDatabaseConfig, CompiledSchema, LocalAuthConfig,
};
use fraiseql_server::server_config::{ServerConfig, scim::ScimServerConfig};
use fraiseql_test_support::try_database_url;
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

/// The variable `[auth.database]` names in this suite.
const AUTH_URL_ENV: &str = "FRAISEQL_TEST_DEFAULT_POOLS_AUTH_URL";
const ADMIN_TOKEN: &str = "default-pools-admin-token-at-least-32-chars";
const TENANT_A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const TENANT_B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
/// How this server signs the sessions it mints.
const FIRST_PARTY: FirstPartyAuth = FirstPartyAuth::new("https://api.example.com", "fraiseql");

fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

/// Create `db` afresh and return a connection to it, for the test's own reads and writes.
async fn scratch_pool(admin_url: &str, db: &str) -> PgPool {
    let admin = PgPool::connect(admin_url).await.expect("connect to admin database");
    // Reason: test SQL built from names and values this test defines.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)")))
        .execute(&admin)
        .await
        .expect("drop scratch database");
    // Reason: test SQL built from names and values this test defines.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {db}")))
        .execute(&admin)
        .await
        .expect("create scratch database");
    admin.close().await;
    std::env::set_var(AUTH_URL_ENV, with_database(admin_url, db));
    PgPool::connect(&with_database(admin_url, db))
        .await
        .expect("connect to scratch database")
}

async fn drop_scratch(admin_url: &str, db: &str) {
    let Ok(admin) = PgPool::connect(admin_url).await else {
        return;
    };
    // Reason: test SQL built from names and values this test defines.
    let _ =
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)")))
            .execute(&admin)
            .await;
    admin.close().await;
}

fn database_url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

/// A schema with first-party auth on the scratch database, and `local` if given.
fn schema(local: Option<LocalAuthConfig>) -> CompiledSchema {
    let mut schema: CompiledSchema = serde_json::from_value(json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
    }))
    .expect("compiled schema");
    schema.auth = Some(AuthClientConfig {
        pkce: None,
        social: None,
        local,
        session: Some(FIRST_PARTY.session()),
        database: Some(AuthDatabaseConfig::new(AUTH_URL_ENV)),
    });
    schema
}

#[test]
fn the_document_loads_without_a_database() {
    schema(None);
}

/// A server on the binary's boot path, serving on a local port.
struct Rig {
    base:   String,
    client: reqwest::Client,
    pool:   PgPool,
    stop:   tokio::sync::oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<Result<(), fraiseql_server::ServerError>>,
}

impl Rig {
    async fn boot(url: &str, db: &str, local: Option<LocalAuthConfig>) -> Self {
        FIRST_PARTY.set_env();
        let pool = scratch_pool(url, db).await;
        let config = FIRST_PARTY.apply(ServerConfig {
            cors_enabled: false,
            database_url: with_database(url, db),
            admin_token: Some(ADMIN_TOKEN.to_string()),
            scim: Some(ScimServerConfig {
                enabled: true,
                ..ScimServerConfig::default()
            }),
            ..ServerConfig::default()
        });

        let server =
            Box::pin(fraiseql_server::boot::build_postgres_server(config, schema(local), None))
                .await
                .expect("the binary's boot path must build this server");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let (stop, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            server
                .serve_on_listener(listener, async {
                    let _ = rx.await;
                })
                .await
        });
        let base = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();
        for _ in 0..100 {
            if client.get(format!("{base}/health")).send().await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        Self {
            base,
            client,
            pool,
            stop,
            handle,
        }
    }

    async fn call(
        &self,
        bearer: Option<&str>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = self.client.request(method, format!("{}{path}", self.base));
        if let Some(bearer) = bearer {
            request = request.bearer_auth(bearer);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.expect("request");
        let status = response.status();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    async fn shutdown(self) {
        let _ = self.stop.send(());
        let _ = self.handle.await;
        self.pool.close().await;
    }
}

/// The `[auth.local]` method a default build can run: anonymous sign-up mints a session.
/// Password and one-time-code sign-in send email, which needs the `inbound-email` feature,
/// and the default build refuses them saying so.
///
/// `[auth.local]` runs on the `[auth.database]` pool, not the application pool, so this
/// passed before #2098 too. It pins that the application pool's arrival leaves it working.
#[tokio::test]
async fn a_local_sign_up_mints_a_session_the_default_build_accepts() {
    let Some(url) = database_url_or_skip("a_local_sign_up_mints_a_session") else {
        return;
    };
    let db = "fraiseql_default_pools_local";
    let rig = Rig::boot(
        &url,
        db,
        Some(LocalAuthConfig {
            anonymous: true,
            ..LocalAuthConfig::default()
        }),
    )
    .await;

    let (status, body) = rig.call(None, Method::POST, "/auth/v1/signup", Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "anonymous sign-up must succeed: {body}");
    assert!(body["access_token"].is_string(), "sign-up mints a session: {body}");

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn scim_and_the_rbac_api_are_mounted_on_the_default_build() {
    let Some(url) = database_url_or_skip("scim_and_the_rbac_api_are_mounted") else {
        return;
    };
    let db = "fraiseql_default_pools_scim_rbac";
    let rig = Rig::boot(&url, db, None).await;

    let (status, body) = rig
        .call(
            Some(ADMIN_TOKEN),
            Method::POST,
            "/api/scim/tokens",
            Some(json!({ "idp_name": "acme-okta" })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "the admin mints a SCIM credential: {body}");
    let scim_token = body["token"].as_str().expect("the token is shown once").to_string();
    let (status, body) = rig.call(Some(&scim_token), Method::GET, "/scim/v2/Users", None).await;
    assert_eq!(status, StatusCode::OK, "the SCIM credential reads /scim/v2/Users: {body}");

    let (status, body) = rig.call(Some(ADMIN_TOKEN), Method::GET, "/api/roles", None).await;
    assert_eq!(status, StatusCode::OK, "the RBAC API answers the admin: {body}");

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn a_tenant_admin_token_is_honoured_for_its_own_tenant_only_on_the_default_build() {
    let Some(url) = database_url_or_skip("a_tenant_admin_token_is_honoured") else {
        return;
    };
    let db = "fraiseql_default_pools_tenant_admin";
    let rig = Rig::boot(&url, db, None).await;

    let (status, body) = rig
        .call(
            Some(ADMIN_TOKEN),
            Method::POST,
            "/api/admin-tokens",
            Some(json!({ "tenant_id": TENANT_A })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "the platform mints a tenant admin token: {body}");
    let tenant_a = body["token"].as_str().expect("the token is shown once").to_string();

    let role = |name: &str| json!({ "name": name, "permissions": [] });
    let (status, body) = rig
        .call(Some(&tenant_a), Method::POST, "/api/roles", Some(role("a-editor")))
        .await;
    assert_eq!(status, StatusCode::CREATED, "a tenant admin manages its own tenant: {body}");
    assert_eq!(body["tenant_id"], TENANT_A, "naming no tenant means its own: {body}");

    let mut foreign = role("b-editor");
    foreign["tenant_id"] = json!(TENANT_B);
    let (status, body) = rig.call(Some(&tenant_a), Method::POST, "/api/roles", Some(foreign)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "another tenant's role is refused: {body}");

    let (status, body) = rig
        .call(Some(ADMIN_TOKEN), Method::POST, "/api/roles", Some(role("global")))
        .await;
    assert_eq!(status, StatusCode::CREATED, "the platform token still works: {body}");

    // With the store unreadable, the gate answers that the store is down; it neither
    // accepts the token nor calls it wrong.
    sqlx::raw_sql("ALTER TABLE core.tb_admin_token RENAME TO tb_admin_token_away")
        .execute(&rig.pool)
        .await
        .expect("take the token store away");
    let (status, body) = rig.call(Some(&tenant_a), Method::GET, "/api/roles", None).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "a store lookup error answers 503: {body}"
    );

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

/// With `observers`, the binary opens two pools, never one in both roles: the observer
/// runtime's listener must not take a connection a request needs. Runs on its own CI line
/// with `observers` on; the application pool keeps no connection idle and the observer
/// pool keeps its own minimum, which a single shared pool could not do at once.
#[cfg(feature = "observers")]
#[tokio::test]
async fn the_binary_opens_two_distinct_pools_with_observers() {
    let Some(url) = database_url_or_skip("the_binary_opens_two_distinct_pools") else {
        return;
    };
    let db = "fraiseql_default_pools_two";
    let pool = scratch_pool(&url, db).await;
    let config = ServerConfig {
        cors_enabled: false,
        database_url: with_database(&url, db),
        ..ServerConfig::default()
    };
    let mut no_auth = schema(None);
    no_auth.auth = None;
    let server = Box::pin(fraiseql_server::boot::build_postgres_server(config, no_auth, None))
        .await
        .expect("the binary's boot path must build this server");

    let pools = server.pools();
    let application = pools.application().expect("the binary opens the application pool");
    let observer = pools.observer().expect("an observers build opens the observer pool");
    assert_eq!(application.pool().options().get_min_connections(), 0);
    assert_eq!(
        observer.pool().options().get_min_connections(),
        fraiseql_server::server_config::ObserverPoolConfig::default().min_connections,
    );
    assert!(application.pool().size() == 0, "the application pool opened a connection idle");

    drop(server);
    pool.close().await;
    drop_scratch(&url, db).await;
}
