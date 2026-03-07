//! OAuth2 and OIDC client implementations.

use std::{sync::Arc, time::Duration as StdDuration};

use serde::{Deserialize, Serialize};

use super::super::jwks::JwksCache;
use super::pkce::PKCEChallenge;
use super::types::{IdTokenClaims, TokenResponse, UserInfo};

/// OIDC provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OIDCProviderConfig {
    /// Provider issuer URL
    pub issuer:                   String,
    /// Authorization endpoint
    pub authorization_endpoint:   String,
    /// Token endpoint
    pub token_endpoint:           String,
    /// Userinfo endpoint
    pub userinfo_endpoint:        Option<String>,
    /// JWKS URI for public keys
    pub jwks_uri:                 String,
    /// Scopes supported by provider
    pub scopes_supported:         Vec<String>,
    /// Response types supported
    pub response_types_supported: Vec<String>,
}

impl OIDCProviderConfig {
    /// Create new provider configuration
    pub fn new(
        issuer: String,
        authorization_endpoint: String,
        token_endpoint: String,
        jwks_uri: String,
    ) -> Self {
        Self {
            issuer,
            authorization_endpoint,
            token_endpoint,
            userinfo_endpoint: None,
            jwks_uri,
            scopes_supported: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            response_types_supported: vec!["code".to_string()],
        }
    }
}

/// Result of [`OAuth2Client::authorization_url`].
///
/// The caller MUST store `state` (for CSRF verification at callback) and, when
/// present, the PKCE `pkce.code_verifier` (for token exchange).
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    /// The full authorization URL to redirect the user to.
    pub url: String,
    /// CSRF state value — verify this matches the `state` query param at callback.
    pub state: String,
    /// PKCE challenge, present only when `use_pkce = true`.
    pub pkce: Option<PKCEChallenge>,
}

/// OAuth2 client for authorization code flow.
#[derive(Debug, Clone)]
pub struct OAuth2Client {
    /// Client ID from provider.
    pub client_id:              String,
    /// Client secret from provider.
    client_secret:              String,
    /// Authorization endpoint.
    pub authorization_endpoint: String,
    /// Token endpoint.
    token_endpoint:             String,
    /// Scopes to request.
    pub scopes:                 Vec<String>,
    /// Use PKCE for additional security.
    pub use_pkce:               bool,
    /// HTTP client for token requests.
    http_client:                reqwest::Client,
}

impl OAuth2Client {
    /// Create new OAuth2 client.
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        authorization_endpoint: impl Into<String>,
        token_endpoint: impl Into<String>,
    ) -> Self {
        Self {
            client_id:              client_id.into(),
            client_secret:          client_secret.into(),
            authorization_endpoint: authorization_endpoint.into(),
            token_endpoint:         token_endpoint.into(),
            scopes:                 vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            use_pkce:               false,
            http_client:            reqwest::Client::new(),
        }
    }

    /// Set scopes for request.
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Enable PKCE protection.
    pub fn with_pkce(mut self, enabled: bool) -> Self {
        self.use_pkce = enabled;
        self
    }

    /// Generate authorization URL.
    ///
    /// Returns an [`AuthorizationRequest`] containing the URL, the CSRF state
    /// value (must be stored and verified at callback), and an optional PKCE
    /// challenge (when `use_pkce = true`; the `code_verifier` must be stored
    /// and sent during token exchange).
    pub fn authorization_url(&self, redirect_uri: &str) -> AuthorizationRequest {
        let state = uuid::Uuid::new_v4().to_string();
        let scope = self.scopes.join(" ");

        let mut url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
            self.authorization_endpoint,
            urlencoding::encode(&self.client_id),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scope),
            urlencoding::encode(&state),
        );

        let pkce = if self.use_pkce {
            let challenge = PKCEChallenge::new();
            url.push_str(&format!(
                "&code_challenge={}&code_challenge_method=S256",
                urlencoding::encode(&challenge.code_challenge),
            ));
            Some(challenge)
        } else {
            None
        };

        AuthorizationRequest { url, state, pkce }
    }

    /// Post a form request to the token endpoint and parse the response.
    async fn post_token_request(&self, params: &[(&str, &str)]) -> Result<TokenResponse, String> {
        let response = self
            .http_client
            .post(&self.token_endpoint)
            .form(params)
            .send()
            .await
            .map_err(|e| format!("Token request failed: {e}"))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Token endpoint returned error: {body}"));
        }

        response
            .json::<TokenResponse>()
            .await
            .map_err(|e| format!("Failed to parse token response: {e}"))
    }

    /// Exchange authorization code for tokens.
    pub async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<TokenResponse, String> {
        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
            ("redirect_uri", redirect_uri),
        ];
        self.post_token_request(&params).await
    }

    /// Refresh access token using a refresh token.
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse, String> {
        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
        ];
        self.post_token_request(&params).await
    }
}

/// OIDC client for OpenID Connect flow.
#[derive(Debug)]
pub struct OIDCClient {
    /// Provider configuration.
    pub config:     OIDCProviderConfig,
    /// Client ID.
    pub client_id:  String,
    /// Client secret — retained for token revocation and introspection endpoints.
    // Reason: needed for token revocation and introspection
    #[allow(dead_code)]
    client_secret:  String,
    /// JWKS key cache for ID token signature verification.
    pub jwks_cache: Arc<JwksCache>,
    /// HTTP client for userinfo requests.
    http_client:    reqwest::Client,
}

impl OIDCClient {
    /// Create new OIDC client with JWKS caching.
    ///
    /// The JWKS cache TTL defaults to 1 hour.
    pub fn new(
        config: OIDCProviderConfig,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        let jwks_cache = Arc::new(JwksCache::new(&config.jwks_uri, StdDuration::from_secs(3600)));
        Self {
            config,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            jwks_cache,
            http_client: reqwest::Client::new(),
        }
    }

    /// Create OIDC client with a pre-built JWKS cache (for testing).
    pub fn with_jwks_cache(
        config: OIDCProviderConfig,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        jwks_cache: Arc<JwksCache>,
    ) -> Self {
        Self {
            config,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            jwks_cache,
            http_client: reqwest::Client::new(),
        }
    }

    /// Verify an ID token's JWT signature and claims.
    ///
    /// Decodes the JWT header to extract the `kid`, fetches the matching public
    /// key from the JWKS cache, then validates signature, issuer, audience, and
    /// required claims. Optionally checks the nonce for replay protection.
    ///
    /// # Errors
    ///
    /// Returns an error if the token is malformed, the signature is invalid,
    /// claims validation fails, or the nonce doesn't match.
    pub async fn verify_id_token(
        &self,
        id_token: &str,
        expected_nonce: Option<&str>,
    ) -> Result<IdTokenClaims, String> {
        // 1. Decode header to get kid
        let header = jsonwebtoken::decode_header(id_token)
            .map_err(|e| format!("Invalid JWT header: {e}"))?;
        let kid = header.kid.ok_or("JWT missing 'kid' in header")?;

        // 2. Get key from JWKS cache
        let key = self
            .jwks_cache
            .get_key(&kid)
            .await
            .map_err(|e| format!("JWKS fetch error: {e}"))?
            .ok_or_else(|| format!("No key found for kid '{kid}'"))?;

        // 3. Build validation criteria
        let mut validation = jsonwebtoken::Validation::new(header.alg);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.client_id]);
        validation.set_required_spec_claims(&["exp", "iat", "iss", "aud", "sub"]);

        // 4. Decode and validate
        let token_data = jsonwebtoken::decode::<IdTokenClaims>(id_token, &key, &validation)
            .map_err(|e| format!("ID token validation failed: {e}"))?;

        // 5. Verify nonce if provided
        if let Some(expected) = expected_nonce {
            if token_data.claims.nonce.as_deref() != Some(expected) {
                return Err("Nonce mismatch".to_string());
            }
        }

        Ok(token_data.claims)
    }

    /// Fetch user information from the provider's userinfo endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if no userinfo endpoint is configured, the HTTP request
    /// fails, or the response cannot be parsed.
    pub async fn get_userinfo(&self, access_token: &str) -> Result<UserInfo, String> {
        let endpoint = self
            .config
            .userinfo_endpoint
            .as_ref()
            .ok_or("No userinfo endpoint configured for this provider")?;

        let response = self
            .http_client
            .get(endpoint)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| format!("Userinfo request failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("Userinfo endpoint returned {}", response.status()));
        }

        response
            .json::<UserInfo>()
            .await
            .map_err(|e| format!("Failed to parse userinfo response: {e}"))
    }
}
