//! OAuth2 token and user information types.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::jwt::{MAX_CLOCK_SKEW_SECS, MAX_TOKEN_AGE_SECS};

/// OAuth2 token response from provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// Access token for API calls
    pub access_token:  String,
    /// Refresh token for getting new access tokens
    pub refresh_token: Option<String>,
    /// Token type (typically "Bearer")
    pub token_type:    String,
    /// Seconds until access token expires
    pub expires_in:    u64,
    /// ID token (JWT) for OIDC
    pub id_token:      Option<String>,
    /// Requested scopes
    pub scope:         Option<String>,
}

impl TokenResponse {
    /// Create new token response
    #[must_use]
    pub const fn new(access_token: String, token_type: String, expires_in: u64) -> Self {
        Self {
            access_token,
            refresh_token: None,
            token_type,
            expires_in,
            id_token: None,
            scope: None,
        }
    }

    /// Calculate expiry time
    #[must_use]
    pub fn expiry_time(&self) -> DateTime<Utc> {
        Utc::now() + Duration::seconds(self.expires_in.cast_signed())
    }

    /// Check if token is expired
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expiry_time() <= Utc::now()
    }
}

/// JWT ID token claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    /// Issuer (provider identifier)
    pub iss:            String,
    /// Subject (unique user ID)
    pub sub:            String,
    /// Audience (should be client_id)
    pub aud:            String,
    /// Expiration time (Unix timestamp)
    pub exp:            i64,
    /// Issued at time (Unix timestamp)
    pub iat:            i64,
    /// Not-before time (Unix timestamp) — optional per RFC 7519 §4.1.5.
    ///
    /// When present, the token MUST NOT be accepted before this time (plus
    /// [`MAX_CLOCK_SKEW_SECS`]).  When absent, the not-before check is skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf:            Option<i64>,
    /// Authentication time (Unix timestamp)
    pub auth_time:      Option<i64>,
    /// Nonce (for replay protection)
    pub nonce:          Option<String>,
    /// Email address
    pub email:          Option<String>,
    /// Email verified flag
    pub email_verified: Option<bool>,
    /// User name
    pub name:           Option<String>,
    /// Profile picture URL
    pub picture:        Option<String>,
    /// Locale
    pub locale:         Option<String>,
}

impl IdTokenClaims {
    /// Create new ID token claims
    #[must_use]
    pub const fn new(iss: String, sub: String, aud: String, exp: i64, iat: i64) -> Self {
        Self {
            iss,
            sub,
            aud,
            exp,
            iat,
            nbf: None,
            auth_time: None,
            nonce: None,
            email: None,
            email_verified: None,
            name: None,
            picture: None,
            locale: None,
        }
    }

    /// Validate temporal claims: `iat` staleness/skew and `nbf` not-before.
    ///
    /// Enforces the same three guards as [`crate::jwt::Claims::validate_temporal_claims`]:
    ///
    /// - `iat` must not be more than [`MAX_CLOCK_SKEW_SECS`] seconds in the future.
    /// - `iat` must not be more than [`MAX_TOKEN_AGE_SECS`] seconds in the past.
    /// - `nbf` (if present) must not be more than [`MAX_CLOCK_SKEW_SECS`] seconds in the future
    ///   (RFC 7519 §4.1.5).
    ///
    /// # Errors
    ///
    /// Returns a `String` describing the validation failure, compatible with
    /// [`crate::oauth::client::OIDCClient::verify_id_token`]'s error return type.
    pub fn validate_temporal_claims(&self) -> std::result::Result<(), String> {
        let now = Utc::now().timestamp();
        let max_skew = i64::try_from(MAX_CLOCK_SKEW_SECS).unwrap_or(300);
        let max_age = i64::try_from(MAX_TOKEN_AGE_SECS).unwrap_or(86_400);

        // iat: must not be substantially in the future (forgery / clock-skew guard).
        if self.iat > now.saturating_add(max_skew) {
            return Err(
                "iat claim is too far in the future — possible forgery or clock skew".to_string()
            );
        }

        // iat: must not be older than MAX_TOKEN_AGE_SECS (replay guard).
        if now.saturating_sub(self.iat) > max_age {
            return Err("iat claim indicates token is too old (possible replay)".to_string());
        }

        // nbf: not-before — token must not be used before the claim (with clock skew).
        if let Some(nbf) = self.nbf {
            if nbf > now.saturating_add(max_skew) {
                return Err("token is not yet valid (nbf claim is in the future)".to_string());
            }
        }

        Ok(())
    }

    /// Check if token is expired
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.exp <= Utc::now().timestamp()
    }

    /// Check if token will be expired within grace period
    #[must_use]
    pub fn is_expiring_soon(&self, grace_seconds: i64) -> bool {
        self.exp <= (Utc::now().timestamp() + grace_seconds)
    }
}

/// Userinfo response from provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// Subject (unique user ID)
    pub sub:            String,
    /// Email address
    pub email:          Option<String>,
    /// Email verified flag
    pub email_verified: Option<bool>,
    /// User name
    pub name:           Option<String>,
    /// Profile picture URL
    pub picture:        Option<String>,
    /// Locale
    pub locale:         Option<String>,
}

impl UserInfo {
    /// Create new userinfo
    #[must_use]
    pub const fn new(sub: String) -> Self {
        Self {
            sub,
            email: None,
            email_verified: None,
            name: None,
            picture: None,
            locale: None,
        }
    }
}
