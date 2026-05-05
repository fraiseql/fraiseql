//! Server-side OIDC client for PKCE authorization code flows.
//!
//! This is a minimal, runtime-facing client that:
//! 1. Builds the OIDC `/authorize` redirect URL with PKCE parameters.
//! 2. Exchanges the authorization code + `code_verifier` for tokens.
//!
//! It is intentionally separate from the more general [`crate::oauth::OAuth2Client`] and
//! [`crate::oauth::OIDCClient`] types in `oauth`: those carry JWKS caches and session
//! management state that the PKCE route handlers do not need.
// The client secret is loaded from the environment at runtime and is NEVER
// stored in the compiled schema or TOML config.

use std::{fmt, sync::Arc};

use serde::Deserialize;
use zeroize::Zeroizing;

// ---------------------------------------------------------------------------
// Resolved OIDC endpoints (cached in compiled schema)
// ---------------------------------------------------------------------------

/// OIDC endpoints fetched from the discovery document and cached in the
/// compiled schema under `"auth_endpoints"`.
#[derive(Debug, Clone, Deserialize)]
pub struct OidcEndpoints {
    /// The provider's `/authorize` URL.
    pub authorization_endpoint: String,
    /// The provider's `/token` URL.
    pub token_endpoint:         String,
}

// ---------------------------------------------------------------------------
// Token response from the provider
// ---------------------------------------------------------------------------

/// Minimal token response from the OIDC `/token` endpoint.
#[derive(Debug, Deserialize)]
pub struct OidcTokenResponse {
    /// The access token.
    pub access_token:  String,
    /// The OpenID Connect identity token (if requested).
    pub id_token:      Option<String>,
    /// Seconds until the access token expires.
    pub expires_in:    Option<u64>,
    /// Refresh token (if the provider issued one).
    pub refresh_token: Option<String>,
}

// ---------------------------------------------------------------------------
// OidcServerClient
// ---------------------------------------------------------------------------

/// Minimal OIDC client for server-side PKCE code exchange.
///
/// Constructed once at server startup from the compiled schema.
/// The client secret is read from the environment at that time and
/// held in memory — it is never written to disk or emitted in logs.
pub struct OidcServerClient {
    client_id:              String,
    /// Intentionally private: the secret must never be accessible via a field.
    /// Stored as `Zeroizing<String>` so the key material is wiped from memory
    /// when this struct is dropped.
    pub(crate) client_secret:          Zeroizing<String>,
    server_redirect_uri:    String,
    authorization_endpoint: String,
    token_endpoint:         String,
}

/// Custom `Debug` implementation that redacts the client secret.
#[allow(clippy::missing_fields_in_debug)] // Reason: endpoint fields omitted to keep debug concise and avoid leaking config in logs
impl fmt::Debug for OidcServerClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OidcServerClient")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("authorization_endpoint", &self.authorization_endpoint)
            .finish_non_exhaustive()
    }
}

impl OidcServerClient {
    /// Maximum byte length for an inbound PKCE `code_verifier` (RFC 7636 §4.1).
    ///
    /// Values longer than 128 characters exceed the RFC ceiling; rejecting them
    /// before the outbound token request prevents log injection and memory
    /// exhaustion on the provider side.
    pub(crate) const MAX_CODE_VERIFIER_BYTES: usize = 128;
    /// Maximum byte size accepted from the OIDC token endpoint response.
    ///
    /// A well-formed token response is a few KiB at most.  1 MiB prevents a
    /// malicious or compromised OIDC provider from exhausting server memory.
    pub(crate) const MAX_OIDC_RESPONSE_BYTES: usize = 1024 * 1024;
    /// Minimum byte length for an inbound PKCE `code_verifier` (RFC 7636 §4.1).
    ///
    /// RFC 7636 mandates that the code verifier be between 43 and 128 characters.
    /// Rejecting values below this floor prevents malformed verifiers — which
    /// a browser extension or man-in-the-browser attack might supply — from
    /// reaching the upstream OIDC provider.
    pub(crate) const MIN_CODE_VERIFIER_BYTES: usize = 43;

    /// Construct a client directly from resolved credentials and endpoints.
    ///
    /// Prefer [`Self::from_compiled_schema`] in production code.
    /// This constructor exists for testing and direct wiring.
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        server_redirect_uri: impl Into<String>,
        authorization_endpoint: impl Into<String>,
        token_endpoint: impl Into<String>,
    ) -> Self {
        Self {
            client_id:              client_id.into(),
            client_secret:          Zeroizing::new(client_secret.into()),
            server_redirect_uri:    server_redirect_uri.into(),
            authorization_endpoint: authorization_endpoint.into(),
            token_endpoint:         token_endpoint.into(),
        }
    }

    /// Build an `OidcServerClient` from the compiled schema JSON.
    ///
    /// Returns `None` if:
    /// - `schema_json["auth"]` is absent, or
    /// - the env var named by `client_secret_env` is not set, or
    /// - the OIDC endpoint cache (`schema_json["auth_endpoints"]`) is absent.
    ///
    /// In all failure cases an explanatory `tracing::error!` is emitted so
    /// operators can diagnose startup issues without reading source code.
    pub fn from_compiled_schema(schema_json: &serde_json::Value) -> Option<Arc<Self>> {
        // ── Load [auth] config ────────────────────────────────────────────
        #[derive(Deserialize)]
        struct AuthCfg {
            client_id:           String,
            client_secret_env:   String,
            server_redirect_uri: String,
        }

        let auth_cfg: AuthCfg =
            schema_json.get("auth").and_then(|v| serde_json::from_value(v.clone()).ok())?;

        // ── Read client secret from env ───────────────────────────────────
        let Ok(client_secret) = std::env::var(&auth_cfg.client_secret_env) else {
            tracing::error!(
                env_var = %auth_cfg.client_secret_env,
                "PKCE init failed: env var for OIDC client secret is not set"
            );
            return None;
        };

        // ── Load cached endpoints ─────────────────────────────────────────
        let Some(endpoints): Option<OidcEndpoints> = schema_json
            .get("auth_endpoints")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
        else {
            tracing::error!(
                "PKCE init failed: 'auth_endpoints' not found in compiled schema. \
                 Re-compile the schema so that the CLI caches the OIDC discovery \
                 document (authorization_endpoint, token_endpoint)."
            );
            return None;
        };

        Some(Arc::new(Self {
            client_id:              auth_cfg.client_id,
            client_secret:          Zeroizing::new(client_secret),
            server_redirect_uri:    auth_cfg.server_redirect_uri,
            authorization_endpoint: endpoints.authorization_endpoint,
            token_endpoint:         endpoints.token_endpoint,
        }))
    }

    /// Build the OIDC `/authorize` redirect URL with all required PKCE params.
    ///
    /// The `state`, `code_challenge`, and `redirect_uri` values are
    /// percent-encoded so that base64-url characters (+, /, =) do not
    /// break query string parsing on the provider side.
    pub fn authorization_url(
        &self,
        state: &str,
        code_challenge: &str,
        code_challenge_method: &str,
    ) -> String {
        format!(
            "{}?response_type=code\
             &client_id={}\
             &redirect_uri={}\
             &scope=openid%20email%20profile\
             &state={}\
             &code_challenge={}\
             &code_challenge_method={}",
            self.authorization_endpoint,
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&self.server_redirect_uri),
            urlencoding::encode(state),
            urlencoding::encode(code_challenge),
            code_challenge_method,
        )
    }

    // 1 MiB

    /// Exchange an authorization code for tokens.
    ///
    /// Sends a `POST` to the provider's `/token` endpoint with the PKCE
    /// `code_verifier` and all required OAuth2 fields.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails, the provider returns a
    /// non-success status, the response exceeds `MAX_OIDC_RESPONSE_BYTES`, or
    /// the response body cannot be parsed as JSON.
    pub async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
        http: &reqwest::Client,
    ) -> Result<OidcTokenResponse, anyhow::Error> {
        // SECURITY: RFC 7636 §4.1 mandates 43–128 ASCII characters for code_verifier.
        // Reject out-of-range values before they reach the upstream OIDC provider.
        anyhow::ensure!(
            code_verifier.len() >= Self::MIN_CODE_VERIFIER_BYTES,
            "code_verifier too short ({} bytes, min {})",
            code_verifier.len(),
            Self::MIN_CODE_VERIFIER_BYTES,
        );
        anyhow::ensure!(
            code_verifier.len() <= Self::MAX_CODE_VERIFIER_BYTES,
            "code_verifier too long ({} bytes, max {})",
            code_verifier.len(),
            Self::MAX_CODE_VERIFIER_BYTES,
        );

        let resp = http
            .post(&self.token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("code_verifier", code_verifier),
                ("redirect_uri", self.server_redirect_uri.as_str()),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
            ])
            .send()
            .await?;

        let status = resp.status();

        // Read body with error propagation — unwrap_or_default() would silently
        // discard network errors and return an empty body, masking failures.
        let body_bytes = resp
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read token response: {e}"))?;

        // Size guard BEFORE the status check: a compromised provider could exhaust
        // memory by sending an oversized non-2xx response that bypassed a later cap.
        anyhow::ensure!(
            body_bytes.len() <= Self::MAX_OIDC_RESPONSE_BYTES,
            "OIDC token response too large ({} bytes, max {})",
            body_bytes.len(),
            Self::MAX_OIDC_RESPONSE_BYTES
        );

        if !status.is_success() {
            // Body is already bounded by the size check above — no need for .min().
            let body = String::from_utf8_lossy(&body_bytes);
            anyhow::bail!("token endpoint returned {status}: {body}");
        }

        Ok(serde_json::from_slice::<OidcTokenResponse>(&body_bytes)?)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------
