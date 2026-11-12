//! `[auth_issuer]`: the server signs the sessions it mints with ES256 keys of its own,
//! publishes them, and validates its own tokens as it would any issuer's.
//!
//! These tests boot the real server over a scratch database and check:
//!
//! - a sign-in mints an ES256 `at+jwt` naming a `kid` the published JWK set carries, which
//!   publishes public halves only, and `/graphql` admits it; an HS256 token is refused
//! - every token the server signs lives no longer than `[auth.session] ttl_secs`
//! - after a rotation, a token signed by the new key is admitted, and so is one signed by the old
//!   key, still published
//! - the admin JWKS refresh reaches the ring: a key rotated in after the ring last read is admitted
//!   once the operator asks for a refresh
//! - revoke-all ends the user's sessions: the refresh token no longer refreshes, and the access
//!   token is refused
//! - beside an external identity provider (`[auth]`), its tokens reach `/graphql` but not the
//!   routes acting on the caller's own account, which only the server's own sessions do
//!
//! Self-skips when no `DATABASE_URL` is set.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `fraiseql_first_party_*` databases →
//! run `--test-threads=1`.
#![cfg(all(feature = "auth", feature = "inbound-email"))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)]
// Reason: test code — panics and skip diagnostics are acceptable
// Reason: `{token}` in the URL template is FraiseQL's own placeholder, which the server
// substitutes — not a Rust format argument.
#![allow(clippy::literal_string_with_formatting_args)]

mod common;

use std::sync::Arc;

use common::first_party::{FirstPartyAuth, SESSION_TTL_SECS};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    schema::{AuthClientConfig, CompiledSchema, LocalAuthConfig},
};
use fraiseql_server::{
    Server,
    server_config::{AsyncOperationsConfig, ServerConfig},
    token_revocation::{InMemoryRevocationStore, TokenRevocationManager},
};
use fraiseql_test_support::{issuer::TestIssuer, try_database_url};
use reqwest::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

/// The variable `[auth.database]` names in this suite. Auth runs on the scratch
/// database's own URL here; `auth_database_e2e_pg` covers auth on a role of its own.
const AUTH_URL_ENV: &str = "FRAISEQL_TEST_FIRST_PARTY_AUTH_URL";
const SMTP_PASSWORD_ENV: &str = "FRAISEQL_TEST_FIRST_PARTY_SMTP_PASSWORD";
const ISSUER: &str = "https://sp.example.com";
const AUDIENCE: &str = "fraiseql-first-party-test";
/// How this server signs the sessions it mints.
const FIRST_PARTY: FirstPartyAuth = FirstPartyAuth::new(ISSUER, AUDIENCE);
const ADMIN_TOKEN: &str = "first-party-suite-admin-token-0123456789";
const PASSWORD: &str = "correct horse battery";

fn set_test_env() {
    FIRST_PARTY.set_env();
    std::env::set_var(SMTP_PASSWORD_ENV, "unused");
    // The playground is refused outside development mode, and the cookie tests reach it.
    std::env::set_var("FRAISEQL_ENV", "development");
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
    std::env::set_var(AUTH_URL_ENV, with_database(admin_url, db));
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

/// Password sign-in with `MFA` available, refreshable, signing its sessions.
fn schema() -> CompiledSchema {
    let mut schema: CompiledSchema = serde_json::from_value(json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
    }))
    .expect("compiled schema");
    schema.auth = Some(AuthClientConfig {
        pkce:     None,
        social:   None,
        local:    Some(LocalAuthConfig {
            password: true,
            mfa: true,
            email_from: Some("support".to_string()),
            reset_url_template: Some("https://app.example.com/reset?token={token}".to_string()),
            ..LocalAuthConfig::default()
        }),
        session:  Some(fraiseql_core::schema::SessionConfig {
            absolute_ttl_secs: Some(86_400),
            ..FIRST_PARTY.session()
        }),
        database: Some(fraiseql_core::schema::AuthDatabaseConfig::new(AUTH_URL_ENV)),
    });
    schema
}

/// The document this suite serves loads, checked with no database: every other test here
/// builds it only after it has found one, so a refusal of it would read as a skip.
#[test]
fn the_document_loads_without_a_database() {
    let _ = schema();
}

/// A `[mailbox.support]` whose `SMTP` half points at a closed port: the transport
/// builds, and nothing here sends.
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
    FIRST_PARTY.apply(ServerConfig {
        cors_enabled: false,
        database_url: scratch_url.to_string(),
        mailbox: unreachable_mailbox(),
        admin_api_enabled: true,
        admin_token: Some(ADMIN_TOKEN.to_string()),
        ..ServerConfig::default()
    })
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

async fn boot(config: ServerConfig, pool: PgPool, scratch_url: &str) -> RunningServer {
    let adapter = Arc::new(PostgresAdapter::new(scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let server = Box::pin(Server::new(config, schema(), adapter, Some(pool)))
        .await
        .expect("Server::new with [auth_issuer] must succeed")
        .with_revocation_manager(Arc::new(TokenRevocationManager::new(
            Arc::new(InMemoryRevocationStore::new()),
            false,
            false,
            3_600,
        )));
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

/// A scratch database and a server over it, configured by `config`.
async fn start_with(
    test: &str,
    db: &str,
    config: impl FnOnce(ServerConfig) -> ServerConfig,
) -> Option<(String, PgPool, RunningServer)> {
    let url = database_url_or_skip(test)?;
    set_test_env();
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let server = boot(config(server_config(&scratch_url)), pool.clone(), &scratch_url).await;
    Some((url, pool, server))
}

async fn start(test: &str, db: &str) -> Option<(String, PgPool, RunningServer)> {
    start_with(test, db, |config| config).await
}

async fn finish(url: &str, db: &str, server: RunningServer) {
    server.stop().await;
    drop_scratch(url, db).await;
}

async fn post_json(
    base: &str,
    path: &str,
    bearer: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = reqwest::Client::new().post(format!("{base}{path}")).json(&body);
    if let Some(bearer) = bearer {
        request = request.bearer_auth(bearer);
    }
    let resp = request.send().await.expect("request");
    let status = resp.status();
    let json = resp.json().await.unwrap_or(Value::Null);
    (status, json)
}

async fn get_json(base: &str, path: &str) -> (StatusCode, Value) {
    let resp = reqwest::get(format!("{base}{path}")).await.expect("request");
    let status = resp.status();
    (status, resp.json().await.unwrap_or(Value::Null))
}

/// What `/graphql` answers a holder of `access`: 401 when the token is not accepted.
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

struct Session {
    access:  String,
    refresh: String,
}

async fn signup(base: &str, email: &str) -> Session {
    let (status, body) = post_json(
        base,
        "/auth/v1/password/signup",
        None,
        json!({ "email": email, "password": PASSWORD }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "signup: {body}");
    Session {
        access:  body["access_token"].as_str().expect("access_token").to_string(),
        refresh: body["refresh_token"].as_str().expect("refresh_token").to_string(),
    }
}

/// One base64url segment of a JWT, as JSON.
fn segment(token: &str, index: usize) -> Value {
    use base64::Engine as _;
    let part = token.split('.').nth(index).expect("a JWT has three segments");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(part)
        .expect("base64url");
    serde_json::from_slice(&bytes).expect("a JSON segment")
}

fn kid_of(token: &str) -> String {
    segment(token, 0)["kid"].as_str().expect("a kid").to_string()
}

fn lifetime_of(token: &str) -> u64 {
    let claims = segment(token, 1);
    claims["exp"].as_u64().expect("exp") - claims["iat"].as_u64().expect("iat")
}

fn published_kids(jwks: &Value) -> Vec<String> {
    jwks["keys"]
        .as_array()
        .expect("a JWK set")
        .iter()
        .map(|key| key["kid"].as_str().expect("a kid").to_string())
        .collect()
}

/// Rotate the ring in `pool`'s database, as another replica would.
async fn rotate(pool: &PgPool) -> String {
    let wrap = fraiseql_server::first_party::SealedWithFieldEncryption::new(&[7; 32])
        .expect("a 32-byte key-encryption key");
    let store = fraiseql_auth::PgSigningKeyStore::new(pool.clone(), Arc::new(wrap));
    let now = fraiseql_server::first_party::unix_now().expect("the clock");
    store.rotate(now).await.expect("rotate")
}

/// A token for `sub` signed with the ring's active key, as the server would sign it.
async fn signed_now(pool: &PgPool, sub: &str) -> String {
    let now = chrono::Utc::now().timestamp();
    FIRST_PARTY
        .sign(
            pool,
            &json!({
                "sub": sub, "iss": ISSUER, "aud": [AUDIENCE],
                "iat": now, "nbf": now, "exp": now + 600, "jti": uuid::Uuid::new_v4().to_string(),
            }),
        )
        .await
}

#[tokio::test]
async fn a_sign_in_mints_an_es256_token_under_a_published_key_and_graphql_admits_it() {
    let db = "fraiseql_first_party_sign_in";
    let Some((url, _pool, server)) = start("a_sign_in_mints_an_es256_token", db).await else {
        return;
    };
    let base = &server.base;

    let session = signup(base, "es256@example.com").await;
    let header = segment(&session.access, 0);
    assert_eq!(header["alg"], "ES256", "{header}");
    assert_eq!(header["typ"], "at+jwt", "{header}");
    let claims = segment(&session.access, 1);
    assert_eq!(claims["iss"], ISSUER, "{claims}");
    assert_eq!(claims["nbf"], claims["iat"], "{claims}");

    let (status, discovery) = get_json(base, "/.well-known/openid-configuration").await;
    assert_eq!(status, StatusCode::OK, "{discovery}");
    assert_eq!(discovery["issuer"], ISSUER, "{discovery}");
    assert_eq!(discovery["jwks_uri"], format!("{ISSUER}/.well-known/jwks.json"), "{discovery}");
    let (status, jwks) = get_json(base, "/.well-known/jwks.json").await;
    assert_eq!(status, StatusCode::OK, "{jwks}");
    assert!(published_kids(&jwks).contains(&kid_of(&session.access)), "{jwks}");
    for key in jwks["keys"].as_array().expect("keys") {
        assert!(key.get("d").is_none(), "a published key carries no private half: {key}");
        assert_eq!(key["alg"], "ES256", "{key}");
    }

    assert_eq!(probe(base, &session.access).await, StatusCode::OK, "its own token is admitted");

    finish(&url, db, server).await;
}

#[tokio::test]
async fn an_hs256_token_is_refused() {
    let db = "fraiseql_first_party_hs256";
    let Some((url, _pool, server)) = start("an_hs256_token_is_refused", db).await else {
        return;
    };
    let now = chrono::Utc::now().timestamp();
    let hs256 = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &json!({
            "sub": "someone", "iss": ISSUER, "aud": [AUDIENCE],
            "iat": now, "nbf": now, "exp": now + 600,
        }),
        &jsonwebtoken::EncodingKey::from_secret(b"any-hs256-secret-of-thirty-two-b"),
    )
    .expect("sign");

    assert_eq!(probe(&server.base, &hs256).await, StatusCode::UNAUTHORIZED);

    finish(&url, db, server).await;
}

#[tokio::test]
async fn no_token_the_server_signs_outlives_the_session_lifetime() {
    let db = "fraiseql_first_party_lifetime";
    let Some((url, _pool, server)) = start("no_token_outlives_the_lifetime", db).await else {
        return;
    };
    let base = &server.base;

    let session = signup(base, "lifetime@example.com").await;
    assert_eq!(lifetime_of(&session.access), SESSION_TTL_SECS, "the sign-in");
    let (status, body) = post_json(
        base,
        "/auth/v1/session/refresh",
        None,
        json!({ "refresh_token": session.refresh }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "refresh: {body}");
    let refreshed = body["access_token"].as_str().expect("access_token");
    assert_eq!(lifetime_of(refreshed), SESSION_TTL_SECS, "the refresh");

    finish(&url, db, server).await;
}

#[tokio::test]
async fn after_a_rotation_tokens_of_the_new_and_the_old_key_are_admitted() {
    let db = "fraiseql_first_party_rotation";
    let Some((url, pool, server)) = start("after_a_rotation", db).await else {
        return;
    };
    let base = &server.base;
    let before = signup(base, "rotation@example.com").await;
    let sub = segment(&before.access, 1)["sub"].as_str().expect("sub").to_string();

    let rotated = rotate(&pool).await;
    let after = signed_now(&pool, &sub).await;
    assert_eq!(kid_of(&after), rotated, "signed by the rotated-in key");

    assert_eq!(probe(base, &after).await, StatusCode::OK, "the new key's token");
    assert_eq!(
        probe(base, &before.access).await,
        StatusCode::OK,
        "the old key's, still published"
    );
    let (_, jwks) = get_json(base, "/.well-known/jwks.json").await;
    let kids = published_kids(&jwks);
    assert!(kids.contains(&rotated) && kids.contains(&kid_of(&before.access)), "{jwks}");

    finish(&url, db, server).await;
}

#[tokio::test]
async fn the_admin_jwks_refresh_reaches_the_ring() {
    let db = "fraiseql_first_party_admin_refresh";
    let Some((url, pool, server)) = start("the_admin_jwks_refresh", db).await else {
        return;
    };
    let base = &server.base;
    let sub = "admin-refresh-user";

    // The first unknown `kid` re-reads the ring; the next, within the cooldown, does not.
    rotate(&pool).await;
    assert_eq!(
        probe(base, &signed_now(&pool, sub).await).await,
        StatusCode::OK,
        "first rotation"
    );
    rotate(&pool).await;
    let second = signed_now(&pool, sub).await;
    assert_eq!(
        probe(base, &second).await,
        StatusCode::UNAUTHORIZED,
        "within the cooldown, an unknown kid does not re-read the ring"
    );

    let (status, body) =
        post_json(base, "/admin/v1/auth/refresh-jwks", Some(ADMIN_TOKEN), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(probe(base, &second).await, StatusCode::OK, "the refresh re-read the ring");

    finish(&url, db, server).await;
}

#[tokio::test]
async fn revoke_all_ends_the_users_sessions_and_refuses_their_access_token() {
    let db = "fraiseql_first_party_revoke_all";
    let Some((url, _pool, server)) = start("revoke_all_ends_the_sessions", db).await else {
        return;
    };
    let base = &server.base;
    let session = signup(base, "revoked@example.com").await;
    let sub = segment(&session.access, 1)["sub"].as_str().expect("sub").to_string();

    let (status, body) =
        post_json(base, "/auth/revoke-all", Some(&session.access), json!({ "sub": sub })).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = post_json(
        base,
        "/auth/v1/session/refresh",
        None,
        json!({ "refresh_token": session.refresh }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "an ended session does not refresh: {body}");
    assert!(body.get("access_token").is_none(), "{body}");
    assert_eq!(probe(base, &session.access).await, StatusCode::UNAUTHORIZED, "the access token");

    finish(&url, db, server).await;
}

#[tokio::test]
async fn an_external_providers_token_reaches_graphql_but_not_the_account_routes() {
    let db = "fraiseql_first_party_external";
    let external = TestIssuer::start().await;
    let oidc = common::test_issuer::oidc_config(&external, true);
    let Some((url, _pool, server)) =
        start_with("an_external_providers_token", db, |config| ServerConfig {
            auth: Some(oidc),
            ..config
        })
        .await
    else {
        return;
    };
    let base = &server.base;
    let enroll = json!({ "account_name": "someone@example.com" });

    let own = signup(base, "own@example.com").await;
    let (status, body) =
        post_json(base, "/auth/v1/mfa/enroll", Some(&own.access), enroll.clone()).await;
    assert_eq!(status, StatusCode::OK, "the server's own session enrolls: {body}");

    let sub = segment(&own.access, 1)["sub"].as_str().expect("sub").to_string();
    let foreign = external.mint(&external.claims().set("sub", sub));
    assert_eq!(
        probe(base, &foreign).await,
        StatusCode::OK,
        "the provider's token reaches /graphql"
    );
    let (status, body) = post_json(base, "/auth/v1/mfa/enroll", Some(&foreign), enroll).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "but not an account route: {body}");

    finish(&url, db, server).await;
}

// ── The access cookie on the OIDC mounts ────────────────────────────────────────

/// `config` with every OIDC mount that may take the access cookie behind authentication.
fn every_cookie_mount(config: ServerConfig) -> ServerConfig {
    ServerConfig {
        async_operations: Some(AsyncOperationsConfig {
            operations: vec!["noSuchOperation".to_string()],
            ..AsyncOperationsConfig::default()
        }),
        playground_enabled: true,
        playground_require_auth: Some(true),
        introspection_enabled: true,
        introspection_require_auth: true,
        design_api_require_auth: true,
        ..config
    }
}

/// A mount the access cookie may authenticate: what it is, its method, path and body.
type Mount = (&'static str, &'static str, &'static str, &'static str);

/// Every OIDC mount that may take the cookie, but `/auth/me` (mounted only with `[auth]`).
/// Last: revoke-all, after which every earlier token of the caller is refused.
///
/// A mount answering 401 to the cookie is mounted: a path nothing serves answers 404. That
/// is checked first, because a handler past authentication may answer 404 itself (an
/// operation that does not exist).
const COOKIE_MOUNTS: [Mount; 10] = [
    ("GraphQL GET", "GET", "/graphql?query=%7B__typename%7D", ""),
    ("GraphQL POST", "POST", "/graphql", r#"{"query":"{__typename}"}"#),
    (
        "async operations",
        "GET",
        "/operations/v1/00000000-0000-0000-0000-000000000000",
        "",
    ),
    ("the playground", "GET", "/playground", ""),
    ("introspection", "GET", "/introspection", ""),
    ("schema export", "GET", "/api/v1/schema.json", ""),
    ("schema metadata", "GET", "/api/v1/schema/metadata", ""),
    ("the design audit", "POST", "/api/v1/design/audit", "{}"),
    ("/auth/revoke", "POST", "/auth/revoke", "{}"),
    ("/auth/revoke-all", "POST", "/auth/revoke-all", "SUB"),
];

/// How a request presents its token.
enum Carrier {
    Cookie,
    Bearer,
}

/// `mount`'s answer to `token` for `sub`, carried by `carrier`.
async fn call(base: &str, mount: Mount, sub: &str, token: &str, carrier: Carrier) -> StatusCode {
    let (_, method, path, body) = mount;
    let url = format!("{base}{path}");
    let client = reqwest::Client::new();
    let mut request = if method == "GET" {
        client.get(url)
    } else {
        let body = if body == "SUB" {
            json!({ "sub": sub }).to_string()
        } else {
            body.to_string()
        };
        client.post(url).header("content-type", "application/json").body(body)
    };
    request = match carrier {
        Carrier::Cookie => request.header("cookie", format!("__Host-access_token={token}")),
        Carrier::Bearer => request.bearer_auth(token),
    };
    request.send().await.expect("request").status()
}

/// In `json` transport the server never sets the cookie with one of its own sessions, so
/// no mount takes one there; the same token as a bearer is admitted on each.
#[tokio::test]
async fn in_json_transport_no_mount_takes_a_session_of_its_own_from_the_cookie() {
    let db = "fraiseql_first_party_cookie";
    let Some((url, pool, server)) =
        start_with("no_mount_takes_a_session_from_the_cookie", db, every_cookie_mount).await
    else {
        return;
    };
    let base = &server.base;
    let sub = "cookie-user";

    for mount in COOKIE_MOUNTS {
        let what = mount.0;
        let token = signed_now(&pool, sub).await;
        let status = call(base, mount, sub, &token, Carrier::Cookie).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{what}: a session of its own in the cookie");
        let token = signed_now(&pool, sub).await;
        let status = call(base, mount, sub, &token, Carrier::Bearer).await;
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{what}: the same as a bearer");
    }

    finish(&url, db, server).await;
}

/// Beside an external provider, whose sign-in sets the cookie, every mount that took its
/// cookie before the first-party issuer existed still does; a session of the server's own
/// in it is refused, in `json` transport, as without `[auth]`.
#[tokio::test]
async fn beside_an_external_provider_its_cookie_counts_and_a_session_of_its_own_does_not() {
    let db = "fraiseql_first_party_cookie_external";
    let external = TestIssuer::start().await;
    let mut oidc = common::test_issuer::oidc_config(&external, true);
    oidc.me = Some(fraiseql_core::security::oidc::MeEndpointConfig {
        enabled:       true,
        expose_claims: Vec::new(),
    });
    let Some((url, pool, server)) =
        start_with("beside_an_external_provider_its_cookie", db, |config| {
            every_cookie_mount(ServerConfig {
                auth: Some(oidc),
                ..config
            })
        })
        .await
    else {
        return;
    };
    let base = &server.base;
    let sub = "cookie-user";
    let me: Mount = ("/auth/me", "GET", "/auth/me", "");

    for mount in std::iter::once(me).chain(COOKIE_MOUNTS) {
        let what = mount.0;
        let own = signed_now(&pool, sub).await;
        let status = call(base, mount, sub, &own, Carrier::Cookie).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{what}: a session of its own in the cookie");
        let theirs = external.mint(&external.claims().set("sub", sub));
        let status = call(base, mount, sub, &theirs, Carrier::Cookie).await;
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{what}: the provider's cookie");
    }

    finish(&url, db, server).await;
}
