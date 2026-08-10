//! Discord `OAuth2` provider (#944).
//!
//! Discord is plain `OAuth2`, not OIDC — there is no discovery document and no
//! `sub` claim — so this follows the GitHub template: fixed well-known
//! endpoints, network-free construction, and an SSRF-guarded base-URL override.
//!
//! # The email is on the user object, and the flag must be read
//!
//! Unlike GitHub, Discord needs no second hop: `/api/users/@me` carries both
//! `email` and `verified`. That makes it easy to *assume* the address is
//! verified, which is the mistake this provider must not make. An unverified
//! Discord email keys on `(discord, id)` and can never collapse into an
//! existing email-keyed account. Only because that flag is honoured is
//! `discord` in the default [`TrustedEmailProviders`] set — the trust and the
//! check ship together.
//!
//! [`TrustedEmailProviders`]: crate::TrustedEmailProviders

use std::{fmt::Write as _, time::Duration};

use async_trait::async_trait;
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::{
    error::{AuthError, Result},
    oidc_provider::validate_oauth_endpoint_url,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// Timeout for all Discord API HTTP requests.
const DISCORD_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum byte size for a Discord API response. The user object is a small
/// JSON document; this cap blocks allocation bombs from a network intermediary.
const MAX_DISCORD_RESPONSE_BYTES: usize = 1024 * 1024; // 1 MiB

/// Default Discord base URL (authorize, token and API endpoints live under it).
const DEFAULT_BASE_URL: &str = "https://discord.com";

/// Host serving user avatars, used to build [`UserInfo::picture`].
const AVATAR_CDN: &str = "https://cdn.discordapp.com";

/// Scopes requested: `identify` for the user object, `email` for the address
/// and its `verified` flag.
const DISCORD_SCOPES: &str = "identify email";

/// Discord user object (`GET /api/users/@me`).
#[derive(Debug, Clone, Deserialize)]
pub struct DiscordUser {
    /// Snowflake user ID, stable across username changes.
    pub id:          String,
    /// Current username.
    pub username:    String,
    /// Display name, when the account has one.
    #[serde(default)]
    pub global_name: Option<String>,
    /// Email address. Absent unless the `email` scope was granted.
    #[serde(default)]
    pub email:       Option<String>,
    /// Whether Discord has verified the address. Absent is **not** verified.
    #[serde(default)]
    pub verified:    Option<bool>,
    /// Avatar hash, used to build the CDN URL.
    #[serde(default)]
    pub avatar:      Option<String>,
}

/// Discord's token-endpoint response.
#[derive(Debug, Deserialize)]
struct DiscordTokenResponse {
    access_token:  String,
    #[serde(default)]
    token_type:    Option<String>,
    #[serde(default)]
    expires_in:    Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
}

/// Discord `OAuth2` provider.
pub struct DiscordOAuth {
    client_id:     String,
    /// Wiped from memory when the provider is dropped.
    client_secret: Zeroizing<String>,
    redirect_uri:  String,
    /// Base URL (`https://discord.com`, or a stub in tests).
    base_url:      String,
    client:        reqwest::Client,
}

impl std::fmt::Debug for DiscordOAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscordOAuth")
            .field("client_id", &self.client_id)
            .field("redirect_uri", &self.redirect_uri)
            .field("base_url", &self.base_url)
            .finish_non_exhaustive() // client_secret omitted for security
    }
}

/// Resolve the linkable email from a Discord user object.
///
/// Returns the address with the verified flag Discord actually reported. An
/// absent flag is `false`, so an address Discord has not confirmed can never
/// key an account (fail-closed). An empty/whitespace address is no address.
#[must_use]
pub fn select_linkable_email(user: &DiscordUser) -> (Option<String>, bool) {
    let email = user.email.clone().filter(|e| !e.trim().is_empty());
    let verified = email.is_some() && user.verified.unwrap_or(false);
    (email, verified)
}

impl DiscordOAuth {
    /// Create a provider against the well-known `discord.com` endpoints.
    ///
    /// Construction is network-free — Discord serves no OIDC discovery
    /// document, so there is nothing to fetch at boot.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ConfigError`] if the HTTP client cannot be built.
    pub fn new(client_id: String, client_secret: String, redirect_uri: String) -> Result<Self> {
        Self::with_base_url(client_id, client_secret, redirect_uri, DEFAULT_BASE_URL.to_string())
    }

    /// Create a provider against an explicit base URL — a stub `IdP` in tests.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::OidcMetadataError`] when the URL fails the shared
    /// SSRF guard (non-HTTPS scheme or a private/loopback/link-local address;
    /// the `FRAISEQL_OIDC_ALLOW_INSECURE` development bypass applies), or
    /// [`AuthError::ConfigError`] if the HTTP client cannot be built.
    pub fn with_base_url(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
        base_url: String,
    ) -> Result<Self> {
        validate_oauth_endpoint_url(&base_url)?;
        let client =
            reqwest::Client::builder()
                .timeout(DISCORD_REQUEST_TIMEOUT)
                .build()
                .map_err(|e| AuthError::ConfigError {
                    message: format!("Failed to create HTTP client: {e}"),
                })?;
        Ok(Self {
            client_id,
            client_secret: Zeroizing::new(client_secret),
            redirect_uri,
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        })
    }

    /// POST a form to the token endpoint and parse the response.
    async fn post_token(&self, params: &[(&str, &str)]) -> Result<TokenResponse> {
        let resp = self
            .client
            .post(format!("{}/api/oauth2/token", self.base_url))
            .form(params)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Discord token endpoint request failed: {e}"),
            })?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| AuthError::OAuthError {
            message: format!("Failed to read Discord token response: {e}"),
        })?;
        if !status.is_success() {
            return Err(AuthError::OAuthError {
                message: format!("Discord token endpoint returned HTTP {status}"),
            });
        }
        if bytes.len() > MAX_DISCORD_RESPONSE_BYTES {
            return Err(AuthError::OAuthError {
                message: format!("Discord token response too large ({} bytes)", bytes.len()),
            });
        }
        let response: DiscordTokenResponse =
            serde_json::from_slice(&bytes).map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse Discord token response: {e}"),
            })?;
        Ok(TokenResponse {
            access_token:  response.access_token,
            refresh_token: response.refresh_token,
            expires_in:    response.expires_in.unwrap_or(0),
            token_type:    response.token_type.unwrap_or_else(|| "Bearer".to_string()),
            // Plain OAuth2 — Discord issues no ID token.
            id_token:      None,
        })
    }

    /// Fetch the Discord user object.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::OAuthError`] if the request fails, returns a
    /// non-success status, exceeds the size cap, or does not parse.
    pub async fn get_user(&self, access_token: &str) -> Result<DiscordUser> {
        let resp = self
            .client
            .get(format!("{}/api/users/@me", self.base_url))
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to fetch Discord user: {e}"),
            })?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| AuthError::OAuthError {
            message: format!("Failed to read Discord user response: {e}"),
        })?;
        if !status.is_success() {
            return Err(AuthError::OAuthError {
                message: format!("Discord users/@me returned HTTP {status}"),
            });
        }
        if bytes.len() > MAX_DISCORD_RESPONSE_BYTES {
            return Err(AuthError::OAuthError {
                message: format!("Discord user response too large ({} bytes)", bytes.len()),
            });
        }
        serde_json::from_slice(&bytes).map_err(|e| AuthError::OAuthError {
            message: format!("Failed to parse Discord user: {e}"),
        })
    }
}

// Reason: OAuthProvider is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl OAuthProvider for DiscordOAuth {
    fn name(&self) -> &'static str {
        "discord"
    }

    fn authorization_url(&self, state: &str) -> String {
        let mut url = format!("{}/oauth2/authorize", self.base_url);
        write!(
            url,
            "?client_id={}&redirect_uri={}&state={}&response_type=code&scope={}",
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&self.redirect_uri),
            urlencoding::encode(state),
            urlencoding::encode(DISCORD_SCOPES),
        )
        .expect("write to String is infallible");
        url
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        self.post_token(&[
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.redirect_uri.as_str()),
        ])
        .await
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        let user = self.get_user(access_token).await?;
        let (email, email_verified) = select_linkable_email(&user);

        let mut raw_claims = serde_json::Map::new();
        raw_claims.insert("discord_id".to_string(), serde_json::json!(user.id));
        raw_claims.insert("discord_username".to_string(), serde_json::json!(user.username));
        if let Some(ref email) = email {
            raw_claims.insert("email".to_string(), serde_json::json!(email));
        }
        raw_claims.insert("email_verified".to_string(), serde_json::json!(email_verified));

        let picture = user
            .avatar
            .as_ref()
            .map(|hash| format!("{AVATAR_CDN}/avatars/{}/{hash}.png", user.id));

        Ok(UserInfo {
            id: user.id.clone(),
            email,
            email_verified,
            name: user.global_name.clone().or_else(|| Some(user.username.clone())),
            picture,
            raw_claims: serde_json::Value::Object(raw_claims),
        })
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        self.post_token(&[
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])
        .await
    }
}
