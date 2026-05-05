//! Generic OIDC provider implementation using RFC 8414 discovery.
use std::{fmt::Write as _, time::Duration};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Timeout for all OIDC HTTP operations (discovery, token exchange, user info, refresh).
pub(crate) const OIDC_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum byte size for an OIDC discovery document response.
///
/// A well-formed `.well-known/openid-configuration` payload is a few `KiB`.
/// 64 `KiB` blocks allocation bombs from a compromised OIDC provider.
pub(crate) const MAX_OIDC_DISCOVERY_BYTES: usize = 64 * 1024; // 64 KiB

/// Maximum byte size for OIDC token and user-info responses.
///
/// Token responses carry JWTs and a handful of metadata fields.
/// 1 `MiB` is generous while preventing runaway allocation.
pub(crate) const MAX_OIDC_TOKEN_BYTES: usize = 1024 * 1024; // 1 MiB

use crate::{
    error::{AuthError, Result},
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// OIDC Discovery document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OidcDiscovery {
    pub(crate) issuer:                 String,
    pub(crate) authorization_endpoint: String,
    pub(crate) token_endpoint:         String,
    pub(crate) userinfo_endpoint:      String,
    pub(crate) jwks_uri:               Option<String>,
    #[serde(default)]
    pub(crate) revocation_endpoint:    Option<String>,
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
    pub(crate) name:          String,
    pub(crate) issuer_url:    String,
    pub(crate) client_id:     String,
    /// Stored as `Zeroizing<String>` so the key material is wiped from memory
    /// when this struct is dropped.
    pub(crate) client_secret: Zeroizing<String>,
    pub(crate) redirect_uri:  String,
    pub(crate) discovery:     OidcDiscovery,
    pub(crate) client:        reqwest::Client,
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

/// Validate an OIDC issuer URL against SSRF-prone destinations.
///
/// Rejects:
/// - Non-HTTPS schemes (OIDC issuers must use TLS)
/// - Loopback addresses (`127.0.0.0/8`, `::1`, `localhost`)
/// - RFC 1918 private ranges (`10/8`, `172.16/12`, `192.168/16`)
/// - Link-local addresses (`169.254/16`) — includes AWS IMDSv1/v2
/// - IPv6 ULA (`fc00::/7`)
///
/// # Errors
///
/// Returns [`AuthError::OidcMetadataError`] if the URL is invalid, uses a non-HTTPS
/// scheme, or targets a private/loopback address.
pub(crate) fn validate_oidc_issuer_url(issuer_url: &str) -> Result<()> {
    // When `FRAISEQL_OIDC_ALLOW_INSECURE=true` all SSRF guards are disabled.
    // This is intended for local development and integration testing only —
    // never set in production.
    let allow_insecure = std::env::var("FRAISEQL_OIDC_ALLOW_INSECURE")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);
    if allow_insecure {
        return Ok(());
    }

    let lower = issuer_url.to_ascii_lowercase();
    if !lower.starts_with("https://") {
        return Err(AuthError::OidcMetadataError {
            message: format!(
                "OIDC issuer URL must use https:// scheme (SSRF protection): {issuer_url}"
            ),
        });
    }

    // Use a URL parser to safely extract the host — manual string-splitting is
    // fragile for IPv6 literals like `[::1]`.
    let parsed = reqwest::Url::parse(issuer_url).map_err(|e| AuthError::OidcMetadataError {
        message: format!("OIDC issuer URL is not a valid URL ({issuer_url}): {e}"),
    })?;

    let host_raw = parsed.host_str().unwrap_or("");
    // The `url` crate wraps IPv6 literals in brackets in `host_str()`.
    // Strip them so the IP address can be parsed by `IpAddr::from_str`.
    let host = if host_raw.starts_with('[') && host_raw.ends_with(']') {
        &host_raw[1..host_raw.len() - 1]
    } else {
        host_raw
    };

    if is_ssrf_blocked_oidc_host(host) {
        return Err(AuthError::OidcMetadataError {
            message: format!(
                "OIDC issuer URL targets a private/loopback address (SSRF protection): \
                 {issuer_url}"
            ),
        });
    }

    Ok(())
}

fn is_ssrf_blocked_oidc_host(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" {
        return true;
    }
    if let Ok(addr) = host.parse::<std::net::Ipv4Addr>() {
        return addr.is_loopback() || addr.is_private() || addr.is_link_local();
    }
    if let Ok(addr) = host.parse::<std::net::Ipv6Addr>() {
        // Block loopback, unspecified, and ULA (fc00::/7)
        return addr.is_loopback() || addr.is_unspecified() || is_ula_v6_oidc(addr);
    }
    false
}

const fn is_ula_v6_oidc(addr: std::net::Ipv6Addr) -> bool {
    // fc00::/7
    (addr.segments()[0] & 0xFE00) == 0xFC00
}

impl OidcProvider {
    /// Create a new OIDC provider
    ///
    /// # Arguments
    /// * `name` - Provider name (for logging)
    /// * `issuer_url` - The issuer URL (e.g., <https://accounts.google.com>); must use `https://`
    /// * `client_id` - OAuth client ID
    /// * `client_secret` - OAuth client secret
    /// * `redirect_uri` - Redirect URI after authentication
    ///
    /// # Errors
    /// Returns error if the issuer URL is invalid, uses a non-HTTPS scheme, targets a
    /// private/loopback address (SSRF protection), or if metadata discovery fails.
    pub async fn new(
        name: &str,
        issuer_url: &str,
        client_id: &str,
        client_secret: &str,
        redirect_uri: &str,
    ) -> Result<Self> {
        // SECURITY: Validate issuer URL before making any HTTP request to prevent SSRF.
        // Requires https:// scheme and rejects private/loopback destinations.
        validate_oidc_issuer_url(issuer_url)?;

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
            client_secret: Zeroizing::new(client_secret.to_string()),
            redirect_uri: redirect_uri.to_string(),
            discovery,
            client,
        })
    }

    /// Add authorization URL parameter
    pub(crate) fn add_auth_params(&self, url: &mut String, state: &str, pkce_challenge: Option<&str>) {
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
            grant_type:    "authorization_code".to_string(),
            code:          code.to_string(),
            client_id:     self.client_id.clone(),
            client_secret: (*self.client_secret).clone(),
            redirect_uri:  self.redirect_uri.clone(),
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
            access_token:  response.access_token,
            refresh_token: response.refresh_token,
            expires_in:    response.expires_in,
            token_type:    response.token_type.unwrap_or_else(|| "Bearer".to_string()),
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
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
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
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
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
