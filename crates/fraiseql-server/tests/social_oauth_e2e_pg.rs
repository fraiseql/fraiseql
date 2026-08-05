//! #368: social login through the shipped server's mount, against a stub `IdP`.
//!
//! The library slice shipped the trust-gated `multi_provider` flow, but nothing
//! in the binary mounted it — `with_social_login` had zero callers and
//! `[auth.social]` could not even be typed. These tests boot the real server
//! from a compiled `[auth.social]` block and prove the whole loop against a
//! local stub `IdP`:
//!
//! - `Google` (`OIDC`): authorize redirect → callback → token exchange → userinfo → verified-email
//!   account linking → HS256 session tokens.
//! - `GitHub` (plain `OAuth2`): the token endpoint answers JSON only when `Accept:
//!   application/json` is sent, carries **no** `expires_in`, and `/user` hides the email — the
//!   `/user/emails` second hop resolves the primary verified address and links by email (#368's
//!   `GitHub` gap).
//! - The `/auth/v1/authorize` flood faces the `auth_start` path bucket (#788).
//! - Bogus state and unknown providers are refused.
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

use std::sync::Arc;

use axum::{
    Json, Router,
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
};
use fraiseql_auth::AccountStore as _;
use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{
    Server,
    server_config::{ServerConfig, hs256::Hs256Config},
};
use fraiseql_test_support::try_database_url;
use sqlx::PgPool;

const HS256_SECRET: &str = "p26-social-hs256-secret-32bytes!";
const HS256_SECRET_ENV: &str = "FRAISEQL_TEST_P26_SOCIAL_E2E_HS256";
const GOOGLE_SECRET_ENV: &str = "FRAISEQL_TEST_P26_SOCIAL_E2E_GOOGLE";
const GITHUB_SECRET_ENV: &str = "FRAISEQL_TEST_P26_SOCIAL_E2E_GITHUB";

/// Process-wide test environment. Fixed valid values, set unconditionally:
/// safe under the parallel runner because every reader wants exactly these.
fn set_test_env() {
    std::env::set_var("FRAISEQL_ENV", "development");
    std::env::set_var("FRAISEQL_OIDC_ALLOW_INSECURE", "1");
    std::env::set_var(HS256_SECRET_ENV, HS256_SECRET);
    std::env::set_var(GOOGLE_SECRET_ENV, "google-client-secret");
    std::env::set_var(GITHUB_SECRET_ENV, "github-client-secret");
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

// ── Stub IdP ─────────────────────────────────────────────────────────────────

/// Boot a stub `IdP` serving both a `Google`-shaped `OIDC` surface and a
/// `GitHub`-shaped plain-`OAuth2` surface. Returns its base URL.
async fn stub_idp() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind stub idp");
    let base = format!("http://127.0.0.1:{}", listener.local_addr().expect("addr").port());

    let discovery_base = base.clone();
    let app = Router::new()
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
        );
    // /user/teams is deliberately unmounted: the 404 exercises the
    // best-effort teams path.

    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service()).await.unwrap();
    });
    base
}

// ── Server fixture ───────────────────────────────────────────────────────────

fn empty_schema() -> CompiledSchema {
    serde_json::from_value(serde_json::json!({
        "version": "2.0.0",
        "types": [],
        "queries": [],
        "mutations": [],
    }))
    .expect("compiled schema")
}

fn social_schema(stub_base: &str) -> CompiledSchema {
    let mut schema = empty_schema();
    schema.auth = Some(fraiseql_core::schema::AuthClientConfig {
        pkce:   None,
        local:  None,
        social: Some(fraiseql_core::schema::SocialAuthConfig {
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
        }),
    });
    schema
}

/// The operator-chosen HS256 issuer/audience — deliberately NOT the session
/// store's historical hard-coded defaults (`fraiseql` / `fraiseql-api`), so the
/// claims assertion below catches a mint/validate mismatch: a session minted
/// with claims the configured validator rejects is a login that "succeeds" and
/// then 401s on every request.
const HS256_ISSUER: &str = "https://sp.example.com";
const HS256_AUDIENCE: &str = "fraiseql-api-test-audience";

fn social_config(scratch_url: &str) -> ServerConfig {
    ServerConfig {
        cors_enabled: false,
        database_url: scratch_url.to_string(),
        auth_hs256: Some(Hs256Config {
            secret_env: HS256_SECRET_ENV.to_string(),
            issuer:     Some(HS256_ISSUER.to_string()),
            audience:   Some(HS256_AUDIENCE.to_string()),
        }),
        ..ServerConfig::default()
    }
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
        .expect("Server::new with [auth.social] + [auth_hs256] + pool must succeed");
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
/// match the configured `[auth_hs256]` — the claims this server's own
/// validator demands. A mismatch is the "login succeeds, then every request
/// 401s" defect.
fn jwt_sub(token: &str) -> String {
    let claims = jwt_claims(token);
    assert_eq!(
        claims["iss"].as_str(),
        Some(HS256_ISSUER),
        "minted session must carry the configured issuer: {claims}"
    );
    assert!(
        claims["aud"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(HS256_AUDIENCE))),
        "minted session must carry the configured audience: {claims}"
    );
    claims["sub"].as_str().expect("sub claim").to_string()
}

/// Drive `provider` through authorize → callback; returns the token-response JSON.
async fn login(base: &str, provider: &str) -> serde_json::Value {
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
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let state = state_from_location(&location);

    let resp = client
        .get(format!("{base}/auth/v1/callback?code=any-code&state={state}"))
        .send()
        .await
        .expect("callback request");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "callback must complete the login ({provider}): {}",
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

    let (base, tx, handle) =
        boot_server(social_config(&scratch_url), social_schema(&stub), pool.clone(), &scratch_url)
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
    assert_eq!(names, vec!["github", "google"], "both configured providers are registered");

    // Google (OIDC): full loop mints HS256 session tokens…
    let google_tokens = login(&base, "google").await;
    assert_eq!(google_tokens["provider"], "google");
    let google_user = jwt_sub(google_tokens["access_token"].as_str().expect("access_token"));

    // …and the identity landed in the account store keyed by the verified
    // email: linking the same email from the same provider again is not new.
    let store = fraiseql_auth::PostgresAccountStore::new(pool.clone());
    let relink = store
        .link_or_create_user(Some("alice@example.com"), true, "google", "g-sub-1")
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
        .link_or_create_user(Some("bob@example.com"), true, "google", "g-sub-bob")
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

    let (base, tx, handle) =
        boot_server(social_config(&scratch_url), social_schema(&stub), pool, &scratch_url).await;
    let client = no_redirect_client();

    // An unregistered provider is refused, not silently defaulted.
    let resp = client
        .get(format!(
            "{base}/auth/v1/authorize?provider=facebook&redirect_uri=https://app.example.com/cb"
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
        boot_server(social_config(&scratch_url), schema, pool, &scratch_url).await;
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
