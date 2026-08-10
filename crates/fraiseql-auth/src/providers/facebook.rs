//! Facebook `OAuth2` provider (#944).
//!
//! Plain `OAuth2` on the GitHub template — fixed well-known endpoints,
//! network-free construction, SSRF-guarded base-URL overrides — with two
//! Facebook-specific facts that shape the whole implementation.
//!
//! # The API version is in the path, so it is configuration
//!
//! Every Graph endpoint carries a version segment (`/v21.0/me`), and Meta
//! deprecates versions on its own schedule. Hard-coding it would mean the
//! provider breaks on Meta's timetable rather than on a release of ours, so the
//! version is configurable and merely *defaults* to [`DEFAULT_API_VERSION`].
//!
//! # Facebook cannot say whether an email is verified
//!
//! `email` may be absent entirely — a phone-number-only account, or a user who
//! declined the permission — and there is **no verification flag at all** in the
//! response. There is therefore nothing to gate on: this provider reports
//! `email_verified = false` unconditionally, so every Facebook identity keys on
//! `(facebook, id)` and can never link into an existing email-keyed account.
//! `facebook` is correspondingly **absent** from the default
//! [`TrustedEmailProviders`](crate::TrustedEmailProviders) set — two independent
//! reasons the address cannot become a linking key.
//!
//! # What is deliberately not here
//!
//! Facebook issues no refresh token; the analogous operation is exchanging a
//! short-lived token for a long-lived one (`grant_type=fb_exchange_token`),
//! which takes an access token rather than a refresh token and has no consumer
//! in FraiseQL's flow. [`OAuthProvider::refresh_token`]'s default — refuse and
//! say so — is the honest answer, rather than a method nothing calls.

use std::{fmt::Write as _, time::Duration};

use async_trait::async_trait;
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::{
    error::{AuthError, Result},
    oidc_provider::validate_oauth_endpoint_url,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// Timeout for all Facebook HTTP requests.
const FACEBOOK_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum byte size for a Facebook response.
const MAX_FACEBOOK_RESPONSE_BYTES: usize = 1024 * 1024; // 1 MiB

/// Default web base URL — the authorization dialog lives here.
const DEFAULT_BASE_URL: &str = "https://www.facebook.com";

/// Default Graph API base URL — the token and `me` endpoints live here.
const DEFAULT_GRAPH_BASE_URL: &str = "https://graph.facebook.com";

/// Graph API version used when the operator names none. Meta deprecates
/// versions on a schedule, which is why this is a default and not a constant
/// baked into the request paths.
pub const DEFAULT_API_VERSION: &str = "v21.0";

/// Scope requested. `email` is a permission the user may decline, which is one
/// of the two reasons the address may simply not arrive.
const FACEBOOK_SCOPES: &str = "email";

/// Fields requested from `/me`. Facebook returns only what is asked for.
const FACEBOOK_ME_FIELDS: &str = "id,name,email";

/// Facebook `/me` response.
#[derive(Debug, Clone, Deserialize)]
pub struct FacebookUser {
    /// App-scoped user ID, stable for this app.
    pub id:    String,
    /// Display name.
    #[serde(default)]
    pub name:  Option<String>,
    /// Email address — **may be absent**, and never carries a verified flag.
    #[serde(default)]
    pub email: Option<String>,
}

/// Facebook's token-endpoint response. There is no refresh token.
#[derive(Debug, Deserialize)]
struct FacebookTokenResponse {
    access_token: String,
    #[serde(default)]
    token_type:   Option<String>,
    #[serde(default)]
    expires_in:   Option<u64>,
}

/// Facebook `OAuth2` provider.
pub struct FacebookOAuth {
    client_id:      String,
    /// Wiped from memory when the provider is dropped.
    client_secret:  Zeroizing<String>,
    redirect_uri:   String,
    /// Web base URL (`https://www.facebook.com`).
    base_url:       String,
    /// Graph API base URL (`https://graph.facebook.com`).
    graph_base_url: String,
    /// Graph API version segment, e.g. `v21.0`.
    api_version:    String,
    client:         reqwest::Client,
}

impl std::fmt::Debug for FacebookOAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FacebookOAuth")
            .field("client_id", &self.client_id)
            .field("redirect_uri", &self.redirect_uri)
            .field("base_url", &self.base_url)
            .field("graph_base_url", &self.graph_base_url)
            .field("api_version", &self.api_version)
            .finish_non_exhaustive() // client_secret omitted for security
    }
}

impl FacebookOAuth {
    /// Create a provider against the well-known Facebook endpoints, on
    /// [`DEFAULT_API_VERSION`].
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ConfigError`] if the HTTP client cannot be built.
    pub fn new(client_id: String, client_secret: String, redirect_uri: String) -> Result<Self> {
        Self::with_endpoints(
            client_id,
            client_secret,
            redirect_uri,
            DEFAULT_BASE_URL.to_string(),
            DEFAULT_GRAPH_BASE_URL.to_string(),
            DEFAULT_API_VERSION.to_string(),
        )
    }

    /// Create a provider against explicit endpoints and API version — a newer
    /// Graph version, or a stub `IdP` in tests.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::OidcMetadataError`] when either URL fails the shared
    /// SSRF guard (non-HTTPS scheme or a private/loopback/link-local address;
    /// the `FRAISEQL_OIDC_ALLOW_INSECURE` development bypass applies),
    /// [`AuthError::ConfigError`] if `api_version` is empty or contains a path
    /// separator (it is interpolated into request paths), or
    /// [`AuthError::ConfigError`] if the HTTP client cannot be built.
    pub fn with_endpoints(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
        base_url: String,
        graph_base_url: String,
        api_version: String,
    ) -> Result<Self> {
        validate_oauth_endpoint_url(&base_url)?;
        validate_oauth_endpoint_url(&graph_base_url)?;
        // The version lands in a URL path, so a value carrying `/`, `?` or `#`
        // would re-point the request at an endpoint the operator did not name.
        let version = api_version.trim();
        if version.is_empty() || version.contains(['/', '?', '#', '\\']) {
            return Err(AuthError::ConfigError {
                message: format!(
                    "[auth.social.facebook] api_version {api_version:?} is not a Graph API \
                     version segment (expected something like {DEFAULT_API_VERSION})"
                ),
            });
        }
        let client =
            reqwest::Client::builder()
                .timeout(FACEBOOK_REQUEST_TIMEOUT)
                .build()
                .map_err(|e| AuthError::ConfigError {
                    message: format!("Failed to create HTTP client: {e}"),
                })?;
        Ok(Self {
            client_id,
            client_secret: Zeroizing::new(client_secret),
            redirect_uri,
            base_url: base_url.trim_end_matches('/').to_string(),
            graph_base_url: graph_base_url.trim_end_matches('/').to_string(),
            api_version: version.to_string(),
            client,
        })
    }

    /// Fetch the `/me` profile.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::OAuthError`] if the request fails, returns a
    /// non-success status, exceeds the size cap, or does not parse.
    pub async fn get_user(&self, access_token: &str) -> Result<FacebookUser> {
        let resp = self
            .client
            .get(format!(
                "{}/{}/me?fields={FACEBOOK_ME_FIELDS}",
                self.graph_base_url, self.api_version
            ))
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to fetch Facebook profile: {e}"),
            })?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| AuthError::OAuthError {
            message: format!("Failed to read Facebook profile response: {e}"),
        })?;
        if !status.is_success() {
            return Err(AuthError::OAuthError {
                message: format!("Facebook /me returned HTTP {status}"),
            });
        }
        if bytes.len() > MAX_FACEBOOK_RESPONSE_BYTES {
            return Err(AuthError::OAuthError {
                message: format!("Facebook profile response too large ({} bytes)", bytes.len()),
            });
        }
        serde_json::from_slice(&bytes).map_err(|e| AuthError::OAuthError {
            message: format!("Failed to parse Facebook profile: {e}"),
        })
    }
}

// Reason: OAuthProvider is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl OAuthProvider for FacebookOAuth {
    fn name(&self) -> &'static str {
        "facebook"
    }

    fn authorization_url(&self, state: &str) -> String {
        let mut url = format!("{}/{}/dialog/oauth", self.base_url, self.api_version);
        write!(
            url,
            "?client_id={}&redirect_uri={}&state={}&response_type=code&scope={}",
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&self.redirect_uri),
            urlencoding::encode(state),
            urlencoding::encode(FACEBOOK_SCOPES),
        )
        .expect("write to String is infallible");
        url
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        // Facebook documents this endpoint as a GET with the secret in the query
        // string; it accepts a POST body too, and that keeps the client secret
        // out of proxy and CDN access logs.
        let resp = self
            .client
            .post(format!("{}/{}/oauth/access_token", self.graph_base_url, self.api_version))
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("code", code),
                ("redirect_uri", self.redirect_uri.as_str()),
            ])
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Facebook token endpoint request failed: {e}"),
            })?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| AuthError::OAuthError {
            message: format!("Failed to read Facebook token response: {e}"),
        })?;
        if !status.is_success() {
            return Err(AuthError::OAuthError {
                message: format!("Facebook token endpoint returned HTTP {status}"),
            });
        }
        if bytes.len() > MAX_FACEBOOK_RESPONSE_BYTES {
            return Err(AuthError::OAuthError {
                message: format!("Facebook token response too large ({} bytes)", bytes.len()),
            });
        }
        let response: FacebookTokenResponse =
            serde_json::from_slice(&bytes).map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse Facebook token response: {e}"),
            })?;
        Ok(TokenResponse {
            access_token:  response.access_token,
            // Facebook issues none — see the module docs.
            refresh_token: None,
            expires_in:    response.expires_in.unwrap_or(0),
            token_type:    response.token_type.unwrap_or_else(|| "Bearer".to_string()),
            id_token:      None,
        })
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        let user = self.get_user(access_token).await?;
        // Normalize an empty claim to `None`; it could never key a link anyway,
        // because the flag below is unconditionally false.
        let email = user.email.clone().filter(|e| !e.trim().is_empty());

        let mut raw_claims = serde_json::Map::new();
        raw_claims.insert("facebook_id".to_string(), serde_json::json!(user.id));
        if let Some(ref email) = email {
            raw_claims.insert("email".to_string(), serde_json::json!(email));
        }
        // Facebook publishes no verification signal whatsoever. Reporting false
        // is not caution, it is the truth: there is nothing to report.
        raw_claims.insert("email_verified".to_string(), serde_json::json!(false));

        Ok(UserInfo {
            id: user.id.clone(),
            email,
            email_verified: false,
            name: user.name,
            picture: None,
            raw_claims: serde_json::Value::Object(raw_claims),
        })
    }
}
