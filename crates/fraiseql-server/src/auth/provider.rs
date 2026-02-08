// OAuth 2.0 / OIDC provider trait and implementations
use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::auth::error::{AuthError, Result};

/// User information retrieved from OAuth provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// Unique user identifier from provider
    pub id:         String,
    /// User's email address
    pub email:      String,
    /// User's display name (optional)
    pub name:       Option<String>,
    /// User's profile picture URL (optional)
    pub picture:    Option<String>,
    /// Raw claims from provider (for custom fields)
    pub raw_claims: serde_json::Value,
}

/// Token response from OAuth provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// Access token (short-lived)
    pub access_token:  String,
    /// Refresh token if provider supports it
    pub refresh_token: Option<String>,
    /// Token expiration in seconds
    pub expires_in:    u64,
    /// Token type (typically "Bearer")
    pub token_type:    String,
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
    pub verifier:  String,
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

        Ok(Self {
            verifier,
            challenge,
        })
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
///
/// # SECURITY
///
/// This uses `rand::thread_rng()` which is cryptographically secure on all major platforms.
/// It generates a 128-character random string using only unreserved characters as per RFC 7636.
///
/// The generated verifier meets these requirements:
/// - Length: exactly 128 characters (within 43-128 range)
/// - Characters: only unreserved ASCII characters: [A-Z a-z 0-9 - . _ ~]
/// - Randomness: cryptographically secure pseudorandom generation
/// - No padding: can be used directly in PKCE challenge
///
/// # Errors
///
/// Returns error if:
/// - Random number generation fails (extremely rare)
/// - Generated verifier is invalid (should never happen given the constraints)
///
/// # Implementation Notes
///
/// We use a fixed 128-character length (maximum allowed by RFC 7636) for:
/// 1. Maximum security: more entropy means harder to guess
/// 2. Consistency: predictable length for tests and monitoring
/// 3. Compatibility: all OAuth providers support 128-char verifiers
fn generate_pkce_verifier() -> Result<String> {
    use rand::Rng;

    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    const VERIFIER_LENGTH: usize = 128; // Maximum allowed by RFC 7636
    const MIN_VERIFIER_LENGTH: usize = 43; // Minimum allowed by RFC 7636

    let mut rng = rand::thread_rng();
    let verifier: String = (0..VERIFIER_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();

    // Validate the generated verifier meets RFC 7636 requirements
    if verifier.len() < MIN_VERIFIER_LENGTH {
        return Err(AuthError::PkceError {
            message: format!(
                "Generated PKCE verifier too short: {} < {} chars",
                verifier.len(),
                MIN_VERIFIER_LENGTH
            ),
        });
    }

    if verifier.len() > 128 {
        return Err(AuthError::PkceError {
            message: format!(
                "Generated PKCE verifier too long: {} > 128 chars",
                verifier.len()
            ),
        });
    }

    // Verify all characters are from the allowed charset
    let allowed_chars = std::collections::HashSet::<char>::from_iter(
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~".chars(),
    );

    for (i, c) in verifier.chars().enumerate() {
        if !allowed_chars.contains(&c) {
            return Err(AuthError::PkceError {
                message: format!(
                    "Generated PKCE verifier contains invalid character '{}' at position {}",
                    c, i
                ),
            });
        }
    }

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
        // Test proper error handling - generation should always succeed
        let challenge_result = PkceChallenge::generate();
        assert!(
            challenge_result.is_ok(),
            "PKCE challenge generation should succeed"
        );

        let challenge = challenge_result.unwrap();
        assert!(!challenge.verifier.is_empty(), "Verifier should not be empty");
        assert!(!challenge.challenge.is_empty(), "Challenge should not be empty");
        assert!(
            challenge.verifier.len() >= 43 && challenge.verifier.len() <= 128,
            "Verifier length must be 43-128 characters per RFC 7636"
        );
    }

    #[test]
    fn test_pkce_verifier_contains_valid_characters() {
        // Verify that generated verifier only contains unreserved characters
        let challenge = PkceChallenge::generate().unwrap();

        let allowed_chars = std::collections::HashSet::<char>::from_iter(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~".chars(),
        );

        for c in challenge.verifier.chars() {
            assert!(
                allowed_chars.contains(&c),
                "PKCE verifier contains invalid character: {}",
                c
            );
        }
    }

    #[test]
    fn test_pkce_validation() {
        // Test that validation works correctly
        let challenge = PkceChallenge::generate().unwrap();
        assert!(
            challenge.validate(&challenge.verifier),
            "Challenge should validate against its own verifier"
        );

        let wrong_verifier = "wrong_verifier";
        assert!(
            !challenge.validate(wrong_verifier),
            "Challenge should reject invalid verifier"
        );
    }

    #[test]
    fn test_pkce_generation_is_unique() {
        // Test that multiple PKCE generations produce different verifiers
        let challenge1 = PkceChallenge::generate().unwrap();
        let challenge2 = PkceChallenge::generate().unwrap();

        assert_ne!(
            challenge1.verifier, challenge2.verifier,
            "Generated verifiers should be unique"
        );
        assert_ne!(
            challenge1.challenge, challenge2.challenge,
            "Generated challenges should be unique"
        );
    }

    #[test]
    fn test_pkce_challenge_is_base64_url_safe() {
        // Verify that challenge is URL-safe base64 encoded
        let challenge = PkceChallenge::generate().unwrap();

        // URL-safe base64 should not contain + or / (only -, _, and =)
        assert!(
            !challenge.challenge.contains('+'),
            "Challenge should not contain + (not URL-safe)"
        );
        assert!(
            !challenge.challenge.contains('/'),
            "Challenge should not contain / (not URL-safe)"
        );

        // But should only contain valid base64 characters
        for c in challenge.challenge.chars() {
            assert!(
                c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '=',
                "Challenge contains unexpected character: {}",
                c
            );
        }
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
