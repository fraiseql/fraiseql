//! Signing key configuration for JWT verification.

use jsonwebtoken::{Algorithm, DecodingKey};
use zeroize::Zeroizing;

use crate::security::errors::SecurityError;

// ============================================================================
// Signing Key Configuration
// ============================================================================

/// Signing key for JWT signature verification.
///
/// Supports both symmetric (HS256) and asymmetric (RS256/RS384/RS512) algorithms.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SigningKey {
    /// HMAC-SHA256 symmetric key.
    ///
    /// Use for internal services where the same secret is shared
    /// between token issuer and validator.
    Hs256(Zeroizing<Vec<u8>>),

    /// HMAC-SHA384 symmetric key.
    Hs384(Zeroizing<Vec<u8>>),

    /// HMAC-SHA512 symmetric key.
    Hs512(Zeroizing<Vec<u8>>),

    /// RSA public key in PEM format (RS256 algorithm).
    ///
    /// Use for external identity providers. The public key is used
    /// to verify tokens signed with the provider's private key.
    Rs256Pem(String),

    /// RSA public key in PEM format (RS384 algorithm).
    Rs384Pem(String),

    /// RSA public key in PEM format (RS512 algorithm).
    Rs512Pem(String),

    /// RSA public key components (n, e) for RS256.
    ///
    /// Use when receiving keys from JWKS endpoints.
    Rs256Components {
        /// RSA modulus (n) in base64url encoding
        n: String,
        /// RSA exponent (e) in base64url encoding
        e: String,
    },
}

impl SigningKey {
    /// Create an HS256 signing key from a secret string.
    #[must_use]
    pub fn hs256(secret: &str) -> Self {
        Self::Hs256(Zeroizing::new(secret.as_bytes().to_vec()))
    }

    /// Create an HS256 signing key from raw bytes.
    #[must_use]
    pub fn hs256_bytes(secret: &[u8]) -> Self {
        Self::Hs256(Zeroizing::new(secret.to_vec()))
    }

    /// Create an RS256 signing key from PEM-encoded public key.
    #[must_use]
    pub fn rs256_pem(pem: &str) -> Self {
        Self::Rs256Pem(pem.to_string())
    }

    /// Create an RS256 signing key from RSA components.
    ///
    /// This is useful when parsing JWKS responses.
    #[must_use]
    pub fn rs256_components(n: &str, e: &str) -> Self {
        Self::Rs256Components {
            n: n.to_string(),
            e: e.to_string(),
        }
    }

    /// Get the algorithm for this signing key.
    #[must_use]
    pub const fn algorithm(&self) -> Algorithm {
        match self {
            Self::Hs256(_) => Algorithm::HS256,
            Self::Hs384(_) => Algorithm::HS384,
            Self::Hs512(_) => Algorithm::HS512,
            Self::Rs256Pem(_) | Self::Rs256Components { .. } => Algorithm::RS256,
            Self::Rs384Pem(_) => Algorithm::RS384,
            Self::Rs512Pem(_) => Algorithm::RS512,
        }
    }

    /// Convert to a jsonwebtoken `DecodingKey`.
    pub(super) fn to_decoding_key(&self) -> std::result::Result<DecodingKey, SecurityError> {
        match self {
            Self::Hs256(secret) | Self::Hs384(secret) | Self::Hs512(secret) => {
                Ok(DecodingKey::from_secret(secret))
            },
            Self::Rs256Pem(pem) | Self::Rs384Pem(pem) | Self::Rs512Pem(pem) => {
                DecodingKey::from_rsa_pem(pem.as_bytes()).map_err(|e| {
                    SecurityError::SecurityConfigError(format!("Invalid RSA PEM key: {e}"))
                })
            },
            Self::Rs256Components { n, e } => DecodingKey::from_rsa_components(n, e).map_err(|e| {
                SecurityError::SecurityConfigError(format!("Invalid RSA components: {e}"))
            }),
        }
    }
}
