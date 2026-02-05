// Generic OIDC provider implementation
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::auth::{
    error::{AuthError, Result},
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// OIDC Discovery document
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OidcDiscovery {
    issuer:                 String,
    authorization_endpoint: String,
    token_endpoint:         String,
    userinfo_endpoint:      String,
    jwks_uri:               Option<String>,
    #[serde(default)]
    revocation_endpoint:    Option<String>,
}

/// Generic OIDC provider that works with any OIDC-compliant service
///
/// # Examples
///
/// ```ignore
/// let provider = OidcProvider::new(
///     "google",
///     "https://accounts.google.com",
///     "client_id",
///     "client_secret",
///     "http://localhost:8000/auth/callback",
/// ).await?;
/// ```
pub struct OidcProvider {
    name:          String,
    issuer_url:    String,
    client_id:     String,
    client_secret: String,
    redirect_uri:  String,
    discovery:     OidcDiscovery,
    client:        reqwest::Client,
}

#[derive(Debug, Serialize)]
struct TokenRequest {
    grant_type:    String,
    code:          String,
    client_id:     String,
    client_secret: String,
    redirect_uri:  String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code_verifier: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponseRaw {
    access_token:  String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in:    u64,
    #[serde(default)]
    token_type:    Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserInfoRaw {
    sub:     String,
    email:   Option<String>,
    name:    Option<String>,
    picture: Option<String>,
    #[serde(flatten)]
    extra:   serde_json::Map<String, serde_json::Value>,
}

impl OidcProvider {
    /// Create a new OIDC provider
    ///
    /// # Arguments
    /// * `name` - Provider name (for logging)
    /// * `issuer_url` - The issuer URL (e.g., "https://accounts.google.com")
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
        let client = reqwest::Client::new();

        // Fetch OIDC discovery document
        let discovery_url =
            format!("{}/.well-known/openid-configuration", issuer_url.trim_end_matches('/'));

        let discovery: OidcDiscovery = client
            .get(&discovery_url)
            .send()
            .await
            .map_err(|e| AuthError::OidcMetadataError {
                message: format!("Failed to fetch OIDC metadata from {}: {}", discovery_url, e),
            })?
            .json()
            .await
            .map_err(|e| AuthError::OidcMetadataError {
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
        url.push_str(&format!("client_id={}", urlencoding::encode(&self.client_id)));
        url.push_str(&format!("&redirect_uri={}", urlencoding::encode(&self.redirect_uri)));
        url.push_str(&format!("&response_type=code"));
        url.push_str(&format!("&state={}", urlencoding::encode(state)));
        url.push_str(&format!("&scope=openid+email+profile"));

        if let Some(challenge) = pkce_challenge {
            url.push_str(&format!("&code_challenge={}", urlencoding::encode(challenge)));
            url.push_str(&format!("&code_challenge_method=S256"));
        }
    }
}

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
            grant_type:    "authorization_code".to_string(),
            code:          code.to_string(),
            client_id:     self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            redirect_uri:  self.redirect_uri.clone(),
            code_verifier: None,
        };

        let response: TokenResponseRaw = self
            .client
            .post(&self.discovery.token_endpoint)
            .form(&request)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to exchange code: {}", e),
            })?
            .json()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse token response: {}", e),
            })?;

        Ok(TokenResponse {
            access_token:  response.access_token,
            refresh_token: response.refresh_token,
            expires_in:    response.expires_in,
            token_type:    response.token_type.unwrap_or_else(|| "Bearer".to_string()),
        })
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        let response: UserInfoRaw = self
            .client
            .get(&self.discovery.userinfo_endpoint)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to get user info: {}", e),
            })?
            .json()
            .await
            .map_err(|e| AuthError::OAuthError {
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
            id:         response.sub,
            email:      response.email.unwrap_or_default(),
            name:       response.name,
            picture:    response.picture,
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

        let response: TokenResponseRaw = self
            .client
            .post(&self.discovery.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to refresh token: {}", e),
            })?
            .json()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse refresh response: {}", e),
            })?;

        Ok(TokenResponse {
            access_token:  response.access_token,
            refresh_token: response.refresh_token,
            expires_in:    response.expires_in,
            token_type:    response.token_type.unwrap_or_else(|| "Bearer".to_string()),
        })
    }

    async fn revoke_token(&self, token: &str) -> Result<()> {
        if let Some(revocation_endpoint) = &self.discovery.revocation_endpoint {
            let params = [
                ("token", token),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
            ];

            self.client.post(revocation_endpoint).form(&params).send().await.map_err(|e| {
                AuthError::OAuthError {
                    message: format!("Failed to revoke token: {}", e),
                }
            })?;
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
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_provider_debug() {
        let provider = OidcProvider {
            name:          "test".to_string(),
            issuer_url:    "https://example.com".to_string(),
            client_id:     "client_id".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri:  "http://localhost:8000/callback".to_string(),
            discovery:     OidcDiscovery {
                issuer:                 "https://example.com".to_string(),
                authorization_endpoint: "https://example.com/auth".to_string(),
                token_endpoint:         "https://example.com/token".to_string(),
                userinfo_endpoint:      "https://example.com/userinfo".to_string(),
                jwks_uri:               None,
                revocation_endpoint:    None,
            },
            client:        reqwest::Client::new(),
        };

        let debug_str = format!("{:?}", provider);
        assert!(debug_str.contains("OidcProvider"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_add_auth_params() {
        let provider = OidcProvider {
            name:          "test".to_string(),
            issuer_url:    "https://example.com".to_string(),
            client_id:     "my_client".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri:  "http://localhost:8000/callback".to_string(),
            discovery:     OidcDiscovery {
                issuer:                 "https://example.com".to_string(),
                authorization_endpoint: "https://example.com/auth".to_string(),
                token_endpoint:         "https://example.com/token".to_string(),
                userinfo_endpoint:      "https://example.com/userinfo".to_string(),
                jwks_uri:               None,
                revocation_endpoint:    None,
            },
            client:        reqwest::Client::new(),
        };

        let mut url = "https://example.com/auth".to_string();
        provider.add_auth_params(&mut url, "state123", None);

        assert!(url.contains("client_id=my_client"));
        assert!(url.contains("state=state123"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("scope=openid"));
    }
}
