// OAuth 2.0 / OIDC provider trait and implementations
use crate::auth::error::{AuthError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

/// User information retrieved from OAuth provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// Unique user identifier from provider
    pub id: String,
    /// User's email address
    pub email: String,
    /// User's display name (optional)
    pub name: Option<String>,
    /// User's profile picture URL (optional)
    pub picture: Option<String>,
    /// Raw claims from provider (for custom fields)
    pub raw_claims: serde_json::Value,
}

/// Token response from OAuth provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// Access token (short-lived)
    pub access_token: String,
    /// Refresh token if provider supports it
    pub refresh_token: Option<String>,
    /// Token expiration in seconds
    pub expires_in: u64,
    /// Token type (typically "Bearer")
    pub token_type: String,
}

/// OAuth 2.0 / OIDC provider trait
///
/// Implement this trait to add support for custom OAuth providers.
#[async_trait]
pub trait OAuthProvider: Send + Sync + fmt::Debug {
    /// Provider name for logging/debugging
    fn name(&self) -> &str;

    /// Generate authorization URL for user to visit
    ///
    /// # Arguments
    /// * `state` - CSRF protection state (should be cryptographically random)
    fn authorization_url(&self, state: &str) -> String;

    /// Exchange authorization code for tokens
    ///
    /// # Arguments
    /// * `code` - Authorization code from provider
    ///
    /// # Returns
    /// Token response with access_token and optional refresh_token
    async fn exchange_code(&self, code: &str) -> Result<TokenResponse>;

    /// Get user information using access token
    ///
    /// # Arguments
    /// * `access_token` - The access token to use for API call
    ///
    /// # Returns
    /// UserInfo with user details from provider
    async fn user_info(&self, access_token: &str) -> Result<UserInfo>;

    /// Refresh the access token (optional, default returns error)
    ///
    /// # Arguments
    /// * `refresh_token` - The refresh token
    ///
    /// # Returns
    /// New TokenResponse if provider supports refresh
    async fn refresh_token(&self, _refresh_token: &str) -> Result<TokenResponse> {
        Err(AuthError::OAuthError {
            message: format!("{} does not support token refresh", self.name()),
        })
    }

    /// Revoke a token (optional, default is no-op)
    ///
    /// # Arguments
    /// * `token` - Token to revoke
    async fn revoke_token(&self, _token: &str) -> Result<()> {
        Ok(())
    }
}

/// PKCE (Proof Key for Public Clients) helper
///
/// Used to prevent authorization code interception attacks
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    /// Generated code verifier (cryptographically random)
    pub verifier: String,
    /// Code challenge (SHA256 hash of verifier)
    pub challenge: String,
}

impl PkceChallenge {
    /// Generate a new PKCE challenge
    pub fn generate() -> Result<Self> {
        use sha2::{Digest, Sha256};

        let verifier = generate_pkce_verifier()?;

        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge_bytes = hasher.finalize();
        let challenge = base64_url_encode(&challenge_bytes);

        Ok(Self { verifier, challenge })
    }

    /// Validate a verifier against a challenge
    pub fn validate(&self, verifier: &str) -> bool {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let hash = hasher.finalize();
        let encoded = base64_url_encode(&hash);

        encoded == self.challenge
    }
}

/// Generate a PKCE verifier (43-128 characters of unreserved characters)
fn generate_pkce_verifier() -> Result<String> {
    use rand::Rng;

    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    const VERIFIER_LENGTH: usize = 128;

    let mut rng = rand::thread_rng();
    let verifier: String = (0..VERIFIER_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();

    Ok(verifier)
}

/// URL-safe base64 encoding for PKCE
fn base64_url_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_challenge_generation() {
        let challenge = PkceChallenge::generate().expect("Failed to generate challenge");
        assert!(!challenge.verifier.is_empty());
        assert!(!challenge.challenge.is_empty());
        assert!(challenge.verifier.len() >= 43 && challenge.verifier.len() <= 128);
    }

    #[test]
    fn test_pkce_validation() {
        let challenge = PkceChallenge::generate().expect("Failed to generate challenge");
        assert!(challenge.validate(&challenge.verifier));

        let wrong_verifier = "wrong_verifier";
        assert!(!challenge.validate(wrong_verifier));
    }

    #[test]
    fn test_base64_url_encode() {
        let bytes = b"hello world";
        let encoded = base64_url_encode(bytes);
        assert!(!encoded.is_empty());
        // URL-safe base64 should not contain + or /
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
    }
}
