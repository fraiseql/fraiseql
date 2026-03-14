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
