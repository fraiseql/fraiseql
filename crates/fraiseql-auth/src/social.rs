//! Unified social login entry point.
//!
//! Provides `GET /auth/v1/authorize?provider=<name>` — a single redirect entry
//! point that looks up the configured `OAuth` provider by name and redirects the
//! user to that provider's authorization URL with a `CSRF` state token.

use std::{collections::HashMap, sync::Arc};

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use base64::Engine as _;
use serde::Deserialize;

use crate::{
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    provider::OAuthProvider,
    rate_limiting::RateLimiters,
    session::unix_now,
    state_store::StateStore,
};

// ─── Provider registry ───────────────────────────────────────────────────────

/// Registry of configured `OAuth` providers, keyed by provider name.
pub struct SocialProviderRegistry {
    providers: HashMap<String, Arc<dyn OAuthProvider>>,
}

impl SocialProviderRegistry {
    /// Create an empty provider registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register an `OAuth` provider under `name`.
    pub fn register(&mut self, name: impl Into<String>, provider: Arc<dyn OAuthProvider>) {
        self.providers.insert(name.into(), provider);
    }

    /// Look up a provider by name. Returns `None` if the provider is not registered.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn OAuthProvider>> {
        self.providers.get(name).cloned()
    }

    /// Return the list of registered provider names (order is unspecified).
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.providers.keys().map(String::as_str).collect()
    }

    /// Return the number of registered providers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Return `true` if no providers are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl Default for SocialProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Route state ─────────────────────────────────────────────────────────────

/// Axum route state for social-login endpoints.
#[derive(Clone)]
pub struct SocialLoginState {
    /// Configured `OAuth` provider registry.
    pub registry:      Arc<SocialProviderRegistry>,
    /// `OAuth` `CSRF` state store.
    pub state_store:   Arc<dyn StateStore>,
    /// Rate limiters for auth endpoints (per-IP sliding-window).
    pub rate_limiters: Arc<RateLimiters>,
}

// ─── Query params ─────────────────────────────────────────────────────────────

/// Query parameters for `GET /auth/v1/authorize`.
#[derive(Debug, Deserialize)]
pub struct SocialAuthorizeParams {
    /// `OAuth` provider name (e.g. `"github"`, `"google"`).
    pub provider: String,
}

// ─── Constants ───────────────────────────────────────────────────────────────

/// `OAuth` state `TTL` in seconds (10 minutes).
const STATE_TTL_SECS: u64 = 600;

// ─── Handler ─────────────────────────────────────────────────────────────────

/// `GET /auth/v1/authorize?provider=<name>`
///
/// Looks up the named `OAuth` provider in the registry and returns a 302 redirect
/// to that provider's authorization URL, embedding a `CSRF` state token. Returns
/// 400 if the provider is not configured.
///
/// # Errors
///
/// Returns 400 Bad Request if `provider` is not registered.
/// Returns 500 Internal Server Error if the `CSRF` state store fails.
pub async fn social_authorize(
    State(state): State<Arc<SocialLoginState>>,
    Query(params): Query<SocialAuthorizeParams>,
) -> Response {
    let logger = get_audit_logger();

    let Some(provider) = state.registry.get(&params.provider) else {
        logger.log_failure(
            AuditEventType::AuthFailure,
            SecretType::StateToken,
            None,
            "social_authorize",
            &format!("unknown provider: {}", params.provider),
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error":   "unknown_provider",
                "message": format!("Provider '{}' is not configured", params.provider)
            })),
        )
            .into_response();
    };

    // Generate a cryptographically random CSRF state token.
    let csrf_state = generate_state_token();

    // Compute expiry for the state entry.
    let expiry = match unix_now() {
        Ok(now) => now + STATE_TTL_SECS,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        },
    };

    // Store the state so the callback can validate it.
    if let Err(e) = state
        .state_store
        .store(csrf_state.clone(), params.provider.clone(), expiry)
        .await
    {
        tracing::error!(error = %e, "Failed to store OAuth CSRF state");
        return (StatusCode::INTERNAL_SERVER_ERROR, "failed to store auth state").into_response();
    }

    logger.log_success(
        AuditEventType::OauthStart,
        SecretType::StateToken,
        None,
        &format!("social_authorize:{}", params.provider),
    );

    // Redirect the user to the provider's authorization URL.
    let auth_url = provider.authorization_url(&csrf_state);
    Redirect::to(&auth_url).into_response()
}

// ─── Token generation ─────────────────────────────────────────────────────────

/// Generate a 32-byte cryptographically random `URL`-safe base64 state token.
///
/// # SECURITY
///
/// `OsRng` ensures `OS`-level entropy, avoiding the bootstrapping window where
/// `thread_rng()` may be predictable at process startup.
fn generate_state_token() -> String {
    use rand::RngCore as _;
    // SECURITY: rand::rng() uses OS-level entropy for OAuth CSRF state tokens.
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use tower::ServiceExt as _;

    use super::*;
    use crate::{
        error::{AuthError, Result},
        provider::{TokenResponse, UserInfo},
        rate_limiting::RateLimiters,
        state_store::InMemoryStateStore,
    };

    // ── Mock provider ─────────────────────────────────────────────────────

    /// Minimal mock `OAuth` provider for unit tests.
    #[derive(Debug, Clone)]
    struct MockOAuthProvider {
        name:     &'static str,
        base_url: String,
    }

    // Reason: async_trait required for dyn-compatibility; remove when RTN + Send is stable
    #[async_trait]
    impl OAuthProvider for MockOAuthProvider {
        fn name(&self) -> &str {
            self.name
        }

        fn authorization_url(&self, state: &str) -> String {
            format!("{}?state={}", self.base_url, state)
        }

        async fn exchange_code(&self, _code: &str) -> Result<TokenResponse> {
            Err(AuthError::OAuthError {
                message: "mock: not implemented".into(),
            })
        }

        async fn user_info(&self, _access_token: &str) -> Result<UserInfo> {
            Err(AuthError::OAuthError {
                message: "mock: not implemented".into(),
            })
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    fn build_test_state(providers: Vec<(&'static str, &'static str)>) -> Arc<SocialLoginState> {
        let mut registry = SocialProviderRegistry::new();
        for (name, base_url) in providers {
            registry.register(
                name,
                Arc::new(MockOAuthProvider {
                    name,
                    base_url: base_url.to_string(),
                }) as Arc<dyn OAuthProvider>,
            );
        }
        Arc::new(SocialLoginState {
            registry:      Arc::new(registry),
            state_store:   Arc::new(InMemoryStateStore::new()),
            rate_limiters: Arc::new(RateLimiters::new()),
        })
    }

    fn build_app(state: Arc<SocialLoginState>) -> Router {
        Router::new()
            .route("/auth/v1/authorize", get(social_authorize))
            .with_state(state)
    }

    /// Execute a single `GET /auth/v1/authorize?provider=github` request and
    /// extract the `state=` query value from the Location header.
    async fn extract_state_token(state: Arc<SocialLoginState>) -> String {
        let app = build_app(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/auth/v1/authorize?provider=github")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let loc = resp.headers().get("location").unwrap().to_str().unwrap().to_string();
        loc.split("?state=").nth(1).unwrap().to_string()
    }

    // ── Cycle 1 tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_authorize_known_provider_returns_redirect() {
        let state = build_test_state(vec![("github", "https://github.com/login/oauth/authorize")]);
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/v1/authorize?provider=github")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // axum's Redirect::to returns 303 See Other
        assert_eq!(response.status(), StatusCode::SEE_OTHER, "known provider should redirect");

        let location = response.headers().get("location").unwrap().to_str().unwrap();
        assert!(
            location.starts_with("https://github.com/login/oauth/authorize?state="),
            "redirect location should point to provider URL, got: {location}"
        );
    }

    #[tokio::test]
    async fn test_authorize_google_provider_redirects_to_google() {
        let state =
            build_test_state(vec![("google", "https://accounts.google.com/o/oauth2/v2/auth")]);
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/v1/authorize?provider=google")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response.headers().get("location").unwrap().to_str().unwrap();
        assert!(
            location.starts_with("https://accounts.google.com/o/oauth2/v2/auth?state="),
            "google redirect location malformed: {location}"
        );
    }

    #[tokio::test]
    async fn test_authorize_unknown_provider_returns_400() {
        let state = build_test_state(vec![("github", "https://github.com/login/oauth/authorize")]);
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/v1/authorize?provider=bitbucket")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "unknown provider should return 400"
        );
    }

    #[tokio::test]
    async fn test_authorize_empty_registry_returns_400() {
        let state = build_test_state(vec![]);
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/v1/authorize?provider=github")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_authorize_state_token_stored_in_state_store() {
        // Set up state with a shared state store we can inspect afterwards.
        let state_store = Arc::new(InMemoryStateStore::new());
        let mut registry = SocialProviderRegistry::new();
        registry.register(
            "github",
            Arc::new(MockOAuthProvider {
                name:     "github",
                base_url: "https://github.com/login/oauth/authorize".to_string(),
            }) as Arc<dyn OAuthProvider>,
        );

        let login_state = Arc::new(SocialLoginState {
            registry:      Arc::new(registry),
            state_store:   Arc::clone(&state_store) as Arc<dyn StateStore>,
            rate_limiters: Arc::new(RateLimiters::new()),
        });

        let app = build_app(login_state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/v1/authorize?provider=github")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);

        // Extract the `state` param from the Location header and verify it is
        // stored in the state store (retrieve consumes it — single-use).
        let location = response.headers().get("location").unwrap().to_str().unwrap();
        let state_value = location.split("?state=").nth(1).unwrap();
        let result = state_store.retrieve(state_value).await;
        assert!(
            result.is_ok(),
            "CSRF state token should be stored after authorize, got: {result:?}"
        );
        let (provider, _expiry) = result.unwrap();
        assert_eq!(provider, "github");
    }

    #[tokio::test]
    async fn test_authorize_produces_unique_state_tokens() {
        // Two successive authorize calls must produce different CSRF tokens.
        let state_store = Arc::new(InMemoryStateStore::new());
        let mut registry = SocialProviderRegistry::new();
        registry.register(
            "github",
            Arc::new(MockOAuthProvider {
                name:     "github",
                base_url: "https://github.com/login/oauth/authorize".to_string(),
            }) as Arc<dyn OAuthProvider>,
        );
        let login_state = Arc::new(SocialLoginState {
            registry:      Arc::new(registry),
            state_store:   Arc::clone(&state_store) as Arc<dyn StateStore>,
            rate_limiters: Arc::new(RateLimiters::new()),
        });

        let token1 = extract_state_token(Arc::clone(&login_state)).await;
        let token2 = extract_state_token(login_state).await;
        assert_ne!(token1, token2, "each authorize call must produce a unique CSRF state token");
    }

    #[test]
    fn test_social_provider_registry_register_and_get() {
        let mut registry = SocialProviderRegistry::new();
        assert!(registry.is_empty());

        registry.register(
            "github",
            Arc::new(MockOAuthProvider {
                name:     "github",
                base_url: "https://github.com/login/oauth/authorize".to_string(),
            }) as Arc<dyn OAuthProvider>,
        );

        assert_eq!(registry.len(), 1);
        assert!(registry.get("github").is_some());
        assert!(registry.get("google").is_none());
    }

    #[test]
    fn test_social_provider_registry_names() {
        let mut registry = SocialProviderRegistry::new();
        registry.register(
            "github",
            Arc::new(MockOAuthProvider {
                name:     "github",
                base_url: String::new(),
            }) as Arc<dyn OAuthProvider>,
        );
        registry.register(
            "google",
            Arc::new(MockOAuthProvider {
                name:     "google",
                base_url: String::new(),
            }) as Arc<dyn OAuthProvider>,
        );

        let mut names = registry.names();
        names.sort_unstable();
        assert_eq!(names, vec!["github", "google"]);
    }
}
