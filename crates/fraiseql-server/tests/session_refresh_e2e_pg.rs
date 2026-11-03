//! #1853: `[auth.session]` through the shipped server's mount.
//!
//! Sessions minted by local sign-in used to last one hour with no way to renew them:
//! the refresh token expired with the access token, and nothing served a refresh or a
//! logout. These tests boot the real server and drive the session lifetime end to end:
//!
//! - with no `[auth.session]`, nothing changes: no new route, one-hour tokens
//! - `ttl_secs` sets the access-token lifetime of password, `OTP` and `MFA` sign-in
//! - refresh rotates the pair; a rotated token reused within 10 s answers 409 and ends nothing, and
//!   reused later ends the whole session
//! - refresh stops at `absolute_ttl_secs`, re-reads the account, and skips the `MFA` challenge the
//!   session already passed
//! - logout ends the session; the access token it already issued lives on unless
//!   `[security.token_revocation]` is configured
//!
//! Time is never faked. The 10-second boundary is pinned by moving a row's server-side
//! `rotated_at` back, and the absolute limit by moving its `expires_at`. One test sleeps
//! past a real expiry to pin the validator's 60-second leeway.
//!
//! Self-skips when no `DATABASE_URL` is set.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `fraiseql_session_*` databases →
//! run `--test-threads=1`.
#![cfg(all(feature = "auth", feature = "inbound-email"))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)]
// Reason: test code — panics and skip diagnostics are acceptable
// Reason: `{token}` in the URL templates is FraiseQL's own placeholder, which the server
// substitutes — not a Rust format argument.
#![allow(clippy::literal_string_with_formatting_args)]

use std::{fmt::Write as _, sync::Arc};

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    schema::{AuthClientConfig, CompiledSchema, LocalAuthConfig, SessionConfig},
};
use fraiseql_server::{
    Server,
    server_config::{ServerConfig, hs256::Hs256Config},
    token_revocation::{
        InMemoryRevocationStore, RevocationError, RevocationStore, TokenRevocationManager,
    },
};
use fraiseql_test_support::try_database_url;
use reqwest::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

const HS256_SECRET: &str = "session-refresh-hs256-secret-32b!";
const HS256_SECRET_ENV: &str = "FRAISEQL_TEST_SESSION_REFRESH_HS256";
const HS256_ISSUER: &str = "https://sp.example.com";
const HS256_AUDIENCE: &str = "fraiseql-session-test";
const SMTP_PASSWORD_ENV: &str = "FRAISEQL_TEST_SESSION_REFRESH_SMTP_PASSWORD";

/// The access-token lifetime most tests configure.
const TTL: u64 = 900;
/// The refresh limit most tests configure.
const ABSOLUTE: u64 = 86_400;

const REFRESH: &str = "/auth/v1/session/refresh";
const LOGOUT: &str = "/auth/v1/session/logout";
const PASSWORD: &str = "correct horse battery";

fn set_test_env() {
    std::env::set_var(HS256_SECRET_ENV, HS256_SECRET);
    std::env::set_var(SMTP_PASSWORD_ENV, "unused");
}

fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

async fn scratch_pool(admin_url: &str, db: &str) -> PgPool {
    let admin = PgPool::connect(admin_url).await.expect("connect to admin database");
    sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
        .execute(&admin)
        .await
        .expect("drop scratch database");
    sqlx::raw_sql(&format!("CREATE DATABASE {db}"))
        .execute(&admin)
        .await
        .expect("create scratch database");
    admin.close().await;
    PgPool::connect(&with_database(admin_url, db))
        .await
        .expect("connect to scratch database")
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

fn database_url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

fn empty_schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
    }))
    .expect("compiled schema")
}

/// Password, `OTP` and `MFA` sign-in, under `session`.
fn schema_with(session: Option<SessionConfig>) -> CompiledSchema {
    let mut schema = empty_schema();
    schema.auth = Some(AuthClientConfig {
        pkce: None,
        social: None,
        local: Some(LocalAuthConfig {
            password: true,
            otp: true,
            mfa: true,
            email_from: Some("support".to_string()),
            reset_url_template: Some("https://app.example.com/reset?token={token}".to_string()),
            ..LocalAuthConfig::default()
        }),
        session,
    });
    schema
}

fn refreshable() -> CompiledSchema {
    schema_with(Some(SessionConfig {
        ttl_secs:          Some(TTL),
        absolute_ttl_secs: Some(ABSOLUTE),
    }))
}

/// A `[mailbox.support]` whose `SMTP` half points at a closed port: the transport
/// builds, a send fails, and the tests read `OTP` codes from the store.
fn unreachable_mailbox()
-> std::collections::HashMap<String, fraiseql_server::inbound::email::MailboxConfig> {
    let mut mailbox = std::collections::HashMap::new();
    mailbox.insert(
        "support".to_string(),
        fraiseql_server::inbound::email::MailboxConfig {
            imap: None,
            smtp: Some(fraiseql_server::inbound::email::MailboxSmtpConfig {
                host:         "127.0.0.1".to_string(),
                port:         9, // discard: nothing listens
                address:      "support@example.com".to_string(),
                username:     "support@example.com".to_string(),
                password_env: SMTP_PASSWORD_ENV.to_string(),
                tls:          fraiseql_server::inbound::email::SmtpTlsMode::None,
                timeout_secs: 2,
                return_path:  None,
            }),
        },
    );
    mailbox
}

fn server_config(scratch_url: &str) -> ServerConfig {
    ServerConfig {
        cors_enabled: false,
        database_url: scratch_url.to_string(),
        auth_hs256: Some(Hs256Config {
            secret_env: HS256_SECRET_ENV.to_string(),
            issuer:     Some(HS256_ISSUER.to_string()),
            audience:   Some(HS256_AUDIENCE.to_string()),
        }),
        mailbox: unreachable_mailbox(),
        ..ServerConfig::default()
    }
}

struct RunningServer {
    base:     String,
    shutdown: tokio::sync::oneshot::Sender<()>,
    handle:   tokio::task::JoinHandle<()>,
}

impl RunningServer {
    async fn stop(self) {
        let _ = self.shutdown.send(());
        let _ = self.handle.await;
    }
}

async fn wait_until_serving(base: &str) {
    let ready = async {
        while reqwest::get(format!("{base}/health")).await.is_err() {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(30), ready)
        .await
        .expect("the server answers within 30s");
}

async fn boot(
    schema: CompiledSchema,
    pool: PgPool,
    scratch_url: &str,
    revocation: Option<Arc<TokenRevocationManager>>,
) -> RunningServer {
    let adapter = Arc::new(PostgresAdapter::new(scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let mut server = Box::pin(Server::new(server_config(scratch_url), schema, adapter, Some(pool)))
        .await
        .expect("Server::new with [auth.local] + [auth.session] must succeed");
    if let Some(revocation) = revocation {
        server = server.with_revocation_manager(revocation);
    }
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _ = server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await;
    });
    let base = format!("http://127.0.0.1:{port}");
    wait_until_serving(&base).await;
    RunningServer {
        base,
        shutdown: tx,
        handle,
    }
}

/// A scratch database and a server over it.
async fn start(
    test: &str,
    db: &str,
    schema: CompiledSchema,
    revocation: Option<Arc<TokenRevocationManager>>,
) -> Option<(String, PgPool, RunningServer)> {
    let url = database_url_or_skip(test)?;
    set_test_env();
    let pool = scratch_pool(&url, db).await;
    let server = boot(schema, pool.clone(), &with_database(&url, db), revocation).await;
    Some((url, pool, server))
}

async fn finish(url: &str, db: &str, server: RunningServer) {
    server.stop().await;
    drop_scratch(url, db).await;
}

async fn post_json(base: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let resp = reqwest::Client::new()
        .post(format!("{base}{path}"))
        .json(&body)
        .send()
        .await
        .expect("request");
    let status = resp.status();
    let json = resp.json().await.unwrap_or(Value::Null);
    (status, json)
}

async fn post_from(from: std::net::IpAddr, base: &str, path: &str, body: Value) -> StatusCode {
    reqwest::Client::builder()
        .local_address(from)
        .build()
        .expect("client bound to a loopback address")
        .post(format!("{base}{path}"))
        .json(&body)
        .send()
        .await
        .expect("request")
        .status()
}

/// What the server answers a holder of `access`: a `GraphQL` request is refused with
/// 401 when the token is not accepted.
async fn probe(base: &str, access: &str) -> StatusCode {
    reqwest::Client::new()
        .post(format!("{base}/graphql"))
        .bearer_auth(access)
        .json(&json!({ "query": "{ __typename }" }))
        .send()
        .await
        .expect("request")
        .status()
}

/// The claims of a token this server minted, checked against `[auth_hs256]`.
fn claims(token: &str) -> Value {
    use base64::Engine as _;
    let payload = token.split('.').nth(1).expect("JWT has a payload");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("payload decodes");
    let claims: Value = serde_json::from_slice(&bytes).expect("claims parse");
    assert_eq!(claims["iss"].as_str(), Some(HS256_ISSUER), "{claims}");
    claims
}

fn assert_lifetime(what: &str, token: &str, ttl: u64) {
    let claims = claims(token);
    let iat = claims["iat"].as_u64().expect("iat");
    let exp = claims["exp"].as_u64().expect("exp");
    assert_eq!(exp - iat, ttl, "{what}: the access token must live {ttl}s: {claims}");
}

struct Session {
    access:  String,
    refresh: String,
}

fn session_of(what: &str, status: StatusCode, body: &Value) -> Session {
    assert_eq!(status, StatusCode::OK, "{what}: {body}");
    Session {
        access:  body["access_token"].as_str().expect("access_token").to_string(),
        refresh: body["refresh_token"].as_str().expect("refresh_token").to_string(),
    }
}

async fn signup(base: &str, email: &str) -> Session {
    let (status, body) = post_json(
        base,
        "/auth/v1/password/signup",
        json!({ "email": email, "password": PASSWORD }),
    )
    .await;
    session_of("signup", status, &body)
}

async fn refresh(base: &str, token: &str) -> (StatusCode, Value) {
    post_json(base, REFRESH, json!({ "refresh_token": token })).await
}

fn assert_no_tokens(what: &str, body: &Value) {
    assert!(
        body.get("access_token").is_none() && body.get("refresh_token").is_none(),
        "{what} must hand out no tokens: {body}"
    );
}

/// The stored hash of a refresh token, as the session store keys it.
fn token_hash(token: &str) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(token.as_bytes()).iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

/// Move the server-side moment `token` was rotated to `secs_ago` seconds before now.
///
/// The server reads whole seconds from the clock this process shares, so the moment is
/// stamped from that clock, not Postgres's (whose `::bigint` rounds), and just after a
/// second begins: the request that follows lands in the same second, and the 10-second
/// boundary cannot tick over between the stamp and the server's reading.
async fn rotated_secs_ago(pool: &PgPool, token: &str, secs_ago: i64) {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock");
    let into_second = std::time::Duration::from_nanos(u64::from(since_epoch.subsec_nanos()));
    tokio::time::sleep(std::time::Duration::from_secs(1).saturating_sub(into_second)).await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let moved = sqlx::query(
        "UPDATE _system.sessions SET rotated_at = $2 \
         WHERE refresh_token_hash = $1 AND rotated_at IS NOT NULL",
    )
    .bind(token_hash(token))
    .bind(i64::try_from(now).expect("epoch fits") - secs_ago)
    .execute(pool)
    .await
    .expect("move rotated_at")
    .rows_affected();
    assert_eq!(moved, 1, "the token must have been rotated by the server");
}

fn user_of(access: &str) -> String {
    claims(access)["sub"].as_str().expect("sub").to_string()
}

#[test]
fn the_document_loads_without_a_database() {
    let _ = refreshable();
    let _ = schema_with(None);
}

// ── Opt-in ───────────────────────────────────────────────────────────────────

/// With no `[auth.session]`, the session routes are absent and sign-in still mints an
/// hour, as before #1853.
#[tokio::test]
async fn without_a_session_section_nothing_changes() {
    let db = "fraiseql_session_unset";
    let Some((url, _pool, server)) =
        start("without_a_session_section", db, schema_with(None), None).await
    else {
        return;
    };
    let base = &server.base;

    let session = signup(base, "unset@example.com").await;
    assert_lifetime("password signup", &session.access, 3_600);
    for path in [REFRESH, LOGOUT] {
        let (status, body) =
            post_json(base, path, json!({ "refresh_token": session.refresh })).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path} must not be mounted: {body}");
    }

    finish(&url, db, server).await;
}

// ── Lifetimes ────────────────────────────────────────────────────────────────

/// `ttl_secs` sets the access-token lifetime of every sign-in path: password, `OTP`,
/// `MFA`, and a refresh.
#[tokio::test]
async fn ttl_secs_sets_the_lifetime_of_every_sign_in() {
    let db = "fraiseql_session_ttl";
    let Some((url, pool, server)) =
        start("ttl_secs_sets_the_lifetime", db, refreshable(), None).await
    else {
        return;
    };
    let base = &server.base;

    let signed_up = signup(base, "ttl@example.com").await;
    assert_lifetime("password signup", &signed_up.access, TTL);
    let (status, body) = post_json(
        base,
        "/auth/v1/password/login",
        json!({ "email": "ttl@example.com", "password": PASSWORD }),
    )
    .await;
    assert_lifetime("password login", &session_of("login", status, &body).access, TTL);

    // OTP: the send fails at the closed relay, and the code it stored still verifies.
    let _ = post_json(base, "/auth/v1/otp", json!({ "email": "otp-ttl@example.com" })).await;
    let code = stored_otp_code(&pool, "otp-ttl@example.com").await;
    let (status, body) =
        post_json(base, "/auth/v1/verify", json!({ "email": "otp-ttl@example.com", "code": code }))
            .await;
    assert_lifetime("OTP verify", &session_of("OTP verify", status, &body).access, TTL);

    let secret = seed_confirmed_mfa(&pool, &user_of(&signed_up.access)).await;
    let (status, body) = post_json(
        base,
        "/auth/v1/password/login",
        json!({ "email": "ttl@example.com", "password": PASSWORD }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "an MFA user owes a challenge: {body}");
    let challenge = body["challenge_token"].as_str().expect("challenge_token").to_string();
    let (status, body) = post_json(
        base,
        "/auth/v1/mfa/verify",
        json!({ "challenge_token": challenge, "code": totp_code(&secret) }),
    )
    .await;
    let after_mfa = session_of("MFA verify", status, &body);
    assert_lifetime("MFA verify", &after_mfa.access, TTL);

    let (status, body) = refresh(base, &after_mfa.refresh).await;
    assert_lifetime("refresh", &session_of("refresh", status, &body).access, TTL);

    finish(&url, db, server).await;
}

// ── Refresh ──────────────────────────────────────────────────────────────────

/// A refresh hands out a new pair, and the new access token is accepted.
#[tokio::test]
async fn a_refresh_rotates_the_pair() {
    let db = "fraiseql_session_rotate";
    let Some((url, _pool, server)) =
        start("a_refresh_rotates_the_pair", db, refreshable(), None).await
    else {
        return;
    };
    let base = &server.base;

    let first = signup(base, "rotate@example.com").await;
    let (status, body) = refresh(base, &first.refresh).await;
    let second = session_of("refresh", status, &body);
    assert_ne!(second.refresh, first.refresh, "the refresh token must rotate");
    assert_ne!(second.access, first.access, "a new access token must be minted");
    assert_eq!(user_of(&second.access), user_of(&first.access), "same account");
    assert_eq!(probe(base, &second.access).await, StatusCode::OK, "the new access token works");

    let (status, body) = refresh(base, &second.refresh).await;
    session_of("a second refresh, with the rotated token", status, &body);

    finish(&url, db, server).await;
}

/// A rotated token reused within 10 seconds of its rotation is a client's race, not a
/// theft: 409, no tokens, and the session goes on.
#[tokio::test]
async fn a_rotated_token_reused_within_ten_seconds_ends_nothing() {
    let db = "fraiseql_session_grace";
    let Some((url, pool, server)) =
        start("reused_within_ten_seconds", db, refreshable(), None).await
    else {
        return;
    };
    let base = &server.base;

    let first = signup(base, "grace@example.com").await;
    let (status, body) = refresh(base, &first.refresh).await;
    let second = session_of("refresh", status, &body);

    for secs_ago in [0, 9, 10] {
        rotated_secs_ago(&pool, &first.refresh, secs_ago).await;
        let (status, body) = refresh(base, &first.refresh).await;
        assert_eq!(status, StatusCode::CONFLICT, "reuse {secs_ago}s after rotation: {body}");
        assert_no_tokens("a reuse within the grace window", &body);
    }

    let (status, body) = refresh(base, &second.refresh).await;
    session_of("the newest token, after reuses within the window", status, &body);

    finish(&url, db, server).await;
}

/// A rotated token reused more than 10 seconds after its rotation means two holders:
/// the whole session ends, the newest token with it.
#[tokio::test]
async fn a_rotated_token_reused_after_ten_seconds_ends_the_session() {
    let db = "fraiseql_session_reuse";
    let Some((url, pool, server)) =
        start("reused_after_ten_seconds", db, refreshable(), None).await
    else {
        return;
    };
    let base = &server.base;

    let first = signup(base, "reuse@example.com").await;
    let (status, body) = refresh(base, &first.refresh).await;
    let second = session_of("refresh", status, &body);

    rotated_secs_ago(&pool, &first.refresh, 11).await;
    let (status, body) = refresh(base, &first.refresh).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "reuse 11s after rotation: {body}");
    assert_no_tokens("a reuse past the grace window", &body);

    let (status, body) = refresh(base, &second.refresh).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the newest token must be ended too: {body}");
    assert_no_tokens("the ended session's newest token", &body);

    // A second sign-in of the same account is its own session, and is untouched.
    let (status, body) = post_json(
        base,
        "/auth/v1/password/login",
        json!({ "email": "reuse@example.com", "password": PASSWORD }),
    )
    .await;
    let other = session_of("a fresh login", status, &body);
    let (status, body) = refresh(base, &other.refresh).await;
    session_of("another session of the same account", status, &body);

    finish(&url, db, server).await;
}

/// Refresh is refused once the session's absolute limit has passed.
#[tokio::test]
async fn refresh_stops_at_the_absolute_limit() {
    let db = "fraiseql_session_absolute";
    let Some((url, pool, server)) =
        start("refresh_stops_at_the_absolute_limit", db, refreshable(), None).await
    else {
        return;
    };
    let base = &server.base;

    let first = signup(base, "absolute@example.com").await;
    let (status, body) = refresh(base, &first.refresh).await;
    let second = session_of("refresh", status, &body);
    let expires_at: i64 =
        sqlx::query_scalar("SELECT expires_at FROM _system.sessions WHERE refresh_token_hash = $1")
            .bind(token_hash(&second.refresh))
            .fetch_one(&pool)
            .await
            .expect("the refreshed row");
    let issued_at = claims(&first.access)["iat"].as_i64().expect("iat");
    assert_eq!(
        expires_at - issued_at,
        ABSOLUTE.cast_signed(),
        "a refresh keeps the limit counted from sign-in, never from the refresh"
    );

    sqlx::query(
        "UPDATE _system.sessions SET expires_at = floor(EXTRACT(EPOCH FROM now()))::bigint - 1 \
         WHERE refresh_token_hash = $1",
    )
    .bind(token_hash(&second.refresh))
    .execute(&pool)
    .await
    .expect("move the limit into the past");
    let (status, body) = refresh(base, &second.refresh).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "past the absolute limit: {body}");
    assert_no_tokens("a refresh past the limit", &body);

    finish(&url, db, server).await;
}

/// A refresh re-reads the account: a deactivated one is refused, and the session ends
/// with it rather than resuming when the account is reactivated.
#[tokio::test]
async fn a_deactivated_account_cannot_refresh() {
    let db = "fraiseql_session_deactivated";
    let Some((url, pool, server)) =
        start("a_deactivated_account_cannot_refresh", db, refreshable(), None).await
    else {
        return;
    };
    let base = &server.base;

    let session = signup(base, "deactivated@example.com").await;
    let user = user_of(&session.access);
    let set_active = |active: bool| {
        sqlx::query("UPDATE core.tb_user SET active = $2 WHERE user_id = $1")
            .bind(user.clone())
            .bind(active)
            .execute(&pool)
    };
    set_active(false).await.expect("deactivate");
    let (status, body) = refresh(base, &session.refresh).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "a deactivated account: {body}");
    assert_no_tokens("a deactivated account's refresh", &body);

    set_active(true).await.expect("reactivate");
    let (status, body) = refresh(base, &session.refresh).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the session ended with the refusal: {body}");

    finish(&url, db, server).await;
}

/// A refresh mints the account's tenant as it is now, not as it was at sign-in (#1450).
#[tokio::test]
async fn a_refresh_carries_the_accounts_current_tenant() {
    let db = "fraiseql_session_tenant";
    let Some((url, pool, server)) =
        start("a_refresh_carries_the_current_tenant", db, refreshable(), None).await
    else {
        return;
    };
    let base = &server.base;

    let session = signup(base, "rehomed@example.com").await;
    let tenant = uuid::Uuid::new_v4();
    sqlx::query("UPDATE core.tb_user SET tenant_id = $2 WHERE user_id = $1")
        .bind(user_of(&session.access))
        .bind(tenant)
        .execute(&pool)
        .await
        .expect("re-home the account");
    let (status, body) = refresh(base, &session.refresh).await;
    let refreshed = session_of("refresh", status, &body);
    // The store mints a tenant as 32 hex digits, as every sign-in path always has.
    assert_eq!(
        claims(&refreshed.access)["tenant_id"].as_str(),
        Some(tenant.as_simple().to_string().as_str()),
        "the refreshed token must carry the account's tenant now"
    );

    finish(&url, db, server).await;
}

/// A user enrolled in `MFA` passed the second factor when the session began; a refresh
/// hands out tokens, never a new challenge.
#[tokio::test]
async fn an_mfa_user_refreshes_without_a_challenge() {
    let db = "fraiseql_session_mfa";
    let Some((url, pool, server)) = start("an_mfa_user_refreshes", db, refreshable(), None).await
    else {
        return;
    };
    let base = &server.base;

    let signed_up = signup(base, "mfa@example.com").await;
    let secret = seed_confirmed_mfa(&pool, &user_of(&signed_up.access)).await;
    let (_, body) = post_json(
        base,
        "/auth/v1/password/login",
        json!({ "email": "mfa@example.com", "password": PASSWORD }),
    )
    .await;
    let challenge = body["challenge_token"].as_str().expect("challenge_token").to_string();
    let (status, body) = post_json(
        base,
        "/auth/v1/mfa/verify",
        json!({ "challenge_token": challenge, "code": totp_code(&secret) }),
    )
    .await;
    let session = session_of("MFA verify", status, &body);

    let (status, body) = refresh(base, &session.refresh).await;
    session_of("an MFA user's refresh", status, &body);

    finish(&url, db, server).await;
}

// ── Logout ───────────────────────────────────────────────────────────────────

/// Logout answers 204 whatever it is given, and ends the session it names. Without
/// `[security.token_revocation]`, the access token already issued is still accepted
/// until it expires: nothing on the request path reads the session row.
#[tokio::test]
async fn logout_ends_the_session_but_not_an_issued_access_token() {
    let db = "fraiseql_session_logout";
    let Some((url, _pool, server)) =
        start("logout_ends_the_session", db, refreshable(), None).await
    else {
        return;
    };
    let base = &server.base;

    let (status, body) = post_json(base, LOGOUT, json!({ "refresh_token": "never-issued" })).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "an unknown token: {body}");

    let first = signup(base, "logout@example.com").await;
    let (status, body) = refresh(base, &first.refresh).await;
    let second = session_of("refresh", status, &body);

    let (status, body) = post_json(base, LOGOUT, json!({ "refresh_token": first.refresh })).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "a rotated token of a live session: {body}");
    let (status, body) = refresh(base, &second.refresh).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the session must be over: {body}");

    assert_eq!(
        probe(base, &second.access).await,
        StatusCode::OK,
        "without token revocation an issued access token outlives logout"
    );

    finish(&url, db, server).await;
}

/// With `[security.token_revocation]`, logout also ends the access token in use.
#[tokio::test]
async fn with_token_revocation_logout_ends_the_access_token() {
    let db = "fraiseql_session_logout_revoked";
    let revocation = Arc::new(TokenRevocationManager::new(
        Arc::new(InMemoryRevocationStore::new()),
        false,
        false,
        3600,
    ));
    let Some((url, _pool, server)) =
        start("logout_ends_the_access_token", db, refreshable(), Some(revocation)).await
    else {
        return;
    };
    let base = &server.base;

    let session = signup(base, "logout-revoked@example.com").await;
    assert_eq!(probe(base, &session.access).await, StatusCode::OK, "before logout");
    let (status, body) = post_json(base, LOGOUT, json!({ "refresh_token": session.refresh })).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    assert_eq!(
        probe(base, &session.access).await,
        StatusCode::UNAUTHORIZED,
        "with token revocation, logout ends the access token"
    );

    finish(&url, db, server).await;
}

/// A revocation store that reads but cannot write: a logout whose denial fails.
struct UnwritableRevocations;

// Reason: implements the dyn-dispatched `RevocationStore`, an `async_trait`
#[async_trait::async_trait]
impl RevocationStore for UnwritableRevocations {
    async fn is_revoked(&self, _jti: &str) -> Result<bool, RevocationError> {
        Ok(false)
    }

    async fn revoke(&self, _jti: &str, _ttl_secs: u64) -> Result<(), RevocationError> {
        Err(RevocationError::Backend("the store refuses writes".to_string()))
    }

    async fn revoke_all_for_user(&self, _sub: &str, _ttl: u64) -> Result<(), RevocationError> {
        Err(RevocationError::Backend("the store refuses writes".to_string()))
    }

    async fn user_revoked_after(&self, _sub: &str) -> Result<Option<i64>, RevocationError> {
        Ok(None)
    }
}

/// With token revocation configured, a logout that cannot deny the access token answers
/// 500: a caller told it logged out must have been. The session's refresh tokens still end.
#[tokio::test]
async fn a_logout_that_cannot_deny_its_access_token_says_so() {
    let db = "fraiseql_session_logout_unwritable";
    let revocation =
        Arc::new(TokenRevocationManager::new(Arc::new(UnwritableRevocations), false, false, 3600));
    let Some((url, _pool, server)) =
        start("a_logout_that_cannot_deny", db, refreshable(), Some(revocation)).await
    else {
        return;
    };
    let base = &server.base;

    let session = signup(base, "logout-unwritable@example.com").await;
    let (status, body) = post_json(base, LOGOUT, json!({ "refresh_token": session.refresh })).await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "a denial that failed must not answer 204: {body}"
    );
    let (status, body) = refresh(base, &session.refresh).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the refresh tokens still ended: {body}");

    finish(&url, db, server).await;
}

// ── Rate limits ──────────────────────────────────────────────────────────────

/// The session routes spend the `auth_refresh` and `auth_logout` budgets per client.
#[tokio::test]
async fn the_session_routes_are_limited_per_client() {
    const BUDGET: u32 = 5;

    let db = "fraiseql_session_limits";
    let mut schema = refreshable();
    schema.security = Some(fraiseql_core::schema::SecurityConfig {
        rate_limiting: Some(fraiseql_core::schema::RateLimitingSecurityConfig {
            auth_refresh_max_requests: BUDGET,
            auth_refresh_window_secs: 3_600,
            auth_logout_max_requests: BUDGET,
            auth_logout_window_secs: 3_600,
            ..fraiseql_core::schema::RateLimitingSecurityConfig::default()
        }),
        ..fraiseql_core::schema::SecurityConfig::default()
    });
    let Some((url, _pool, server)) =
        start("the_session_routes_are_limited", db, schema, None).await
    else {
        return;
    };
    let base = &server.base;
    let one: std::net::IpAddr = [127, 0, 0, 1].into();
    let two: std::net::IpAddr = [127, 0, 0, 2].into();

    for path in [REFRESH, LOGOUT] {
        let body = |n: u32| json!({ "refresh_token": format!("unknown-{n}") });
        for n in 0..BUDGET {
            assert_ne!(
                post_from(one, base, path, body(n)).await,
                StatusCode::TOO_MANY_REQUESTS,
                "{path}: request {n} of a budget of {BUDGET}"
            );
        }
        assert_eq!(
            post_from(one, base, path, body(BUDGET)).await,
            StatusCode::TOO_MANY_REQUESTS,
            "{path}: the request past the budget"
        );
        assert_ne!(
            post_from(two, base, path, body(0)).await,
            StatusCode::TOO_MANY_REQUESTS,
            "{path}: another client keeps its own budget"
        );
    }

    finish(&url, db, server).await;
}

// ── Sweep ────────────────────────────────────────────────────────────────────

/// The sweep removes rows past their limit, and keeps a rotated row until then: reuse
/// detection needs it for the session's whole life.
#[tokio::test]
async fn the_sweep_keeps_a_rotated_row_until_the_limit() {
    let db = "fraiseql_session_sweep";
    let Some((url, pool, server)) =
        start("the_sweep_keeps_a_rotated_row", db, refreshable(), None).await
    else {
        return;
    };
    let base = &server.base;

    let first = signup(base, "sweep@example.com").await;
    let (status, body) = refresh(base, &first.refresh).await;
    let current = session_of("refresh", status, &body);
    rotated_secs_ago(&pool, &first.refresh, 3_600).await;

    let rows = |token: &str| {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM _system.sessions WHERE refresh_token_hash = $1",
        )
        .bind(token_hash(token))
        .fetch_one(&pool)
    };
    let sessions =
        fraiseql_auth::PostgresSessionStore::with_hs256_secret(pool.clone(), HS256_SECRET.into());

    sessions.sweep_expired().await.expect("sweep");
    assert_eq!(rows(&first.refresh).await.unwrap(), 1, "a rotated row inside the limit stays");

    // A store built without `[auth.session]` refreshes nothing, not even a live session's
    // current token: it has no access-token lifetime to mint with.
    assert!(
        matches!(sessions.refresh(&current.refresh).await, Ok(fraiseql_auth::Refresh::Refused)),
        "a store without a refreshable lifetime must refuse"
    );

    sqlx::query(
        "UPDATE _system.sessions SET expires_at = floor(EXTRACT(EPOCH FROM now()))::bigint - 1 \
         WHERE refresh_token_hash = $1",
    )
    .bind(token_hash(&first.refresh))
    .execute(&pool)
    .await
    .expect("move the limit into the past");
    sessions.sweep_expired().await.expect("sweep");
    assert_eq!(rows(&first.refresh).await.unwrap(), 0, "a row past its limit is removed");

    finish(&url, db, server).await;
}

// ── Expiry and the validator's leeway ────────────────────────────────────────

/// Real time, no rewrite: an access token is accepted until `exp` + 60 s (the `HS256`
/// validator's leeway), refused after it, and a refresh then continues the session.
/// Sleeps about two minutes.
#[tokio::test]
async fn sleeps_two_minutes_access_ends_at_ttl_plus_the_leeway_and_refresh_continues() {
    const SHORT_TTL: u64 = 60;
    const LEEWAY: u64 = 60;

    let db = "fraiseql_session_expiry";
    let schema = schema_with(Some(SessionConfig {
        ttl_secs:          Some(SHORT_TTL),
        absolute_ttl_secs: Some(3_600),
    }));
    let Some((url, _pool, server)) =
        start("access_ends_at_ttl_plus_the_leeway", db, schema, None).await
    else {
        return;
    };
    let base = &server.base;

    let session = signup(base, "expiry@example.com").await;
    assert_lifetime("password signup", &session.access, SHORT_TTL);
    let exp = claims(&session.access)["exp"].as_u64().expect("exp");
    let sleep_until = |at: u64| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs();
        tokio::time::sleep(std::time::Duration::from_secs(at.saturating_sub(now)))
    };

    sleep_until(exp + LEEWAY - 5).await;
    assert_eq!(
        probe(base, &session.access).await,
        StatusCode::OK,
        "inside the leeway the token is still accepted"
    );

    sleep_until(exp + LEEWAY + 2).await;
    assert_eq!(
        probe(base, &session.access).await,
        StatusCode::UNAUTHORIZED,
        "past exp + leeway the token is refused"
    );

    let (status, body) = refresh(base, &session.refresh).await;
    let refreshed = session_of("refresh after expiry", status, &body);
    assert_eq!(probe(base, &refreshed.access).await, StatusCode::OK, "access continues");

    finish(&url, db, server).await;
}

// ── Helpers shared with the local-auth suite ─────────────────────────────────

/// With only `absolute_ttl_secs`, every access token lives an hour, refreshed ones included.
#[tokio::test]
async fn without_ttl_secs_a_refreshable_session_mints_hour_long_tokens() {
    let db = "fraiseql_session_default_ttl";
    let schema = schema_with(Some(SessionConfig {
        ttl_secs:          None,
        absolute_ttl_secs: Some(ABSOLUTE),
    }));
    let Some((url, _pool, server)) = start("without_ttl_secs", db, schema, None).await else {
        return;
    };
    let base = &server.base;

    let session = signup(base, "hourly@example.com").await;
    assert_lifetime("password signup", &session.access, 3_600);
    let (status, body) = refresh(base, &session.refresh).await;
    let refreshed = session_of("refresh", status, &body);
    assert_lifetime("refresh", &refreshed.access, 3_600);

    finish(&url, db, server).await;
}

/// `[auth.session]` does not reach anonymous sign-up: its sessions keep their fixed seven
/// days and are never refreshable.
#[tokio::test]
async fn an_anonymous_session_keeps_seven_days_and_never_refreshes() {
    let db = "fraiseql_session_anonymous";
    let mut schema = refreshable();
    if let Some(local) = schema.auth.as_mut().and_then(|a| a.local.as_mut()) {
        local.anonymous = true;
    }
    let Some((url, _pool, server)) =
        start("an_anonymous_session_keeps_seven_days", db, schema, None).await
    else {
        return;
    };
    let base = &server.base;

    let (status, body) = post_json(base, "/auth/v1/signup", json!({})).await;
    let anonymous = session_of("anonymous signup", status, &body);
    assert_lifetime("anonymous signup", &anonymous.access, 7 * 24 * 3600);
    let (status, body) = refresh(base, &anonymous.refresh).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "an anonymous session never refreshes: {body}");
    assert_no_tokens("an anonymous refresh", &body);

    finish(&url, db, server).await;
}

/// What the compiler refuses, the server refuses at boot, for a schema written by hand;
/// and refresh with no sign-in that mints a refreshable session refuses to boot.
#[tokio::test]
async fn a_session_section_that_could_not_work_does_not_boot() {
    let Some(url) = database_url_or_skip("a_session_section_that_could_not_work") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_session_refusals";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    let short = schema_with(Some(SessionConfig {
        ttl_secs:          Some(59),
        absolute_ttl_secs: None,
    }));
    let mut anonymous_only = schema_with(Some(SessionConfig {
        ttl_secs:          None,
        absolute_ttl_secs: Some(ABSOLUTE),
    }));
    anonymous_only.auth.as_mut().expect("auth").local = Some(LocalAuthConfig {
        anonymous: true,
        ..LocalAuthConfig::default()
    });
    for (what, schema, names) in [
        ("a ttl_secs under a minute", short, "ttl_secs = 59"),
        ("refresh with only anonymous sign-up", anonymous_only, "absolute_ttl_secs"),
    ] {
        let adapter =
            Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
        let refused =
            Box::pin(Server::new(server_config(&scratch_url), schema, adapter, Some(pool.clone())))
                .await;
        let Err(error) = refused else {
            panic!("{what} must refuse to boot");
        };
        assert!(error.to_string().contains(names), "{what}: the refusal names it: {error}");
    }

    pool.close().await;
    drop_scratch(&url, db).await;
}

fn totp_code(secret: &str) -> String {
    totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        totp_rs::Secret::Encoded(secret.to_string()).to_bytes().expect("secret decodes"),
        None,
        String::new(),
    )
    .expect("totp")
    .generate_current()
    .expect("generate code")
}

async fn seed_confirmed_mfa(pool: &PgPool, user_id: &str) -> String {
    use fraiseql_auth::MfaStore as _;

    let store = fraiseql_auth::PgMfaStore::new(pool.clone());
    let enrolled = store
        .begin_enrollment(user_id, "FraiseQL", "user@example.com", None)
        .await
        .unwrap();
    store
        .confirm_enrollment(user_id, &totp_code(&enrolled.secret_base32))
        .await
        .unwrap();
    enrolled.secret_base32
}

/// The code `/auth/v1/otp` last stored for `email`, recovered from its hash.
async fn stored_otp_code(pool: &PgPool, email: &str) -> String {
    let (code_hash,): (Vec<u8>,) =
        sqlx::query_as("SELECT code_hash FROM core.tb_otp_code WHERE email = $1")
            .bind(email)
            .fetch_one(pool)
            .await
            .expect("the send must have persisted a code");
    (0..1_000_000u32)
        .map(|n| format!("{n:06}"))
        .find(|candidate| {
            use sha2::Digest as _;
            sha2::Sha256::digest(candidate.as_bytes()).to_vec() == code_hash
        })
        .expect("a SHA-256 of a 6-digit code")
}
