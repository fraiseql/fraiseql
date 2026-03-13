//! OAuth2 token and user information types.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

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
    pub fn new(access_token: String, token_type: String, expires_in: u64) -> Self {
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
    pub fn expiry_time(&self) -> DateTime<Utc> {
        Utc::now() + Duration::seconds(self.expires_in as i64)
    }

    /// Check if token is expired
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
    pub fn new(iss: String, sub: String, aud: String, exp: i64, iat: i64) -> Self {
        Self {
            iss,
            sub,
            aud,
            exp,
            iat,
            auth_time: None,
            nonce: None,
            email: None,
            email_verified: None,
            name: None,
            picture: None,
            locale: None,
        }
    }

    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        self.exp <= Utc::now().timestamp()
    }

    /// Check if token will be expired within grace period
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
    pub fn new(sub: String) -> Self {
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
