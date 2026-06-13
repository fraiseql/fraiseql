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
    account_linking::AccountStore, handlers::generate_secure_state, provider::OAuthProvider,
    session::SessionStore, state_store::StateStore,
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
    /// Optional user store for account linking (same email → same user).
    user_store:    Option<Arc<dyn AccountStore>>,
}

impl MultiProviderAuthState {
    /// Create a new multi-provider auth state.
    pub fn new(state_store: Arc<dyn StateStore>, session_store: Arc<dyn SessionStore>) -> Self {
        Self {
            providers: HashMap::new(),
            state_store,
            session_store,
            user_store: None,
        }
    }

    /// Set the user store for account linking.
    ///
    /// When set, the callback handler uses [`AccountStore::link_or_create_user`] to
    /// resolve provider identities to local users, enabling automatic account
    /// linking when the same email appears across different providers.
    pub fn with_user_store(mut self, user_store: Arc<dyn AccountStore>) -> Self {
        self.user_store = Some(user_store);
        self
    }

    /// Register an OAuth provider under the given name.
    pub fn register_provider(&mut self, name: impl Into<String>, provider: Arc<dyn OAuthProvider>) {
        self.providers.insert(name.into(), provider);
    }

    /// List the names of all registered providers.
    #[must_use]
    pub fn provider_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.providers.keys().cloned().collect();
        names.sort();
        names
    }

    /// Look up a provider by name.
    #[must_use]
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

impl AuthTokenResponse {
    /// Returns a builder for `AuthTokenResponse`.
    #[must_use = "builder does nothing until .build() is called"]
    pub fn builder() -> AuthTokenResponseBuilder {
        AuthTokenResponseBuilder::default()
    }
}

/// Builder for [`AuthTokenResponse`].
#[derive(Debug, Default)]
pub struct AuthTokenResponseBuilder {
    access_token:  Option<String>,
    refresh_token: Option<String>,
    token_type:    Option<String>,
    expires_in:    Option<u64>,
    provider:      Option<String>,
}

impl AuthTokenResponseBuilder {
    /// Sets the access token.
    pub fn access_token(mut self, access_token: impl Into<String>) -> Self {
        self.access_token = Some(access_token.into());
        self
    }

    /// Sets the refresh token.
    pub fn refresh_token(mut self, refresh_token: impl Into<String>) -> Self {
        self.refresh_token = Some(refresh_token.into());
        self
    }

    /// Sets the token type (typically `"Bearer"`).
    pub fn token_type(mut self, token_type: impl Into<String>) -> Self {
        self.token_type = Some(token_type.into());
        self
    }

    /// Sets the number of seconds until the access token expires.
    #[must_use = "builder method returns modified builder"]
    pub const fn expires_in(mut self, expires_in: u64) -> Self {
        self.expires_in = Some(expires_in);
        self
    }

    /// Sets the provider that authenticated the user.
    pub fn provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    /// Builds the [`AuthTokenResponse`].
    ///
    /// # Errors
    ///
    /// Returns an error string if any required field (`access_token`, `token_type`,
    /// `expires_in`, or `provider`) was not set.
    pub fn build(self) -> Result<AuthTokenResponse, String> {
        Ok(AuthTokenResponse {
            access_token:  self
                .access_token
                .ok_or("AuthTokenResponse: access_token is required")?,
            refresh_token: self.refresh_token,
            token_type:    self.token_type.ok_or("AuthTokenResponse: token_type is required")?,
            expires_in:    self.expires_in.ok_or("AuthTokenResponse: expires_in is required")?,
            provider:      self.provider.ok_or("AuthTokenResponse: provider is required")?,
        })
    }
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
/// - `redirect_uri` — **required**: client application callback URI. It is validated for presence
///   and length but is **not currently used for a server-side redirect**: [`callback`] returns the
///   session tokens as JSON for the client to handle. A server-side redirect to this URI is
///   intentionally not implemented yet because it would be an open-redirect vector without a
///   configured allow-list of permitted redirect URIs. Tracked as a follow-up feature in #427
///   (allow-list-backed redirect flow).
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
        return json_error(StatusCode::BAD_REQUEST, &format!("unknown provider: {}", q.provider));
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

    if let Err(e) = state.state_store.store(state_value.clone(), q.provider.clone(), expiry).await {
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

    // Check state expiry. Fail-closed: if the clock cannot be read, reject rather than
    // treat the (possibly expired) CSRF state as valid (matches the authorize path).
    let Ok(now) = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
    else {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "system clock error");
    };

    if now > expiry {
        return json_error(StatusCode::BAD_REQUEST, "state token expired");
    }

    // Look up provider
    let Some(provider) = state.get_provider(&provider_name) else {
        tracing::error!(provider = %provider_name, "provider from state not found in registry");
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "provider configuration error");
    };

    // Exchange code for tokens
    let token_response = match provider.exchange_code(&code).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "token exchange failed");
            return json_error(StatusCode::BAD_GATEWAY, "token exchange with provider failed");
        },
    };

    // Get user info from provider
    let user_info = match provider.user_info(&token_response.access_token).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(error = %e, "user info fetch failed");
            return json_error(StatusCode::BAD_GATEWAY, "failed to retrieve user information");
        },
    };

    // Resolve local user ID — use AccountStore for account linking when available,
    // otherwise fall back to raw provider user ID.
    let local_user_id = if let Some(account_store) = &state.user_store {
        match account_store
            .link_or_create_user(
                user_info.email.as_deref(),
                user_info.email_verified,
                &provider_name,
                &user_info.id,
            )
            .await
        {
            Ok(result) => result.user_id,
            Err(e) => {
                tracing::error!(error = %e, "account store lookup failed");
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, "user resolution failed");
            },
        }
    } else {
        user_info.id.clone()
    };

    // Create session (7-day expiry)
    let session_expiry = now + (7 * 24 * 60 * 60);
    let session_tokens = match state
        .session_store
        .create_session(&local_user_id, session_expiry)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "session creation failed");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "session could not be created");
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
