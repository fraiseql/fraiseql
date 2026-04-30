//! Multi-provider authentication — unified entry point for social login.
//!
//! Enables `GET /auth/v1/authorize?provider=github&redirect_uri=...` with
//! automatic provider resolution and state-encoded provider tracking through
//! the OAuth callback.

use std::{collections::HashMap, sync::Arc};

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    handlers::generate_secure_state,
    provider::OAuthProvider,
    session::SessionStore,
    state_store::StateStore,
};

/// Maximum length for the `redirect_uri` query parameter.
const MAX_REDIRECT_URI_BYTES: usize = 2_048;

/// Maximum length for the `provider` query parameter.
const MAX_PROVIDER_NAME_BYTES: usize = 128;

/// Shared state for the multi-provider auth endpoints.
#[derive(Clone)]
pub struct MultiProviderAuthState {
    /// OAuth providers keyed by name (e.g., "github", "google").
    providers:     HashMap<String, Arc<dyn OAuthProvider>>,
    /// CSRF state store (in-memory or Redis).
    state_store:   Arc<dyn StateStore>,
    /// Session backend for creating sessions after successful auth.
    session_store: Arc<dyn SessionStore>,
}

impl MultiProviderAuthState {
    /// Create a new multi-provider auth state.
    pub fn new(
        state_store: Arc<dyn StateStore>,
        session_store: Arc<dyn SessionStore>,
    ) -> Self {
        Self {
            providers: HashMap::new(),
            state_store,
            session_store,
        }
    }

    /// Register an OAuth provider under the given name.
    pub fn register_provider(&mut self, name: impl Into<String>, provider: Arc<dyn OAuthProvider>) {
        self.providers.insert(name.into(), provider);
    }

    /// List the names of all registered providers.
    pub fn provider_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.providers.keys().cloned().collect();
        names.sort();
        names
    }

    /// Look up a provider by name.
    pub fn get_provider(&self, name: &str) -> Option<&Arc<dyn OAuthProvider>> {
        self.providers.get(name)
    }
}

// ---------------------------------------------------------------------------
// Query / response types
// ---------------------------------------------------------------------------

/// Query parameters for `GET /auth/v1/authorize`.
#[derive(Debug, Deserialize)]
pub struct AuthorizeQuery {
    /// Provider name (e.g., "github", "google").
    pub provider:     String,
    /// Client application callback URI.
    pub redirect_uri: String,
}

/// Query parameters for `GET /auth/v1/callback`.
#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    /// Authorization code from the provider.
    pub code:              Option<String>,
    /// CSRF state token.
    pub state:             Option<String>,
    /// Provider error code.
    pub error:             Option<String>,
    /// Provider error description.
    pub error_description: Option<String>,
}

/// Response for `GET /auth/v1/providers`.
#[derive(Debug, Serialize)]
pub struct ProvidersResponse {
    /// Available provider names.
    pub providers: Vec<String>,
}

/// Token response returned after a successful callback.
#[derive(Debug, Serialize)]
pub struct AuthTokenResponse {
    /// Access token for API requests.
    pub access_token:  String,
    /// Refresh token (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Token type (always "Bearer").
    pub token_type:    String,
    /// Seconds until the access token expires.
    pub expires_in:    u64,
    /// Provider that authenticated the user.
    pub provider:      String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

// ---------------------------------------------------------------------------
// GET /auth/v1/providers
// ---------------------------------------------------------------------------

/// List available authentication providers.
///
/// # Responses
///
/// - `200` JSON `{ providers: ["github", "google", ...] }`
pub async fn list_providers(
    State(state): State<Arc<MultiProviderAuthState>>,
) -> Json<ProvidersResponse> {
    Json(ProvidersResponse {
        providers: state.provider_names(),
    })
}

// ---------------------------------------------------------------------------
// GET /auth/v1/authorize
// ---------------------------------------------------------------------------

/// Initiate the OAuth flow for a specific provider.
///
/// Generates a CSRF state token, stores it with the provider name, then
/// redirects to the provider's authorization URL.
///
/// # Query parameters
///
/// - `provider` — **required**: provider name (must match a registered provider).
/// - `redirect_uri` — **required**: client application callback URI.
///
/// # Responses
///
/// - `302` — redirect to the provider's authorization endpoint.
/// - `400` — missing or invalid parameters, unknown provider.
///
/// # Errors
///
/// Returns a `400` JSON error if the provider is unknown, redirect_uri is empty/oversized,
/// or the state store is at capacity.
pub async fn authorize(
    State(state): State<Arc<MultiProviderAuthState>>,
    Query(q): Query<AuthorizeQuery>,
) -> Response {
    // Validate provider name length
    if q.provider.len() > MAX_PROVIDER_NAME_BYTES {
        return json_error(StatusCode::BAD_REQUEST, "provider name exceeds maximum length");
    }

    // Validate redirect_uri
    if q.redirect_uri.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "redirect_uri is required");
    }
    if q.redirect_uri.len() > MAX_REDIRECT_URI_BYTES {
        return json_error(StatusCode::BAD_REQUEST, "redirect_uri exceeds maximum length");
    }

    // Look up provider
    let Some(provider) = state.get_provider(&q.provider) else {
        return json_error(
            StatusCode::BAD_REQUEST,
            &format!("unknown provider: {}", q.provider),
        );
    };

    // Generate state and store with provider name
    let state_value = generate_secure_state();

    let Ok(now) = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
    else {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "system clock error");
    };

    let expiry = now + 600; // 10 minutes

    if let Err(e) = state
        .state_store
        .store(state_value.clone(), q.provider.clone(), expiry)
        .await
    {
        tracing::error!("state store failed: {e}");
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "authorization flow could not be started",
        );
    }

    // Generate authorization URL
    let authorization_url = provider.authorization_url(&state_value);

    Redirect::to(&authorization_url).into_response()
}

// ---------------------------------------------------------------------------
// GET /auth/v1/callback
// ---------------------------------------------------------------------------

/// Complete the OAuth flow after the provider redirects back.
///
/// Validates the state token, resolves the provider from the stored state,
/// exchanges the authorization code for tokens, retrieves user info, and
/// creates a session.
///
/// # Query parameters
///
/// - `code` — authorization code from the provider.
/// - `state` — CSRF state token.
///
/// # Responses
///
/// - `200` JSON `{ access_token, refresh_token?, token_type, expires_in, provider }`
/// - `400` — invalid state, missing parameters, or provider error.
/// - `502` — token exchange with the provider failed.
///
/// # Errors
///
/// Returns `400` if the state is invalid/expired, code is missing, or the provider
/// returned an error. Returns `502` if the token exchange or user info fetch fails.
#[allow(clippy::cognitive_complexity)] // Reason: OAuth callback with state validation, token exchange, user info, and session creation
pub async fn callback(
    State(state): State<Arc<MultiProviderAuthState>>,
    Query(q): Query<CallbackQuery>,
) -> Response {
    // Surface provider errors
    if let Some(err) = q.error {
        let desc = q.error_description.as_deref().unwrap_or("(no description)");
        tracing::warn!(provider_error = %err, description = %desc, "OAuth provider returned error");
        let client_message = match err.as_str() {
            "access_denied" => "Access was denied",
            "login_required" => "Authentication is required",
            "invalid_request" | "invalid_scope" => "Invalid authorization request",
            "server_error" | "temporarily_unavailable" => "Authorization server error",
            _ => "Authorization failed",
        };
        return json_error(StatusCode::BAD_REQUEST, client_message);
    }

    // Validate required parameters
    let (Some(code), Some(state_token)) = (q.code, q.state) else {
        return json_error(StatusCode::BAD_REQUEST, "missing code or state parameter");
    };

    // Consume state (atomic remove) and get provider name
    let Ok((provider_name, expiry)) = state.state_store.retrieve(&state_token).await else {
        return json_error(StatusCode::BAD_REQUEST, "invalid or expired state token");
    };

    // Check state expiry
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if now > expiry {
        return json_error(StatusCode::BAD_REQUEST, "state token expired");
    }

    // Look up provider
    let Some(provider) = state.get_provider(&provider_name) else {
        tracing::error!(provider = %provider_name, "provider from state not found in registry");
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "provider configuration error",
        );
    };

    // Exchange code for tokens
    let token_response = match provider.exchange_code(&code).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "token exchange failed");
            return json_error(StatusCode::BAD_GATEWAY, "token exchange with provider failed");
        },
    };

    // Get user info
    let user_info = match provider.user_info(&token_response.access_token).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(error = %e, "user info fetch failed");
            return json_error(StatusCode::BAD_GATEWAY, "failed to retrieve user information");
        },
    };

    // Create session (7-day expiry)
    let session_expiry = now + (7 * 24 * 60 * 60);
    let session_tokens = match state.session_store.create_session(&user_info.id, session_expiry).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "session creation failed");
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session could not be created",
            );
        },
    };

    Json(AuthTokenResponse {
        access_token:  session_tokens.access_token,
        refresh_token: Some(session_tokens.refresh_token),
        token_type:    "Bearer".to_string(),
        expires_in:    session_tokens.expires_in,
        provider:      provider_name,
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt as _;

    use super::*;
    use crate::{
        error::Result as AuthResult,
        provider::{OAuthProvider, TokenResponse, UserInfo},
        session::InMemorySessionStore,
        state_store::InMemoryStateStore,
    };

    // ── Mock provider ────────────────────────────────────────────────────

    #[derive(Debug, Clone)]
    struct MockProvider {
        name:      String,
        auth_url:  String,
        user_info: UserInfo,
    }

    impl MockProvider {
        fn new(name: &str) -> Self {
            Self {
                name:      name.to_string(),
                auth_url:  format!("https://{name}.example.com/authorize"),
                user_info: UserInfo {
                    id:         format!("{name}-user-1"),
                    email:      format!("user@{name}.com"),
                    name:       Some("Test User".to_string()),
                    picture:    None,
                    raw_claims: serde_json::json!({}),
                },
            }
        }
    }

    #[async_trait]
    impl OAuthProvider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn authorization_url(&self, state: &str) -> String {
            format!("{}?state={}&client_id=test", self.auth_url, state)
        }

        async fn exchange_code(&self, _code: &str) -> AuthResult<TokenResponse> {
            Ok(TokenResponse {
                access_token:  "mock_access_token".to_string(),
                refresh_token: Some("mock_refresh_token".to_string()),
                expires_in:    3600,
                token_type:    "Bearer".to_string(),
            })
        }

        async fn user_info(&self, _access_token: &str) -> AuthResult<UserInfo> {
            Ok(self.user_info.clone())
        }
    }

    // ── Test helpers ─────────────────────────────────────────────────────

    fn build_multi_provider_state(providers: Vec<(&str, MockProvider)>) -> Arc<MultiProviderAuthState> {
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
        let mut auth_state = MultiProviderAuthState::new(state_store, session_store);
        for (name, provider) in providers {
            auth_state.register_provider(name, Arc::new(provider));
        }
        Arc::new(auth_state)
    }

    fn multi_auth_router(state: Arc<MultiProviderAuthState>) -> Router {
        Router::new()
            .route("/auth/v1/providers", get(list_providers))
            .route("/auth/v1/authorize", get(authorize))
            .route("/auth/v1/callback", get(callback))
            .with_state(state)
    }

    // ── /auth/v1/providers tests ─────────────────────────────────────────

    #[tokio::test]
    async fn test_list_providers_returns_registered_providers() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
            ("google", MockProvider::new("google")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/providers")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let providers = json["providers"].as_array().unwrap();
        assert_eq!(providers.len(), 2);
        assert!(providers.contains(&serde_json::json!("github")));
        assert!(providers.contains(&serde_json::json!("google")));
    }

    #[tokio::test]
    async fn test_list_providers_empty_when_none_registered() {
        let state = build_multi_provider_state(vec![]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/providers")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["providers"].as_array().unwrap().len(), 0);
    }

    // ── /auth/v1/authorize tests ─────────────────────────────────────────

    #[tokio::test]
    async fn test_authorize_redirects_to_provider() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/authorize?provider=github&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert!(
            resp.status().is_redirection(),
            "expected redirect, got {}",
            resp.status()
        );

        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .expect("Location header must be present");

        assert!(
            location.starts_with("https://github.example.com/authorize"),
            "redirect should go to github authorize URL, got: {location}"
        );
        assert!(location.contains("state="), "redirect must include state parameter");
        assert!(location.contains("client_id=test"), "redirect must include client_id");
    }

    #[tokio::test]
    async fn test_authorize_unknown_provider_returns_400() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/authorize?provider=twitter&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["error"].as_str().unwrap().contains("unknown provider"),
            "error must mention unknown provider: {json}"
        );
    }

    #[tokio::test]
    async fn test_authorize_missing_redirect_uri_returns_400() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        // Missing redirect_uri entirely → axum returns 422 (deserialization failure)
        let req = Request::builder()
            .uri("/auth/v1/authorize?provider=github")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert!(resp.status().is_client_error());
    }

    #[tokio::test]
    async fn test_authorize_empty_redirect_uri_returns_400() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/authorize?provider=github&redirect_uri=")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_authorize_oversized_redirect_uri_returns_400() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let long_uri = "https://example.com/".to_string() + &"a".repeat(2100);
        let encoded = urlencoding::encode(&long_uri);
        let req = Request::builder()
            .uri(format!("/auth/v1/authorize?provider=github&redirect_uri={encoded}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── /auth/v1/callback tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_callback_unknown_state_returns_400() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/callback?code=test123&state=unknown-state-token")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_callback_missing_code_returns_400() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/callback?state=some-state")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_callback_provider_error_returns_sanitized_message() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/callback?error=access_denied&error_description=internal+details")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error_msg = json["error"].as_str().unwrap();
        assert_eq!(error_msg, "Access was denied");
        assert!(!error_msg.contains("internal details"));
    }

    // ── Full round-trip: authorize → callback ────────────────────────────

    #[tokio::test]
    async fn test_authorize_to_callback_round_trip() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        // Step 1: GET /auth/v1/authorize → 302 redirect with state
        let req = Request::builder()
            .uri("/auth/v1/authorize?provider=github&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();

        assert!(resp.status().is_redirection());

        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap()
            .to_string();

        // Extract state from redirect URL
        let parsed = reqwest::Url::parse(&location).unwrap();
        let state_token = parsed
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned())
            .expect("state must be in redirect URL");

        // Step 2: GET /auth/v1/callback with the state from step 1
        let callback_uri = format!("/auth/v1/callback?code=auth_code_123&state={state_token}");
        let req2 = Request::builder()
            .uri(&callback_uri)
            .body(Body::empty())
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();

        assert_eq!(resp2.status(), StatusCode::OK, "callback should return 200");

        let body = axum::body::to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json["access_token"].is_string(), "must have access_token");
        assert!(json["refresh_token"].is_string(), "must have refresh_token");
        assert_eq!(json["token_type"], "Bearer");
        assert!(json["expires_in"].is_number(), "must have expires_in");
        assert_eq!(json["provider"], "github", "must include provider name");
    }

    #[tokio::test]
    async fn test_callback_state_consumed_on_first_use() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        // Authorize to get a state token
        let req = Request::builder()
            .uri("/auth/v1/authorize?provider=github&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let location = resp.headers().get("location").unwrap().to_str().unwrap().to_string();
        let parsed = reqwest::Url::parse(&location).unwrap();
        let state_token = parsed
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned())
            .unwrap();

        // First callback succeeds
        let req1 = Request::builder()
            .uri(format!("/auth/v1/callback?code=code1&state={state_token}"))
            .body(Body::empty())
            .unwrap();
        let resp1 = app.clone().oneshot(req1).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);

        // Replay attempt fails
        let req2 = Request::builder()
            .uri(format!("/auth/v1/callback?code=code2&state={state_token}"))
            .body(Body::empty())
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::BAD_REQUEST, "state replay must be rejected");
    }

    // ── Multi-provider isolation ─────────────────────────────────────────

    #[tokio::test]
    async fn test_different_providers_produce_different_callbacks() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
            ("google", MockProvider::new("google")),
        ]);
        let app = multi_auth_router(state);

        // Authorize with github
        let req_gh = Request::builder()
            .uri("/auth/v1/authorize?provider=github&redirect_uri=https%3A%2F%2Fapp.example.com")
            .body(Body::empty())
            .unwrap();
        let resp_gh = app.clone().oneshot(req_gh).await.unwrap();
        let loc_gh = resp_gh.headers().get("location").unwrap().to_str().unwrap().to_string();

        // Authorize with google
        let req_gg = Request::builder()
            .uri("/auth/v1/authorize?provider=google&redirect_uri=https%3A%2F%2Fapp.example.com")
            .body(Body::empty())
            .unwrap();
        let resp_gg = app.clone().oneshot(req_gg).await.unwrap();
        let loc_gg = resp_gg.headers().get("location").unwrap().to_str().unwrap().to_string();

        assert!(loc_gh.starts_with("https://github.example.com/"), "github redirect wrong");
        assert!(loc_gg.starts_with("https://google.example.com/"), "google redirect wrong");

        // Extract state tokens
        let state_gh = reqwest::Url::parse(&loc_gh).unwrap()
            .query_pairs().find(|(k, _)| k == "state").map(|(_, v)| v.into_owned()).unwrap();
        let state_gg = reqwest::Url::parse(&loc_gg).unwrap()
            .query_pairs().find(|(k, _)| k == "state").map(|(_, v)| v.into_owned()).unwrap();

        // Callback with github state → provider="github"
        let req_cb_gh = Request::builder()
            .uri(format!("/auth/v1/callback?code=c1&state={state_gh}"))
            .body(Body::empty())
            .unwrap();
        let resp_cb_gh = app.clone().oneshot(req_cb_gh).await.unwrap();
        let body_gh = axum::body::to_bytes(resp_cb_gh.into_body(), usize::MAX).await.unwrap();
        let json_gh: serde_json::Value = serde_json::from_slice(&body_gh).unwrap();
        assert_eq!(json_gh["provider"], "github");

        // Callback with google state → provider="google"
        let req_cb_gg = Request::builder()
            .uri(format!("/auth/v1/callback?code=c2&state={state_gg}"))
            .body(Body::empty())
            .unwrap();
        let resp_cb_gg = app.oneshot(req_cb_gg).await.unwrap();
        let body_gg = axum::body::to_bytes(resp_cb_gg.into_body(), usize::MAX).await.unwrap();
        let json_gg: serde_json::Value = serde_json::from_slice(&body_gg).unwrap();
        assert_eq!(json_gg["provider"], "google");
    }

    // ── MultiProviderAuthState unit tests ────────────────────────────────

    #[test]
    fn test_provider_names_sorted() {
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
        let mut auth_state = MultiProviderAuthState::new(state_store, session_store);
        auth_state.register_provider("google", Arc::new(MockProvider::new("google")));
        auth_state.register_provider("auth0", Arc::new(MockProvider::new("auth0")));
        auth_state.register_provider("github", Arc::new(MockProvider::new("github")));

        let names = auth_state.provider_names();
        assert_eq!(names, vec!["auth0", "github", "google"]);
    }

    #[test]
    fn test_get_provider_returns_none_for_unknown() {
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
        let auth_state = MultiProviderAuthState::new(state_store, session_store);
        assert!(auth_state.get_provider("nonexistent").is_none());
    }
}
