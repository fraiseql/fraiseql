//! Unified social login entry point.
//!
//! Provides `GET /auth/v1/authorize?provider=<name>` — a single redirect entry
//! point that looks up the configured `OAuth` provider by name and redirects the
//! user to that provider's authorization URL with a `CSRF` state token.

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use axum::{
    Json,
    extract::{ConnectInfo, Query, State},
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
/// # Rate limiting
///
/// Rate-limited per client IP via the shared `auth_start` limiter. Without it,
/// each request inserts a `CSRF` state into the (bounded) state store, so an
/// unthrottled caller could keep the store full and deny social login for
/// everyone (audit H25) — the `rate_limiters` field was carried but never
/// consulted.
///
/// # Errors
///
/// Returns 429 Too Many Requests if the per-IP rate limit is exceeded.
/// Returns 400 Bad Request if `provider` is not registered.
/// Returns 500 Internal Server Error if the `CSRF` state store fails.
pub async fn social_authorize(
    State(state): State<Arc<SocialLoginState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<SocialAuthorizeParams>,
) -> Response {
    let logger = get_audit_logger();

    // Rate-limit per transport peer IP before touching the state store, so a
    // flood cannot fill it (H25). Keyed on the connection peer only — never an
    // attacker-spoofable forwarded header.
    let client_ip = addr.ip().to_string();
    if state.rate_limiters.auth_start.check(&client_ip).is_err() {
        let retry_after = state.rate_limiters.auth_start.clone_config().window_secs;
        logger.log_failure(
            AuditEventType::AuthFailure,
            SecretType::StateToken,
            None,
            "social_authorize",
            "rate limited",
        );
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::RETRY_AFTER, retry_after.to_string())],
            Json(serde_json::json!({
                "error":   "rate_limited",
                "message": "Too many authorization requests; please retry later"
            })),
        )
            .into_response();
    }

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

#[cfg(test)]
mod tests;
