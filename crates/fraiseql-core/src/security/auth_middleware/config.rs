//! Authentication configuration.

use super::signing_key::SigningKey;

// ============================================================================
// Authentication Configuration
// ============================================================================

/// Authentication configuration
///
/// Defines what authentication requirements must be met for a request. Every
/// `AuthConfig` carries a signing key: there is no way to build one that skips signature
/// verification.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// If true, authentication is required for all requests
    pub required: bool,

    /// Token lifetime in seconds (for validation purposes)
    pub token_expiry_secs: u64,

    /// Signing key every token's signature is verified against.
    pub signing_key: SigningKey,

    /// Expected issuer (iss claim).
    ///
    /// If set, tokens must have this value in their `iss` claim.
    pub issuer: Option<String>,

    /// Expected audience (aud claim).
    ///
    /// If set, tokens must have this value in their `aud` claim.
    pub audience: Option<String>,

    /// Clock skew tolerance in seconds.
    ///
    /// Allow this many seconds of clock difference when validating exp/nbf claims.
    /// Default: 60 seconds
    pub clock_skew_secs: u64,
}

pub(super) const fn default_clock_skew() -> u64 {
    60
}

impl AuthConfig {
    /// Create a configuration with HS256 signing key.
    ///
    /// This is the recommended configuration for production when using
    /// symmetric key signing (internal services).
    #[must_use]
    pub fn with_hs256(secret: &str) -> Self {
        Self {
            required:          true,
            token_expiry_secs: 3600,
            signing_key:       SigningKey::hs256(secret),
            issuer:            None,
            audience:          None,
            clock_skew_secs:   default_clock_skew(),
        }
    }

    /// Create a configuration with RS256 signing key from PEM.
    ///
    /// This is the recommended configuration for production when using
    /// asymmetric key signing (external identity providers).
    #[must_use]
    pub fn with_rs256_pem(pem: &str) -> Self {
        Self {
            required:          true,
            token_expiry_secs: 3600,
            signing_key:       SigningKey::rs256_pem(pem),
            issuer:            None,
            audience:          None,
            clock_skew_secs:   default_clock_skew(),
        }
    }

    /// Set the expected issuer.
    #[must_use]
    pub fn with_issuer(mut self, issuer: &str) -> Self {
        self.issuer = Some(issuer.to_string());
        self
    }

    /// Set the expected audience.
    #[must_use]
    pub fn with_audience(mut self, audience: &str) -> Self {
        self.audience = Some(audience.to_string());
        self
    }
}
