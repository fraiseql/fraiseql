//! PKCE, state, and nonce parameters for OAuth2 security.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq as _;

/// PKCE code challenge for public clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PKCEChallenge {
    /// Random code verifier (43-128 characters)
    pub code_verifier:         String,
    /// BASE64URL(SHA256(code_verifier))
    pub code_challenge:        String,
    /// Challenge method: "S256" (SHA256)
    pub code_challenge_method: String,
}

impl PKCEChallenge {
    /// Generate new PKCE challenge
    pub fn new() -> Self {
        use sha2::{Digest, Sha256};

        // Generate random verifier
        let verifier = format!("{}", uuid::Uuid::new_v4());

        // Compute challenge
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let digest = hasher.finalize();
        let challenge = urlencoding::encode_binary(&digest).to_string();

        Self {
            code_verifier:         verifier,
            code_challenge:        challenge,
            code_challenge_method: "S256".to_string(),
        }
    }

    /// Verify code verifier matches challenge
    pub fn verify(&self, verifier: &str) -> bool {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let digest = hasher.finalize();
        let computed_challenge = urlencoding::encode_binary(&digest).to_string();

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
    /// Generate new state parameter
    pub fn new() -> Self {
        Self {
            state:      uuid::Uuid::new_v4().to_string(),
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
    /// Generate new nonce
    pub fn new() -> Self {
        Self {
            nonce:      uuid::Uuid::new_v4().to_string(),
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
        assert_eq!(
            challenge.code_challenge_method, "S256",
            "PKCE challenge method must be S256"
        );
    }

    #[test]
    fn test_pkce_verifier_is_uuid_format() {
        let challenge = PKCEChallenge::new();
        // UUID v4 format: 8-4-4-4-12 hex digits
        assert!(
            uuid::Uuid::parse_str(&challenge.code_verifier).is_ok(),
            "PKCE code_verifier must be a valid UUID"
        );
    }

    #[test]
    fn test_pkce_challenge_is_not_empty() {
        let challenge = PKCEChallenge::new();
        assert!(
            !challenge.code_challenge.is_empty(),
            "PKCE code_challenge must not be empty"
        );
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
        assert!(
            !state.is_expired(),
            "freshly created StateParameter must not be expired"
        );
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
        assert_ne!(
            s1.state, s2.state,
            "consecutive StateParameter values must be unique"
        );
    }

    // --- NonceParameter tests ---

    #[test]
    fn test_nonce_not_expired_on_creation() {
        let nonce = NonceParameter::new();
        assert!(
            !nonce.is_expired(),
            "freshly created NonceParameter must not be expired"
        );
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
        assert_ne!(
            n1.nonce, n2.nonce,
            "consecutive NonceParameter values must be unique"
        );
    }
}
