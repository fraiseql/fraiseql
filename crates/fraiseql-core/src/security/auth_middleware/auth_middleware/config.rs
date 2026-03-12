//! Authentication configuration types.

use serde::{Deserialize, Serialize};

use super::signing_key::SigningKey;

pub(super) fn default_clock_skew() -> u64 {
    60
}

/// Authentication configuration
///
/// Defines what authentication requirements must be met for a request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// If true, authentication is required for all requests
    pub required: bool,

    /// Token lifetime in seconds (for validation purposes)
    pub token_expiry_secs: u64,

    /// Signing key for JWT signature verification.
    ///
    /// If `None`, signature verification is disabled (NOT RECOMMENDED for production).
    /// Use `SigningKey::hs256()` or `SigningKey::rs256_pem()` to enable verification.
    #[serde(skip)]
    pub signing_key: Option<SigningKey>,

    /// Expected issuer (iss claim).
    ///
    /// If set, tokens must have this value in their `iss` claim.
    #[serde(default)]
    pub issuer: Option<String>,

    /// Expected audience (aud claim).
    ///
    /// If set, tokens must have this value in their `aud` claim.
    #[serde(default)]
    pub audience: Option<String>,

    /// Clock skew tolerance in seconds.
    ///
    /// Allow this many seconds of clock difference when validating exp/nbf claims.
    /// Default: 60 seconds
    #[serde(default = "default_clock_skew")]
    pub clock_skew_secs: u64,
}

impl AuthConfig {
    /// Create a permissive authentication configuration (auth optional)
    ///
    /// - Authentication optional
    /// - Token expiry: 3600 seconds (1 hour)
    /// - No signature verification (for testing only)
    #[must_use]
    pub fn permissive() -> Self {
        Self {
            required:          false,
            token_expiry_secs: 3600,
            signing_key:       None,
            issuer:            None,
            audience:          None,
            clock_skew_secs:   default_clock_skew(),
        }
    }

    /// Create a standard authentication configuration (auth required)
    ///
    /// - Authentication required
    /// - Token expiry: 3600 seconds (1 hour)
    /// - No signature verification (configure `signing_key` for production)
    #[must_use]
    pub fn standard() -> Self {
        Self {
            required:          true,
            token_expiry_secs: 3600,
            signing_key:       None,
            issuer:            None,
            audience:          None,
            clock_skew_secs:   default_clock_skew(),
        }
    }

    /// Create a strict authentication configuration (auth required, short expiry)
    ///
    /// - Authentication required
    /// - Token expiry: 1800 seconds (30 minutes)
    /// - No signature verification (configure `signing_key` for production)
    #[must_use]
    pub fn strict() -> Self {
        Self {
            required:          true,
            token_expiry_secs: 1800,
            signing_key:       None,
            issuer:            None,
            audience:          None,
            clock_skew_secs:   default_clock_skew(),
        }
    }

    /// Create a configuration with HS256 signing key.
    ///
    /// This is the recommended configuration for production when using
    /// symmetric key signing (internal services).
    #[must_use]
    pub fn with_hs256(secret: &str) -> Self {
        Self {
            required:          true,
            token_expiry_secs: 3600,
            signing_key:       Some(SigningKey::hs256(secret)),
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
            signing_key:       Some(SigningKey::rs256_pem(pem)),
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

    /// Check if signature verification is enabled.
    #[must_use]
    pub const fn has_signing_key(&self) -> bool {
        self.signing_key.is_some()
    }
}
