//! Pipeline 6 — composed HTTP integration test: PKCE `auth_start` → `auth_callback`.
//!
//! Drives the complete PKCE flow at the HTTP level against a real Axum router
//! built from the production `auth_start` and `auth_callback` handlers.  No
//! real OIDC `IdP` is involved — the test verifies the middleware layers
//! (state creation, state consumption, replay prevention) that are NOT covered
//! by the `PkceStateStore` unit tests alone.
//!
//! # What is NOT tested here
//! - Real OIDC token exchange (requires a live `IdP`)
//! - Encrypted PKCE state (requires `state_encryption` feature config)
//! - Redis-backed PKCE store (see the `#[ignore]` variant in the redis integration variant)
//!
//! # Why this test exists
//! Each stage of Pipeline 6 has unit tests, but no single test wires
//! `auth_start` → encrypted outbound token → `auth_callback` → state consumed
//! through the real HTTP router.  This composed test catches integration bugs
//! that per-stage unit tests cannot see.
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics use usize/u64→f64 for reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are small and bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are small and bounded
#![allow(clippy::cast_lossless)] // Reason: test code readability
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions, panics are expected
#![allow(clippy::missing_errors_doc)] // Reason: test helper functions
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site
#![allow(clippy::used_underscore_binding)] // Reason: test variables prefixed with _ by convention
#![allow(clippy::needless_pass_by_value)] // Reason: test helper signatures follow test patterns

use std::sync::Arc;

use axum::{Router, body::Body, routing::get};
use fraiseql_auth::OidcServerClient;
use fraiseql_server::{
    auth::PkceStateStore,
    routes::{AuthPkceState, auth_callback, auth_start},
};
use http::{Request, StatusCode};
use tower::ServiceExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal `OidcServerClient` whose `authorization_url()` redirects to
/// `https://auth.example.com/authorize`.  The token endpoint is set to a
/// non-routable address so that the code exchange step will fail with a 502
/// rather than a 400 (state error) or 500 (internal error).
fn test_oidc_client() -> OidcServerClient {
    OidcServerClient::new(
        "test-client",
        "test-secret",
        "http://localhost/auth/callback",
        "https://auth.example.com/authorize",
        "https://192.0.2.1/token", // non-routable: exchange will fail with 502
    )
}

/// Build an Axum router that mounts only the PKCE auth routes.
///
/// Uses an in-memory `PkceStateStore` with a 300-second TTL and no encryption,
/// which is valid for single-process tests.
fn auth_router() -> Router {
    let pkce_store = PkceStateStore::new(300, None);
    let oidc_client = test_oidc_client();

    let state = Arc::new(AuthPkceState {
        pkce_store:              Arc::new(pkce_store),
        oidc_client:             Arc::new(oidc_client),
        http_client:             Arc::new(reqwest::Client::new()),
        post_login_redirect_uri: None,
    });

    Router::new()
        .route("/auth/start", get(auth_start))
        .route("/auth/callback", get(auth_callback))
        .with_state(state)
}

/// Send a GET request to the given router and return the status + location
/// header (if any).
async fn get_request(router: &Router, uri: &str) -> (StatusCode, Option<String>) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();

    let status = response.status();
    let location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    (status, location)
}

/// Extract the `state=` query parameter from a URL string.
///
/// Returns the raw (URL-encoded) value so the test can pass it back in
/// the callback URL without double-encoding.
fn extract_state_param(url: &str) -> &str {
    // Find "state=" and take everything up to the next "&" or end.
    let start = url
        .find("state=")
        .map(|pos| pos + "state=".len())
        .expect("redirect URL must contain state= parameter");

    let end = url[start..].find('&').map_or(url.len(), |rel| start + rel);

    &url[start..end]
}

// ---------------------------------------------------------------------------
// Composed auth_start → auth_callback HTTP tests
// ---------------------------------------------------------------------------

/// Pipeline 6, Stage A: `GET /auth/start` must redirect to the configured `IdP`.
///
/// Verifies:
/// - Response status is 303 (Axum `Redirect::to()` always uses See Other).
/// - `Location` header points to the configured `authorization_endpoint`.
/// - `code_challenge` parameter is present (PKCE S256).
/// - `state` parameter is present (encrypted or plain opaque token).
#[tokio::test]
async fn auth_start_redirects_to_idp() {
    let router = auth_router();

    let (status, location) =
        get_request(&router, "/auth/start?redirect_uri=https://app.example.com/after-login").await;

    // Axum's Redirect::to() returns 303 See Other.
    assert_eq!(status, StatusCode::SEE_OTHER, "auth_start must redirect (303)");

    let loc = location.expect("auth_start must set Location header");
    assert!(
        loc.contains("auth.example.com"),
        "redirect must point to the configured IdP: {loc}"
    );
    assert!(
        loc.contains("code_challenge"),
        "redirect must include PKCE code_challenge: {loc}"
    );
    assert!(loc.contains("state="), "redirect must include opaque state token: {loc}");
}

/// Pipeline 6, Stages A+B+C: full flow `auth_start` → `auth_callback`.
///
/// Step 1 — `auth_start` creates state and redirects.
/// Step 2 — `auth_callback` with the correct state token is able to *consume*
///           that state (state lookup succeeds), then fails at the token
///           exchange step (502 from the non-routable token endpoint).
///
/// The key assertion is that the callback returns something OTHER than 400
/// (which would indicate state-not-found or state-expired), proving that the
/// state was created in Step 1 and consumed in Step 2.
///
/// Step 3 — A second `auth_callback` with the SAME state token must return 400,
///           proving that the state was consumed atomically (replay prevention).
#[tokio::test]
async fn auth_start_then_callback_completes_pkce_flow() {
    let router = auth_router();

    // ── Step 1: auth_start ────────────────────────────────────────────────
    let (status, location) =
        get_request(&router, "/auth/start?redirect_uri=https://app.example.com/after-login").await;

    assert_eq!(status, StatusCode::SEE_OTHER, "auth_start must redirect (303)");
    let loc = location.expect("auth_start must provide Location header");

    let state_token = extract_state_param(&loc);
    assert!(!state_token.is_empty(), "state token must not be empty");

    // ── Step 2: auth_callback — state consumed, exchange fails ────────────
    // The state token is valid; the token exchange fails because the endpoint
    // is non-routable (192.0.2.1).  That should produce 502, not 400.
    let callback_uri = format!("/auth/callback?code=fake_code&state={state_token}");
    let (callback_status, _) = get_request(&router, &callback_uri).await;

    // The token exchange fails (non-routable IP) → 502.
    // What matters: it must NOT be 400, which would indicate state was not found.
    assert_ne!(
        callback_status,
        StatusCode::BAD_REQUEST,
        "state token must be valid — failure must come from IdP exchange \
         (502), not state lookup (400). Got: {callback_status}"
    );

    // ── Step 3: replay — state already consumed ───────────────────────────
    let (replay_status, _) = get_request(&router, &callback_uri).await;

    assert_eq!(
        replay_status,
        StatusCode::BAD_REQUEST,
        "second use of the same state token must be rejected (state consumed)"
    );
}

/// Pipeline 6 error path: `auth_start` without `redirect_uri` must return 400.
#[tokio::test]
async fn auth_start_missing_redirect_uri_returns_400() {
    let router = auth_router();
    let (status, _) = get_request(&router, "/auth/start").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "missing redirect_uri must return 400");
}

/// Pipeline 6 error path: `auth_callback` with unknown state token must return 400.
#[tokio::test]
async fn auth_callback_unknown_state_returns_400() {
    let router = auth_router();
    let (status, _) =
        get_request(&router, "/auth/callback?code=any_code&state=unknown-state-token").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "unknown state token must return 400");
}

/// Pipeline 6 error path: `auth_callback` with a provider error parameter
/// must return 400.
#[tokio::test]
async fn auth_callback_provider_error_returns_400() {
    let router = auth_router();
    let (status, _) = get_request(&router, "/auth/callback?error=access_denied").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "provider error must return 400");
}

/// Pipeline 6 error path: `auth_callback` with a missing code and missing state
/// must return 400.
#[tokio::test]
async fn auth_callback_missing_code_and_state_returns_400() {
    let router = auth_router();
    let (status, _) = get_request(&router, "/auth/callback").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "callback with no params must return 400");
}

// ---------------------------------------------------------------------------
// C15: Session cookie mode (__Host-access_token Set-Cookie)
// ---------------------------------------------------------------------------

/// Build a router where:
/// - The OIDC token endpoint is a `wiremock` mock returning a valid token.
/// - `post_login_redirect_uri` is set, so the callback uses cookie mode.
async fn session_cookie_router(
    mock_server: &MockServer,
) -> Router {
    // Mount a mock that replies to POST /token with a valid token response.
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "test-access-token-xyz",
            "id_token": "test-id-token",
            "expires_in": 3600,
            "token_type": "Bearer"
        })))
        .mount(mock_server)
        .await;

    let oidc_client = OidcServerClient::new(
        "test-client",
        "test-secret",
        "http://localhost/auth/callback",
        "https://auth.example.com/authorize",
        format!("{}/token", mock_server.uri()),
    );

    let pkce_store = PkceStateStore::new(300, None);

    let state = Arc::new(AuthPkceState {
        pkce_store:              Arc::new(pkce_store),
        oidc_client:             Arc::new(oidc_client),
        http_client:             Arc::new(reqwest::Client::new()),
        post_login_redirect_uri: Some("https://app.example.com/dashboard".to_string()),
    });

    Router::new()
        .route("/auth/start", get(auth_start))
        .route("/auth/callback", get(auth_callback))
        .with_state(state)
}

/// C15: When `post_login_redirect_uri` is configured the callback must:
/// 1. Return a 302/303 redirect to the configured URI (NOT the caller's `redirect_uri`).
/// 2. Set a `__Host-access_token` cookie with `HttpOnly`, `Secure`, `SameSite=Strict`.
/// 3. Include the access token in the cookie value.
#[tokio::test]
async fn auth_callback_session_cookie_mode() {
    let mock_server = MockServer::start().await;
    let router = session_cookie_router(&mock_server).await;

    // ── Step 1: auth_start → redirect to IdP ─────────────────────────────
    let (status, location) =
        get_request(&router, "/auth/start?redirect_uri=https://app.example.com/after-login").await;
    assert_eq!(status, StatusCode::SEE_OTHER, "auth_start must redirect (303)");
    let loc = location.expect("auth_start must set Location header");
    let state_token = extract_state_param(&loc);
    assert!(!state_token.is_empty(), "state token must not be empty");

    // ── Step 2: auth_callback → cookie redirect ──────────────────────────
    let callback_uri = format!("/auth/callback?code=valid_code&state={state_token}");
    let response = router
        .clone()
        .oneshot(Request::builder().uri(&callback_uri).body(Body::empty()).unwrap())
        .await
        .unwrap();

    let status = response.status();
    // Axum's Redirect::to() returns 303 See Other.
    assert!(
        status == StatusCode::SEE_OTHER || status == StatusCode::FOUND,
        "session cookie mode must redirect, got: {status}"
    );

    // ── Verify redirect target ───────────────────────────────────────────
    let redirect_location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .expect("redirect response must have Location header");
    assert_eq!(
        redirect_location, "https://app.example.com/dashboard",
        "redirect must point to post_login_redirect_uri, not the caller's redirect_uri"
    );

    // ── Verify Set-Cookie header ─────────────────────────────────────────
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .expect("session cookie mode must set a Set-Cookie header");

    // Cookie name must use the __Host- prefix.
    assert!(
        set_cookie.starts_with("__Host-access_token="),
        "cookie must use __Host-access_token prefix, got: {set_cookie}"
    );

    // Cookie must contain the access token.
    assert!(
        set_cookie.contains("test-access-token-xyz"),
        "cookie must contain the access token, got: {set_cookie}"
    );

    // HttpOnly attribute prevents JavaScript access.
    assert!(
        set_cookie.contains("HttpOnly"),
        "cookie must have HttpOnly attribute, got: {set_cookie}"
    );

    // Secure attribute (required by __Host- prefix).
    assert!(
        set_cookie.contains("Secure"),
        "cookie must have Secure attribute, got: {set_cookie}"
    );

    // SameSite=Strict prevents CSRF via cross-origin requests.
    assert!(
        set_cookie.contains("SameSite=Strict"),
        "cookie must have SameSite=Strict, got: {set_cookie}"
    );

    // Path=/ is required by the __Host- cookie prefix.
    assert!(
        set_cookie.contains("Path=/"),
        "cookie must have Path=/, got: {set_cookie}"
    );

    // Max-Age must match the token's expires_in (3600).
    assert!(
        set_cookie.contains("Max-Age=3600"),
        "cookie Max-Age must match token expires_in (3600), got: {set_cookie}"
    );
}

// ---------------------------------------------------------------------------
// Redis PkceStateStore variant (requires REDIS_TEST_URL)
// ---------------------------------------------------------------------------

/// Same flow as the in-memory tests but using the Redis-backed PKCE state store.
///
/// Skipped unless `REDIS_TEST_URL` is set in the environment.
#[cfg(feature = "redis-pkce")]
#[tokio::test]
#[ignore = "requires REDIS_TEST_URL"]
async fn auth_pkce_flow_with_redis_store() {
    let redis_url =
        std::env::var("REDIS_TEST_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

    let pkce_store = PkceStateStore::new_redis(&redis_url, 300, None)
        .await
        .expect("Redis PKCE store must connect");

    let oidc_client = test_oidc_client();
    let state = Arc::new(AuthPkceState {
        pkce_store:              Arc::new(pkce_store),
        oidc_client:             Arc::new(oidc_client),
        http_client:             Arc::new(reqwest::Client::new()),
        post_login_redirect_uri: None,
    });
    let router = Router::new()
        .route("/auth/start", get(auth_start))
        .route("/auth/callback", get(auth_callback))
        .with_state(state);

    // Step 1
    let (status, location) =
        get_request(&router, "/auth/start?redirect_uri=https://app.example.com/after-login").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let loc = location.unwrap();
    let state_token = extract_state_param(&loc);

    // Step 2 — exchange fails (non-routable) but state is consumed
    let callback_uri = format!("/auth/callback?code=fake_code&state={state_token}");
    let (callback_status, _) = get_request(&router, &callback_uri).await;
    assert_ne!(callback_status, StatusCode::BAD_REQUEST);

    // Step 3 — replay rejected
    let (replay_status, _) = get_request(&router, &callback_uri).await;
    assert_eq!(replay_status, StatusCode::BAD_REQUEST);
}
