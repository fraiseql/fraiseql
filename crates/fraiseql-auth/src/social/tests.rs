#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    extract::connect_info::MockConnectInfo,
    http::{Request, StatusCode},
    routing::get,
};
use tower::ServiceExt as _;

use super::*;
use crate::{
    error::{AuthError, Result},
    provider::{TokenResponse, UserInfo},
    rate_limiting::{AuthRateLimitConfig, RateLimiters},
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
    // `social_authorize` extracts `ConnectInfo<SocketAddr>` for per-IP rate
    // limiting; `MockConnectInfo` supplies a peer address under `oneshot`.
    Router::new()
        .route("/auth/v1/authorize", get(social_authorize))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4567))))
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
    let state = build_test_state(vec![("google", "https://accounts.google.com/o/oauth2/v2/auth")]);
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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "unknown provider should return 400");
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

#[tokio::test]
async fn test_authorize_rate_limited_returns_429_before_filling_state_store() {
    // H25: every authorize inserts a CSRF state into the bounded store, so an
    // unthrottled IP can keep it full and deny social login for everyone. The
    // handler must rate-limit per IP and return 429 (with Retry-After) before
    // touching the store. A limit of 1 lets the first request through and
    // throttles the second from the same `MockConnectInfo` peer.
    let rate_limiters = Arc::new(RateLimiters::with_configs(
        AuthRateLimitConfig {
            enabled:      true,
            max_requests: 1,
            window_secs:  60,
        },
        AuthRateLimitConfig::per_ip_strict(),
        AuthRateLimitConfig::per_user_standard(),
        AuthRateLimitConfig::per_user_standard(),
        AuthRateLimitConfig::failed_login_attempts(),
    ));
    let mut registry = SocialProviderRegistry::new();
    registry.register(
        "github",
        Arc::new(MockOAuthProvider {
            name:     "github",
            base_url: "https://github.com/login/oauth/authorize".to_string(),
        }) as Arc<dyn OAuthProvider>,
    );
    let state = Arc::new(SocialLoginState {
        registry: Arc::new(registry),
        state_store: Arc::new(InMemoryStateStore::new()),
        rate_limiters,
    });

    let authorize = || {
        let app = build_app(Arc::clone(&state));
        app.oneshot(
            Request::builder()
                .uri("/auth/v1/authorize?provider=github")
                .body(Body::empty())
                .unwrap(),
        )
    };

    let first = authorize().await.unwrap();
    assert_eq!(first.status(), StatusCode::SEE_OTHER, "first request must be allowed");

    let second = authorize().await.unwrap();
    assert_eq!(
        second.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "a second request from the same IP must be rate-limited"
    );
    assert!(
        second.headers().get("retry-after").is_some(),
        "a 429 response must carry a Retry-After header"
    );
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
