//! #367: the `[auth.local]` reachability tier through the shipped server's mount.
//!
//! Every method here existed in `fraiseql-auth` and was **unreachable from the
//! binary**: `with_mfa` / `with_anon_signup` had zero callers, the `MFA` and anon
//! route groups were registered against fields hard-coded to `None`, `OTP` had no
//! route at all in the server, and password auth had neither a route nor a
//! concrete `ResetEmailSender`. These tests boot the real server from a compiled
//! `[auth.local]` block and drive the loops:
//!
//! - password: signup → session → login → wrong password 401 → duplicate signup 409
//! - `OTP`: send → verify → session, with the identity resolved through the **account store**
//!   (#367's `otp:<email>` gap) so the same email converges on one account
//! - `MFA`: enroll → confirm → challenge → verify against the **`Postgres`** store, and the
//!   enrollment survives a full server restart (the whole reason that store exists)
//! - anonymous: `POST /auth/v1/signup` mints a guest session only when enabled
//! - email verification (#945): signup → authenticated start → the link this server actually mailed
//!   → confirm → the account is promoted to verified, plus the 401/422 refusals
//! - the mounted set matches the enabled set: a disabled method 404s
//!
//! Mail delivery is exercised through a capturing `EmailDelivery` in the `OTP` case
//! (the `SMTP` relay itself is P21's territory and has its own suite); what this
//! proves is that the code the server generated is the code that verifies.
//!
//! Self-skips when no `DATABASE_URL` is set.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `fraiseql_p26_local_*` databases →
//! run `--test-threads=1`.
#![cfg(feature = "auth")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)]
// Reason: test code — panics and skip diagnostics are acceptable
// Reason: `{token}` / `{code}` in the URL templates are FraiseQL's own placeholders, which
// the server substitutes — not Rust format arguments.
#![allow(clippy::literal_string_with_formatting_args)]

use std::sync::Arc;

#[cfg(feature = "inbound-email")]
use fraiseql_auth::AccountStore as _;
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    schema::{AuthClientConfig, CompiledSchema, LocalAuthConfig},
};
use fraiseql_server::{
    Server,
    server_config::{ServerConfig, hs256::Hs256Config},
};
use fraiseql_test_support::try_database_url;
use sqlx::PgPool;

const HS256_SECRET: &str = "p26-local-hs256-secret-32-bytes!";
const HS256_SECRET_ENV: &str = "FRAISEQL_TEST_P26_LOCAL_E2E_HS256";
const HS256_ISSUER: &str = "https://sp.example.com";
const HS256_AUDIENCE: &str = "fraiseql-local-test";

fn set_test_env() {
    // Fixed valid values, set unconditionally: every reader wants exactly these.
    std::env::set_var(HS256_SECRET_ENV, HS256_SECRET);
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
    serde_json::from_value(serde_json::json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
    }))
    .expect("compiled schema")
}

fn local_schema(local: LocalAuthConfig) -> CompiledSchema {
    let mut schema = empty_schema();
    schema.auth = Some(AuthClientConfig {
        pkce:   None,
        social: None,
        local:  Some(local),
    });
    schema
}

/// A `[mailbox.support]` whose `SMTP` half points at a closed loopback port.
///
/// The transport builds (which is what `[auth.local]` requires at boot) but a
/// real send fails fast with connection-refused. That is deliberate: the mail
/// relay itself is P21's suite, and the `OTP` identity assertions below read the
/// code from the store the server wrote, never from a delivered message.
#[cfg(feature = "inbound-email")]
fn unreachable_mailbox()
-> std::collections::HashMap<String, fraiseql_server::inbound::email::MailboxConfig> {
    std::env::set_var("FRAISEQL_TEST_P26_LOCAL_SMTP_PASSWORD", "unused");
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
                password_env: "FRAISEQL_TEST_P26_LOCAL_SMTP_PASSWORD".to_string(),
                tls:          fraiseql_server::inbound::email::SmtpTlsMode::None,
                timeout_secs: 2,
                return_path:  None,
            }),
        },
    );
    mailbox
}

fn local_config(scratch_url: &str) -> ServerConfig {
    ServerConfig {
        // #874: production validate() refuses cors_enabled = true + empty origins.
        cors_enabled: false,
        database_url: scratch_url.to_string(),
        auth_hs256: Some(Hs256Config {
            secret_env: HS256_SECRET_ENV.to_string(),
            issuer:     Some(HS256_ISSUER.to_string()),
            audience:   Some(HS256_AUDIENCE.to_string()),
        }),
        #[cfg(feature = "inbound-email")]
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

/// Wait for the server's first answer. It provisions its tables before it accepts a
/// connection, so from then on a fixture may write to them; a fixed sleep raced that.
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

async fn boot(local: LocalAuthConfig, pool: PgPool, scratch_url: &str) -> RunningServer {
    boot_with(local_config(scratch_url), local, pool, scratch_url).await
}

/// [`boot`] with `config` in place of [`local_config`].
async fn boot_with(
    config: ServerConfig,
    local: LocalAuthConfig,
    pool: PgPool,
    scratch_url: &str,
) -> RunningServer {
    let adapter = Arc::new(PostgresAdapter::new(scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let server = Box::pin(Server::new(config, local_schema(local), adapter, Some(pool)))
        .await
        .expect("Server::new with [auth.local] + [auth_hs256] + pool must succeed");
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

/// Decode a JWT's claims without validating it, asserting the minted
/// issuer/audience match the configured `[auth_hs256]` — a mismatch is the
/// "login succeeds, then every request 401s" defect.
fn jwt_sub(token: &str) -> String {
    use base64::Engine as _;
    let payload = token.split('.').nth(1).expect("JWT has a payload");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("payload decodes");
    let claims: serde_json::Value = serde_json::from_slice(&bytes).expect("claims parse");
    assert_eq!(claims["iss"].as_str(), Some(HS256_ISSUER), "{claims}");
    assert!(
        claims["aud"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(HS256_AUDIENCE))),
        "{claims}"
    );
    claims["sub"].as_str().expect("sub claim").to_string()
}

async fn post_json(
    base: &str,
    path: &str,
    body: serde_json::Value,
) -> (reqwest::StatusCode, serde_json::Value) {
    let resp = reqwest::Client::new()
        .post(format!("{base}{path}"))
        .json(&body)
        .send()
        .await
        .expect("request");
    let status = resp.status();
    let json = resp.json().await.unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// A session access token for `user_id`, signed as this server signs its own.
fn session_for(user_id: &str) -> String {
    let now = chrono::Utc::now().timestamp();
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &serde_json::json!({
            "sub": user_id, "iss": HS256_ISSUER, "aud": [HS256_AUDIENCE],
            "iat": now, "exp": now + 3600, "jti": format!("test-{user_id}-{now}"),
        }),
        &jsonwebtoken::EncodingKey::from_secret(HS256_SECRET.as_bytes()),
    )
    .expect("sign a session token")
}

/// The challenge a sign-in of `user_id` earns, from where every sign-in path mints its
/// session.
async fn challenge_for(pool: &PgPool, user_id: &str) -> String {
    use fraiseql_auth::SessionStore as _;

    let sessions =
        fraiseql_auth::PostgresSessionStore::with_hs256_secret(pool.clone(), HS256_SECRET.into());
    match sessions.create_session(user_id, u64::MAX / 2).await {
        Err(fraiseql_auth::AuthError::SecondFactorRequired { challenge }) => {
            challenge.expose().to_string()
        },
        other => panic!("a sign-in of {user_id} must owe the second factor, got {other:?}"),
    }
}

/// `POST` `body` to `path` as the holder of `bearer`.
async fn post_json_as(
    base: &str,
    path: &str,
    bearer: &str,
    body: serde_json::Value,
) -> (reqwest::StatusCode, serde_json::Value) {
    let resp = reqwest::Client::new()
        .post(format!("{base}{path}"))
        .bearer_auth(bearer)
        .json(&body)
        .send()
        .await
        .expect("request");
    let status = resp.status();
    let json = resp.json().await.unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ── Tests ────────────────────────────────────────────────────────────────────

// Requires the SMTP transport (`inbound-email`): both flows deliver mail, and
// `[auth.local]` refuses to boot without a sender rather than mounting a login
// that mails nobody. The Dagger server-integration line runs this suite with
// the feature on, so this gate never silently removes coverage.
#[cfg(feature = "inbound-email")]
#[tokio::test]
async fn password_signup_login_and_failure_paths() {
    let Some(url) = database_url_or_skip("password_signup_login") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_password";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    let server = boot(
        LocalAuthConfig {
            password: true,
            email_from: Some("support".to_string()),
            reset_url_template: Some("https://app.example.com/reset?token={token}".to_string()),
            ..LocalAuthConfig::default()
        },
        pool.clone(),
        &scratch_url,
    )
    .await;
    let base = &server.base;

    // Signup mints a session for the new account.
    let (status, body) = post_json(
        base,
        "/auth/v1/password/signup",
        serde_json::json!({ "email": "carol@example.com", "password": "correct horse battery" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "signup must succeed: {body}");
    let signup_user = jwt_sub(body["access_token"].as_str().expect("access_token"));

    // Login with the right password lands on the SAME account.
    let (status, body) = post_json(
        base,
        "/auth/v1/password/login",
        serde_json::json!({ "email": "carol@example.com", "password": "correct horse battery" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "login must succeed: {body}");
    assert_eq!(
        jwt_sub(body["access_token"].as_str().expect("access_token")),
        signup_user,
        "login must resolve to the account signup created"
    );

    // A wrong password is a generic 401 — never a different shape for an
    // unknown account vs a wrong password.
    let (status, _) = post_json(
        base,
        "/auth/v1/password/login",
        serde_json::json!({ "email": "carol@example.com", "password": "wrong" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
    let (unknown_status, _) = post_json(
        base,
        "/auth/v1/password/login",
        serde_json::json!({ "email": "nobody@example.com", "password": "wrong" }),
    )
    .await;
    assert_eq!(
        unknown_status, status,
        "an unknown account and a wrong password must be indistinguishable"
    );

    // A duplicate signup is refused rather than silently re-issuing a session.
    let (status, body) = post_json(
        base,
        "/auth/v1/password/signup",
        serde_json::json!({ "email": "carol@example.com", "password": "another valid secret" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body}");

    // A password below the 12-character policy floor is refused as invalid
    // registration, distinctly from the duplicate case above.
    let (status, body) = post_json(
        base,
        "/auth/v1/password/signup",
        serde_json::json!({ "email": "short@example.com", "password": "tooshort" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // Reset-start is always 202, whether or not the account exists — the
    // response must not be an account-existence oracle.
    for email in ["carol@example.com", "nobody@example.com"] {
        let (status, _) =
            post_json(base, "/auth/v1/password/reset", serde_json::json!({ "email": email })).await;
        assert_eq!(
            status,
            reqwest::StatusCode::ACCEPTED,
            "reset start must be constant-202 for {email}"
        );
    }

    // A forged reset token is refused.
    let (status, _) = post_json(
        base,
        "/auth/v1/password/reset/confirm",
        serde_json::json!({ "token": "forged.token", "new_password": "brand new secret" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    server.stop().await;
    drop_scratch(&url, db).await;
}

// Requires the SMTP transport (`inbound-email`): both flows deliver mail, and
// `[auth.local]` refuses to boot without a sender rather than mounting a login
// that mails nobody. The Dagger server-integration line runs this suite with
// the feature on, so this gate never silently removes coverage.
#[cfg(feature = "inbound-email")]
#[tokio::test]
async fn otp_identity_resolves_through_the_account_store() {
    let Some(url) = database_url_or_skip("otp_identity_resolves") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_otp";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    let server = boot(
        LocalAuthConfig {
            otp: true,
            email_from: Some("support".to_string()),
            ..LocalAuthConfig::default()
        },
        pool.clone(),
        &scratch_url,
    )
    .await;
    let base = &server.base;

    // Send a code. The mailbox is not configured to relay in this rig, so the
    // send fails at delivery — but the code has been minted and stored, which
    // is what the identity assertion below needs. Read it from the store the
    // server wrote to (never from a log).
    let (send_status, _) =
        post_json(base, "/auth/v1/otp", serde_json::json!({ "email": "dave@example.com" })).await;
    assert!(
        send_status == reqwest::StatusCode::OK
            || send_status == reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        "OTP send reached the handler (delivery may fail in this rig): {send_status}"
    );

    let stored: Option<(Vec<u8>,)> =
        sqlx::query_as("SELECT code_hash FROM core.tb_otp_code WHERE email = $1")
            .bind("dave@example.com")
            .fetch_optional(&pool)
            .await
            .expect("query the OTP store");
    let code_hash = stored.expect("the send must have persisted a code").0;

    // Brute-force the 6-digit space against the stored hash: the test needs the
    // plaintext code, and this proves the store really hashes it (a plaintext
    // column would make this loop find nothing).
    let code = (0..1_000_000u32)
        .map(|n| format!("{n:06}"))
        .find(|candidate| {
            use sha2::Digest as _;
            sha2::Sha256::digest(candidate.as_bytes()).to_vec() == code_hash
        })
        .expect("the stored value must be a SHA-256 of a 6-digit code");

    let (status, body) = post_json(
        base,
        "/auth/v1/verify",
        serde_json::json!({ "email": "dave@example.com", "code": code }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "a correct code must verify: {body}");
    let otp_user = jwt_sub(body["access_token"].as_str().expect("access_token"));

    // The identity is a real account (#367): it is NOT the legacy
    // `otp:<email>` pseudo-identity, and a later verified-email sign-in for the
    // same address lands on the same user.
    assert!(
        !otp_user.starts_with("otp:"),
        "OTP must mint a real account id, not the unlinked otp:<email> pseudo-identity: {otp_user}"
    );
    let store = fraiseql_auth::PostgresAccountStore::new(pool.clone());
    let cross = store
        .link_or_create_user(None, Some("dave@example.com"), true, "google", "g-sub-dave")
        .await
        .expect("cross-provider link");
    assert!(!cross.is_new, "a verified same-email login must land on the OTP account");
    assert_eq!(cross.user_id, otp_user);

    // The code is single-use.
    let (status, _) = post_json(
        base,
        "/auth/v1/verify",
        serde_json::json!({ "email": "dave@example.com", "code": code }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "a consumed code must not verify twice"
    );

    server.stop().await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn mfa_enrollment_survives_a_restart() {
    let Some(url) = database_url_or_skip("mfa_enrollment_survives_a_restart") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_mfa";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    let server = boot(
        LocalAuthConfig {
            mfa: true,
            mfa_issuer: Some("Acme".to_string()),
            ..LocalAuthConfig::default()
        },
        pool.clone(),
        &scratch_url,
    )
    .await;

    // Enroll, then confirm with a live TOTP code derived from the returned URI, both as
    // the account's own session.
    let session = session_for("user-1");
    let (status, body) = post_json_as(
        &server.base,
        "/auth/v1/mfa/enroll",
        &session,
        serde_json::json!({ "account_name": "erin@example.com" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "enroll must succeed: {body}");
    let uri = body["otpauth_uri"].as_str().expect("otpauth_uri").to_string();
    assert!(uri.contains("issuer=Acme"), "the configured issuer must reach the URI: {uri}");
    let recovery: Vec<String> = body["recovery_codes"]
        .as_array()
        .expect("recovery_codes")
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    assert_eq!(recovery.len(), 8);

    // The enrollment row exists and is NOT yet confirmed.
    let confirmed: (bool,) =
        sqlx::query_as("SELECT confirmed FROM core.tb_mfa_enrollment WHERE user_id = $1")
            .bind("user-1")
            .fetch_one(&pool)
            .await
            .expect("enrollment row");
    assert!(!confirmed.0, "a fresh enrollment is unconfirmed until the first code verifies");

    // Confirm through the HTTP route. This used to reach past the surface an operator
    // has and call `PgMfaStore::confirm_enrollment` directly, because no route was
    // mounted — which meant MFA could be enrolled but never used through the API alone
    // (#950). Reaching past the surface is exactly how the gap survived: the test still
    // passed. Confirming through the route is the operator's path.
    let secret = uri
        .split("secret=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .expect("secret in otpauth URI")
        .to_string();
    let totp = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        totp_rs::Secret::Encoded(secret).to_bytes().expect("secret decodes"),
        None,
        String::new(),
    )
    .expect("totp");
    let code = totp.generate_current().expect("generate code");
    let (status, body) = post_json_as(
        &server.base,
        "/auth/v1/mfa/confirm",
        &session,
        serde_json::json!({ "code": code }),
    )
    .await;
    assert_eq!(status, 200, "confirm through the route: {body}");

    let confirmed: (bool,) =
        sqlx::query_as("SELECT confirmed FROM core.tb_mfa_enrollment WHERE user_id = $1")
            .bind("user-1")
            .fetch_one(&pool)
            .await
            .expect("enrollment row");
    assert!(confirmed.0, "the route must confirm the enrollment, not just answer 200");

    // Restart the server: an in-memory store would lose the enrollment here,
    // which is exactly why `[auth.local] mfa` is backed by Postgres.
    server.stop().await;
    let server = boot(
        LocalAuthConfig {
            mfa: true,
            mfa_issuer: Some("Acme".to_string()),
            ..LocalAuthConfig::default()
        },
        pool.clone(),
        &scratch_url,
    )
    .await;

    // Challenge + verify against the restarted process.
    // A sign-in after the restart owes the second factor: the enrollment survived.
    let challenge = challenge_for(&pool, "user-1").await;

    // A wrong code is refused…
    let (status, _) = post_json(
        &server.base,
        "/auth/v1/mfa/verify",
        serde_json::json!({ "challenge_token": challenge, "code": "000000" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    // …and a recovery code verifies and mints a session.
    let challenge = challenge_for(&pool, "user-1").await;
    let (status, body) = post_json(
        &server.base,
        "/auth/v1/mfa/verify",
        serde_json::json!({ "challenge_token": challenge, "code": recovery[0] }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "a recovery code must verify: {body}");
    assert_eq!(jwt_sub(body["access_token"].as_str().expect("access_token")), "user-1");

    // The used recovery code is consumed, not replayable.
    let remaining: (Vec<String>,) = sqlx::query_as(
        "SELECT recovery_code_hashes FROM core.tb_mfa_enrollment WHERE user_id = $1",
    )
    .bind("user-1")
    .fetch_one(&pool)
    .await
    .expect("enrollment row");
    assert_eq!(remaining.0.len(), 7, "the consumed recovery code must be removed");

    server.stop().await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn only_enabled_methods_are_mounted() {
    let Some(url) = database_url_or_skip("only_enabled_methods_are_mounted") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_mounts";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    // Only anonymous signup is on.
    let server = boot(
        LocalAuthConfig {
            anonymous: true,
            ..LocalAuthConfig::default()
        },
        pool.clone(),
        &scratch_url,
    )
    .await;
    let base = &server.base;

    let (status, body) = post_json(base, "/auth/v1/signup", serde_json::json!({})).await;
    assert_eq!(status, reqwest::StatusCode::OK, "anonymous signup must be mounted: {body}");
    let anon_user = jwt_sub(body["access_token"].as_str().expect("access_token"));
    assert!(
        anon_user.starts_with("anon_"),
        "an anonymous session must carry the anon_ prefix: {anon_user}"
    );

    // Every disabled method is genuinely absent — not mounted-and-broken, which
    // is the exact shape #367 was filed against.
    for path in [
        "/auth/v1/password/signup",
        "/auth/v1/password/login",
        "/auth/v1/otp",
        "/auth/v1/verify",
        "/auth/v1/mfa/enroll",
        "/auth/v1/mfa/challenge",
        "/auth/v1/email/verify/start",
        "/auth/v1/email/verify/confirm",
    ] {
        let (status, _) = post_json(base, path, serde_json::json!({})).await;
        assert_eq!(
            status,
            reqwest::StatusCode::NOT_FOUND,
            "{path} must not be mounted when its method is disabled"
        );
    }

    server.stop().await;
    drop_scratch(&url, db).await;
}

// ── #945: email verification through the shipped mount ───────────────────────

/// A minimal capturing SMTP server: enough of the protocol for `lettre`'s
/// plaintext relay to complete a send, and nothing more.
///
/// The other tests here point the mailbox at a discard port and read what the
/// server *stored*. Verification cannot work that way — only the token's SHA-256
/// is persisted, so the sole place the usable token exists is the delivered
/// message. Capturing it is what makes this an end-to-end proof rather than a
/// re-assertion of the library test: the link the operator's user clicks is the
/// link built from `verification_url_template` by the server's own sender.
#[cfg(feature = "inbound-email")]
struct CapturingSmtp {
    port:     u16,
    captured: Arc<std::sync::Mutex<Vec<String>>>,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

#[cfg(feature = "inbound-email")]
impl CapturingSmtp {
    async fn start() -> Self {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let listener =
            tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind SMTP sink port");
        let port = listener.local_addr().expect("sink addr").port();
        let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = Arc::clone(&captured);
        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            loop {
                let stream = tokio::select! {
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _)) => stream,
                        Err(_) => break,
                    },
                    _ = &mut rx => break,
                };
                let sink = Arc::clone(&sink);
                tokio::spawn(async move {
                    let (read_half, mut write) = stream.into_split();
                    let mut reader = BufReader::new(read_half);
                    let mut line = String::new();
                    if write.write_all(b"220 localhost ESMTP sink\r\n").await.is_err() {
                        return;
                    }
                    loop {
                        line.clear();
                        match reader.read_line(&mut line).await {
                            Ok(0) | Err(_) => return,
                            Ok(_) => {},
                        }
                        let command = line.trim_end().to_ascii_uppercase();
                        let reply: &[u8] = if command.starts_with("EHLO")
                            || command.starts_with("HELO")
                        {
                            // AUTH must be advertised: the transport is built with
                            // credentials, and lettre refuses to send without a
                            // mechanism it recognizes.
                            b"250-localhost\r\n250-AUTH PLAIN LOGIN\r\n250 8BITMIME\r\n"
                        } else if command.starts_with("AUTH") {
                            b"235 2.7.0 accepted\r\n"
                        } else if command.starts_with("MAIL FROM")
                            || command.starts_with("RCPT TO")
                            || command.starts_with("RSET")
                        {
                            b"250 2.1.0 ok\r\n"
                        } else if command.starts_with("DATA") {
                            if write.write_all(b"354 end with <CRLF>.<CRLF>\r\n").await.is_err() {
                                return;
                            }
                            let mut body = String::new();
                            loop {
                                line.clear();
                                match reader.read_line(&mut line).await {
                                    Ok(0) | Err(_) => return,
                                    Ok(_) => {},
                                }
                                if line.trim_end() == "." {
                                    break;
                                }
                                body.push_str(&line);
                            }
                            sink.lock().unwrap().push(body);
                            b"250 2.0.0 queued\r\n"
                        } else if command.starts_with("QUIT") {
                            let _ = write.write_all(b"221 2.0.0 bye\r\n").await;
                            return;
                        } else {
                            b"250 2.0.0 ok\r\n"
                        };
                        if write.write_all(reply).await.is_err() {
                            return;
                        }
                    }
                });
            }
        });

        Self {
            port,
            captured,
            shutdown: tx,
        }
    }

    /// Wait for one delivered message and return its body.
    async fn next_message(&self) -> String {
        for _ in 0..200 {
            // Bound the guard to a statement: holding it across the match arm would
            // keep the mutex locked while the sink thread wants it.
            let delivered = self.captured.lock().unwrap().first().cloned();
            if let Some(message) = delivered {
                return message;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("no message was delivered to the SMTP sink within 5s");
    }

    fn stop(self) {
        let _ = self.shutdown.send(());
    }
}

/// A `[mailbox.support]` whose SMTP half points at the capturing sink.
#[cfg(feature = "inbound-email")]
fn capturing_mailbox(
    port: u16,
) -> std::collections::HashMap<String, fraiseql_server::inbound::email::MailboxConfig> {
    let mut mailbox = unreachable_mailbox();
    if let Some(entry) = mailbox.get_mut("support") {
        if let Some(smtp) = entry.smtp.as_mut() {
            smtp.port = port;
        }
    }
    mailbox
}

/// Boot with a mailbox that actually delivers, so the mailed link can be read back.
#[cfg(feature = "inbound-email")]
async fn boot_with_smtp(
    local: LocalAuthConfig,
    pool: PgPool,
    scratch_url: &str,
    smtp_port: u16,
) -> RunningServer {
    boot_with_smtp_and_revocation(local, pool, scratch_url, smtp_port, None).await
}

/// [`boot_with_smtp`], enforcing `revocation` as the binary enforces its revocation store.
#[cfg(feature = "inbound-email")]
async fn boot_with_smtp_and_revocation(
    local: LocalAuthConfig,
    pool: PgPool,
    scratch_url: &str,
    smtp_port: u16,
    revocation: Option<Arc<fraiseql_server::token_revocation::TokenRevocationManager>>,
) -> RunningServer {
    let adapter = Arc::new(PostgresAdapter::new(scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let config = ServerConfig {
        mailbox: capturing_mailbox(smtp_port),
        ..local_config(scratch_url)
    };
    let mut server = Box::pin(Server::new(config, local_schema(local), adapter, Some(pool)))
        .await
        .expect("Server::new with [auth.local] email_verification must succeed");
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

/// POST with a bearer session token.
#[cfg(feature = "inbound-email")]
async fn post_authed(
    base: &str,
    path: &str,
    token: &str,
    body: serde_json::Value,
) -> (reqwest::StatusCode, serde_json::Value) {
    let resp = reqwest::Client::new()
        .post(format!("{base}{path}"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .expect("request");
    let status = resp.status();
    let json = resp.json().await.unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Decode the quoted-printable body lettre sends.
///
/// Not decorative: the link contains `?token=`, and quoted-printable renders that
/// `=` as `=3D`, so a naive substring search finds the prefix and then reads a
/// corrupted token. Soft line breaks (`=` at end of line) also split the token
/// across lines.
#[cfg(feature = "inbound-email")]
fn decode_quoted_printable(body: &str) -> String {
    let bytes = body.replace("=\r\n", "").replace("=\n", "").into_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).expect("decoded body is UTF-8")
}

/// Pull the `{token}` value out of the verification link the server mailed.
#[cfg(feature = "inbound-email")]
fn token_from_message(message: &str, template_prefix: &str) -> String {
    let decoded = decode_quoted_printable(message);
    let start = decoded
        .find(template_prefix)
        .unwrap_or_else(|| panic!("verification link not found in message:\n{decoded}"))
        + template_prefix.len();
    let rest = &decoded[start..];
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    urlencoding::decode(&rest[..end])
        .expect("token is percent-decodable")
        .into_owned()
}

#[cfg(feature = "inbound-email")]
#[tokio::test]
async fn email_verification_start_confirm_promotes_the_account() {
    let Some(url) = database_url_or_skip("email_verification_start_confirm") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_verify";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    let smtp = CapturingSmtp::start().await;
    let server = boot_with_smtp(
        LocalAuthConfig {
            password: true,
            email_verification: true,
            email_from: Some("support".to_string()),
            reset_url_template: Some("https://app.example.com/reset?token={token}".to_string()),
            verification_url_template: Some(
                "https://app.example.com/verify-email?token={token}".to_string(),
            ),
            ..LocalAuthConfig::default()
        },
        pool.clone(),
        &scratch_url,
        smtp.port,
    )
    .await;
    let base = &server.base;

    // Sign up: a local account starts unverified, by design.
    let (status, body) = post_json(
        base,
        "/auth/v1/password/signup",
        serde_json::json!({ "email": "dave@example.com", "password": "correct horse battery" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "signup must succeed: {body}");
    let access = body["access_token"].as_str().expect("access_token").to_string();
    let user_id = jwt_sub(&access);
    let verified_email = |pool: PgPool, user_id: String| async move {
        sqlx::query_scalar::<_, Option<String>>("SELECT email FROM core.tb_user WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .expect("user row")
    };
    assert_eq!(
        verified_email(pool.clone(), user_id.clone()).await,
        None,
        "a fresh local signup is unverified"
    );

    // Unauthenticated start is refused: these routes act on the caller's account,
    // so there is no caller-less form of them.
    let (status, _) = post_json(base, "/auth/v1/email/verify/start", serde_json::json!({})).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED, "start needs a session");
    let (status, _) = post_json(
        base,
        "/auth/v1/email/verify/confirm",
        serde_json::json!({ "token": "whatever" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED, "confirm needs a session");

    // Authenticated start mails the link.
    let (status, body) =
        post_authed(base, "/auth/v1/email/verify/start", &access, serde_json::json!({})).await;
    assert_eq!(status, reqwest::StatusCode::ACCEPTED, "start must be accepted: {body}");

    let message = smtp.next_message().await;
    assert!(message.contains("dave@example.com"), "addressed to the account's own address");
    let token = token_from_message(&message, "https://app.example.com/verify-email?token=");
    assert!(token.contains('.'), "the mailed token is selector.verifier: {token}");

    // A token issued to this account, presented by a *different* account, is
    // rejected exactly like a forged one — the confused-deputy refusal, over HTTP.
    let (status, body) = post_json(
        base,
        "/auth/v1/password/signup",
        serde_json::json!({ "email": "erin@example.com", "password": "correct horse battery" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "second signup: {body}");
    let other_access = body["access_token"].as_str().expect("access_token").to_string();
    let (status, body) = post_authed(
        base,
        "/auth/v1/email/verify/confirm",
        &other_access,
        serde_json::json!({ "token": token }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "another account must not spend this token: {body}"
    );
    assert_eq!(
        verified_email(pool.clone(), user_id.clone()).await,
        None,
        "and nothing was promoted"
    );

    // The owner confirms: the account is promoted to verified.
    let (status, body) = post_authed(
        base,
        "/auth/v1/email/verify/confirm",
        &access,
        serde_json::json!({ "token": token }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "confirm must succeed: {body}");
    assert_eq!(body["verified"], serde_json::json!(true), "{body}");
    assert_eq!(body["email"], serde_json::json!("dave@example.com"), "{body}");
    assert_eq!(
        verified_email(pool.clone(), user_id.clone()).await.as_deref(),
        Some("dave@example.com"),
        "the address is now on the account's row, where a later social sign-in finds it"
    );

    // Single-use, over HTTP.
    let (status, _) = post_authed(
        base,
        "/auth/v1/email/verify/confirm",
        &access,
        serde_json::json!({ "token": token }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::UNPROCESSABLE_ENTITY, "a spent link is dead");

    server.stop().await;
    smtp.stop();
    drop_scratch(&url, db).await;
}

/// `[auth.local] email_verification = true` without `password` refuses to boot:
/// verification proves the address a *local* identity claims, and there is none.
#[cfg(feature = "inbound-email")]
#[tokio::test]
async fn email_verification_without_password_refuses_to_boot() {
    let Some(url) = database_url_or_skip("email_verification_without_password") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_verify_nopw";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
    let err = Box::pin(Server::new(
        local_config(&scratch_url),
        local_schema(LocalAuthConfig {
            email_verification: true,
            email_from: Some("support".to_string()),
            verification_url_template: Some(
                "https://app.example.com/verify-email?token={token}".to_string(),
            ),
            ..LocalAuthConfig::default()
        }),
        adapter,
        Some(pool),
    ))
    .await
    .err()
    .unwrap_or_else(|| panic!("email_verification without password must refuse to boot"));
    let message = err.to_string();
    assert!(
        message.contains("email_verification") && message.contains("password"),
        "the refusal must name both keys: {message}"
    );

    drop_scratch(&url, db).await;
}

/// A verification-enabled config with no `verification_url_template` refuses to
/// boot rather than mailing a bare token or a dead link.
#[cfg(feature = "inbound-email")]
#[tokio::test]
async fn email_verification_without_a_link_template_refuses_to_boot() {
    let Some(url) = database_url_or_skip("email_verification_without_template") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_verify_notpl";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
    let err = Box::pin(Server::new(
        local_config(&scratch_url),
        local_schema(LocalAuthConfig {
            password: true,
            email_verification: true,
            email_from: Some("support".to_string()),
            reset_url_template: Some("https://app.example.com/reset?token={token}".to_string()),
            ..LocalAuthConfig::default()
        }),
        adapter,
        Some(pool),
    ))
    .await
    .err()
    .unwrap_or_else(|| panic!("email_verification without a link template must refuse to boot"));
    assert!(
        err.to_string().contains("verification_url_template"),
        "the refusal must name the missing key: {err}"
    );

    drop_scratch(&url, db).await;
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
    empty_schema();
}

/// The current `TOTP` code for a base32 `secret`.
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

/// The base32 secret of an `otpauth://` URI.
fn otpauth_secret(uri: &str) -> String {
    uri.split("secret=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .expect("secret in otpauth URI")
        .to_string()
}

/// An anonymous caller cannot take over an account through the `MFA` routes.
///
/// The victim has a confirmed enrollment. The caller has no credential of any kind, only
/// the victim's id. Each step the caller tries is recorded, so a failure shows how far
/// the chain got: enroll a secret of their own for the victim, confirm it, open a
/// challenge, and verify it into a session.
#[tokio::test]
async fn an_anonymous_caller_cannot_take_over_an_account_through_mfa() {
    use fraiseql_auth::MfaStore as _;

    let Some(url) =
        database_url_or_skip("an_anonymous_caller_cannot_take_over_an_account_through_mfa")
    else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_mfa_takeover";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let server = boot(
        LocalAuthConfig {
            mfa: true,
            ..LocalAuthConfig::default()
        },
        pool.clone(),
        &scratch_url,
    )
    .await;

    // The victim's own enrollment, made and confirmed through the store.
    let victim = "victim-1";
    let store = fraiseql_auth::PgMfaStore::new(pool.clone());
    let enrolled = store
        .begin_enrollment(victim, "FraiseQL", "victim@example.com", None)
        .await
        .unwrap();
    store
        .confirm_enrollment(victim, &totp_code(&enrolled.secret_base32))
        .await
        .unwrap();
    let stored_secret = || async {
        sqlx::query_as::<_, (String, bool)>(
            "SELECT secret_base32, confirmed FROM core.tb_mfa_enrollment WHERE user_id = $1",
        )
        .bind(victim)
        .fetch_one(&pool)
        .await
        .expect("the victim's enrollment row")
    };

    let base = &server.base;
    let mut steps = Vec::new();
    let mut session_sub = None;
    let (status, body) = post_json(
        base,
        "/auth/v1/mfa/enroll",
        serde_json::json!({ "user_id": victim, "account_name": "attacker@example.com" }),
    )
    .await;
    steps.push(format!("enroll → {status}"));
    if let Some(uri) = body["otpauth_uri"].as_str() {
        let secret = otpauth_secret(uri);
        let (status, _) = post_json(
            base,
            "/auth/v1/mfa/confirm",
            serde_json::json!({ "user_id": victim, "code": totp_code(&secret) }),
        )
        .await;
        steps.push(format!("confirm → {status}"));
        let (status, body) =
            post_json(base, "/auth/v1/mfa/challenge", serde_json::json!({ "user_id": victim }))
                .await;
        steps.push(format!("challenge → {status}"));
        if let Some(challenge) = body["challenge_token"].as_str() {
            let (status, body) = post_json(
                base,
                "/auth/v1/mfa/verify",
                serde_json::json!({ "challenge_token": challenge, "code": totp_code(&secret) }),
            )
            .await;
            steps.push(format!("verify → {status}"));
            session_sub = body["access_token"].as_str().map(jwt_sub);
        }
    }

    assert_eq!(session_sub, None, "an anonymous caller obtained a session; steps: {steps:?}");
    assert_eq!(
        stored_secret().await,
        (enrolled.secret_base32.clone(), true),
        "an anonymous caller replaced or unconfirmed the victim's enrollment; steps: {steps:?}"
    );

    server.stop().await;
    drop_scratch(&url, db).await;
}

/// Enroll `user_id` in `MFA` and confirm it, through the store; returns the secret.
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

/// A sign-in answer that asks for the second factor: no session, and a challenge to
/// redeem at `/auth/v1/mfa/verify`. Returns the challenge.
#[cfg(feature = "inbound-email")]
fn expect_second_factor(
    path: &str,
    status: reqwest::StatusCode,
    body: &serde_json::Value,
) -> String {
    assert!(
        body.get("access_token").is_none() && body.get("refresh_token").is_none(),
        "{path} minted a session for a user enrolled in MFA, skipping the second factor: \
         {status} {body}"
    );
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED, "{path}: {body}");
    assert_eq!(body["error"], "mfa_required", "{path}: {body}");
    body["challenge_token"].as_str().expect("a challenge to redeem").to_string()
}

/// The code `/auth/v1/otp` last stored for `email`, recovered from its hash.
#[cfg(feature = "inbound-email")]
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

/// A password sign-in of a user enrolled in `MFA` yields a challenge, not a session, and
/// the challenge redeems into that user's session.
#[cfg(feature = "inbound-email")]
#[tokio::test]
async fn a_password_sign_in_of_an_mfa_user_yields_a_challenge() {
    let Some(url) = database_url_or_skip("a_password_sign_in_of_an_mfa_user_yields_a_challenge")
    else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_mfa_password";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let server = boot(
        LocalAuthConfig {
            password: true,
            mfa: true,
            email_from: Some("support".to_string()),
            reset_url_template: Some("https://app.example.com/reset?token={token}".to_string()),
            ..LocalAuthConfig::default()
        },
        pool.clone(),
        &scratch_url,
    )
    .await;
    let base = &server.base;
    let credentials = serde_json::json!({ "email": "mallory-target@example.com", "password": "correct horse battery" });

    let (status, body) = post_json(base, "/auth/v1/password/signup", credentials.clone()).await;
    assert_eq!(status, reqwest::StatusCode::OK, "signup: {body}");
    let user = jwt_sub(body["access_token"].as_str().expect("access_token"));
    let secret = seed_confirmed_mfa(&pool, &user).await;

    let (status, body) = post_json(base, "/auth/v1/password/login", credentials).await;
    let challenge = expect_second_factor("/auth/v1/password/login", status, &body);
    let (status, body) = post_json(
        base,
        "/auth/v1/mfa/verify",
        serde_json::json!({ "challenge_token": challenge, "code": totp_code(&secret) }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "the challenge redeems: {body}");
    assert_eq!(jwt_sub(body["access_token"].as_str().expect("access_token")), user);

    server.stop().await;
    drop_scratch(&url, db).await;
}

/// An `OTP` sign-in of a user enrolled in `MFA` yields a challenge, not a session.
#[cfg(feature = "inbound-email")]
#[tokio::test]
async fn an_otp_sign_in_of_an_mfa_user_yields_a_challenge() {
    let Some(url) = database_url_or_skip("an_otp_sign_in_of_an_mfa_user_yields_a_challenge") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_mfa_otp";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let server = boot(
        LocalAuthConfig {
            otp: true,
            mfa: true,
            email_from: Some("support".to_string()),
            ..LocalAuthConfig::default()
        },
        pool.clone(),
        &scratch_url,
    )
    .await;
    let base = &server.base;
    let email = "erin-target@example.com";
    let sign_in = || async {
        let _ = post_json(base, "/auth/v1/otp", serde_json::json!({ "email": email })).await;
        let code = stored_otp_code(&pool, email).await;
        post_json(base, "/auth/v1/verify", serde_json::json!({ "email": email, "code": code }))
            .await
    };

    let (status, body) = sign_in().await;
    assert_eq!(status, reqwest::StatusCode::OK, "the first sign-in creates the account: {body}");
    let user = jwt_sub(body["access_token"].as_str().expect("access_token"));
    seed_confirmed_mfa(&pool, &user).await;

    let (status, body) = sign_in().await;
    expect_second_factor("/auth/v1/verify", status, &body);

    server.stop().await;
    drop_scratch(&url, db).await;
}

/// The account's own session replaces its confirmed enrollment only on a current code of
/// it: whoever holds a session, a stolen one included, cannot swap the factor out from
/// under its owner.
#[tokio::test]
async fn a_session_replaces_a_confirmed_enrollment_only_with_a_current_code() {
    let Some(url) = database_url_or_skip("a_session_replaces_a_confirmed_enrollment") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_mfa_replace";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let server = boot(
        LocalAuthConfig {
            mfa: true,
            ..LocalAuthConfig::default()
        },
        pool.clone(),
        &scratch_url,
    )
    .await;
    let user = "owner-1";
    let secret = seed_confirmed_mfa(&pool, user).await;
    let session = session_for(user);
    let enroll = |code: Option<String>| {
        let mut body = serde_json::json!({ "account_name": "owner@example.com" });
        if let Some(code) = code {
            body["code"] = serde_json::json!(code);
        }
        post_json_as(&server.base, "/auth/v1/mfa/enroll", &session, body)
    };

    for code in [None, Some("000000".to_string())] {
        let (status, body) = enroll(code.clone()).await;
        assert_eq!(status, reqwest::StatusCode::UNPROCESSABLE_ENTITY, "code {code:?}: {body}");
    }
    let stored: (String, bool) = sqlx::query_as(
        "SELECT secret_base32, confirmed FROM core.tb_mfa_enrollment WHERE user_id = $1",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, (secret.clone(), true), "the confirmed enrollment stands");

    let (status, body) = enroll(Some(totp_code(&secret))).await;
    assert_eq!(status, reqwest::StatusCode::OK, "a current code replaces it: {body}");

    // A token this server did not sign is no session.
    let forged = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &serde_json::json!({ "sub": user, "iss": HS256_ISSUER, "aud": [HS256_AUDIENCE],
                             "exp": chrono::Utc::now().timestamp() + 3600 }),
        &jsonwebtoken::EncodingKey::from_secret(b"not-this-servers-secret-32-bytes"),
    )
    .unwrap();
    let (status, _) = post_json_as(
        &server.base,
        "/auth/v1/mfa/unenroll",
        &forged,
        serde_json::json!({ "code": totp_code(&secret) }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED, "a forged session is refused");

    server.stop().await;
    drop_scratch(&url, db).await;
}

/// A revoked session is no session to the verification routes either: they act on the
/// caller's account, as every authenticated route does, so they honour the revocation the
/// server's authentication layer enforces.
#[cfg(feature = "inbound-email")]
#[tokio::test]
async fn a_revoked_session_cannot_start_or_confirm_email_verification() {
    let Some(url) = database_url_or_skip("a_revoked_session_cannot_start_email_verification")
    else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_verify_revoked";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let revocation = Arc::new(fraiseql_server::token_revocation::TokenRevocationManager::new(
        Arc::new(fraiseql_server::token_revocation::InMemoryRevocationStore::new()),
        false,
        false,
        3600,
    ));
    let smtp = CapturingSmtp::start().await;
    let server = boot_with_smtp_and_revocation(
        LocalAuthConfig {
            password: true,
            email_verification: true,
            email_from: Some("support".to_string()),
            reset_url_template: Some("https://app.example.com/reset?token={token}".to_string()),
            verification_url_template: Some(
                "https://app.example.com/verify-email?token={token}".to_string(),
            ),
            ..LocalAuthConfig::default()
        },
        pool.clone(),
        &scratch_url,
        smtp.port,
        Some(Arc::clone(&revocation)),
    )
    .await;
    let base = &server.base;
    let (status, body) = post_json(
        base,
        "/auth/v1/password/signup",
        serde_json::json!({ "email": "revoked@example.com", "password": "correct horse battery" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "signup: {body}");
    let access = body["access_token"].as_str().expect("access_token").to_string();
    let jti = {
        use base64::Engine as _;
        let payload = access.split('.').nth(1).expect("payload");
        let claims: serde_json::Value = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(payload)
                .expect("base64"),
        )
        .expect("claims");
        claims["jti"].as_str().expect("a minted session carries a jti").to_string()
    };
    revocation.revoke(&jti, 3600).await.expect("revoke");

    for (path, payload) in [
        ("/auth/v1/email/verify/start", serde_json::json!({})),
        ("/auth/v1/email/verify/confirm", serde_json::json!({ "token": "whatever" })),
    ] {
        let (status, body) = post_authed(base, path, &access, payload).await;
        assert_eq!(
            status,
            reqwest::StatusCode::UNAUTHORIZED,
            "{path} with a revoked session: {body}"
        );
    }

    server.stop().await;
    drop_scratch(&url, db).await;
}

/// The `/auth/v1/mfa/verify` budget per client, which `auth_local` fixes.
const VERIFY_BUDGET: usize = 50;

/// A server whose request limiter trusts forwarded addresses from `trusted_proxies`.
async fn boot_behind(trusted_proxies: &str, pool: PgPool, scratch_url: &str) -> RunningServer {
    let config = ServerConfig {
        rate_limiting: Some(fraiseql_server::middleware::RateLimitConfig {
            trust_proxy_headers: true,
            trusted_proxy_cidrs: vec![trusted_proxies.parse().expect("a CIDR")],
            ..fraiseql_server::middleware::RateLimitConfig::default()
        }),
        ..local_config(scratch_url)
    };
    let local = LocalAuthConfig {
        mfa: true,
        ..LocalAuthConfig::default()
    };
    boot_with(config, local, pool, scratch_url).await
}

/// One `/verify` guess forwarded for `client`.
async fn guess_as(base: &str, client: &str) -> reqwest::StatusCode {
    reqwest::Client::new()
        .post(format!("{base}/auth/v1/mfa/verify"))
        .header("x-forwarded-for", client)
        .header("x-real-ip", client)
        .json(&serde_json::json!({ "challenge_token": "unknown", "code": "000000" }))
        .send()
        .await
        .expect("request")
        .status()
}

/// Behind a trusted proxy every request arrives from the proxy, so the verify budget
/// keys on the client the proxy reports: one client spending its budget leaves another's.
#[tokio::test]
async fn the_verify_budget_is_per_client_behind_a_trusted_proxy() {
    let Some(url) = database_url_or_skip("the_verify_budget_is_per_client") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_mfa_proxy";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let server = boot_behind("127.0.0.0/8", pool, &scratch_url).await;

    for _ in 0..VERIFY_BUDGET {
        assert_eq!(
            guess_as(&server.base, "203.0.113.1").await,
            reqwest::StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(
        guess_as(&server.base, "203.0.113.1").await,
        reqwest::StatusCode::TOO_MANY_REQUESTS,
        "the first client has spent its budget"
    );
    assert_eq!(
        guess_as(&server.base, "203.0.113.2").await,
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "another client behind the same proxy has its own"
    );

    server.stop().await;
    drop_scratch(&url, db).await;
}

/// A forwarded address from a peer that is not a trusted proxy is a claim, not an
/// address: rotating it does not buy a client a fresh budget.
#[tokio::test]
async fn a_forwarded_address_from_an_untrusted_peer_buys_no_budget() {
    let Some(url) = database_url_or_skip("a_forwarded_address_from_an_untrusted_peer") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_mfa_spoof";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let server = boot_behind("10.0.0.0/8", pool, &scratch_url).await;

    for n in 0..VERIFY_BUDGET {
        let spoofed = format!("198.51.100.{}", n + 1);
        assert_eq!(
            guess_as(&server.base, &spoofed).await,
            reqwest::StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(
        guess_as(&server.base, "198.51.100.200").await,
        reqwest::StatusCode::TOO_MANY_REQUESTS,
        "the loopback peer is not a trusted proxy, so its forwarded addresses are ignored"
    );

    server.stop().await;
    drop_scratch(&url, db).await;
}

/// With MFA off, an account enrolled in it could not sign in: its sign-ins owe the second
/// factor, and nothing would take it. The server refuses to boot, and says what to do,
/// rather than lock the account out without a word.
#[cfg(feature = "inbound-email")]
#[tokio::test]
async fn mfa_off_with_confirmed_enrollments_refuses_to_boot() {
    let Some(url) = database_url_or_skip("mfa_off_with_confirmed_enrollments") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_local_mfa_off";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    fraiseql_auth::PgMfaStore::new(pool.clone()).init().await.unwrap();
    let password_only = || LocalAuthConfig {
        password: true,
        email_from: Some("support".to_string()),
        reset_url_template: Some("https://app.example.com/reset?token={token}".to_string()),
        ..LocalAuthConfig::default()
    };
    let boots = |pool: PgPool| async {
        let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.unwrap());
        Box::pin(Server::new(
            local_config(&scratch_url),
            local_schema(password_only()),
            adapter,
            Some(pool),
        ))
        .await
        .map(|_| ())
    };

    boots(pool.clone()).await.expect("no enrollment: MFA off boots");
    seed_confirmed_mfa(&pool, "enrolled-1").await;
    let refused = boots(pool.clone()).await.expect_err("a confirmed enrollment with MFA off");
    let message = refused.to_string();
    assert!(
        message.contains("1 account(s)") && message.contains("mfa = true"),
        "the refusal names the accounts and the way forward: {message}"
    );

    drop_scratch(&url, db).await;
}
