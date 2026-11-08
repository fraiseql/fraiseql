//! `[auth.database]`: first-party auth runs on its own role, through its own pool.
//!
//! - an auth surface without `[auth.database]` refuses to boot, and so does `[auth.database]` with
//!   no auth surface to serve;
//! - the variable `url_env` names must be set and non-empty;
//! - with the engine on a role that cannot read schema `auth`, sign-in still works: every auth
//!   table belongs to the auth role, and the engine role reads none of them;
//! - boot logs name the auth database by its variable, not its value;
//! - a schema `auth` that FraiseQL did not create refuses the boot;
//! - on two roles, each surface reaches its tables through auth's pool: the MFA check at boot, the
//!   session sweep, refresh, SCIM and social sign-in.
//!
//! Self-skips when no `DATABASE_URL` is set. The URL must be able to create roles and
//! databases.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL`, optionally
//! `AUTH_DATABASE_URL` (the auth role's credentials) · **Parallelism:** creates and drops
//! its own `fraiseql_u10_*` databases → run `--test-threads=1`.
#![cfg(feature = "auth")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)]
// Reason: test code — panics and skip diagnostics are acceptable

use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{
    Server,
    server_config::{ServerConfig, hs256::Hs256Config},
};
use fraiseql_test_support::try_database_url;
use sqlx::PgPool;

const HS256_SECRET: &str = "u10-auth-database-hs256-secret-32";
const HS256_SECRET_ENV: &str = "FRAISEQL_TEST_U10_HS256";
const HS256_ISSUER: &str = "https://sp.example.com";
const HS256_AUDIENCE: &str = "fraiseql-u10-test";

/// The engine role every test boots the engine on: it can connect and create in its
/// scratch database, and has no grant on schema `auth`.
const ENGINE_ROLE: &str = "fraiseql_u10_engine";
const ENGINE_PASSWORD: &str = "fraiseql_u10_engine_password";

/// The auth role's defaults when `AUTH_DATABASE_URL` does not supply them.
const AUTH_ROLE: &str = "fraiseql_auth";
const AUTH_PASSWORD: &str = "fraiseql_auth_test_password";

fn set_test_env() {
    std::env::set_var(HS256_SECRET_ENV, HS256_SECRET);
}

fn database_url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

/// `url` with its database replaced by `db`.
fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

/// `url` with its user and password replaced.
fn with_user(url: &str, user: &str, password: &str) -> String {
    let mut out = url::Url::parse(url).expect("database URL parses");
    out.set_username(user).expect("set user");
    out.set_password(Some(password)).expect("set password");
    out.to_string()
}

/// The auth role's name and password: `AUTH_DATABASE_URL`'s when set, else the defaults.
fn auth_credentials() -> (String, String) {
    std::env::var("AUTH_DATABASE_URL").ok().filter(|v| !v.is_empty()).map_or_else(
        || (AUTH_ROLE.to_string(), AUTH_PASSWORD.to_string()),
        |v| {
            let u = url::Url::parse(&v).expect("AUTH_DATABASE_URL parses");
            (u.username().to_string(), u.password().unwrap_or_default().to_string())
        },
    )
}

/// Create `role` with `password` when it does not exist yet.
async fn ensure_role(admin: &PgPool, role: &str, password: &str) {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)")
            .bind(role)
            .fetch_one(admin)
            .await
            .expect("read pg_roles");
    if !exists {
        sqlx::raw_sql(&format!(
            "CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOBYPASSRLS"
        ))
        .execute(admin)
        .await
        .expect("create role");
    }
}

/// A fresh scratch database both roles can connect to and create in.
///
/// Returns the superuser pool on it, the engine role's URL and the auth role's URL.
async fn scratch(admin_url: &str, db: &str) -> (PgPool, String, String) {
    let admin = PgPool::connect(admin_url).await.expect("connect to admin database");
    sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
        .execute(&admin)
        .await
        .expect("drop scratch database");
    sqlx::raw_sql(&format!("CREATE DATABASE {db}"))
        .execute(&admin)
        .await
        .expect("create scratch database");
    let (auth_role, auth_password) = auth_credentials();
    ensure_role(&admin, ENGINE_ROLE, ENGINE_PASSWORD).await;
    ensure_role(&admin, &auth_role, &auth_password).await;
    sqlx::raw_sql(&format!("GRANT CONNECT, CREATE ON DATABASE {db} TO {ENGINE_ROLE}, {auth_role}"))
        .execute(&admin)
        .await
        .expect("grant on scratch database");
    admin.close().await;

    let scratch_url = with_database(admin_url, db);
    let pool = PgPool::connect(&scratch_url).await.expect("connect to scratch database");
    // PostgreSQL 15+ gives PUBLIC no CREATE on `public`; the engine needs it.
    sqlx::raw_sql(&format!("GRANT USAGE, CREATE ON SCHEMA public TO {ENGINE_ROLE}"))
        .execute(&pool)
        .await
        .expect("grant on schema public");
    (
        pool,
        with_user(&scratch_url, ENGINE_ROLE, ENGINE_PASSWORD),
        with_user(&scratch_url, &auth_role, &auth_password),
    )
}

async fn drop_scratch(admin_url: &str, db: &str) {
    let Ok(admin) = PgPool::connect(admin_url).await else {
        return;
    };
    let _ = sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
        .execute(&admin)
        .await;
    admin.close().await;
}

/// A compiled schema with anonymous sign-up on and, when given, `[auth.database]`.
fn schema(database: Option<serde_json::Value>) -> CompiledSchema {
    let mut auth = serde_json::json!({ "local": { "anonymous": true } });
    if let Some(database) = database {
        auth["database"] = database;
    }
    serde_json::from_value(serde_json::json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
        "auth": auth,
    }))
    .expect("compiled schema")
}

/// A compiled schema with no auth surface and, when given, `[auth.database]`.
fn schema_without_surface(database: &serde_json::Value) -> CompiledSchema {
    serde_json::from_value(serde_json::json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
        "auth": { "database": database },
    }))
    .expect("compiled schema")
}

fn config(engine_url: &str) -> ServerConfig {
    ServerConfig {
        cors_enabled: false,
        database_url: engine_url.to_string(),
        auth_hs256: Some(Hs256Config {
            secret_env: HS256_SECRET_ENV.to_string(),
            issuer:     Some(HS256_ISSUER.to_string()),
            audience:   Some(HS256_AUDIENCE.to_string()),
        }),
        ..ServerConfig::default()
    }
}

/// Construct the server on the engine role. The error, if construction refuses.
async fn construct(
    config: ServerConfig,
    schema: CompiledSchema,
    engine_url: &str,
) -> Result<Server, String> {
    let adapter = Arc::new(PostgresAdapter::new(engine_url).await.expect("PostgresAdapter::new"));
    let engine_pool = PgPool::connect(engine_url).await.expect("connect as the engine role");
    Box::pin(Server::new(config, schema, adapter, Some(engine_pool)))
        .await
        .map_err(|e| e.to_string())
}

struct RunningServer {
    base:     String,
    shutdown: tokio::sync::oneshot::Sender<()>,
    handle:   tokio::task::JoinHandle<Result<(), String>>,
}

impl RunningServer {
    async fn stop(self) {
        let _ = self.shutdown.send(());
        let _ = self.handle.await;
    }
}

/// Serve `server`. `Ok` once it answers; the serve error when it stops before that.
async fn serve(server: Server) -> Result<RunningServer, String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let mut handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
            .map_err(|e| e.to_string())
    });
    let base = format!("http://127.0.0.1:{port}");
    let ready = async {
        while reqwest::get(format!("{base}/health")).await.is_err() {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    };
    tokio::select! {
        () = ready => Ok(RunningServer { base, shutdown: tx, handle }),
        stopped = &mut handle => Err(match stopped {
            Ok(Ok(())) => "the server stopped without an error before it answered".to_string(),
            Ok(Err(e)) => e,
            Err(e) => format!("the serve task failed: {e}"),
        }),
        () = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
            panic!("the server neither answered nor stopped within 30s")
        },
    }
}

async fn anonymous_signup(base: &str) -> (reqwest::StatusCode, serde_json::Value) {
    let resp = reqwest::Client::new()
        .post(format!("{base}/auth/v1/signup"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("request");
    let status = resp.status();
    (status, resp.json().await.unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn an_auth_surface_without_auth_database_refuses_to_boot() {
    let Some(url) = database_url_or_skip("an_auth_surface_without_auth_database") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_u10_no_section";
    let (_pool, engine_url, _auth_url) = scratch(&url, db).await;

    let refused = construct(config(&engine_url), schema(None), &engine_url).await;

    let message = refused.err().expect("an auth surface without [auth.database] must refuse");
    assert!(
        message.contains("[auth.database]"),
        "the refusal must name the section: {message}"
    );
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn auth_database_without_an_auth_surface_refuses_to_boot() {
    let Some(url) = database_url_or_skip("auth_database_without_an_auth_surface") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_u10_no_surface";
    let (_pool, engine_url, auth_url) = scratch(&url, db).await;
    std::env::set_var("FRAISEQL_TEST_U10_NO_SURFACE_URL", &auth_url);

    let refused = construct(
        config(&engine_url),
        schema_without_surface(
            &serde_json::json!({ "url_env": "FRAISEQL_TEST_U10_NO_SURFACE_URL" }),
        ),
        &engine_url,
    )
    .await;

    let message = refused.err().expect("[auth.database] with nothing to serve must refuse");
    assert!(
        message.contains("[auth.database]"),
        "the refusal must name the section: {message}"
    );
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn an_unset_auth_database_variable_refuses_to_boot_naming_it() {
    let Some(url) = database_url_or_skip("an_unset_auth_database_variable") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_u10_unset";
    let (_pool, engine_url, _auth_url) = scratch(&url, db).await;
    std::env::remove_var("FRAISEQL_TEST_U10_UNSET_URL");

    let refused = construct(
        config(&engine_url),
        schema(Some(serde_json::json!({ "url_env": "FRAISEQL_TEST_U10_UNSET_URL" }))),
        &engine_url,
    )
    .await;

    let message = refused.err().expect("an unset auth database variable must refuse");
    assert!(
        message.contains("FRAISEQL_TEST_U10_UNSET_URL") && message.contains("not set"),
        "the refusal must name the unset variable: {message}"
    );
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn an_empty_auth_database_variable_refuses_to_boot_naming_it() {
    let Some(url) = database_url_or_skip("an_empty_auth_database_variable") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_u10_empty";
    let (_pool, engine_url, _auth_url) = scratch(&url, db).await;
    std::env::set_var("FRAISEQL_TEST_U10_EMPTY_URL", "");

    let refused = construct(
        config(&engine_url),
        schema(Some(serde_json::json!({ "url_env": "FRAISEQL_TEST_U10_EMPTY_URL" }))),
        &engine_url,
    )
    .await;

    let message = refused.err().expect("an empty auth database variable must refuse");
    assert!(
        message.contains("FRAISEQL_TEST_U10_EMPTY_URL") && message.contains("empty"),
        "the refusal must name the empty variable: {message}"
    );
    drop_scratch(&url, db).await;
}

/// A compiled schema is a document anyone can write by hand, so the boot checks
/// `[auth.database]` again: a URL where the variable's name goes, and a pool with no
/// connection, refuse.
#[tokio::test]
async fn a_malformed_auth_database_refuses_to_boot() {
    let Some(url) = database_url_or_skip("a_malformed_auth_database") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_u10_malformed";
    let (_pool, engine_url, _auth_url) = scratch(&url, db).await;

    let pasted = construct(
        config(&engine_url),
        schema(Some(serde_json::json!({ "url_env": "postgres://auth:hunter2@db/app" }))),
        &engine_url,
    )
    .await;
    let message = pasted.err().expect("a URL where the variable's name goes must refuse");
    assert!(
        message.contains("not an environment variable name") && !message.contains("hunter2"),
        "the refusal must say what is wrong without echoing the value: {message}"
    );

    std::env::set_var("FRAISEQL_TEST_U10_MALFORMED_URL", "postgres://unused@127.0.0.1:1/x");
    let empty_pool = construct(
        config(&engine_url),
        schema(Some(serde_json::json!({
            "url_env": "FRAISEQL_TEST_U10_MALFORMED_URL",
            "pool_max": 0,
        }))),
        &engine_url,
    )
    .await;
    let message = empty_pool.err().expect("pool_max = 0 must refuse");
    assert!(message.contains("pool_max = 0"), "the refusal must name the key: {message}");
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn auth_runs_on_its_own_role_and_the_engine_role_reads_none_of_its_tables() {
    let Some(url) = database_url_or_skip("auth_runs_on_its_own_role") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_u10_two_roles";
    let (pool, engine_url, auth_url) = scratch(&url, db).await;
    std::env::set_var("FRAISEQL_TEST_U10_TWO_ROLES_URL", &auth_url);

    let server = construct(
        config(&engine_url),
        schema(Some(serde_json::json!({ "url_env": "FRAISEQL_TEST_U10_TWO_ROLES_URL" }))),
        &engine_url,
    )
    .await
    .expect("a server with [auth.database] must construct");
    let running = serve(server).await.expect("a server with [auth.database] must serve");

    let (status, body) = anonymous_signup(&running.base).await;
    assert_eq!(status, reqwest::StatusCode::OK, "anonymous sign-up must succeed: {body}");

    // Every auth table belongs to the auth role.
    let (auth_role, _) = auth_credentials();
    let foreign: Vec<(String, String)> = sqlx::query_as(
        "SELECT tablename::text, tableowner::text FROM pg_tables \
         WHERE schemaname = 'auth' AND tableowner <> $1 ORDER BY 1",
    )
    .bind(&auth_role)
    .fetch_all(&pool)
    .await
    .expect("read pg_tables");
    assert!(foreign.is_empty(), "every auth table must belong to {auth_role}: {foreign:?}");

    // The session it minted is in auth's table, and the engine role cannot read it.
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM auth.sessions")
        .fetch_one(&pool)
        .await
        .expect("count sessions");
    assert_eq!(sessions, 1, "the sign-up's session is stored in auth.sessions");
    let engine = PgPool::connect(&engine_url).await.expect("connect as the engine role");
    let denied = sqlx::query("SELECT 1 FROM auth.sessions").fetch_optional(&engine).await;
    assert!(denied.is_err(), "the engine role must not read auth.sessions");
    engine.close().await;

    running.stop().await;
    drop_scratch(&url, db).await;
}

/// Collects everything a `tracing` subscriber writes.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn boot_logs_name_the_auth_database_by_its_variable_not_its_value() {
    let Some(url) = database_url_or_skip("boot_logs_name_the_auth_database") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_u10_logs";
    let (_pool, engine_url, auth_url) = scratch(&url, db).await;
    std::env::set_var("FRAISEQL_TEST_U10_LOGS_URL", &auth_url);

    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let guard = fraiseql_test_support::tracing_capture::set_default(subscriber);

    let server = construct(
        config(&engine_url),
        schema(Some(serde_json::json!({ "url_env": "FRAISEQL_TEST_U10_LOGS_URL" }))),
        &engine_url,
    )
    .await
    .expect("a server with [auth.database] must construct");
    let running = serve(server).await.expect("a server with [auth.database] must serve");
    let (status, body) = anonymous_signup(&running.base).await;
    assert_eq!(status, reqwest::StatusCode::OK, "anonymous sign-up must succeed: {body}");
    running.stop().await;
    drop(guard);

    let logs = String::from_utf8(captured.0.lock().unwrap().clone()).expect("utf-8 logs");
    let (_, auth_password) = auth_credentials();
    assert!(
        logs.contains("FRAISEQL_TEST_U10_LOGS_URL"),
        "boot logs must name the auth database by its variable"
    );
    assert!(!logs.contains(&auth_url), "boot logs must not carry the auth database URL");
    assert!(
        !logs.contains(&auth_password),
        "boot logs must not carry the auth role's password"
    );
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn a_schema_auth_fraiseql_did_not_create_refuses_the_boot() {
    let Some(url) = database_url_or_skip("a_schema_auth_fraiseql_did_not_create") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_u10_collision";
    let (pool, engine_url, auth_url) = scratch(&url, db).await;
    std::env::set_var("FRAISEQL_TEST_U10_COLLISION_URL", &auth_url);
    // The application's own `auth` schema, with a table of its own.
    let (auth_role, _) = auth_credentials();
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA auth AUTHORIZATION {auth_role}; \
         CREATE TABLE auth.tb_user (id integer PRIMARY KEY); \
         ALTER TABLE auth.tb_user OWNER TO {auth_role};"
    ))
    .execute(&pool)
    .await
    .expect("create the application's own auth schema");

    let server = construct(
        config(&engine_url),
        schema(Some(serde_json::json!({ "url_env": "FRAISEQL_TEST_U10_COLLISION_URL" }))),
        &engine_url,
    )
    .await
    .expect("construction runs no DDL, so it does not see the schema yet");
    let refused = serve(server).await;

    let message = match refused {
        Ok(running) => {
            running.stop().await;
            panic!("a schema `auth` FraiseQL did not create must refuse the boot");
        },
        Err(message) => message,
    };
    assert!(
        message.contains("schema \"auth\"") && message.contains("not created by FraiseQL"),
        "the refusal must say whose schema it is: {message}"
    );
    drop_scratch(&url, db).await;
}

// ── One surface each, on two roles ──────────────────────────────────────────
//
// The engine role cannot read schema `auth`, so a surface that reached auth's tables
// through the engine's pool fails each test below.

/// `[auth.database]` naming `var`, set to `auth_url`.
fn auth_database(var: &str, auth_url: &str) -> serde_json::Value {
    std::env::set_var(var, auth_url);
    serde_json::json!({ "url_env": var })
}

/// Rows in `auth.sessions`, read as the superuser.
async fn session_rows(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auth.sessions")
        .fetch_one(pool)
        .await
        .expect("count sessions")
}

/// An account with a confirmed MFA enrollment, written as the auth role, refuses a boot
/// with MFA off: the boot reads the enrollments through auth's pool.
#[tokio::test]
async fn a_confirmed_mfa_enrollment_refuses_a_boot_without_mfa() {
    let Some(url) = database_url_or_skip("a_confirmed_mfa_enrollment") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_u10_mfa";
    let (_pool, engine_url, auth_url) = scratch(&url, db).await;
    let database = auth_database("FRAISEQL_TEST_U10_MFA_URL", &auth_url);

    let server = construct(config(&engine_url), schema(Some(database.clone())), &engine_url)
        .await
        .expect("the first boot has no enrollment to refuse");
    serve(server).await.expect("the first boot must serve").stop().await;

    let auth = PgPool::connect(&auth_url).await.expect("connect as the auth role");
    fraiseql_auth::PgMfaStore::new(auth.clone()).init().await.expect("MFA tables");
    sqlx::query(
        "INSERT INTO auth.tb_mfa_enrollment (user_id, secret_base32, confirmed) \
         VALUES ('user_u10', 'JBSWY3DPEHPK3PXP', true)",
    )
    .execute(&auth)
    .await
    .expect("enroll an account");
    auth.close().await;

    let refused = construct(config(&engine_url), schema(Some(database)), &engine_url).await;

    let message = refused.err().expect("a confirmed enrollment with MFA off must refuse");
    assert!(
        message.contains("confirmed TOTP MFA enrollment"),
        "the refusal must name the enrollment: {message}"
    );
    drop_scratch(&url, db).await;
}

/// The session sweep runs on auth's pool: its first pass, at boot, removes a session
/// past its limit.
#[tokio::test]
async fn the_session_sweep_removes_an_expired_session_at_boot() {
    let Some(url) = database_url_or_skip("the_session_sweep_removes") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_u10_sweep";
    let (pool, engine_url, auth_url) = scratch(&url, db).await;
    let database = auth_database("FRAISEQL_TEST_U10_SWEEP_URL", &auth_url);

    let server = construct(config(&engine_url), schema(Some(database.clone())), &engine_url)
        .await
        .expect("construct");
    let running = serve(server).await.expect("serve");
    let (status, body) = anonymous_signup(&running.base).await;
    assert_eq!(status, reqwest::StatusCode::OK, "anonymous sign-up must succeed: {body}");
    running.stop().await;
    sqlx::query(
        "UPDATE auth.sessions SET expires_at = floor(EXTRACT(EPOCH FROM now()))::bigint - 1",
    )
    .execute(&pool)
    .await
    .expect("move every session's limit into the past");
    assert_eq!(session_rows(&pool).await, 1, "the expired session is there before the sweep");

    let server = construct(config(&engine_url), schema(Some(database)), &engine_url)
        .await
        .expect("construct again");
    let running = serve(server).await.expect("serve again");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while session_rows(&pool).await > 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let left = session_rows(&pool).await;
    running.stop().await;

    assert_eq!(left, 0, "the boot's sweep must remove the expired session");
    drop_scratch(&url, db).await;
}

/// The refresh route runs on auth's pool: a password sign-in's session refreshes.
#[cfg(feature = "inbound-email")]
#[tokio::test]
async fn a_password_session_refreshes() {
    use fraiseql_core::schema::{LocalAuthConfig, SessionConfig};
    use fraiseql_server::inbound::email::{MailboxConfig, MailboxSmtpConfig, SmtpTlsMode};

    const SMTP_PASSWORD_ENV: &str = "FRAISEQL_TEST_U10_SMTP_PASSWORD";

    let Some(url) = database_url_or_skip("a_password_session_refreshes") else {
        return;
    };
    set_test_env();
    std::env::set_var(SMTP_PASSWORD_ENV, "unused");
    let db = "fraiseql_u10_refresh";
    let (_pool, engine_url, auth_url) = scratch(&url, db).await;

    let mut schema =
        schema_without_surface(&auth_database("FRAISEQL_TEST_U10_REFRESH_URL", &auth_url));
    let auth = schema.auth.as_mut().expect("[auth]");
    auth.local = Some(LocalAuthConfig {
        password: true,
        email_from: Some("support".to_string()),
        reset_url_template: Some("https://app.example.com/reset?token={token}".to_string()),
        ..LocalAuthConfig::default()
    });
    auth.session = Some(SessionConfig {
        ttl_secs:          Some(900),
        absolute_ttl_secs: Some(86_400),
    });
    // A mailbox whose SMTP half points at a closed port: the transport builds, a send fails.
    let mut config = config(&engine_url);
    config.mailbox.insert(
        "support".to_string(),
        MailboxConfig {
            imap: None,
            smtp: Some(MailboxSmtpConfig {
                host:         "127.0.0.1".to_string(),
                port:         9,
                address:      "support@example.com".to_string(),
                username:     "support@example.com".to_string(),
                password_env: SMTP_PASSWORD_ENV.to_string(),
                tls:          SmtpTlsMode::None,
                timeout_secs: 2,
                return_path:  None,
            }),
        },
    );

    let server = construct(config, schema, &engine_url).await.expect("construct");
    let running = serve(server).await.expect("serve");
    let client = reqwest::Client::new();
    let signup = client
        .post(format!("{}/auth/v1/password/signup", running.base))
        .json(
            &serde_json::json!({ "email": "u10@example.com", "password": "correct horse battery" }),
        )
        .send()
        .await
        .expect("sign-up request");
    assert_eq!(signup.status(), reqwest::StatusCode::OK, "password sign-up must succeed");
    let body: serde_json::Value = signup.json().await.expect("sign-up body");
    let refresh_token = body["refresh_token"].as_str().expect("refresh_token").to_string();

    let refreshed = client
        .post(format!("{}/auth/v1/session/refresh", running.base))
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await
        .expect("refresh request");
    let status = refreshed.status();
    let body: serde_json::Value = refreshed.json().await.unwrap_or(serde_json::Value::Null);
    running.stop().await;

    assert_eq!(status, reqwest::StatusCode::OK, "the refresh must succeed: {body}");
    assert!(body["refresh_token"].is_string(), "the refresh rotates the pair: {body}");
    drop_scratch(&url, db).await;
}

/// SCIM runs on auth's pool: a provisioning credential is minted, and a user created.
#[tokio::test]
async fn scim_mints_a_credential_and_creates_a_user() {
    const ADMIN_TOKEN: &str = "u10-scim-admin-token-0123456789abcdef";

    let Some(url) = database_url_or_skip("scim_mints_a_credential") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_u10_scim";
    let (pool, engine_url, auth_url) = scratch(&url, db).await;

    let mut config = config(&engine_url);
    config.admin_token = Some(ADMIN_TOKEN.to_string());
    config.scim = Some(fraiseql_server::server_config::ScimServerConfig {
        enabled:  true,
        base_url: "/scim/v2".to_string(),
    });
    let schema = schema_without_surface(&auth_database("FRAISEQL_TEST_U10_SCIM_URL", &auth_url));

    let server = construct(config, schema, &engine_url).await.expect("construct");
    let running = serve(server).await.expect("serve");
    let client = reqwest::Client::new();
    let minted = client
        .post(format!("{}/api/scim/tokens", running.base))
        .bearer_auth(ADMIN_TOKEN)
        .json(&serde_json::json!({ "idp_name": "u10-idp" }))
        .send()
        .await
        .expect("mint request");
    let mint_status = minted.status();
    let minted: serde_json::Value = minted.json().await.unwrap_or(serde_json::Value::Null);
    assert!(
        mint_status.is_success(),
        "minting a credential must succeed: {mint_status} {minted}"
    );
    let token = minted["token"].as_str().expect("the token is shown once").to_string();

    let created = client
        .post(format!("{}/scim/v2/Users", running.base))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "userName": "ada@example.com",
            "active": true,
        }))
        .send()
        .await
        .expect("create request");
    let status = created.status();
    running.stop().await;

    assert_eq!(status, reqwest::StatusCode::CREATED, "creating a user must answer 201");
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM auth.tb_user")
        .fetch_one(&pool)
        .await
        .expect("count users");
    assert_eq!(users, 1, "the user is stored in auth.tb_user");
    drop_scratch(&url, db).await;
}

/// A GitHub-shaped stub: token, user and emails.
async fn github_stub() -> String {
    use axum::{
        Json, Router,
        routing::{get, post},
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind stub");
    let base = format!("http://127.0.0.1:{}", listener.local_addr().expect("addr").port());
    let app = Router::new()
        .route(
            "/login/oauth/access_token",
            post(|| async {
                Json(serde_json::json!({ "access_token": "gh-at", "token_type": "bearer" }))
            }),
        )
        .route(
            "/user",
            get(|| async { Json(serde_json::json!({ "id": 42, "login": "u10", "email": null })) }),
        )
        .route(
            "/user/emails",
            get(|| async {
                Json(serde_json::json!([
                    { "email": "u10@example.com", "primary": true, "verified": true }
                ]))
            }),
        )
        .route("/user/teams", get(|| async { Json(serde_json::json!([])) }));
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service()).await.unwrap();
    });
    base
}

/// Development mode, which the `GitHub` stub's plain `HTTP` needs, until dropped: then unset,
/// so no later test in this binary runs in development mode.
struct InsecureDevelopment;

impl Drop for InsecureDevelopment {
    fn drop(&mut self) {
        std::env::remove_var("FRAISEQL_ENV");
        std::env::remove_var("FRAISEQL_OIDC_ALLOW_INSECURE");
    }
}

/// Social sign-in runs on auth's pool: the callback links the account and mints a session.
#[tokio::test]
async fn a_social_sign_in_completes() {
    use fraiseql_core::schema::{GitHubSocialConfig, SocialAuthConfig};

    const GITHUB_SECRET_ENV: &str = "FRAISEQL_TEST_U10_GITHUB";

    let Some(url) = database_url_or_skip("a_social_sign_in_completes") else {
        return;
    };
    set_test_env();
    std::env::set_var(GITHUB_SECRET_ENV, "github-client-secret");
    std::env::set_var("FRAISEQL_ENV", "development");
    std::env::set_var("FRAISEQL_OIDC_ALLOW_INSECURE", "1");
    let _insecure = InsecureDevelopment;
    let db = "fraiseql_u10_social";
    let (pool, engine_url, auth_url) = scratch(&url, db).await;
    let stub = github_stub().await;

    let mut schema =
        schema_without_surface(&auth_database("FRAISEQL_TEST_U10_SOCIAL_URL", &auth_url));
    schema.auth.as_mut().expect("[auth]").social = Some(SocialAuthConfig {
        redirect_uri_allowlist: Vec::new(),
        google:                 None,
        github:                 Some(GitHubSocialConfig {
            client_id:         "github-client".to_string(),
            client_secret_env: GITHUB_SECRET_ENV.to_string(),
            redirect_uri:      "https://app.example.com/auth/v1/callback".to_string(),
            base_url:          Some(stub.clone()),
            api_base_url:      Some(stub),
        }),
        apple:                  None,
        discord:                None,
        facebook:               None,
    });

    let server = construct(config(&engine_url), schema, &engine_url).await.expect("construct");
    let running = serve(server).await.expect("serve");
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    let authorize = client
        .get(format!(
            "{}/auth/v1/authorize?provider=github&redirect_uri=https://app.example.com/cb",
            running.base
        ))
        .send()
        .await
        .expect("authorize request");
    assert!(
        authorize.status().is_redirection(),
        "authorize must redirect: {}",
        authorize.status()
    );
    let location = authorize
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .expect("location")
        .to_string();
    let state = url::Url::parse(&location)
        .expect("location parses")
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .expect("state");
    let callback = client
        .get(format!("{}/auth/v1/callback?code=any-code&state={state}", running.base))
        .send()
        .await
        .expect("callback request");
    let status = callback.status();
    let body: serde_json::Value = callback.json().await.unwrap_or(serde_json::Value::Null);
    running.stop().await;

    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "the callback must complete the sign-in: {body}"
    );
    let identities: i64 = sqlx::query_scalar("SELECT count(*) FROM auth.tb_auth_identity")
        .fetch_one(&pool)
        .await
        .expect("count identities");
    assert_eq!(identities, 1, "the GitHub identity is stored in auth");
    drop_scratch(&url, db).await;
}

/// The document every test above serves loads without a database.
#[test]
fn the_document_loads_without_a_database() {
    schema(Some(serde_json::json!({ "url_env": "FRAISEQL_TEST_U10_UNUSED_URL" })));
    schema_without_surface(&serde_json::json!({ "url_env": "FRAISEQL_TEST_U10_UNUSED_URL" }));
}
