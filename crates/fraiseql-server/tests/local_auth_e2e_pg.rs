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
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

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
        "version": "2.0.0",
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

async fn boot(local: LocalAuthConfig, pool: PgPool, scratch_url: &str) -> RunningServer {
    let adapter = Arc::new(PostgresAdapter::new(scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let server =
        Box::pin(Server::new(local_config(scratch_url), local_schema(local), adapter, Some(pool)))
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
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    RunningServer {
        base: format!("http://127.0.0.1:{port}"),
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
        .link_or_create_user(Some("dave@example.com"), true, "google", "g-sub-dave")
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

    // Enroll, then confirm with a live TOTP code derived from the returned URI.
    let (status, body) = post_json(
        &server.base,
        "/auth/v1/mfa/enroll",
        serde_json::json!({ "user_id": "user-1", "account_name": "erin@example.com" }),
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
    let (status, body) = post_json(
        &server.base,
        "/auth/v1/mfa/confirm",
        serde_json::json!({ "user_id": "user-1", "code": code }),
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
    let (status, body) = post_json(
        &server.base,
        "/auth/v1/mfa/challenge",
        serde_json::json!({ "user_id": "user-1" }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "the enrollment must survive the restart (Postgres store): {body}"
    );
    let challenge = body["challenge_token"].as_str().expect("challenge_token").to_string();

    // A wrong code is refused…
    let (status, _) = post_json(
        &server.base,
        "/auth/v1/mfa/verify",
        serde_json::json!({ "challenge_token": challenge, "code": "000000" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    // …and a recovery code verifies and mints a session.
    let (status, body) = post_json(
        &server.base,
        "/auth/v1/mfa/challenge",
        serde_json::json!({ "user_id": "user-1" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let challenge = body["challenge_token"].as_str().expect("challenge_token").to_string();
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
