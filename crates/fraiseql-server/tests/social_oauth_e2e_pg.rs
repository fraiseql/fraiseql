//! #368: social login through the shipped server's mount, against a stub `IdP`.
//!
//! The library slice shipped the trust-gated `multi_provider` flow, but nothing
//! in the binary mounted it — `with_social_login` had zero callers and
//! `[auth.social]` could not even be typed. These tests boot the real server
//! from a compiled `[auth.social]` block and prove the whole loop against a
//! local stub `IdP`:
//!
//! - `Google` (`OIDC`): authorize redirect → callback → token exchange → userinfo → verified-email
//!   account linking → session tokens signed with the server's own ES256 keys.
//! - `GitHub` (plain `OAuth2`): the token endpoint answers JSON only when `Accept:
//!   application/json` is sent, carries **no** `expires_in`, and `/user` hides the email — the
//!   `/user/emails` second hop resolves the primary verified address and links by email (#368's
//!   `GitHub` gap).
//! - `Apple` (#943): the client secret is an ES256 assertion the runtime mints, there is no
//!   userinfo endpoint (the identity is the token endpoint's `id_token`), and the callback arrives
//!   as a form `POST`. The suite proves the POST mount, that a second sign-in carrying no name or
//!   email still resolves to the same account, and — the security-load-bearing one — that the
//!   browser-supplied `user` payload's email never reaches the email-keyed linking space.
//! - `Discord` and `Facebook` (#944, both plain `OAuth2`): the security-load-bearing case for each
//!   — `Discord` reports `verified: false` and `Facebook` reports no verification signal at all, so
//!   neither address may key an account. The provider's own half of that (it reports the flag
//!   verbatim / never claims verification) is pinned by unit tests, because the trust gate is a
//!   second, independent belt that would mask a broken provider here.
//! - The `/auth/v1/authorize` flood faces the `auth_start` path bucket (#788).
//! - Bogus state and unknown providers are refused.
//! - How the session reaches the browser: a `JSON` body with no redirect allow-list, the redirect's
//!   fragment with one, and no session cookie either way.
//! - A login completes only from the browser that started it, by the cookie `authorize` sets.
//! - In cookie transport the callback sets the session cookies on its redirect, and the session
//!   they hold works on `/graphql`, refreshes and logs out by cookie alone; with no redirect
//!   allow-list the boot refuses it.
//!
//! Self-skips when no `DATABASE_URL` is set. The stub `IdP` lives on loopback
//! `HTTP`, so the suite opts into the development SSRF bypass
//! (`FRAISEQL_ENV=development` + `FRAISEQL_OIDC_ALLOW_INSECURE=1`) —
//! process-wide, fixed valid values.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `fraiseql_p26_social_*` databases →
//! run `--test-threads=1`.
#![cfg(feature = "auth")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

mod common;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    Json, Router,
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
};
use common::first_party::{FirstPartyAuth, SESSION_TTL_SECS};
use fraiseql_auth::AccountStore as _;
use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::try_database_url;
use sqlx::PgPool;

/// The variable `[auth.database]` names in this suite (#1965). Auth runs on the scratch
/// database's own URL here; `auth_database_e2e_pg` covers auth on a role of its own.
const AUTH_URL_ENV: &str = "FRAISEQL_TEST_SOCIAL_OAUTH_AUTH_URL";
/// How this server signs the sessions it mints.
const FIRST_PARTY: FirstPartyAuth = FirstPartyAuth::new(ISSUER, AUDIENCE);

fn auth_database() -> fraiseql_core::schema::AuthDatabaseConfig {
    fraiseql_core::schema::AuthDatabaseConfig::new(AUTH_URL_ENV)
}

const GOOGLE_SECRET_ENV: &str = "FRAISEQL_TEST_P26_SOCIAL_E2E_GOOGLE";
const GITHUB_SECRET_ENV: &str = "FRAISEQL_TEST_P26_SOCIAL_E2E_GITHUB";
const APPLE_KEY_ENV: &str = "FRAISEQL_TEST_P26_SOCIAL_E2E_APPLE_P8";
const DISCORD_SECRET_ENV: &str = "FRAISEQL_TEST_P14_SOCIAL_E2E_DISCORD";
const FACEBOOK_SECRET_ENV: &str = "FRAISEQL_TEST_P14_SOCIAL_E2E_FACEBOOK";

/// A non-default Graph version, so a provider that hard-codes one shows up as a
/// 404 against the stub rather than passing by coincidence.
const FACEBOOK_API_VERSION: &str = "v22.0";
/// Discord reports this address as **unverified** — it must stay out of the
/// email-keyed linking space.
const DISCORD_UNVERIFIED_EMAIL: &str = "discord-unverified@example.com";
/// Facebook hands this back with no verification signal of any kind — likewise.
const FACEBOOK_EMAIL: &str = "facebook-user@example.com";

/// Apple's services ID in this suite — the `client_id`, and the `aud` every
/// stub `id_token` must name.
const APPLE_CLIENT_ID: &str = "com.example.fraiseql.service";
/// Apple subject, stable across sign-ins the way a real `sub` is.
const APPLE_SUB: &str = "001999.deadbeef.0000";
/// The address Apple itself asserts, in the `id_token`.
const APPLE_EMAIL: &str = "apple-user@example.com";
/// The address a hostile client puts in the browser-posted `user` payload.
/// Nothing may ever key on it.
const APPLE_FORGED_EMAIL: &str = "victim@example.com";

/// A throwaway P-256 key, generated for this suite and used nowhere else.
/// Apple issues a PKCS#8 PEM `.p8`, which is exactly this shape.
const TEST_P8: &str = "-----BEGIN PRIVATE KEY-----\n\
    MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg6mOJdB87DG8anytc\n\
    jGCaH3eI4OkrTGRc6sZGu1DqyiGhRANCAARKwlK3b7SIes76KDfwwP1Dxf3OgSsa\n\
    dTtT/3rfS3QYTqqyzOH6LW51mUpy3vxAi/IKx1oEdLAJzOCm1Z1p5wFw\n\
    -----END PRIVATE KEY-----\n";

/// Process-wide test environment. Fixed valid values, set unconditionally:
/// safe under the parallel runner because every reader wants exactly these.
fn set_test_env() {
    std::env::set_var("FRAISEQL_ENV", "development");
    std::env::set_var("FRAISEQL_OIDC_ALLOW_INSECURE", "1");
    FIRST_PARTY.set_env();
    std::env::set_var(GOOGLE_SECRET_ENV, "google-client-secret");
    std::env::set_var(GITHUB_SECRET_ENV, "github-client-secret");
    std::env::set_var(APPLE_KEY_ENV, TEST_P8);
    std::env::set_var(DISCORD_SECRET_ENV, "discord-client-secret");
    std::env::set_var(FACEBOOK_SECRET_ENV, "facebook-client-secret");
}

fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

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

// ── Stub IdP ─────────────────────────────────────────────────────────────────

/// Build an `id_token` with a placeholder signature.
///
/// The provider deliberately does not re-verify the signature of a token the
/// token endpoint handed it over TLS (OIDC Core §3.1.3.7 rule 6), so a
/// claims-only token is exactly what the production parser sees. What it *does*
/// check — `iss`, `aud`, `exp` — is real here, and the refusal cases are
/// covered by the provider's unit tests.
/// How many times the stub `IdP` served the `/user/teams` endpoint.
static TEAMS_REQUESTS: AtomicUsize = AtomicUsize::new(0);

fn unsigned_jwt(claims: &serde_json::Value) -> String {
    use base64::Engine as _;
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let header = engine.encode(br#"{"alg":"ES256","typ":"JWT"}"#);
    let payload = engine.encode(serde_json::to_vec(claims).expect("claims serialize"));
    format!("{header}.{payload}.stub-signature")
}

/// Boot a stub `IdP` serving every configured provider's surface — `Google`'s
/// `OIDC` shape, and the plain-`OAuth2` shapes of `Apple`, `Discord`, `Facebook`
/// and `GitHub`. Returns its base URL.
async fn stub_idp() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind stub idp");
    let base = format!("http://127.0.0.1:{}", listener.local_addr().expect("addr").port());

    // Apple hands out the identity in an id_token and nowhere else. The first
    // exchange carries the email claims; every later one carries only `sub`,
    // which is the shape a real second sign-in has — and the reason the account
    // must already be resolvable from `(apple, sub)` alone.
    let apple_issuer = base.clone();
    let apple_exchanges = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let discovery_base = base.clone();
    let app = Router::new()
        // Apple-shaped surface: token endpoint only — no userinfo exists.
        .route(
            "/auth/token",
            post(move || {
                let iss = apple_issuer.clone();
                let seen = Arc::clone(&apple_exchanges);
                async move {
                    let first = seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
                    let exp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("clock")
                        .as_secs()
                        + 3600;
                    let mut claims = serde_json::json!({
                        "iss": iss,
                        "aud": APPLE_CLIENT_ID,
                        "sub": APPLE_SUB,
                        "exp": exp,
                    });
                    if first {
                        claims["email"] = serde_json::json!(APPLE_EMAIL);
                        claims["email_verified"] = serde_json::json!("true");
                    }
                    Json(serde_json::json!({
                        "access_token": "apple-at",
                        "token_type": "Bearer",
                        "expires_in": 3600,
                        "id_token": unsigned_jwt(&claims),
                    }))
                }
            }),
        )
        // Discord-shaped surface. The user object carries `verified` — this
        // account's is false, which is the case that must not link on email.
        .route(
            "/api/oauth2/token",
            post(|| async {
                Json(serde_json::json!({
                    "access_token": "discord-at",
                    "token_type": "Bearer",
                    "expires_in": 604_800,
                    "refresh_token": "discord-rt",
                    "scope": "identify email"
                }))
            }),
        )
        .route(
            "/api/users/@me",
            get(|| async {
                Json(serde_json::json!({
                    "id": "776655443322110099",
                    "username": "carol",
                    "global_name": "Carol",
                    "email": DISCORD_UNVERIFIED_EMAIL,
                    "verified": false,
                    "avatar": null
                }))
            }),
        )
        // Facebook-shaped surface, under the configured API version — a
        // provider that hard-coded a version would 404 here.
        .route(
            &format!("/{FACEBOOK_API_VERSION}/oauth/access_token"),
            post(|| async {
                Json(serde_json::json!({
                    "access_token": "facebook-at",
                    "token_type": "bearer",
                    "expires_in": 5_183_944
                }))
            }),
        )
        .route(
            &format!("/{FACEBOOK_API_VERSION}/me"),
            get(|| async {
                // An address, and no verification signal anywhere — Facebook
                // publishes none.
                Json(serde_json::json!({
                    "id": "10223344556677889",
                    "name": "Dave",
                    "email": FACEBOOK_EMAIL
                }))
            }),
        )
        // Google-shaped OIDC surface.
        .route(
            "/.well-known/openid-configuration",
            get(move || {
                let b = discovery_base.clone();
                async move {
                    Json(serde_json::json!({
                        "issuer": b,
                        "authorization_endpoint": format!("{b}/authorize"),
                        "token_endpoint": format!("{b}/token"),
                        "userinfo_endpoint": format!("{b}/userinfo"),
                        "jwks_uri": format!("{b}/jwks"),
                    }))
                }
            }),
        )
        .route(
            "/token",
            post(|| async {
                Json(serde_json::json!({
                    "access_token": "google-at",
                    "expires_in": 3600,
                    "token_type": "Bearer"
                }))
            }),
        )
        .route(
            "/userinfo",
            get(|| async {
                Json(serde_json::json!({
                    "sub": "g-sub-1",
                    "email": "alice@example.com",
                    "email_verified": true,
                    "name": "Alice"
                }))
            }),
        )
        // GitHub-shaped plain-OAuth2 surface.
        .route(
            "/login/oauth/access_token",
            post(|headers: HeaderMap| async move {
                // Real GitHub answers form-encoded unless JSON is requested —
                // a client that forgets the Accept header must fail loudly
                // here rather than silently pass a lenient stub.
                let accepts_json = headers
                    .get("accept")
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| v.contains("application/json"));
                if !accepts_json {
                    return (
                        axum::http::StatusCode::NOT_ACCEPTABLE,
                        "stub: Accept: application/json required (GitHub answers \
                         form-encoded otherwise)",
                    )
                        .into_response();
                }
                // No expires_in — GitHub OAuth app tokens do not expire.
                Json(serde_json::json!({
                    "access_token": "gh-at",
                    "token_type": "bearer",
                    "scope": "read:user,user:email"
                }))
                .into_response()
            }),
        )
        .route(
            "/user",
            get(|| async {
                // email: null — the private-email GitHub account shape.
                Json(serde_json::json!({
                    "id": 42,
                    "login": "bob",
                    "email": null,
                    "name": "Bob",
                    "avatar_url": null,
                    "bio": null,
                    "company": null,
                    "location": null,
                    "public_repos": 3
                }))
            }),
        )
        .route(
            "/user/emails",
            get(|| async {
                Json(serde_json::json!([
                    { "email": "secondary@example.com", "primary": false, "verified": true },
                    { "email": "bob@example.com", "primary": true, "verified": true }
                ]))
            }),
        )
        .route(
            "/user/teams",
            get(|| async {
                TEAMS_REQUESTS.fetch_add(1, Ordering::SeqCst);
                Json(serde_json::json!([]))
            }),
        );

    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service()).await.unwrap();
    });
    base
}

// ── Server fixture ───────────────────────────────────────────────────────────

fn empty_schema() -> CompiledSchema {
    serde_json::from_value(serde_json::json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
    }))
    .expect("compiled schema")
}

fn social_schema(stub_base: &str) -> CompiledSchema {
    let mut schema = empty_schema();
    schema.auth = Some(fraiseql_core::schema::AuthClientConfig {
        pkce:     None,
        local:    None,
        session:  Some(FIRST_PARTY.session()),
        database: Some(auth_database()),
        social:   Some(fraiseql_core::schema::SocialAuthConfig {
            redirect_uri_allowlist: Vec::new(),
            google:                 Some(fraiseql_core::schema::GoogleSocialConfig {
                client_id:         "google-client".to_string(),
                client_secret_env: GOOGLE_SECRET_ENV.to_string(),
                redirect_uri:      "https://app.example.com/auth/v1/callback".to_string(),
                discovery_url:     Some(stub_base.to_string()),
            }),
            github:                 Some(fraiseql_core::schema::GitHubSocialConfig {
                client_id:         "github-client".to_string(),
                client_secret_env: GITHUB_SECRET_ENV.to_string(),
                redirect_uri:      "https://app.example.com/auth/v1/callback".to_string(),
                base_url:          Some(stub_base.to_string()),
                api_base_url:      Some(stub_base.to_string()),
            }),
            apple:                  Some(fraiseql_core::schema::AppleSocialConfig {
                client_id:        APPLE_CLIENT_ID.to_string(),
                team_id:          "TEAM123456".to_string(),
                key_id:           "KEY7890AB".to_string(),
                private_key_env:  Some(APPLE_KEY_ENV.to_string()),
                private_key_path: None,
                redirect_uri:     "https://app.example.com/auth/v1/callback".to_string(),
                base_url:         Some(stub_base.to_string()),
            }),
            discord:                Some(fraiseql_core::schema::DiscordSocialConfig {
                client_id:         "discord-client".to_string(),
                client_secret_env: DISCORD_SECRET_ENV.to_string(),
                redirect_uri:      "https://app.example.com/auth/v1/callback".to_string(),
                base_url:          Some(stub_base.to_string()),
            }),
            facebook:               Some(fraiseql_core::schema::FacebookSocialConfig {
                client_id:         "facebook-client".to_string(),
                client_secret_env: FACEBOOK_SECRET_ENV.to_string(),
                redirect_uri:      "https://app.example.com/auth/v1/callback".to_string(),
                api_version:       Some(FACEBOOK_API_VERSION.to_string()),
                base_url:          Some(stub_base.to_string()),
                graph_base_url:    Some(stub_base.to_string()),
            }),
        }),
    });
    schema
}

/// The operator-chosen `[auth_issuer]` issuer/audience — deliberately NOT the session
/// store's historical hard-coded defaults (`fraiseql` / `fraiseql-api`), so the
/// claims assertion below catches a mint/validate mismatch: a session minted
/// with claims the configured validator rejects is a login that "succeeds" and
/// then 401s on every request.
const ISSUER: &str = "https://sp.example.com";
const AUDIENCE: &str = "fraiseql-api-test-audience";

fn social_config(scratch_url: &str) -> ServerConfig {
    FIRST_PARTY.apply(ServerConfig {
        cors_enabled: false,
        database_url: scratch_url.to_string(),
        ..ServerConfig::default()
    })
}

/// Boot the server on an ephemeral port; returns (base URL, shutdown sender, join handle).
async fn boot_server(
    config: ServerConfig,
    schema: CompiledSchema,
    pool: PgPool,
    scratch_url: &str,
) -> (String, tokio::sync::oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let adapter = Arc::new(PostgresAdapter::new(scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let server = Box::pin(Server::new(config, schema, adapter, Some(pool)))
        .await
        .expect("Server::new with [auth.social] + [auth_issuer] + pool must succeed");
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _ = server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    (format!("http://127.0.0.1:{port}"), tx, handle)
}

fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client")
}

/// Extract the `state` query parameter from an authorize redirect Location.
fn state_from_location(location: &str) -> String {
    let url = reqwest::Url::parse(location).expect("location parses");
    url.query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .expect("authorize redirect carries a state")
}

/// The `name=value` of the login cookie `authorize` set on `resp`, which the browser that
/// started the login sends back to the callback.
fn login_cookie(resp: &reqwest::Response) -> String {
    resp.headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("__Host-fraiseql_login_"))
        .and_then(|v| v.split(';').next())
        .expect("authorize sets the login cookie")
        .to_string()
}

/// Decode a JWT's claims without validating it (test-side peek).
fn jwt_claims(token: &str) -> serde_json::Value {
    use base64::Engine as _;
    let payload = token.split('.').nth(1).expect("JWT has a payload");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("payload decodes");
    serde_json::from_slice(&bytes).expect("claims parse")
}

/// Decode the `sub` claim from a JWT, asserting the minted issuer/audience
/// match the configured `[auth_issuer]` — the claims this server's own
/// validator demands. A mismatch is the "login succeeds, then every request
/// 401s" defect.
fn jwt_sub(token: &str) -> String {
    let claims = jwt_claims(token);
    assert_eq!(
        claims["iss"].as_str(),
        Some(ISSUER),
        "minted session must carry the configured issuer: {claims}"
    );
    assert!(
        claims["aud"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(AUDIENCE))),
        "minted session must carry the configured audience: {claims}"
    );
    claims["sub"].as_str().expect("sub claim").to_string()
}

/// `exp − iat` of a minted access token.
fn lifetime_of(token: &str) -> u64 {
    let claims = jwt_claims(token);
    claims["exp"].as_u64().expect("exp") - claims["iat"].as_u64().expect("iat")
}

/// Drive `provider` through authorize → callback; returns the token-response JSON.
async fn login(base: &str, provider: &str) -> serde_json::Value {
    let (status, body) = sign_in(base, provider).await;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "callback must complete the login ({provider}): {body}"
    );
    body
}

/// Drive `provider` through authorize → callback; returns the callback's answer.
async fn sign_in(base: &str, provider: &str) -> (reqwest::StatusCode, serde_json::Value) {
    let client = no_redirect_client();
    let resp = client
        .get(format!(
            "{base}/auth/v1/authorize?provider={provider}&redirect_uri=https://app.example.com/cb"
        ))
        .send()
        .await
        .expect("authorize request");
    assert!(
        resp.status().is_redirection(),
        "authorize must redirect to the provider, got {} ({provider})",
        resp.status()
    );
    let cookie = login_cookie(&resp);
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let state = state_from_location(&location);

    let resp = client
        .get(format!("{base}/auth/v1/callback?code=any-code&state={state}"))
        .header(reqwest::header::COOKIE, cookie)
        .send()
        .await
        .expect("callback request");
    let status = resp.status();
    (status, resp.json().await.unwrap_or(serde_json::Value::Null))
}

/// Drive Apple through authorize → **form-POST** callback; returns the
/// token-response JSON. `user_payload` is the browser-supplied first-auth field.
async fn apple_login(base: &str, user_payload: Option<&str>) -> serde_json::Value {
    let client = no_redirect_client();
    let resp = client
        .get(format!(
            "{base}/auth/v1/authorize?provider=apple&redirect_uri=https://app.example.com/cb"
        ))
        .send()
        .await
        .expect("authorize request");
    assert!(resp.status().is_redirection(), "authorize must redirect to Apple");
    let cookie = login_cookie(&resp);
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        location.contains("response_mode=form_post"),
        "Apple's authorize URL must ask for the form-POST callback: {location}"
    );
    let state = state_from_location(&location);

    let mut form = vec![("code", "apple-code".to_string()), ("state", state)];
    if let Some(user) = user_payload {
        form.push(("user", user.to_string()));
    }
    let resp = client
        .post(format!("{base}/auth/v1/callback"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&form)
        .send()
        .await
        .expect("form-POST callback request");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "the form-POST callback must complete the login: {}",
        resp.text().await.unwrap_or_default()
    );
    resp.json().await.expect("token response JSON")
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn google_and_github_full_loops_link_verified_emails() {
    let Some(url) = database_url_or_skip("google_and_github_full_loops") else {
        return;
    };
    set_test_env();

    let db = "fraiseql_p26_social_loop";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;

    let (base, tx, handle) = Box::pin(boot_server(
        social_config(&scratch_url),
        social_schema(&stub),
        pool.clone(),
        &scratch_url,
    ))
    .await;

    // The provider listing is mounted and complete.
    let providers: serde_json::Value = reqwest::get(format!("{base}/auth/v1/providers"))
        .await
        .expect("providers request")
        .json()
        .await
        .expect("providers JSON");
    let names: Vec<&str> = providers["providers"]
        .as_array()
        .expect("providers array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["apple", "discord", "facebook", "github", "google"],
        "every configured provider is registered"
    );

    // Google (OIDC): full loop mints the server's own session tokens…
    let google_tokens = login(&base, "google").await;
    assert_eq!(google_tokens["provider"], "google");
    let google_user = jwt_sub(google_tokens["access_token"].as_str().expect("access_token"));
    // `[auth.session] ttl_secs` bounds a social sign-in as every other (#1853).
    assert_eq!(
        lifetime_of(google_tokens["access_token"].as_str().expect("access_token")),
        SESSION_TTL_SECS,
        "a social sign-in lives the session lifetime"
    );

    // …and the identity landed in the account store keyed by the verified
    // email: linking the same email from the same provider again is not new.
    let store = fraiseql_auth::PostgresAccountStore::new(pool.clone());
    let relink = store
        .link_or_create_user(None, Some("alice@example.com"), true, "google", "g-sub-1")
        .await
        .expect("relink google identity");
    assert!(!relink.is_new, "the callback must have created the account already");
    assert_eq!(relink.user_id, google_user, "the session subject is the linked account");

    // GitHub (plain OAuth2): /user hides the email; the /user/emails second
    // hop resolves the primary verified address, so the identity is keyed by
    // email — a later trusted-provider login with the same email must land on
    // the SAME account (the #368 linking guarantee).
    let github_tokens = login(&base, "github").await;
    assert_eq!(github_tokens["provider"], "github");
    let github_user = jwt_sub(github_tokens["access_token"].as_str().expect("access_token"));
    let cross = store
        .link_or_create_user(None, Some("bob@example.com"), true, "google", "g-sub-bob")
        .await
        .expect("cross-provider link");
    assert!(
        !cross.is_new,
        "a verified same-email login must land on the account the GitHub hop created"
    );
    assert_eq!(
        cross.user_id, github_user,
        "GitHub identity must be email-keyed via the verified /user/emails hop"
    );
    assert_eq!(
        TEAMS_REQUESTS.load(Ordering::SeqCst),
        0,
        "a GitHub login reads no team membership: no role or organisation is derived from it"
    );

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

/// `[auth.session] ttl_secs` sets the lifetime of a social sign-in's access token, as it does
/// every other path's (#1853).
#[tokio::test]
async fn ttl_secs_sets_the_lifetime_of_a_social_sign_in() {
    let Some(url) = database_url_or_skip("ttl_secs_sets_the_lifetime_of_a_social_sign_in") else {
        return;
    };
    set_test_env();

    let db = "fraiseql_p26_social_ttl";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;
    let mut schema = social_schema(&stub);
    schema.auth.as_mut().expect("auth").session = Some(fraiseql_core::schema::SessionConfig {
        transport:         fraiseql_core::schema::SessionTransport::Json,
        ttl_secs:          Some(900),
        absolute_ttl_secs: None,
    });

    let (base, tx, handle) =
        Box::pin(boot_server(social_config(&scratch_url), schema, pool.clone(), &scratch_url))
            .await;

    let tokens = login(&base, "google").await;
    let access = tokens["access_token"].as_str().expect("access_token");
    jwt_sub(access);
    assert_eq!(lifetime_of(access), 900, "ttl_secs sets the social access token's lifetime");

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn bogus_state_and_unknown_provider_are_refused() {
    let Some(url) = database_url_or_skip("bogus_state_and_unknown_provider") else {
        return;
    };
    set_test_env();

    let db = "fraiseql_p26_social_refuse";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;

    let (base, tx, handle) = Box::pin(boot_server(
        social_config(&scratch_url),
        social_schema(&stub),
        pool,
        &scratch_url,
    ))
    .await;
    let client = no_redirect_client();

    // An unregistered provider is refused, not silently defaulted.
    let resp = client
        .get(format!(
            "{base}/auth/v1/authorize?provider=myspace&redirect_uri=https://app.example.com/cb"
        ))
        .send()
        .await
        .expect("unknown provider request");
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    // A callback with a state the server never issued is refused.
    let resp = client
        .get(format!("{base}/auth/v1/callback?code=any&state=forged-state"))
        .send()
        .await
        .expect("forged state request");
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn authorize_flood_faces_the_auth_start_bucket() {
    let Some(url) = database_url_or_skip("authorize_flood") else {
        return;
    };
    set_test_env();

    let db = "fraiseql_p26_social_flood";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;

    // #788/H25: every authorize inserts CSRF state into a bounded store, so
    // the mount must face the auth_start budget. 3 per minute, generous
    // global budget so the path bucket is what trips.
    let mut schema = social_schema(&stub);
    let sec = fraiseql_core::schema::SecurityConfig {
        rate_limiting: Some(fraiseql_core::schema::RateLimitingSecurityConfig {
            enabled: true,
            requests_per_second: 1000,
            burst_size: 1000,
            auth_start_max_requests: 3,
            auth_start_window_secs: 60,
            ..Default::default()
        }),
        ..fraiseql_core::schema::SecurityConfig::default()
    };
    schema.security = Some(sec);

    let (base, tx, handle) =
        Box::pin(boot_server(social_config(&scratch_url), schema, pool, &scratch_url)).await;
    let client = no_redirect_client();

    let authorize =
        format!("{base}/auth/v1/authorize?provider=github&redirect_uri=https://app.example.com/cb");
    for i in 0..3 {
        let resp = client.get(&authorize).send().await.expect("authorize request");
        assert!(
            resp.status().is_redirection(),
            "request {i} within the budget must pass, got {}",
            resp.status()
        );
    }
    let resp = client.get(&authorize).send().await.expect("flooding request");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS,
        "the 4th authorize in the window must be rate limited (#788)"
    );
    assert!(resp.headers().contains_key("retry-after"), "a 429 must carry Retry-After");

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

/// #943: the whole Apple loop through the shipped mount — the form-POST
/// callback, the second sign-in that carries no claims at all, and the one
/// assertion that decides whether this provider is safe: the browser-supplied
/// `user` payload must never reach the email-keyed linking space.
#[tokio::test]
async fn apple_form_post_loop_links_only_the_id_tokens_email() {
    let Some(url) = database_url_or_skip("apple_form_post_loop") else {
        return;
    };
    set_test_env();

    let db = "fraiseql_p14_social_apple";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;

    let (base, tx, handle) = Box::pin(boot_server(
        social_config(&scratch_url),
        social_schema(&stub),
        pool.clone(),
        &scratch_url,
    ))
    .await;

    // First authorization: Apple's id_token asserts APPLE_EMAIL, while the
    // browser posts a `user` payload naming someone else entirely. A client can
    // put anything in that field — it is not signed and not from Apple.
    let forged_user = format!(
        r#"{{"name":{{"firstName":"Ada","lastName":"Lovelace"}},"email":"{APPLE_FORGED_EMAIL}"}}"#
    );
    let first = apple_login(&base, Some(&forged_user)).await;
    assert_eq!(first["provider"], "apple");
    let apple_user = jwt_sub(first["access_token"].as_str().expect("access_token"));

    let store = fraiseql_auth::PostgresAccountStore::new(pool.clone());

    // The account is keyed on the address APPLE asserted: another trusted
    // provider presenting it lands on the same account.
    let same = store
        .link_or_create_user(None, Some(APPLE_EMAIL), true, "google", "g-sub-apple-twin")
        .await
        .expect("link the id_token email");
    assert!(
        !same.is_new,
        "the callback must have created the account under the id_token email"
    );
    assert_eq!(same.user_id, apple_user, "the session subject is that account");

    // …and the forged address owns nothing. If the form payload's email had
    // been honoured, this would resolve to an existing account — the account
    // takeover this provider must not permit.
    let forged = store
        .link_or_create_user(None, Some(APPLE_FORGED_EMAIL), true, "google", "g-sub-victim")
        .await
        .expect("link the forged email");
    assert!(
        forged.is_new,
        "the browser-supplied `user` email must never key an account (it would let any Apple \
         user link into an address they do not own)"
    );
    assert_ne!(forged.user_id, apple_user);

    // Second sign-in: Apple sends `sub` and nothing else — no name, no email,
    // exactly as it does on every authorization after the first. The identity
    // persisted on first sight is what makes this resolve.
    let second = apple_login(&base, None).await;
    let second_user = jwt_sub(second["access_token"].as_str().expect("access_token"));
    assert_eq!(
        second_user, apple_user,
        "a claimless second sign-in must resolve to the account the first one created — \
         otherwise every login after the first forks a new user"
    );

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

/// #943: the form-POST mount is Apple's, and it faces the same CSRF-state gate
/// as the GET shape — a POST with a state this server never issued is refused.
#[tokio::test]
async fn apple_form_post_callback_refuses_a_forged_state() {
    let Some(url) = database_url_or_skip("apple_form_post_forged_state") else {
        return;
    };
    set_test_env();

    let db = "fraiseql_p14_social_apple_csrf";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;

    let (base, tx, handle) = Box::pin(boot_server(
        social_config(&scratch_url),
        social_schema(&stub),
        pool,
        &scratch_url,
    ))
    .await;

    let resp = no_redirect_client()
        .post(format!("{base}/auth/v1/callback"))
        .form(&[("code", "apple-code"), ("state", "never-issued")])
        .send()
        .await
        .expect("forged-state form POST");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "the POST callback must consume the same CSRF state the GET one does"
    );

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

/// #944: the assertion that decides whether Discord and Facebook are safe to
/// ship — an unverified or unverifiable address must stay in the
/// `(provider, id)` key space, so it can never collapse into an email-keyed
/// account belonging to someone else.
///
/// Discord *reports* `verified: false` for this account, and Facebook reports
/// nothing at all. Both addresses therefore own nothing, and a later trusted
/// login on the same address must create a fresh account rather than land on
/// theirs.
#[tokio::test]
async fn discord_and_facebook_keep_unverified_emails_out_of_the_linking_space() {
    let Some(url) = database_url_or_skip("discord_and_facebook_linking_space") else {
        return;
    };
    set_test_env();

    let db = "fraiseql_p14_social_plain_oauth";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;

    let (base, tx, handle) = Box::pin(boot_server(
        social_config(&scratch_url),
        social_schema(&stub),
        pool.clone(),
        &scratch_url,
    ))
    .await;
    let store = fraiseql_auth::PostgresAccountStore::new(pool.clone());

    // ── Discord: `verified: false` on the user object, honoured ──────────
    let discord = login(&base, "discord").await;
    assert_eq!(discord["provider"], "discord");
    let discord_user = jwt_sub(discord["access_token"].as_str().expect("access_token"));

    let discord_claim = store
        .link_or_create_user(
            None,
            Some(DISCORD_UNVERIFIED_EMAIL),
            true,
            "google",
            "g-sub-discord-twin",
        )
        .await
        .expect("claim the discord address from a trusted provider");
    assert!(
        discord_claim.is_new,
        "an address Discord itself calls unverified must not own an account — otherwise anyone \
         who sets that address on Discord without confirming it inherits the real owner's account"
    );
    assert_ne!(discord_claim.user_id, discord_user);

    // The Discord identity is nonetheless durable: signing in again is the same
    // account, resolved on `(discord, id)`.
    let discord_again = login(&base, "discord").await;
    assert_eq!(
        jwt_sub(discord_again["access_token"].as_str().expect("access_token")),
        discord_user,
        "an unverified-email identity must still be stable across sign-ins"
    );

    // ── Facebook: no verification signal exists at all ───────────────────
    let facebook = login(&base, "facebook").await;
    assert_eq!(facebook["provider"], "facebook");
    let facebook_user = jwt_sub(facebook["access_token"].as_str().expect("access_token"));

    let facebook_claim = store
        .link_or_create_user(None, Some(FACEBOOK_EMAIL), true, "google", "g-sub-facebook-twin")
        .await
        .expect("claim the facebook address from a trusted provider");
    assert!(
        facebook_claim.is_new,
        "Facebook publishes no verification signal, so its address must never key an account"
    );
    assert_ne!(facebook_claim.user_id, facebook_user);

    assert_ne!(
        discord_user, facebook_user,
        "two email-less identities must not collapse into one account"
    );

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

/// The document this suite serves loads, checked with no database.
///
/// Every other test here reaches the document only after it has found a database, so in
/// a run without one they skip before it is built and a load-time refusal of it reports
/// as a pass (`2b843cd27`: 12 tests red for a session under a green preflight).
/// This one needs nothing but the loader, so that refusal cannot hide.
///
/// This document is deserialised with `from_value` and never meets `finish_load`.
/// `from_value` ignores a top-level key it does not know, so what can refuse it is a
/// field it does not send becoming required, or a field changing type.
#[test]
fn the_document_loads_without_a_database() {
    empty_schema();
}

/// A social sign-in of a user enrolled in `MFA` yields a challenge bound to that user, not
/// a session.
#[tokio::test]
async fn a_social_sign_in_of_an_mfa_user_yields_a_challenge() {
    use fraiseql_auth::MfaStore as _;

    let Some(url) = database_url_or_skip("a_social_sign_in_of_an_mfa_user_yields_a_challenge")
    else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_social_mfa";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;
    let (base, tx, handle) = Box::pin(boot_server(
        social_config(&scratch_url),
        social_schema(&stub),
        pool.clone(),
        &scratch_url,
    ))
    .await;

    let user =
        jwt_sub(login(&base, "google").await["access_token"].as_str().expect("access_token"));
    sqlx::raw_sql(fraiseql_auth::PG_MFA_SCHEMA_SQL).execute(&pool).await.unwrap();
    let mfa = fraiseql_auth::PgMfaStore::new(pool.clone());
    let enrolled = mfa.begin_enrollment(&user, "FraiseQL", "user@example.com", None).await.unwrap();
    mfa.confirm_enrollment(&user, &totp_code(&enrolled.secret_base32))
        .await
        .unwrap();

    let (status, body) = sign_in(&base, "google").await;
    assert!(
        body.get("access_token").is_none(),
        "the social callback minted a session for a user enrolled in MFA: {status} {body}"
    );
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"], "mfa_required", "{body}");
    let challenge = body["challenge_token"].as_str().expect("a challenge to redeem");
    let redeemed = mfa
        .verify_challenge(challenge, &totp_code(&enrolled.secret_base32))
        .await
        .expect("the challenge redeems");
    assert_eq!(redeemed, user, "the challenge is bound to the user who signed in");

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

// ── How a session reaches the browser ───────────────────────────────────────

/// The `Set-Cookie` headers of `resp`.
fn set_cookies(resp: &reqwest::Response) -> Vec<String> {
    resp.headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap_or_default().to_string())
        .collect()
}

/// Whether any `Set-Cookie` of `resp` carries a session token.
fn sets_a_session_cookie(resp: &reqwest::Response) -> bool {
    set_cookies(resp).iter().any(|c| {
        c.starts_with(fraiseql_auth::session_cookies::ACCESS_COOKIE)
            || c.starts_with(fraiseql_auth::session_cookies::REFRESH_COOKIE)
    })
}

/// Whether `resp` clears the login cookie whose `name=value` is `cookie`.
fn clears(resp: &reqwest::Response, cookie: &str) -> bool {
    let name = cookie.split('=').next().unwrap_or_default();
    set_cookies(resp)
        .iter()
        .any(|c| c.starts_with(&format!("{name}=;")) && c.contains("Max-Age=0"))
}

/// `authorize` for `provider` on `client`, redirecting to `redirect_uri`: the state, the
/// login cookie's `name=value`, and every `Set-Cookie` the answer carried.
async fn authorize(
    client: &reqwest::Client,
    base: &str,
    provider: &str,
    redirect_uri: &str,
) -> (String, String, Vec<String>) {
    let resp = client
        .get(format!(
            "{base}/auth/v1/authorize?provider={provider}&redirect_uri={redirect_uri}"
        ))
        .send()
        .await
        .expect("authorize request");
    assert!(resp.status().is_redirection(), "authorize must redirect ({provider})");
    let state = state_from_location(resp.headers()["location"].to_str().expect("a location"));
    (state, login_cookie(&resp), set_cookies(&resp))
}

/// With no redirect allow-list, the callback answers the session in its `JSON` body.
#[tokio::test]
async fn a_social_sign_in_with_no_bound_redirect_answers_its_tokens_in_json() {
    let Some(url) = database_url_or_skip("a_social_sign_in_with_no_bound_redirect") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_social_json";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;
    let (base, tx, handle) = Box::pin(boot_server(
        social_config(&scratch_url),
        social_schema(&stub),
        pool,
        &scratch_url,
    ))
    .await;
    let client = no_redirect_client();

    let (state, cookie, _) =
        authorize(&client, &base, "google", "https://app.example.com/cb").await;
    let resp = client
        .get(format!("{base}/auth/v1/callback?code=any-code&state={state}"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("callback request");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    assert!(!sets_a_session_cookie(&resp), "the JSON answer sets no session cookie");
    assert!(clears(&resp, &cookie), "the answer clears the login cookie");
    let body: serde_json::Value = resp.json().await.expect("a JSON body");
    assert!(body["access_token"].is_string(), "{body}");
    assert!(body["refresh_token"].is_string(), "{body}");

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

/// With a bound redirect, both callback shapes hand the session in the redirect's fragment.
#[tokio::test]
async fn a_bound_redirect_carries_the_session_in_its_fragment() {
    let Some(url) = database_url_or_skip("a_bound_redirect_carries_the_session") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_social_fragment";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;
    let mut schema = social_schema(&stub);
    schema
        .auth
        .as_mut()
        .and_then(|a| a.social.as_mut())
        .expect("social")
        .redirect_uri_allowlist = vec!["https://app.example.com".to_string()];
    let (base, tx, handle) =
        Box::pin(boot_server(social_config(&scratch_url), schema, pool, &scratch_url)).await;
    let client = no_redirect_client();

    let (state, get_cookie, _) =
        authorize(&client, &base, "google", "https://app.example.com/cb").await;
    let get = client
        .get(format!("{base}/auth/v1/callback?code=any-code&state={state}"))
        .header(reqwest::header::COOKIE, &get_cookie)
        .send()
        .await
        .expect("callback request");
    let (state, post_cookie, _) =
        authorize(&client, &base, "apple", "https://app.example.com/cb").await;
    let post = client
        .post(format!("{base}/auth/v1/callback"))
        .header(reqwest::header::COOKIE, &post_cookie)
        .form(&[("code", "apple-code"), ("state", state.as_str())])
        .send()
        .await
        .expect("form-POST callback request");
    for (shape, resp, cookie) in [("GET", get, get_cookie), ("POST", post, post_cookie)] {
        assert!(resp.status().is_redirection(), "{shape}: {}", resp.status());
        assert!(!sets_a_session_cookie(&resp), "{shape}: the redirect sets no session cookie");
        assert!(clears(&resp, &cookie), "{shape}: the redirect clears the login cookie");
        let location = resp.headers()["location"].to_str().expect("a location").to_string();
        let (target, fragment) = location.split_once('#').expect("the redirect has a fragment");
        assert_eq!(target, "https://app.example.com/cb", "{shape}");
        assert!(fragment.contains("access_token="), "{shape}: {location}");
        assert!(fragment.contains("refresh_token="), "{shape}: {location}");
    }

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

/// A login completes only from the browser that started it: by the cookie `authorize` sets,
/// `SameSite=Lax` for a `GET` callback and `SameSite=None` for Apple's form `POST`.
#[tokio::test]
async fn a_login_completes_only_from_the_browser_that_started_it() {
    let Some(url) = database_url_or_skip("a_login_completes_only_from_its_browser") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_social_other_client";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;
    let (base, tx, handle) = Box::pin(boot_server(
        social_config(&scratch_url),
        social_schema(&stub),
        pool,
        &scratch_url,
    ))
    .await;
    let started_by = no_redirect_client();
    let other = no_redirect_client();
    let callback = |state: &str| format!("{base}/auth/v1/callback?code=any-code&state={state}");
    let cb = "https://app.example.com/cb";

    // The cookie each provider's login sets.
    for (provider, same_site) in [("google", "SameSite=Lax"), ("apple", "SameSite=None")] {
        let (_, _, set) = authorize(&started_by, &base, provider, cb).await;
        let set = set.join("\n");
        for attribute in [
            "__Host-fraiseql_login_",
            "Path=/",
            "HttpOnly",
            "Secure",
            same_site,
        ] {
            assert!(set.contains(attribute), "{provider}: {set}");
        }
    }

    // Another browser, with no cookie: refused, the cookie cleared, and the state spent.
    let (state, cookie, _) = authorize(&started_by, &base, "google", cb).await;
    let resp = other.get(callback(&state)).send().await.expect("callback request");
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(clears(&resp, &cookie), "a refusal clears the login cookie");
    let resp = started_by
        .get(callback(&state))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("callback request");
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST, "a refused state is spent");

    // The cookie's name with another value: refused.
    let (state, cookie, _) = authorize(&started_by, &base, "google", cb).await;
    let name = cookie.split('=').next().expect("a cookie name");
    let resp = other
        .get(callback(&state))
        .header(reqwest::header::COOKIE, format!("{name}=0123abcd"))
        .send()
        .await
        .expect("callback request");
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    // Apple's form POST from another browser: refused.
    let (state, _, _) = authorize(&started_by, &base, "apple", cb).await;
    let resp = other
        .post(format!("{base}/auth/v1/callback"))
        .form(&[("code", "apple-code"), ("state", state.as_str())])
        .send()
        .await
        .expect("form-POST callback request");
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    // The browser that started it, with its cookie: completes.
    let (state, cookie, _) = authorize(&started_by, &base, "google", cb).await;
    let resp = started_by
        .get(callback(&state))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("callback request");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "{}",
        resp.text().await.unwrap_or_default()
    );

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

// ── Cookie transport ────────────────────────────────────────────────────────────────

/// A refreshable `[auth.session]` in cookie transport, and `[auth.social]` allow-listing the
/// front end.
fn cookie_mode(stub: &str) -> CompiledSchema {
    let mut schema = social_schema(stub);
    let auth = schema.auth.as_mut().expect("auth");
    auth.session = Some(fraiseql_core::schema::SessionConfig {
        ttl_secs:          Some(SESSION_TTL_SECS),
        absolute_ttl_secs: Some(COOKIE_ABSOLUTE_TTL_SECS),
        transport:         fraiseql_core::schema::SessionTransport::Cookie,
    });
    auth.social.as_mut().expect("social").redirect_uri_allowlist =
        vec!["https://app.example.com".to_string()];
    schema
}

const COOKIE_ABSOLUTE_TTL_SECS: u64 = 86_400;

/// The value of the cookie `name` that `set_cookies` set.
fn set_cookie_value(set_cookies: &[String], name: &str) -> String {
    set_cookies
        .iter()
        .find_map(|c| c.strip_prefix(&format!("{name}=")))
        .and_then(|rest| rest.split(';').next())
        .unwrap_or_default()
        .to_string()
}

/// An answer: its status, `Location`, `Set-Cookie`s and body.
struct Answer {
    status:      reqwest::StatusCode,
    location:    String,
    set_cookies: Vec<String>,
    body:        String,
}

impl Answer {
    async fn of(request: reqwest::RequestBuilder) -> Self {
        let resp = request.send().await.expect("request");
        let status = resp.status();
        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let set_cookies = set_cookies(&resp);
        let body = resp.text().await.unwrap_or_default();
        Self {
            status,
            location,
            set_cookies,
            body,
        }
    }

    /// The session cookies this answer set: (access, refresh).
    fn session(&self, what: &str) -> (String, String) {
        let access =
            set_cookie_value(&self.set_cookies, fraiseql_auth::session_cookies::ACCESS_COOKIE);
        let refresh =
            set_cookie_value(&self.set_cookies, fraiseql_auth::session_cookies::REFRESH_COOKIE);
        assert!(!access.is_empty() && !refresh.is_empty(), "{what}: {:?}", self.set_cookies);
        (access, refresh)
    }

    /// Neither the body nor the `Location` carries any of `tokens`.
    fn assert_carries_none_of(&self, what: &str, tokens: &[&str]) {
        for token in tokens {
            assert!(!self.body.contains(token), "{what}: the body carries a token: {}", self.body);
            assert!(!self.location.contains(token), "{what}: the Location carries a token");
        }
        for key in ["access_token", "refresh_token"] {
            assert!(!self.body.contains(key), "{what}: {}", self.body);
            assert!(!self.location.contains(key), "{what}: {}", self.location);
        }
    }
}

/// `/graphql` with no credential but the access cookie `access`.
async fn graphql_with_cookie(base: &str, access: &str) -> reqwest::StatusCode {
    reqwest::Client::new()
        .post(format!("{base}/graphql"))
        .header(
            reqwest::header::COOKIE,
            format!("{}={access}", fraiseql_auth::session_cookies::ACCESS_COOKIE),
        )
        .json(&serde_json::json!({ "query": "{ __typename }" }))
        .send()
        .await
        .expect("graphql request")
        .status()
}

/// A session route with no credential but the refresh cookie `refresh`.
async fn session_route(base: &str, route: &str, refresh: &str) -> Answer {
    Answer::of(
        reqwest::Client::new()
            .post(format!("{base}/auth/v1/session/{route}"))
            .header(
                reqwest::header::COOKIE,
                format!("{}={refresh}", fraiseql_auth::session_cookies::REFRESH_COOKIE),
            )
            .json(&serde_json::json!({})),
    )
    .await
}

/// In cookie transport, a social sign-in sets the session cookies on its redirect to the
/// front end, on both callback shapes; no body or `Location` carries a token; and the
/// session the cookies hold works on `/graphql`, refreshes and logs out by cookie alone.
#[tokio::test]
async fn in_cookie_transport_a_social_sign_in_sets_the_session_cookies_on_its_redirect() {
    let Some(url) = database_url_or_skip("in_cookie_transport_a_social_sign_in") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_social_cookie";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;
    let (base, tx, handle) =
        Box::pin(boot_server(social_config(&scratch_url), cookie_mode(&stub), pool, &scratch_url))
            .await;
    let client = no_redirect_client();
    let cb = "https://app.example.com/cb";

    let (state, login, _) = authorize(&client, &base, "google", cb).await;
    let get = Answer::of(
        client
            .get(format!("{base}/auth/v1/callback?code=any-code&state={state}"))
            .header(reqwest::header::COOKIE, &login),
    )
    .await;
    let (state, apple_login, _) = authorize(&client, &base, "apple", cb).await;
    let post = Answer::of(
        client
            .post(format!("{base}/auth/v1/callback"))
            .header(reqwest::header::COOKIE, &apple_login)
            .form(&[("code", "apple-code"), ("state", state.as_str())]),
    )
    .await;
    let mut sessions = Vec::new();
    for (shape, answer, login) in [("GET", &get, &login), ("POST", &post, &apple_login)] {
        assert_eq!(answer.status, reqwest::StatusCode::SEE_OTHER, "{shape}: {}", answer.body);
        assert_eq!(answer.location, cb, "{shape}: the redirect carries nothing but its target");
        let (access, refresh) = answer.session(shape);
        answer.assert_carries_none_of(shape, &[&access, &refresh]);
        let name = login.split('=').next().unwrap_or_default();
        assert!(
            answer.set_cookies.iter().any(|c| c.starts_with(&format!("{name}=;"))),
            "{shape}: the login cookie is cleared: {:?}",
            answer.set_cookies
        );
        sessions.push((access, refresh));
    }

    let (access, refresh) = sessions.swap_remove(0);
    assert_eq!(graphql_with_cookie(&base, &access).await, reqwest::StatusCode::OK);
    assert_eq!(
        graphql_with_cookie(&base, &format!("{access}x")).await,
        reqwest::StatusCode::UNAUTHORIZED,
        "the access cookie is what /graphql checks"
    );

    let refreshed = session_route(&base, "refresh", &refresh).await;
    assert_eq!(refreshed.status, reqwest::StatusCode::OK, "{}", refreshed.body);
    let (next_access, next_refresh) = refreshed.session("refresh");
    refreshed.assert_carries_none_of("refresh", &[&access, &refresh, &next_access, &next_refresh]);
    assert_eq!(graphql_with_cookie(&base, &next_access).await, reqwest::StatusCode::OK);

    let logout = session_route(&base, "logout", &next_refresh).await;
    assert_eq!(logout.status, reqwest::StatusCode::NO_CONTENT, "{}", logout.body);
    for name in [
        fraiseql_auth::session_cookies::ACCESS_COOKIE,
        fraiseql_auth::session_cookies::REFRESH_COOKIE,
    ] {
        assert!(
            logout.set_cookies.iter().any(|c| c.starts_with(&format!("{name}=;"))),
            "logout clears {name}: {:?}",
            logout.set_cookies
        );
    }
    let after = session_route(&base, "refresh", &next_refresh).await;
    assert_eq!(after.status, reqwest::StatusCode::UNAUTHORIZED, "the session is over");

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

/// Cookie transport with no redirect allow-list would leave the callback nowhere to set
/// the cookies but a body: the boot refuses it, naming both keys.
#[tokio::test]
async fn cookie_transport_with_no_redirect_allow_list_does_not_boot() {
    let Some(url) = database_url_or_skip("cookie_transport_with_no_redirect_allow_list") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_p26_social_cookie_refused";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let stub = stub_idp().await;
    let mut schema = cookie_mode(&stub);
    schema
        .auth
        .as_mut()
        .and_then(|a| a.social.as_mut())
        .expect("social")
        .redirect_uri_allowlist = Vec::new();
    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
    let refused =
        Box::pin(Server::new(social_config(&scratch_url), schema, adapter, Some(pool.clone())))
            .await;
    let Err(error) = refused else {
        panic!("cookie transport with no redirect allow-list must refuse to boot");
    };
    let error = error.to_string();
    assert!(
        error.contains("[auth.session] transport") && error.contains("redirect_uri_allowlist"),
        "the refusal names both keys: {error}"
    );

    pool.close().await;
    drop_scratch(&url, db).await;
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
