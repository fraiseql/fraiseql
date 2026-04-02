//! Generic OIDC provider implementation using RFC 8414 discovery.
use std::{fmt::Write as _, time::Duration};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Timeout for all OIDC HTTP operations (discovery, token exchange, user info, refresh).
const OIDC_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum byte size for an OIDC discovery document response.
///
/// A well-formed `.well-known/openid-configuration` payload is a few `KiB`.
/// 64 `KiB` blocks allocation bombs from a compromised OIDC provider.
const MAX_OIDC_DISCOVERY_BYTES: usize = 64 * 1024; // 64 KiB

/// Maximum byte size for OIDC token and user-info responses.
///
/// Token responses carry JWTs and a handful of metadata fields.
/// 1 `MiB` is generous while preventing runaway allocation.
const MAX_OIDC_TOKEN_BYTES: usize = 1024 * 1024; // 1 MiB

use crate::{
    error::{AuthError, Result},
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// OIDC Discovery document
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OidcDiscovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    jwks_uri: Option<String>,
    #[serde(default)]
    revocation_endpoint: Option<String>,
}

/// Generic OIDC provider that works with any OIDC-compliant service
///
/// # Examples
///
/// ```no_run
/// // Requires: live OIDC endpoint (e.g., Google, Logto, Ory).
/// # async fn example() -> fraiseql_auth::error::Result<()> {
/// use fraiseql_auth::oidc_provider::OidcProvider;
/// let provider = OidcProvider::new(
///     "google",
///     "https://accounts.google.com",
///     "client_id",
///     "client_secret",
///     "http://localhost:8000/auth/callback",
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub struct OidcProvider {
    name: String,
    issuer_url: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    discovery: OidcDiscovery,
    client: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct TokenRequest {
    grant_type: String,
    code: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code_verifier: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponseRaw {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: u64,
    #[serde(default)]
    token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserInfoRaw {
    sub: String,
    email: Option<String>,
    name: Option<String>,
    picture: Option<String>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

impl OidcProvider {
    /// Create a new OIDC provider
    ///
    /// # Arguments
    /// * `name` - Provider name (for logging)
    /// * `issuer_url` - The issuer URL (e.g., <https://accounts.google.com>)
    /// * `client_id` - OAuth client ID
    /// * `client_secret` - OAuth client secret
    /// * `redirect_uri` - Redirect URI after authentication
    ///
    /// # Errors
    /// Returns error if metadata discovery fails
    pub async fn new(
        name: &str,
        issuer_url: &str,
        client_id: &str,
        client_secret: &str,
        redirect_uri: &str,
    ) -> Result<Self> {
        let client =
            reqwest::Client::builder().timeout(OIDC_REQUEST_TIMEOUT).build().map_err(|e| {
                AuthError::OidcMetadataError {
                    message: format!("Failed to create HTTP client: {}", e),
                }
            })?;

        // Fetch OIDC discovery document
        let discovery_url =
            format!("{}/.well-known/openid-configuration", issuer_url.trim_end_matches('/'));

        let discovery_bytes = client
            .get(&discovery_url)
            .send()
            .await
            .map_err(|e| AuthError::OidcMetadataError {
                message: format!("Failed to fetch OIDC metadata from {}: {}", discovery_url, e),
            })?
            .bytes()
            .await
            .map_err(|e| AuthError::OidcMetadataError {
                message: format!("Failed to read OIDC metadata: {}", e),
            })?;
        if discovery_bytes.len() > MAX_OIDC_DISCOVERY_BYTES {
            return Err(AuthError::OidcMetadataError {
                message: format!(
                    "OIDC discovery response too large ({} bytes, max {MAX_OIDC_DISCOVERY_BYTES})",
                    discovery_bytes.len()
                ),
            });
        }
        let discovery: OidcDiscovery =
            serde_json::from_slice(&discovery_bytes).map_err(|e| AuthError::OidcMetadataError {
                message: format!("Failed to parse OIDC metadata: {}", e),
            })?;

        Ok(Self {
            name: name.to_string(),
            issuer_url: issuer_url.to_string(),
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            discovery,
            client,
        })
    }

    /// Add authorization URL parameter
    fn add_auth_params(&self, url: &mut String, state: &str, pkce_challenge: Option<&str>) {
        url.push('?');
        write!(url, "client_id={}", urlencoding::encode(&self.client_id))
            .expect("write to String is infallible");
        write!(url, "&redirect_uri={}", urlencoding::encode(&self.redirect_uri))
            .expect("write to String is infallible");
        url.push_str("&response_type=code");
        write!(url, "&state={}", urlencoding::encode(state))
            .expect("write to String is infallible");
        url.push_str("&scope=openid+email+profile");

        if let Some(challenge) = pkce_challenge {
            write!(url, "&code_challenge={}", urlencoding::encode(challenge))
                .expect("write to String is infallible");
            url.push_str("&code_challenge_method=S256");
        }
    }
}

// Reason: OAuthProvider is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl OAuthProvider for OidcProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn authorization_url(&self, state: &str) -> String {
        let mut url = self.discovery.authorization_endpoint.clone();
        self.add_auth_params(&mut url, state, None);
        url
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        let request = TokenRequest {
            grant_type: "authorization_code".to_string(),
            code: code.to_string(),
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            redirect_uri: self.redirect_uri.clone(),
            code_verifier: None,
        };

        let token_bytes = self
            .client
            .post(&self.discovery.token_endpoint)
            .form(&request)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to exchange code: {}", e),
            })?
            .bytes()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to read token response: {}", e),
            })?;
        if token_bytes.len() > MAX_OIDC_TOKEN_BYTES {
            return Err(AuthError::OAuthError {
                message: format!(
                    "Token response too large ({} bytes, max {MAX_OIDC_TOKEN_BYTES})",
                    token_bytes.len()
                ),
            });
        }
        let response: TokenResponseRaw =
            serde_json::from_slice(&token_bytes).map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse token response: {}", e),
            })?;

        Ok(TokenResponse {
            access_token: response.access_token,
            refresh_token: response.refresh_token,
            expires_in: response.expires_in,
            token_type: response.token_type.unwrap_or_else(|| "Bearer".to_string()),
        })
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        let info_bytes = self
            .client
            .get(&self.discovery.userinfo_endpoint)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to get user info: {}", e),
            })?
            .bytes()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to read user info response: {}", e),
            })?;
        if info_bytes.len() > MAX_OIDC_TOKEN_BYTES {
            return Err(AuthError::OAuthError {
                message: format!(
                    "User info response too large ({} bytes, max {MAX_OIDC_TOKEN_BYTES})",
                    info_bytes.len()
                ),
            });
        }
        let response: UserInfoRaw =
            serde_json::from_slice(&info_bytes).map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse user info: {}", e),
            })?;

        // Build raw claims with all fields
        let mut raw_claims = serde_json::Map::new();
        raw_claims.insert("sub".to_string(), serde_json::Value::String(response.sub.clone()));
        if let Some(email) = &response.email {
            raw_claims.insert("email".to_string(), serde_json::Value::String(email.clone()));
        }
        if let Some(name) = &response.name {
            raw_claims.insert("name".to_string(), serde_json::Value::String(name.clone()));
        }
        if let Some(picture) = &response.picture {
            raw_claims.insert("picture".to_string(), serde_json::Value::String(picture.clone()));
        }
        for (key, value) in response.extra {
            raw_claims.insert(key, value);
        }

        Ok(UserInfo {
            id: response.sub,
            email: response.email.unwrap_or_default(),
            name: response.name,
            picture: response.picture,
            raw_claims: serde_json::Value::Object(raw_claims),
        })
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
        ];

        let refresh_bytes = self
            .client
            .post(&self.discovery.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to refresh token: {}", e),
            })?
            .bytes()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to read refresh response: {}", e),
            })?;
        if refresh_bytes.len() > MAX_OIDC_TOKEN_BYTES {
            return Err(AuthError::OAuthError {
                message: format!(
                    "Refresh response too large ({} bytes, max {MAX_OIDC_TOKEN_BYTES})",
                    refresh_bytes.len()
                ),
            });
        }
        let response: TokenResponseRaw =
            serde_json::from_slice(&refresh_bytes).map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse refresh response: {}", e),
            })?;

        Ok(TokenResponse {
            access_token: response.access_token,
            refresh_token: response.refresh_token,
            expires_in: response.expires_in,
            token_type: response.token_type.unwrap_or_else(|| "Bearer".to_string()),
        })
    }

    async fn revoke_token(&self, token: &str) -> Result<()> {
        if let Some(revocation_endpoint) = &self.discovery.revocation_endpoint {
            let params = [
                ("token", token),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
            ];

            let resp =
                self.client.post(revocation_endpoint).form(&params).send().await.map_err(|e| {
                    AuthError::OAuthError {
                        message: format!("Failed to revoke token: {}", e),
                    }
                })?;
            if !resp.status().is_success() {
                return Err(AuthError::OAuthError {
                    message: format!("Token revocation returned HTTP {}", resp.status()),
                });
            }
        }
        Ok(())
    }
}

impl std::fmt::Debug for OidcProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcProvider")
            .field("name", &self.name)
            .field("issuer_url", &self.issuer_url)
            .field("client_id", &self.client_id)
            .field("redirect_uri", &self.redirect_uri)
            .finish_non_exhaustive() // client_secret and client omitted for security
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)]
    // Reason: test module wildcard import; brings all items into test scope
    // Reason: test modules use wildcard imports for conciseness
    use super::*;

    // ── S24-H1: OidcProvider response size caps ────────────────────────────────

    #[test]
    fn oidc_discovery_cap_constant_is_reasonable() {
        const { assert!(MAX_OIDC_DISCOVERY_BYTES >= 1024) }
        const { assert!(MAX_OIDC_DISCOVERY_BYTES <= 10 * 1024 * 1024) }
    }

    #[test]
    fn oidc_token_cap_constant_is_reasonable() {
        const { assert!(MAX_OIDC_TOKEN_BYTES >= 64 * 1024) }
        const { assert!(MAX_OIDC_TOKEN_BYTES <= 100 * 1024 * 1024) }
    }

    #[test]
    fn oidc_request_timeout_is_set() {
        let secs = OIDC_REQUEST_TIMEOUT.as_secs();
        assert!(secs > 0 && secs <= 120, "OIDC timeout should be 1–120 s, got {secs}");
    }

    #[tokio::test]
    async fn oidc_discovery_oversized_response_is_rejected() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        let oversized = vec![b'x'; MAX_OIDC_DISCOVERY_BYTES + 1];
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
            .mount(&mock_server)
            .await;

        let result = OidcProvider::new(
            "test",
            &mock_server.uri(),
            "client_id",
            "client_secret",
            "http://localhost/callback",
        )
        .await;

        assert!(result.is_err(), "oversized discovery response must be rejected");
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("too large") || msg.contains("large"),
            "error must mention size: {msg}"
        );
    }

    #[tokio::test]
    async fn oidc_discovery_within_size_limit_proceeds_to_parse() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        // Valid but minimal discovery document — will fail at parse stage (missing fields),
        // proving the size gate was passed.
        let tiny = b"{}".to_vec();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(tiny))
            .mount(&mock_server)
            .await;

        let result = OidcProvider::new(
            "test",
            &mock_server.uri(),
            "client_id",
            "client_secret",
            "http://localhost/callback",
        )
        .await;

        // Should fail at JSON parse (missing fields), not at the size gate
        assert!(
            result.is_err(),
            "expected Err when discovery doc has missing fields, got: {result:?}"
        );
        let msg = result.err().unwrap().to_string();
        assert!(
            !msg.contains("too large"),
            "size gate must not trigger for a small response: {msg}"
        );
    }

    #[test]
    fn test_oauth_provider_name() {
        let provider = OidcProvider {
            name: "my-oidc".to_string(),
            issuer_url: "https://example.com".to_string(),
            client_id: "client_id".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost:8000/callback".to_string(),
            discovery: OidcDiscovery {
                issuer: "https://example.com".to_string(),
                authorization_endpoint: "https://example.com/auth".to_string(),
                token_endpoint: "https://example.com/token".to_string(),
                userinfo_endpoint: "https://example.com/userinfo".to_string(),
                jwks_uri: None,
                revocation_endpoint: None,
            },
            client: reqwest::Client::new(),
        };
        assert_eq!(OAuthProvider::name(&provider), "my-oidc");
    }

    #[test]
    fn test_oauth_provider_debug() {
        let provider = OidcProvider {
            name: "test".to_string(),
            issuer_url: "https://example.com".to_string(),
            client_id: "client_id".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost:8000/callback".to_string(),
            discovery: OidcDiscovery {
                issuer: "https://example.com".to_string(),
                authorization_endpoint: "https://example.com/auth".to_string(),
                token_endpoint: "https://example.com/token".to_string(),
                userinfo_endpoint: "https://example.com/userinfo".to_string(),
                jwks_uri: None,
                revocation_endpoint: None,
            },
            client: reqwest::Client::new(),
        };

        let debug_str = format!("{:?}", provider);
        assert!(debug_str.contains("OidcProvider"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_add_auth_params() {
        let provider = OidcProvider {
            name: "test".to_string(),
            issuer_url: "https://example.com".to_string(),
            client_id: "my_client".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost:8000/callback".to_string(),
            discovery: OidcDiscovery {
                issuer: "https://example.com".to_string(),
                authorization_endpoint: "https://example.com/auth".to_string(),
                token_endpoint: "https://example.com/token".to_string(),
                userinfo_endpoint: "https://example.com/userinfo".to_string(),
                jwks_uri: None,
                revocation_endpoint: None,
            },
            client: reqwest::Client::new(),
        };

        let mut url = "https://example.com/auth".to_string();
        provider.add_auth_params(&mut url, "state123", None);

        assert!(url.contains("client_id=my_client"));
        assert!(url.contains("state=state123"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("scope=openid"));
    }

    // ── S29-H1: revoke_token HTTP status check ────────────────────────────────

    #[tokio::test]
    async fn revoke_token_non_success_status_is_rejected() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/revoke"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&mock_server)
            .await;

        let provider = OidcProvider {
            name: "test".to_string(),
            issuer_url: mock_server.uri(),
            client_id: "client_id".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost/callback".to_string(),
            discovery: OidcDiscovery {
                issuer: mock_server.uri(),
                authorization_endpoint: format!("{}/auth", mock_server.uri()),
                token_endpoint: format!("{}/token", mock_server.uri()),
                userinfo_endpoint: format!("{}/userinfo", mock_server.uri()),
                jwks_uri: None,
                revocation_endpoint: Some(format!("{}/revoke", mock_server.uri())),
            },
            client: reqwest::Client::new(),
        };

        let result = provider.revoke_token("some_token").await;
        assert!(result.is_err(), "non-2xx revocation response must be rejected");
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("400") || msg.contains("revocation"),
            "error must mention HTTP status or revocation failure: {msg}"
        );
    }

    #[tokio::test]
    async fn revoke_token_success_returns_ok() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/revoke"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let provider = OidcProvider {
            name: "test".to_string(),
            issuer_url: mock_server.uri(),
            client_id: "client_id".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost/callback".to_string(),
            discovery: OidcDiscovery {
                issuer: mock_server.uri(),
                authorization_endpoint: format!("{}/auth", mock_server.uri()),
                token_endpoint: format!("{}/token", mock_server.uri()),
                userinfo_endpoint: format!("{}/userinfo", mock_server.uri()),
                jwks_uri: None,
                revocation_endpoint: Some(format!("{}/revoke", mock_server.uri())),
            },
            client: reqwest::Client::new(),
        };

        provider
            .revoke_token("some_token")
            .await
            .unwrap_or_else(|e| panic!("200 revocation response must return Ok: {e}"));
    }
}
