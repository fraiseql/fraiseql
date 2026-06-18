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
use url::Url;

use crate::{
    account_linking::AccountStore, handlers::generate_secure_state, provider::OAuthProvider,
    session::SessionStore, state_store::StateStore,
};

/// Maximum length for the `redirect_uri` query parameter.
const MAX_REDIRECT_URI_BYTES: usize = 2_048;

/// Maximum length for the `provider` query parameter.
const MAX_PROVIDER_NAME_BYTES: usize = 128;

/// Separator between the provider name and the bound `redirect_uri` in the stored CSRF
/// state value (#427). A newline cannot appear in a provider name or a (percent-encoded)
/// URI, so this round-trips unambiguously; a value with no separator is a legacy
/// provider-only entry with no bound redirect.
const STATE_VALUE_SEPARATOR: char = '\n';

/// Returns `true` if `candidate` is permitted by the `redirect_uri` allow-list (#427).
///
/// A candidate is allowed when some allow-list entry has the **same scheme, host, and
/// port**, and the entry's path is a **path-boundary prefix** of the candidate's path
/// (an exact path, or the entry ends at a `/` boundary). Host comparison is exact:
/// `https://app.example.com` does **not** match `https://app.example.com.evil.com`, and
/// an entry path of `/cb` does not match `/cbEVIL`. An empty allow-list permits nothing;
/// an unparseable candidate is rejected.
#[must_use]
pub fn is_redirect_uri_allowed(candidate: &str, allowlist: &[String]) -> bool {
    let Ok(candidate_url) = Url::parse(candidate) else {
        return false;
    };
    allowlist
        .iter()
        .any(|entry| Url::parse(entry).is_ok_and(|entry_url| redirect_uri_matches(&candidate_url, &entry_url)))
}

/// Match a single candidate URL against a single allow-list entry URL (see
/// [`is_redirect_uri_allowed`]).
fn redirect_uri_matches(candidate: &Url, entry: &Url) -> bool {
    if candidate.scheme() != entry.scheme()
        || candidate.host_str() != entry.host_str()
        || candidate.port_or_known_default() != entry.port_or_known_default()
    {
        return false;
    }
    let (candidate_path, entry_path) = (candidate.path(), entry.path());
    candidate_path == entry_path
        || candidate_path
            .strip_prefix(entry_path)
            .is_some_and(|rest| entry_path.ends_with('/') || rest.starts_with('/'))
}

/// Encode the CSRF-state stored value, optionally binding a validated `redirect_uri`.
fn encode_state_value(provider: &str, redirect_uri: Option<&str>) -> String {
    match redirect_uri {
        Some(uri) => format!("{provider}{STATE_VALUE_SEPARATOR}{uri}"),
        None => provider.to_string(),
    }
}

/// Decode the CSRF-state stored value into `(provider, bound_redirect_uri)`.
fn decode_state_value(value: &str) -> (String, Option<String>) {
    match value.split_once(STATE_VALUE_SEPARATOR) {
        Some((provider, uri)) => (provider.to_string(), Some(uri.to_string())),
        None => (value.to_string(), None),
    }
}

/// Build the fragment-delivery redirect URL for the implicit-style token hand-off (#427).
///
/// Tokens are placed in the URL fragment (`#…`), which browsers neither send to servers nor
/// include in the `Referer` header — the standard OAuth implicit-flow delivery tradeoff (the
/// tokens remain visible in browser history). The `redirect_uri` has already been validated
/// against the allow-list before reaching here.
fn build_redirect_with_tokens(
    redirect_uri: &str,
    access_token: &str,
    refresh_token: &str,
    expires_in: u64,
    provider: &str,
) -> String {
    format!(
        "{redirect_uri}#access_token={}&token_type=Bearer&expires_in={expires_in}&refresh_token={}&provider={}",
        urlencoding::encode(access_token),
        urlencoding::encode(refresh_token),
        urlencoding::encode(provider),
    )
}

/// Shared state for the multi-provider auth endpoints.
#[derive(Clone)]
pub struct MultiProviderAuthState {
    /// OAuth providers keyed by name (e.g., "github", "google").
    providers:             HashMap<String, Arc<dyn OAuthProvider>>,
    /// CSRF state store (in-memory or Redis).
    state_store:           Arc<dyn StateStore>,
    /// Session backend for creating sessions after successful auth.
    session_store:         Arc<dyn SessionStore>,
    /// Optional user store for account linking (same email → same user).
    user_store:            Option<Arc<dyn AccountStore>>,
    /// Allow-list of permitted `redirect_uri` values (#427).
    ///
    /// When **empty** (the default), `callback` returns the session tokens as JSON and no
    /// server-side redirect is performed — there is no open-redirect surface because the
    /// client-supplied `redirect_uri` is never used as a redirect target. When **non-empty**,
    /// `authorize` rejects any `redirect_uri` not matched by the list (400), binds the
    /// validated URI to the CSRF state token, and `callback` performs an implicit-style
    /// fragment redirect to it. Entries are matched by scheme + host + port + path-boundary
    /// prefix (see [`is_redirect_uri_allowed`]).
    redirect_uri_allowlist: Vec<String>,
}

impl MultiProviderAuthState {
    /// Create a new multi-provider auth state.
    pub fn new(state_store: Arc<dyn StateStore>, session_store: Arc<dyn SessionStore>) -> Self {
        Self {
            providers: HashMap::new(),
            state_store,
            session_store,
            user_store: None,
            redirect_uri_allowlist: Vec::new(),
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

    /// Configure the allow-list of permitted `redirect_uri` values (#427).
    ///
    /// Enabling this turns on the server-side redirect flow: `authorize` rejects any
    /// `redirect_uri` not on the list, and `callback` redirects the browser to the
    /// validated URI with the tokens delivered in the URL fragment (OAuth implicit style).
    /// With no allow-list configured, the legacy JSON-token response is preserved.
    #[must_use = "builder method returns the modified state"]
    pub fn with_redirect_uri_allowlist(mut self, allowlist: Vec<String>) -> Self {
        self.redirect_uri_allowlist = allowlist;
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
/// - `redirect_uri` — **required**: client application callback URI. Always validated for
///   presence and length. When a redirect-URI allow-list is configured (#427, via
///   [`MultiProviderAuthState::with_redirect_uri_allowlist`]), it must additionally match the
///   allow-list (else `400`); the validated URI is bound to the CSRF state and [`callback`]
///   redirects the browser to it with the tokens in the fragment. With no allow-list
///   configured, the URI is **not** used as a redirect target and [`callback`] returns the
///   session tokens as JSON — so there is no open-redirect surface in that mode.
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

    // #427: when an allow-list is configured the `redirect_uri` must match it, and is then
    // bound to the CSRF state for a server-side redirect in `callback`. With no allow-list
    // the URI is never used as a redirect target (JSON-token response), so there is no
    // open-redirect surface and only presence/length are enforced.
    let bound_redirect_uri = if state.redirect_uri_allowlist.is_empty() {
        None
    } else if is_redirect_uri_allowed(&q.redirect_uri, &state.redirect_uri_allowlist) {
        Some(q.redirect_uri.clone())
    } else {
        return json_error(StatusCode::BAD_REQUEST, "redirect_uri is not allow-listed");
    };

    // Look up provider
    let Some(provider) = state.get_provider(&q.provider) else {
        return json_error(StatusCode::BAD_REQUEST, &format!("unknown provider: {}", q.provider));
    };

    // Generate state and store with provider name (and the bound redirect_uri, if any)
    let state_value = generate_secure_state();

    let Ok(now) = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
    else {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "system clock error");
    };

    let expiry = now + 600; // 10 minutes

    let state_payload = encode_state_value(&q.provider, bound_redirect_uri.as_deref());
    if let Err(e) = state.state_store.store(state_value.clone(), state_payload, expiry).await {
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

    // Consume state (atomic remove) and decode the provider name + any bound redirect_uri.
    let Ok((state_payload, expiry)) = state.state_store.retrieve(&state_token).await else {
        return json_error(StatusCode::BAD_REQUEST, "invalid or expired state token");
    };
    let (provider_name, bound_redirect_uri) = decode_state_value(&state_payload);

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

    // #427: if a validated redirect_uri was bound at authorize time, hand the tokens off via
    // an implicit-style fragment redirect; otherwise return the legacy JSON token response.
    if let Some(redirect_uri) = bound_redirect_uri {
        let location = build_redirect_with_tokens(
            &redirect_uri,
            &session_tokens.access_token,
            &session_tokens.refresh_token,
            session_tokens.expires_in,
            &provider_name,
        );
        return Redirect::to(&location).into_response();
    }

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
mod tests;
