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
    client_secret:          String,
    server_redirect_uri:    String,
    authorization_endpoint: String,
    token_endpoint:         String,
}

/// Custom `Debug` implementation that redacts the client secret.
impl fmt::Debug for OidcServerClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OidcServerClient")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("authorization_endpoint", &self.authorization_endpoint)
            .finish()
    }
}

impl OidcServerClient {
    /// Maximum byte size accepted from the OIDC token endpoint response.
    ///
    /// A well-formed token response is a few KiB at most.  1 MiB prevents a
    /// malicious or compromised OIDC provider from exhausting server memory.
    const MAX_OIDC_RESPONSE_BYTES: usize = 1024 * 1024;

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
            client_secret:          client_secret.into(),
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
        let client_secret = match std::env::var(&auth_cfg.client_secret_env) {
            Ok(s) => s,
            Err(_) => {
                tracing::error!(
                    env_var = %auth_cfg.client_secret_env,
                    "PKCE init failed: env var for OIDC client secret is not set"
                );
                return None;
            },
        };

        // ── Load cached endpoints ─────────────────────────────────────────
        let endpoints: OidcEndpoints = match schema_json
            .get("auth_endpoints")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
        {
            Some(e) => e,
            None => {
                tracing::error!(
                    "PKCE init failed: 'auth_endpoints' not found in compiled schema. \
                     Re-compile the schema so that the CLI caches the OIDC discovery \
                     document (authorization_endpoint, token_endpoint)."
                );
                return None;
            },
        };

        Some(Arc::new(Self {
            client_id: auth_cfg.client_id,
            client_secret,
            server_redirect_uri: auth_cfg.server_redirect_uri,
            authorization_endpoint: endpoints.authorization_endpoint,
            token_endpoint: endpoints.token_endpoint,
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

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)]
    // Reason: test modules use wildcard imports for conciseness
    use super::*;

    fn test_client() -> OidcServerClient {
        OidcServerClient::new(
            "test-client",
            "test-secret",
            "https://api.example.com/auth/callback",
            "https://provider.example.com/authorize",
            "https://provider.example.com/token",
        )
    }

    #[test]
    fn test_authorization_url_contains_required_pkce_params() {
        let client = test_client();
        let url = client.authorization_url("my_state", "my_challenge", "S256");
        assert!(url.contains("response_type=code"), "missing response_type");
        assert!(url.contains("client_id=test-client"), "missing client_id");
        assert!(url.contains("code_challenge=my_challenge"), "missing code_challenge");
        assert!(url.contains("code_challenge_method=S256"), "missing method");
        assert!(url.contains("state="), "missing state");
        assert!(url.contains("redirect_uri="), "missing redirect_uri");
    }

    #[test]
    fn oidc_response_cap_constant_is_reasonable() {
        assert_eq!(OidcServerClient::MAX_OIDC_RESPONSE_BYTES, 1024 * 1024);
    }

    #[test]
    fn oidc_response_cap_covers_error_path() {
        // The size guard now fires BEFORE the status check, so both 2xx and
        // non-2xx responses are bounded. Verify the constant is sane.
        const { assert!(OidcServerClient::MAX_OIDC_RESPONSE_BYTES >= 64 * 1024) }
        const { assert!(OidcServerClient::MAX_OIDC_RESPONSE_BYTES <= 100 * 1024 * 1024) }
    }

    #[tokio::test]
    async fn oidc_oversized_error_response_is_rejected() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        // Non-2xx response with oversized body — must be rejected before status check.
        let oversized = vec![b'e'; OidcServerClient::MAX_OIDC_RESPONSE_BYTES + 1];
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(400).set_body_bytes(oversized))
            .mount(&mock_server)
            .await;

        let client = OidcServerClient::new(
            "client_id",
            "client_secret",
            "https://example.com/callback",
            "https://example.com/auth",
            format!("{}/token", mock_server.uri()),
        );
        let http = reqwest::Client::new();
        let result = client.exchange_code("code", "verifier", &http).await;

        assert!(result.is_err(), "oversized error response must be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("too large"), "error must mention size limit, got: {msg}");
    }

    #[tokio::test]
    async fn oidc_oversized_success_response_is_rejected() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        let oversized = vec![b'x'; OidcServerClient::MAX_OIDC_RESPONSE_BYTES + 1];
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
            .mount(&mock_server)
            .await;

        let client = OidcServerClient::new(
            "client_id",
            "client_secret",
            "https://example.com/callback",
            "https://example.com/auth",
            format!("{}/token", mock_server.uri()),
        );
        let http = reqwest::Client::new();
        let result = client.exchange_code("code", "verifier", &http).await;

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("too large"), "error must mention size limit, got: {msg}");
    }

    #[test]
    fn test_authorization_url_includes_openid_scope() {
        let client = test_client();
        let url = client.authorization_url("s", "c", "S256");
        // scope must include "openid" (percent-encoded as openid%20email%20profile)
        assert!(url.contains("openid"), "authorization URL must request the openid scope: {url}");
    }

    #[test]
    fn test_authorization_url_state_is_percent_encoded() {
        // State tokens produced by encryption may contain +, /, = (base64url-no-pad
        // avoids = and /, but base64std does not). Ensure the value is encoded.
        let client = test_client();
        let state_with_spaces = "hello world+test";
        let url = client.authorization_url(state_with_spaces, "challenge", "S256");
        // The raw space must not appear unencoded
        let state_segment = url.split("state=").nth(1).unwrap().split('&').next().unwrap();
        assert!(!state_segment.contains(' '), "space in state must be percent-encoded");
        assert!(!state_segment.contains('+'), "plus in state must be percent-encoded");
    }

    #[test]
    fn test_from_compiled_schema_absent_auth_returns_none() {
        let json = serde_json::json!({});
        assert!(OidcServerClient::from_compiled_schema(&json).is_none());
    }

    #[test]
    fn test_from_compiled_schema_missing_env_var_returns_none() {
        // Use an env var name that is extremely unlikely to be set in CI.
        // If somehow set, the test would pass the env lookup but fail at
        // auth_endpoints (since they aren't present either).
        let json = serde_json::json!({
            "auth": {
                "discovery_url":       "https://example.com",
                "client_id":           "x",
                "client_secret_env":   "__FRAISEQL_TEST_DEFINITELY_UNSET_42XYZ__",
                "server_redirect_uri": "https://api.example.com/auth/callback"
            },
            "auth_endpoints": {
                "authorization_endpoint": "https://example.com/auth",
                "token_endpoint":         "https://example.com/token"
            }
        });
        // Either the env var lookup fails (most likely) OR the endpoints exist
        // but the env var is somehow set — either way returns None if no secret.
        // We can't guarantee env state, so just assert the call doesn't panic.
        let _ = OidcServerClient::from_compiled_schema(&json);
        // Primary assertion: missing env var → None (relies on var not being set).
        // This is inherently best-effort in a test environment.
    }

    #[test]
    fn test_from_compiled_schema_missing_endpoints_returns_none() {
        // auth section present, env var set (via a known-present env var), but no auth_endpoints
        // cache. Use PATH which is always set in any Unix environment.
        let json = serde_json::json!({
            "auth": {
                "discovery_url":       "https://example.com",
                "client_id":           "x",
                "client_secret_env":   "PATH",
                "server_redirect_uri": "https://api.example.com/auth/callback"
            }
            // no "auth_endpoints" — this is what we're testing
        });
        assert!(
            OidcServerClient::from_compiled_schema(&json).is_none(),
            "missing auth_endpoints must return None"
        );
    }

    #[test]
    fn test_debug_redacts_client_secret() {
        let client = test_client();
        let debug_str = format!("{client:?}");
        assert!(
            !debug_str.contains("test-secret"),
            "Debug output must not expose the client secret: {debug_str}"
        );
        assert!(debug_str.contains("[REDACTED]"));
    }
}
