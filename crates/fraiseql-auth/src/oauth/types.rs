//! OAuth2 token and user information types.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// OAuth2 token response from provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// Access token for API calls
    pub access_token: String,
    /// Refresh token for getting new access tokens
    pub refresh_token: Option<String>,
    /// Token type (typically "Bearer")
    pub token_type: String,
    /// Seconds until access token expires
    pub expires_in: u64,
    /// ID token (JWT) for OIDC
    pub id_token: Option<String>,
    /// Requested scopes
    pub scope: Option<String>,
}

impl TokenResponse {
    /// Create new token response
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
    pub fn expiry_time(&self) -> DateTime<Utc> {
        Utc::now() + Duration::seconds(self.expires_in.cast_signed())
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
    pub iss: String,
    /// Subject (unique user ID)
    pub sub: String,
    /// Audience (should be client_id)
    pub aud: String,
    /// Expiration time (Unix timestamp)
    pub exp: i64,
    /// Issued at time (Unix timestamp)
    pub iat: i64,
    /// Authentication time (Unix timestamp)
    pub auth_time: Option<i64>,
    /// Nonce (for replay protection)
    pub nonce: Option<String>,
    /// Email address
    pub email: Option<String>,
    /// Email verified flag
    pub email_verified: Option<bool>,
    /// User name
    pub name: Option<String>,
    /// Profile picture URL
    pub picture: Option<String>,
    /// Locale
    pub locale: Option<String>,
}

impl IdTokenClaims {
    /// Create new ID token claims
    pub const fn new(iss: String, sub: String, aud: String, exp: i64, iat: i64) -> Self {
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
    pub sub: String,
    /// Email address
    pub email: Option<String>,
    /// Email verified flag
    pub email_verified: Option<bool>,
    /// User name
    pub name: Option<String>,
    /// Profile picture URL
    pub picture: Option<String>,
    /// Locale
    pub locale: Option<String>,
}

impl UserInfo {
    /// Create new userinfo
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- TokenResponse tests ---

    #[test]
    fn test_token_response_deserializes_from_json() {
        let json = r#"{
            "access_token": "eyJhbGciOiJSUzI1NiJ9.test.sig",
            "token_type": "Bearer",
            "expires_in": 3600,
            "refresh_token": "rt-abc123",
            "scope": "openid profile email"
        }"#;

        let token: TokenResponse = serde_json::from_str(json)
            .expect("valid OAuth token response JSON must deserialize successfully");

        assert_eq!(token.access_token, "eyJhbGciOiJSUzI1NiJ9.test.sig");
        assert_eq!(token.token_type, "Bearer");
        assert_eq!(token.expires_in, 3600);
        assert_eq!(token.refresh_token, Some("rt-abc123".to_string()));
        assert_eq!(token.scope, Some("openid profile email".to_string()));
    }

    #[test]
    fn test_token_response_missing_optional_fields() {
        let json = r#"{
            "access_token": "at_minimal",
            "token_type": "Bearer",
            "expires_in": 3600
        }"#;

        let token: TokenResponse = serde_json::from_str(json)
            .expect("token response without optional fields must still deserialize");

        assert!(token.refresh_token.is_none(), "missing refresh_token must deserialize to None");
        assert!(token.id_token.is_none(), "missing id_token must deserialize to None");
        assert!(token.scope.is_none(), "missing scope must deserialize to None");
    }

    #[test]
    fn test_token_response_missing_access_token_fails() {
        let json = r#"{
            "token_type": "Bearer",
            "expires_in": 3600
        }"#;

        let result: Result<TokenResponse, _> = serde_json::from_str(json);
        assert!(result.is_err(), "token response without access_token must fail to deserialize");
    }

    #[test]
    fn test_token_response_expiry_is_in_future() {
        let token = TokenResponse::new("at".to_string(), "Bearer".to_string(), 3600);
        let expiry = token.expiry_time();
        assert!(
            expiry > Utc::now(),
            "expiry_time for a token with expires_in=3600 must be in the future"
        );
    }

    #[test]
    fn test_token_response_new_is_not_expired() {
        let token = TokenResponse::new("at".to_string(), "Bearer".to_string(), 3600);
        assert!(
            !token.is_expired(),
            "a freshly created token with expires_in=3600 must not be expired"
        );
    }

    // --- IdTokenClaims tests ---

    #[test]
    fn test_id_token_claims_not_expired() {
        let exp = (Utc::now() + chrono::Duration::hours(1)).timestamp();
        let claims = IdTokenClaims::new(
            "https://issuer.example.com".to_string(),
            "user123".to_string(),
            "client_id".to_string(),
            exp,
            Utc::now().timestamp(),
        );
        assert!(!claims.is_expired(), "future exp must not be expired");
    }

    #[test]
    fn test_id_token_claims_expired() {
        let exp = (Utc::now() - chrono::Duration::hours(1)).timestamp();
        let claims = IdTokenClaims::new(
            "https://issuer.example.com".to_string(),
            "user123".to_string(),
            "client_id".to_string(),
            exp,
            Utc::now().timestamp(),
        );
        assert!(claims.is_expired(), "past exp must be expired");
    }

    #[test]
    fn test_id_token_claims_expiring_soon() {
        let exp = (Utc::now() + chrono::Duration::seconds(30)).timestamp();
        let claims = IdTokenClaims::new(
            "https://issuer.example.com".to_string(),
            "user123".to_string(),
            "client_id".to_string(),
            exp,
            Utc::now().timestamp(),
        );
        assert!(
            claims.is_expiring_soon(60),
            "token expiring in 30s must be considered expiring soon with grace=60s"
        );
        assert!(
            !claims.is_expiring_soon(10),
            "token expiring in 30s must not be considered expiring soon with grace=10s"
        );
    }

    // --- UserInfo tests ---

    #[test]
    fn test_userinfo_creation() {
        let user = UserInfo::new("sub_123".to_string());
        assert_eq!(user.sub, "sub_123");
        assert!(user.email.is_none());
        assert!(user.name.is_none());
    }

    #[test]
    fn test_userinfo_deserializes_from_json() {
        let json = r#"{
            "sub": "user_789",
            "email": "user@example.com",
            "email_verified": true,
            "name": "Test User"
        }"#;
        let user: UserInfo =
            serde_json::from_str(json).expect("valid userinfo JSON must deserialize");
        assert_eq!(user.sub, "user_789");
        assert_eq!(user.email, Some("user@example.com".to_string()));
        assert_eq!(user.email_verified, Some(true));
    }
}
