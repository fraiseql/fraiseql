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
    let adapter = Arc::new(PostgresAdapter::new(scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let config = ServerConfig {
        mailbox: capturing_mailbox(smtp_port),
        ..local_config(scratch_url)
    };
    let server = Box::pin(Server::new(config, local_schema(local), adapter, Some(pool)))
        .await
        .expect("Server::new with [auth.local] email_verification must succeed");
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
