//! PKCE, state, and nonce parameters for OAuth2 security.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq as _;

/// Generate a cryptographically random 32-byte token encoded as URL-safe
/// base64 (no padding), yielding 43 characters and ~256 bits of entropy.
///
/// The output alphabet is `[A-Za-z0-9-_]`, which is a subset of the RFC 7636
/// unreserved character set `[A-Za-z0-9\-._~]` and is safe for use as a
/// `code_verifier`, CSRF state parameter, or nonce.
pub(super) fn gen_random_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// PKCE code challenge for public clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PKCEChallenge {
    /// Random code verifier (43-128 characters, RFC 7636 §4.1)
    pub code_verifier:         String,
    /// BASE64URL(SHA256(code_verifier))
    pub code_challenge:        String,
    /// Challenge method: "S256" (SHA256)
    pub code_challenge_method: String,
}

impl PKCEChallenge {
    /// Generate a new PKCE challenge.
    ///
    /// The `code_verifier` is generated using `OsRng` with 32 random bytes
    /// encoded as URL-safe base64 (no padding), yielding 43 characters and
    /// ~256 bits of entropy — compliant with RFC 7636 §4.1.
    pub fn new() -> Self {
        use sha2::{Digest, Sha256};

        // Generate RFC 7636-compliant code_verifier: 32 OsRng bytes → 43-char base64url (no pad)
        let verifier = gen_random_token();

        // code_challenge = BASE64URL(SHA256(ASCII(code_verifier))) — RFC 7636 §4.2
        let digest = Sha256::digest(verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(digest);

        Self {
            code_verifier:         verifier,
            code_challenge:        challenge,
            code_challenge_method: "S256".to_string(),
        }
    }

    /// Verify a code verifier matches this challenge.
    ///
    /// Computes `BASE64URL(SHA256(verifier))` and compares it to the stored
    /// `code_challenge` using constant-time equality to prevent timing attacks.
    pub fn verify(&self, verifier: &str) -> bool {
        use sha2::{Digest, Sha256};

        // code_challenge = BASE64URL(SHA256(ASCII(code_verifier))) — RFC 7636 §4.2
        let computed_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));

        // SECURITY: Use constant-time comparison to prevent timing attacks.
        computed_challenge.as_bytes().ct_eq(self.code_challenge.as_bytes()).into()
    }
}

impl Default for PKCEChallenge {
    fn default() -> Self {
        Self::new()
    }
}

/// OAuth state parameter for CSRF protection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateParameter {
    /// Random state value
    pub state:      String,
    /// When state expires
    pub expires_at: DateTime<Utc>,
}

impl StateParameter {
    /// Generate a new CSRF state parameter.
    ///
    /// Uses `OsRng` with 32 random bytes encoded as URL-safe base64 (no
    /// padding), yielding 43 characters and ~256 bits of entropy.
    pub fn new() -> Self {
        Self {
            state:      gen_random_token(),
            expires_at: Utc::now() + Duration::minutes(10),
        }
    }

    /// Check if state is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }

    /// Verify state matches and is not expired
    pub fn verify(&self, provided_state: &str) -> bool {
        // SECURITY: Use constant-time comparison before checking expiry to prevent
        // timing oracles that could reveal information about the stored state value.
        let match_ok: bool = self.state.as_bytes().ct_eq(provided_state.as_bytes()).into();
        match_ok && !self.is_expired()
    }
}

impl Default for StateParameter {
    fn default() -> Self {
        Self::new()
    }
}

/// Nonce parameter for replay protection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonceParameter {
    /// Random nonce value
    pub nonce:      String,
    /// When nonce expires
    pub expires_at: DateTime<Utc>,
}

impl NonceParameter {
    /// Generate a new nonce parameter.
    ///
    /// Uses `OsRng` with 32 random bytes encoded as URL-safe base64 (no
    /// padding), yielding 43 characters and ~256 bits of entropy.
    pub fn new() -> Self {
        Self {
            nonce:      gen_random_token(),
            expires_at: Utc::now() + Duration::minutes(10),
        }
    }

    /// Check if nonce is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }

    /// Verify nonce matches and is not expired
    pub fn verify(&self, provided_nonce: &str) -> bool {
        // SECURITY: Use constant-time comparison before checking expiry to prevent
        // timing oracles that could reveal information about the stored nonce value.
        let match_ok: bool = self.nonce.as_bytes().ct_eq(provided_nonce.as_bytes()).into();
        match_ok && !self.is_expired()
    }
}

impl Default for NonceParameter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- PKCEChallenge tests ---

    #[test]
    fn test_pkce_challenge_method_is_s256() {
        let challenge = PKCEChallenge::new();
        assert_eq!(challenge.code_challenge_method, "S256", "PKCE challenge method must be S256");
    }

    #[test]
    fn test_pkce_verifier_rfc7636_length_and_charset() {
        let challenge = PKCEChallenge::new();
        // RFC 7636 §4.1: 43–128 chars from [A-Za-z0-9\-._~]
        let len = challenge.code_verifier.len();
        assert!(
            (43..=128).contains(&len),
            "PKCE code_verifier length {len} must be 43–128 chars (RFC 7636 §4.1)"
        );
        assert!(
            challenge
                .code_verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)),
            "PKCE code_verifier must only contain [A-Za-z0-9\\-._~] (RFC 7636 §4.1)"
        );
        assert!(
            !challenge.code_verifier.contains('='),
            "PKCE code_verifier must not contain padding characters"
        );
    }

    #[test]
    fn test_pkce_verifier_has_256_bits_entropy() {
        // 32 OsRng bytes → 43 URL-safe base64 chars → ~256 bits entropy.
        // Two independently generated verifiers must differ with overwhelming probability.
        let c1 = PKCEChallenge::new();
        let c2 = PKCEChallenge::new();
        assert_ne!(
            c1.code_verifier, c2.code_verifier,
            "two PKCE verifiers generated back-to-back must be distinct (entropy check)"
        );
    }

    #[test]
    fn test_pkce_challenge_is_not_empty() {
        let challenge = PKCEChallenge::new();
        assert!(!challenge.code_challenge.is_empty(), "PKCE code_challenge must not be empty");
    }

    #[test]
    fn test_pkce_verify_correct_verifier() {
        let challenge = PKCEChallenge::new();
        let verifier = challenge.code_verifier.clone();
        assert!(
            challenge.verify(&verifier),
            "PKCEChallenge::verify must succeed for the original verifier"
        );
    }

    #[test]
    fn test_pkce_verify_wrong_verifier_fails() {
        let challenge = PKCEChallenge::new();
        assert!(
            !challenge.verify("definitely-wrong-verifier"),
            "PKCEChallenge::verify must fail for an incorrect verifier"
        );
    }

    #[test]
    fn test_pkce_two_challenges_differ() {
        let c1 = PKCEChallenge::new();
        let c2 = PKCEChallenge::new();
        assert_ne!(
            c1.code_verifier, c2.code_verifier,
            "consecutive PKCE challenges must have unique verifiers"
        );
        assert_ne!(
            c1.code_challenge, c2.code_challenge,
            "consecutive PKCE challenges must have unique challenges"
        );
    }

    // --- StateParameter tests ---

    #[test]
    fn test_state_parameter_not_expired_on_creation() {
        let state = StateParameter::new();
        assert!(!state.is_expired(), "freshly created StateParameter must not be expired");
    }

    #[test]
    fn test_state_verify_correct_value() {
        let state = StateParameter::new();
        let value = state.state.clone();
        assert!(
            state.verify(&value),
            "StateParameter::verify must succeed for the correct state value"
        );
    }

    #[test]
    fn test_state_verify_wrong_value_fails() {
        let state = StateParameter::new();
        assert!(
            !state.verify("wrong-state-value"),
            "StateParameter::verify must fail for an incorrect state value"
        );
    }

    #[test]
    fn test_state_parameters_are_unique() {
        let s1 = StateParameter::new();
        let s2 = StateParameter::new();
        assert_ne!(s1.state, s2.state, "consecutive StateParameter values must be unique");
    }

    // --- NonceParameter tests ---

    #[test]
    fn test_nonce_not_expired_on_creation() {
        let nonce = NonceParameter::new();
        assert!(!nonce.is_expired(), "freshly created NonceParameter must not be expired");
    }

    #[test]
    fn test_nonce_verify_correct_value() {
        let nonce = NonceParameter::new();
        let value = nonce.nonce.clone();
        assert!(
            nonce.verify(&value),
            "NonceParameter::verify must succeed for the correct nonce value"
        );
    }

    #[test]
    fn test_nonce_verify_wrong_value_fails() {
        let nonce = NonceParameter::new();
        assert!(
            !nonce.verify("wrong-nonce-value"),
            "NonceParameter::verify must fail for an incorrect nonce value"
        );
    }

    #[test]
    fn test_nonce_parameters_are_unique() {
        let n1 = NonceParameter::new();
        let n2 = NonceParameter::new();
        assert_ne!(n1.nonce, n2.nonce, "consecutive NonceParameter values must be unique");
    }
}
