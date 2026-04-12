//! OIDC token validation: signature verification and claim extraction.
//!
//! JWKS fetching, caching, and key-selection logic lives in [`super::jwks`].

/// Maximum byte size for an OIDC discovery document.
///
/// A well-formed `.well-known/openid-configuration` response is a few `KiB`.
/// 64 `KiB` is generous while blocking allocation-bomb responses from
/// a malicious or misconfigured OIDC provider.
pub(super) const MAX_DISCOVERY_RESPONSE_BYTES: usize = 64 * 1024; // 64 KiB

use std::sync::Arc;

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, Validation, decode, decode_header};
use parking_lot::RwLock;

use crate::security::{
    auth_middleware::AuthenticatedUser,
    errors::{Result, SecurityError},
    oidc::{
        audience::JwtClaims,
        jwks::CachedJwks,
        providers::{MAX_CLOCK_SKEW_SECS, OidcConfig},
        replay_cache::ReplayCache,
    },
};

// ============================================================================
// OidcValidator struct and constructor
// ============================================================================

/// OIDC token validator with JWKS caching.
///
/// Validates JWT tokens against an OIDC provider's public keys.
/// Automatically fetches and caches the JWKS for efficiency.
///
/// JWKS fetch/cache/key-selection helpers are in `impl OidcValidator` blocks
/// defined in the `jwks` sub-module.
pub struct OidcValidator {
    pub(super) config:       OidcConfig,
    pub(super) http_client:  reqwest::Client,
    pub(super) jwks_cache:   Arc<RwLock<Option<CachedJwks>>>,
    pub(super) jwks_uri:     String,
    /// Optional JWT replay cache. When set, each validated token's `jti` is
    /// checked against the cache and rejected if it has been seen before.
    pub(super) replay_cache: Option<Arc<ReplayCache>>,
}

impl OidcValidator {
    /// Create a new OIDC validator.
    ///
    /// This will perform OIDC discovery to find the JWKS URI
    /// unless `jwks_uri` is explicitly set in config.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Config validation fails
    /// - OIDC discovery fails
    /// - JWKS endpoint cannot be determined
    pub async fn new(config: OidcConfig) -> Result<Self> {
        use std::time::Duration;

        use crate::security::oidc::jwks::OidcDiscoveryDocument;

        config.validate()?;

        // Redirects are disabled to prevent redirect-chain SSRF attacks.
        // `https_only` is not set here because `OidcConfig::validate()` already
        // requires HTTPS for the issuer URL (localhost HTTP is allowed for dev/test).
        let http_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| SecurityError::SecurityConfigError(format!("HTTP client error: {e}")))?;

        // Determine JWKS URI
        let jwks_uri = if let Some(ref uri) = config.jwks_uri {
            uri.clone()
        } else {
            // Perform OIDC discovery
            let discovery_url =
                format!("{}/.well-known/openid-configuration", config.issuer.trim_end_matches('/'));

            tracing::debug!(url = %discovery_url, "Performing OIDC discovery");

            let response = http_client.get(&discovery_url).send().await.map_err(|e| {
                SecurityError::SecurityConfigError(format!("OIDC discovery failed: {e}"))
            })?;

            if !response.status().is_success() {
                return Err(SecurityError::SecurityConfigError(format!(
                    "OIDC discovery failed with status: {}",
                    response.status()
                )));
            }

            let body_bytes = response.bytes().await.map_err(|e| {
                SecurityError::SecurityConfigError(format!(
                    "Failed to read OIDC discovery response: {e}"
                ))
            })?;
            if body_bytes.len() > MAX_DISCOVERY_RESPONSE_BYTES {
                return Err(SecurityError::SecurityConfigError(format!(
                    "OIDC discovery response too large ({} bytes, max {MAX_DISCOVERY_RESPONSE_BYTES})",
                    body_bytes.len()
                )));
            }
            let discovery: OidcDiscoveryDocument =
                serde_json::from_slice(&body_bytes).map_err(|e| {
                    SecurityError::SecurityConfigError(format!(
                        "Invalid OIDC discovery response: {e}"
                    ))
                })?;

            tracing::info!(
                issuer = %discovery.issuer,
                jwks_uri = %discovery.jwks_uri,
                "OIDC discovery successful"
            );

            discovery.jwks_uri
        };

        // Validate the JWKS URI before storing it (SSRF prevention pattern).
        let _ = reqwest::Url::parse(&jwks_uri).map_err(|e| {
            SecurityError::SecurityConfigError(format!(
                "OIDC jwks_uri is not a valid URL '{jwks_uri}': {e}"
            ))
        })?;

        Ok(Self {
            config,
            http_client,
            jwks_cache: Arc::new(RwLock::new(None)),
            jwks_uri,
            replay_cache: None,
        })
    }

    /// Create a validator without performing discovery.
    ///
    /// Use this for testing or when you have the JWKS URI directly.
    ///
    /// # Panics
    ///
    /// Panics if the platform TLS backend is unavailable for the HTTP client.
    /// This would indicate a broken system-level TLS installation.
    #[must_use]
    pub fn with_jwks_uri(config: OidcConfig, jwks_uri: String) -> Self {
        // Use the same 30-second timeout as `new()` to prevent indefinitely
        // blocked JWKS fetches when the endpoint is slow or hung.
        // Redirects are disabled to prevent redirect-chain SSRF attacks.
        // `https_only` is not set here because `OidcConfig::validate()` already
        // requires HTTPS for the issuer URL (localhost HTTP is allowed for dev/test).
        let http_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()
            // Reason: TLS backend absence is a catastrophic system misconfiguration;
            // this constructor is #[must_use] and returns Self, not Result.
            .expect("TLS backend should always be available for reqwest HTTP client");
        Self {
            config,
            http_client,
            jwks_cache: Arc::new(RwLock::new(None)),
            jwks_uri,
            replay_cache: None,
        }
    }

    /// Attach a JWT replay cache to this validator.
    ///
    /// When set, every validated token's `jti` claim is checked against the
    /// cache. A token whose `jti` has been seen before is rejected with
    /// `SecurityError::TokenReplayed`, preventing stolen-token replay attacks.
    ///
    /// If `require_jti` is `true` in [`OidcConfig`], tokens without a `jti`
    /// are also rejected before the replay check is reached.
    #[must_use]
    pub fn with_replay_cache(mut self, cache: Arc<ReplayCache>) -> Self {
        self.replay_cache = Some(cache);
        self
    }

    /// Validate a JWT token and extract user information.
    ///
    /// # Arguments
    ///
    /// * `token` - The JWT token string (without "Bearer " prefix)
    ///
    /// # Returns
    ///
    /// `AuthenticatedUser` if token is valid, error otherwise.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Token is malformed
    /// - Signature verification fails
    /// - Required claims are missing
    /// - Token is expired
    /// - Issuer/audience don't match
    pub async fn validate_token(&self, token: &str) -> Result<AuthenticatedUser> {
        // Decode header to get kid
        let header = decode_header(token).map_err(|e| {
            tracing::debug!(error = %e, "Failed to decode JWT header");
            SecurityError::InvalidToken
        })?;

        let kid = header.kid.as_ref().ok_or_else(|| {
            tracing::debug!("JWT missing kid (key ID) in header");
            SecurityError::InvalidToken
        })?;

        // Get the signing key (fetch/cache logic in jwks.rs)
        let decoding_key = self.get_decoding_key(kid).await?;

        // Build validation
        let mut validation = Validation::new(self.get_algorithm(&header)?);
        validation.set_issuer(&[&self.config.issuer]);

        // Set audience validation — always enabled when any audience is configured.
        // The validate() call earlier guarantees at least one audience is set,
        // so this else branch (validate_aud = false) is never reached in practice.
        // It is kept as a defensive fallback; the real protection is the mandatory
        // audience check in OidcConfig::validate().
        if let Some(ref aud) = self.config.audience {
            let mut audiences = vec![aud.clone()];
            audiences.extend(self.config.additional_audiences.clone());
            validation.set_audience(&audiences);
        } else if !self.config.additional_audiences.is_empty() {
            // Only additional_audiences configured (no primary audience):
            // still validate against those.
            validation.set_audience(&self.config.additional_audiences);
        } else {
            // Should be unreachable after OidcConfig::validate() — fail-closed.
            validation.validate_aud = true;
        }

        // Set clock skew tolerance — capped to prevent accepting arbitrarily
        // old expired tokens due to misconfiguration.
        validation.leeway = self.config.clock_skew_secs.min(MAX_CLOCK_SKEW_SECS);

        // Decode and validate token
        let token_data = decode::<JwtClaims>(token, &decoding_key, &validation).map_err(|e| {
            tracing::debug!(error = %e, "JWT validation failed");
            match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => SecurityError::TokenExpired {
                    expired_at: Utc::now(), // Approximate
                },
                // Invalid issuer, audience, signature, and all other errors map to InvalidToken
                _ => SecurityError::InvalidToken,
            }
        })?;

        let claims = token_data.claims;

        // Validate jti claim if required by configuration.
        if self.config.require_jti && claims.jti.is_none() {
            tracing::debug!("JWT missing required jti (JWT ID) claim");
            return Err(SecurityError::TokenMissingClaim {
                claim: "jti".to_string(),
            });
        }

        // Replay check: if a replay cache is configured and the token carries a jti,
        // verify the jti has not been seen before and record it for future checks.
        if let (Some(replay_cache), Some(ref jti)) = (&self.replay_cache, &claims.jti) {
            use std::time::Duration;
            // Compute the token's remaining TTL for the cache entry.
            let ttl = claims
                .exp
                .and_then(|exp| {
                    let remaining = exp - chrono::Utc::now().timestamp();
                    if remaining > 0 {
                        Some(Duration::from_secs(remaining.cast_unsigned()))
                    } else {
                        None
                    }
                })
                .unwrap_or(Duration::from_secs(900)); // Fallback: 15-minute TTL

            replay_cache.check_and_record(jti, ttl).await.map_err(|e| {
                use crate::security::oidc::replay_cache::ReplayCacheError;
                match e {
                    ReplayCacheError::Replayed => {
                        tracing::warn!(jti = %jti, "JWT replay detected");
                        SecurityError::TokenReplayed
                    },
                    ReplayCacheError::Backend(_) => {
                        // Backend error with fail-open is already handled inside
                        // ReplayCache::check_and_record(); reaching here means
                        // fail-closed is configured.
                        tracing::warn!(jti = %jti, error = %e, "Replay cache backend error");
                        SecurityError::InvalidToken
                    },
                }
            })?;
        }

        // Extract scopes first (before moving claims.sub)
        let scopes = self.extract_scopes(&claims);

        // Extract user ID (required)
        let user_id = claims.sub.ok_or(SecurityError::TokenMissingClaim {
            claim: "sub".to_string(),
        })?;

        // Extract expiration (required)
        let exp = claims.exp.ok_or(SecurityError::TokenMissingClaim {
            claim: "exp".to_string(),
        })?;

        let expires_at =
            DateTime::<Utc>::from_timestamp(exp, 0).ok_or(SecurityError::InvalidToken)?;

        tracing::debug!(
            user_id = %user_id,
            scopes = ?scopes,
            expires_at = %expires_at,
            "Token validated successfully"
        );

        Ok(AuthenticatedUser {
            user_id,
            scopes,
            expires_at,
            extra_claims: claims.extra,
        })
    }

    /// Get the algorithm from the JWT header.
    ///
    /// # Errors
    ///
    /// Returns `SecurityError::InvalidTokenAlgorithm` if the algorithm is not in the allow-list.
    pub(super) fn get_algorithm(&self, header: &jsonwebtoken::Header) -> Result<Algorithm> {
        let alg_str = format!("{:?}", header.alg);

        // Check if algorithm is allowed
        if !self.config.allowed_algorithms.contains(&alg_str) {
            return Err(SecurityError::InvalidTokenAlgorithm { algorithm: alg_str });
        }

        Ok(header.alg)
    }

    /// Extract scopes from JWT claims.
    ///
    /// Handles multiple formats:
    /// - `scope`: space-separated string (Auth0, Okta)
    /// - `scp`: array of strings (some providers)
    /// - `permissions`: array of strings (Auth0 RBAC)
    fn extract_scopes(&self, claims: &JwtClaims) -> Vec<String> {
        // Try the configured scope claim first (default: "scope")
        if self.config.scope_claim == "scope" {
            if let Some(ref scope) = claims.scope {
                return scope.split_whitespace().map(String::from).collect();
            }
        }

        // Try scp (array format)
        if let Some(ref scp) = claims.scp {
            return scp.clone();
        }

        // Try permissions (Auth0 RBAC)
        if let Some(ref perms) = claims.permissions {
            return perms.clone();
        }

        // Try scope as space-separated string
        if let Some(ref scope) = claims.scope {
            return scope.split_whitespace().map(String::from).collect();
        }

        Vec::new()
    }

    /// Check if authentication is required.
    #[must_use]
    pub const fn is_required(&self) -> bool {
        self.config.required
    }

    /// Get the configured issuer.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.config.issuer
    }

    /// Clear the JWKS cache.
    ///
    /// Call this if you need to force a refresh of the signing keys.
    pub fn clear_cache(&self) {
        let mut cache = self.jwks_cache.write();
        *cache = None;
    }
}
