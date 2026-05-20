//! JWKS (JSON Web Key Set) types, cache, key selection, and fetch logic.
//!
//! The `impl OidcValidator` block here adds JWKS-specific methods to the
//! validator type defined in [`super::token`].

use std::time::{Duration, Instant};

use jsonwebtoken::DecodingKey;
use serde::Deserialize;

use crate::security::{
    errors::{Result, SecurityError},
    oidc::token::OidcValidator,
};

/// Maximum byte length accepted from a JWKS endpoint response.
///
/// A legitimate JWKS document (a few RSA/EC public keys) is well under 64 `KiB`.
/// A 1 `MiB` cap prevents a malicious or compromised OIDC provider from sending
/// a response large enough to exhaust server memory.
pub const MAX_JWKS_RESPONSE_BYTES: usize = 1024 * 1024; // 1 MiB

// ============================================================================
// OIDC Discovery Response
// ============================================================================

/// OIDC Discovery document (partial).
///
/// Contains the fields we need from `/.well-known/openid-configuration`.
#[derive(Debug, Clone, Deserialize)]
pub struct OidcDiscoveryDocument {
    /// Issuer identifier
    pub issuer: String,

    /// JWKS URI for fetching public keys
    pub jwks_uri: String,

    /// Supported signing algorithms
    #[serde(default)]
    pub id_token_signing_alg_values_supported: Vec<String>,

    /// Authorization endpoint (for reference)
    #[serde(default)]
    pub authorization_endpoint: Option<String>,

    /// Token endpoint (for reference)
    #[serde(default)]
    pub token_endpoint: Option<String>,
}

// ============================================================================
// JWKS Types
// ============================================================================

/// JSON Web Key Set.
#[derive(Debug, Clone, Deserialize)]
pub struct Jwks {
    /// Array of JSON Web Keys
    pub keys: Vec<Jwk>,
}

/// JSON Web Key.
#[derive(Debug, Clone, Deserialize)]
pub struct Jwk {
    /// Key type (e.g., "RSA")
    pub kty: String,

    /// Key ID (used to match with JWT header)
    pub kid: Option<String>,

    /// Algorithm (e.g., "RS256")
    #[serde(default)]
    pub alg: Option<String>,

    /// Intended use (e.g., "sig" for signature)
    #[serde(rename = "use")]
    pub key_use: Option<String>,

    /// RSA modulus (base64url encoded)
    pub n: Option<String>,

    /// RSA exponent (base64url encoded)
    pub e: Option<String>,

    /// X.509 certificate chain
    #[serde(default)]
    pub x5c: Vec<String>,
}

/// Cached JWKS with expiration.
#[derive(Debug)]
pub struct CachedJwks {
    pub(super) jwks: Jwks,
    pub(super) fetched_at: Instant,
    pub(super) ttl: Duration,
}

impl CachedJwks {
    pub(super) fn is_expired(&self) -> bool {
        self.fetched_at.elapsed() > self.ttl
    }
}

// ============================================================================
// OidcValidator — JWKS fetch, cache and key-resolution methods
// ============================================================================

impl OidcValidator {
    /// Get the decoding key for a specific key ID.
    ///
    /// Checks the cache first; fetches fresh JWKS on miss or expiry.
    ///
    /// # Errors
    ///
    /// Returns `SecurityError::InvalidToken` if the key is not found or cannot be decoded.
    pub(super) async fn get_decoding_key(&self, kid: &str) -> Result<DecodingKey> {
        // Check cache first
        {
            let cache = self.jwks_cache.read();
            if let Some(ref cached) = *cache {
                if !cached.is_expired() {
                    if let Some(key) = self.find_key(&cached.jwks, kid) {
                        return self.jwk_to_decoding_key(key);
                    }
                }
            }
        }

        // Fetch fresh JWKS
        let jwks = self.fetch_jwks().await?;

        // SECURITY: Detect key rotation for audit purposes
        if self.detect_key_rotation(&jwks) {
            tracing::warn!(
                "OIDC key rotation detected: some previously cached keys no longer available"
            );
        }

        // Find the key index first, then we can clone the key
        let key_index =
            jwks.keys.iter().position(|k| k.kid.as_deref() == Some(kid)).ok_or_else(|| {
                tracing::debug!(kid = %kid, "Key not found in JWKS");
                SecurityError::InvalidToken
            })?;

        // Clone the key before caching (keys are small, cloning is fine)
        let key = jwks.keys[key_index].clone();

        // Cache the JWKS
        {
            let mut cache = self.jwks_cache.write();
            *cache = Some(CachedJwks {
                jwks,
                fetched_at: Instant::now(),
                ttl: Duration::from_secs(self.config.jwks_cache_ttl_secs),
            });
        }

        self.jwk_to_decoding_key(&key)
    }

    /// Fetch JWKS from the provider.
    ///
    /// # Errors
    ///
    /// Returns `SecurityError::SecurityConfigError` if the HTTP request fails or
    /// the response cannot be parsed as a valid JWKS.
    async fn fetch_jwks(&self) -> Result<Jwks> {
        tracing::debug!(uri = %self.jwks_uri, "Fetching JWKS");

        let response = self.http_client.get(&self.jwks_uri).send().await.map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch JWKS");
            SecurityError::SecurityConfigError(format!("Failed to fetch JWKS: {e}"))
        })?;

        if !response.status().is_success() {
            return Err(SecurityError::SecurityConfigError(format!(
                "JWKS fetch failed with status: {}",
                response.status()
            )));
        }

        // Cap the response body before deserialising to prevent memory exhaustion
        // from a malicious or compromised OIDC provider sending an oversized payload.
        let body_bytes = response.bytes().await.map_err(|e| {
            SecurityError::SecurityConfigError(format!("Failed to read JWKS response body: {e}"))
        })?;

        if body_bytes.len() > MAX_JWKS_RESPONSE_BYTES {
            return Err(SecurityError::SecurityConfigError(format!(
                "JWKS response body too large ({} bytes, max {MAX_JWKS_RESPONSE_BYTES})",
                body_bytes.len()
            )));
        }

        let jwks: Jwks = serde_json::from_slice(&body_bytes).map_err(|e| {
            SecurityError::SecurityConfigError(format!("Invalid JWKS response: {e}"))
        })?;

        tracing::debug!(key_count = jwks.keys.len(), "JWKS fetched successfully");

        Ok(jwks)
    }

    /// Find a key in the JWKS by key ID.
    pub(super) fn find_key<'a>(&self, jwks: &'a Jwks, kid: &str) -> Option<&'a Jwk> {
        jwks.keys.iter().find(|k| k.kid.as_deref() == Some(kid))
    }

    /// Detect if JWKS keys have been rotated (old keys removed).
    ///
    /// Compares current cached keys with newly fetched keys.
    /// Returns true if any previously cached keys are missing from the new JWKS.
    pub(super) fn detect_key_rotation(&self, new_jwks: &Jwks) -> bool {
        let cache = self.jwks_cache.read();
        if let Some(ref cached) = *cache {
            // Get set of old key IDs
            let old_kids: std::collections::HashSet<_> =
                cached.jwks.keys.iter().filter_map(|k| k.kid.as_deref()).collect();

            // Get set of new key IDs
            let new_kids: std::collections::HashSet<_> =
                new_jwks.keys.iter().filter_map(|k| k.kid.as_deref()).collect();

            // Rotation detected if any old keys are missing
            !old_kids.is_subset(&new_kids)
        } else {
            false
        }
    }

    /// Convert a JWK to a jsonwebtoken `DecodingKey`.
    ///
    /// # Errors
    ///
    /// Returns `SecurityError::InvalidToken` if the key type is unsupported or
    /// required RSA components (n, e) are missing.
    pub(super) fn jwk_to_decoding_key(&self, jwk: &Jwk) -> Result<DecodingKey> {
        match jwk.kty.as_str() {
            "RSA" => {
                let n = jwk.n.as_ref().ok_or(SecurityError::InvalidToken)?;
                let e = jwk.e.as_ref().ok_or(SecurityError::InvalidToken)?;

                DecodingKey::from_rsa_components(n, e).map_err(|e| {
                    tracing::debug!(error = %e, "Failed to create RSA decoding key");
                    SecurityError::InvalidToken
                })
            },
            other => {
                tracing::debug!(key_type = %other, "Unsupported key type");
                Err(SecurityError::InvalidTokenAlgorithm {
                    algorithm: other.to_string(),
                })
            },
        }
    }
}
