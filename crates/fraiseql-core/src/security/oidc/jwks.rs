//! OIDC discovery, and the validator's view of the shared JWKS client.
//!
//! The key set itself is **not** cached here. It is fetched, bounded and selected
//! by [`fraiseql_jwks`], the one JWKS client in the workspace (#1335). This module
//! is what the `[auth]` path needed on top of it: the discovery document that
//! locates a `jwks_uri` in the first place, and the small translation from that
//! client's answers into [`SecurityError`].
//!
//! # What used to live here, and why it does not
//!
//! A `CachedJwks` with a TTL, a `fetch_jwks`, a `find_key`, a rotation detector
//! and a `Jwk` → `DecodingKey` conversion that accepted **RSA keys only**. Every
//! one of those had a near-twin in `fraiseql_auth::JwksCache`, and the two had
//! drifted: the twin accepted RSA *and* EC and pinned DNS against rebinding,
//! which this one did not. Neither bounded a refetch, so an unknown `kid` fetched
//! every time — see [`fraiseql_jwks`] for what that amplifies into.

/// Maximum byte length accepted from a JWKS endpoint response.
///
/// Re-exported from [`fraiseql_jwks`] so this crate cannot drift from the value
/// the fetch actually enforces — which is how the two caches came to hold two
/// copies of every other rule.
pub use fraiseql_jwks::MAX_RESPONSE_BYTES as MAX_JWKS_RESPONSE_BYTES;
use serde::Deserialize;

use crate::security::{
    errors::{Result, SecurityError},
    oidc::token::OidcValidator,
};

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

impl OidcValidator {
    /// The decoding key for a specific key ID.
    ///
    /// Delegates to the shared client, which fetches the publisher's set at most
    /// once per [`fraiseql_jwks::REFETCH_COOLDOWN`] however many unknown `kid`s
    /// arrive, single-flights concurrent misses, and never serves an expired set.
    ///
    /// # This must not be the first thing `validate_token` does
    ///
    /// The algorithm allow-list is checked **before** this call
    /// (`token.rs`). It used to be checked after, so a token whose `alg` the
    /// server would refuse on its header alone still cost an outbound request
    /// (#1335).
    ///
    /// # Errors
    ///
    /// `SecurityError::InvalidToken` when the publisher does not publish that
    /// `kid`, or publishes it as a key type this workspace does not verify with —
    /// both are the sender's token being unverifiable, which is one answer.
    /// `SecurityError::SecurityConfigError` when the key set could not be fetched
    /// at all: that is the operator's or the publisher's problem and must not be
    /// confused with the first.
    pub(super) async fn get_decoding_key(&self, kid: &str) -> Result<jsonwebtoken::DecodingKey> {
        let found = self.jwks.key(kid).await.map_err(|error| {
            tracing::error!(error = %error, kid = %kid, "the OIDC key set could not be consulted");
            SecurityError::SecurityConfigError(error.to_string())
        })?;
        let jwk = found.ok_or_else(|| {
            tracing::debug!(kid = %kid, "the provider does not publish this key id");
            SecurityError::InvalidToken
        })?;
        jwk.decoding_key().map_err(|error| {
            // A key type the workspace does not verify with is named in the log
            // rather than reported as a generic failure, because the operator's
            // action differs: an EdDSA-signing IdP needs support added, a
            // malformed key needs the publisher told.
            tracing::debug!(error = %error, kid = %kid, "the published key is not usable");
            SecurityError::InvalidTokenAlgorithm {
                algorithm: jwk.kty.clone(),
            }
        })
    }

    /// Invalidate the cached JWKS so the next token validation refetches keys.
    ///
    /// Use this when an operator learns of an `IdP`-side key compromise or
    /// rotation and wants to close the stolen-key replay window immediately,
    /// rather than waiting up to `jwks_cache_ttl_secs` for the cached entry to
    /// expire. The next token validation performs a fresh fetch.
    pub fn invalidate_jwks_cache(&self) {
        self.jwks.invalidate();
    }

    /// Force an immediate JWKS refetch, replacing the cache with the provider's
    /// current key set.
    ///
    /// Returns the number of keys fetched. Backs the operator-facing
    /// `/admin/v1/auth/refresh-jwks` endpoint, which closes the stolen-key replay
    /// window on demand and confirms the refresh succeeded.
    ///
    /// Not subject to the refetch cooldown: that bound exists to stop an
    /// unauthenticated sender from driving outbound requests, and this is neither
    /// unauthenticated nor sender-triggered.
    ///
    /// # Errors
    ///
    /// Returns `SecurityError::SecurityConfigError` if the JWKS endpoint cannot be
    /// reached or the response cannot be parsed.
    pub async fn refresh_jwks(&self) -> Result<usize> {
        let count = self
            .jwks
            .refresh()
            .await
            .map_err(|error| SecurityError::SecurityConfigError(error.to_string()))?;
        tracing::info!(key_count = count, "JWKS force-refreshed from provider");
        Ok(count)
    }
}
