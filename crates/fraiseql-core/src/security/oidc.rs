//! OIDC Discovery and JWKS Support
//!
//! This module provides OpenID Connect discovery and JSON Web Key Set (JWKS)
//! support for validating JWT tokens from any OIDC-compliant provider.
//!
//! Supported providers include:
//! - Auth0
//! - Keycloak
//! - Okta
//! - AWS Cognito
//! - Microsoft Entra ID (Azure AD)
//! - Google Identity
//! - Any OIDC-compliant provider
//!
//! # Architecture
//!
//! ```text
//! JWT Token from Client
//!     ↓
//! OidcValidator::validate_token()
//!     ├─ Extract kid (key ID) from JWT header
//!     ├─ Fetch/cache JWKS from provider
//!     ├─ Find matching key by kid
//!     ├─ Verify JWT signature
//!     └─ Validate claims (iss, aud, exp)
//!     ↓
//! AuthenticatedUser (if valid)
//! ```
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_core::security::oidc::{OidcConfig, OidcValidator};
//!
//! let config = OidcConfig {
//!     issuer: "https://your-tenant.auth0.com/".to_string(),
//!     audience: Some("your-api-identifier".to_string()),
//!     ..Default::default()
//! };
//!
//! let validator = OidcValidator::new(config).await?;
//! let user = validator.validate_token("eyJhbG...").await?;
//! ```

use crate::security::errors::{Result, SecurityError};
use crate::security::auth_middleware::AuthenticatedUser;
use chrono::{DateTime, Utc};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// OIDC Configuration
// ============================================================================

/// OIDC authentication configuration.
///
/// Configure this with your identity provider's issuer URL.
/// The validator will automatically discover JWKS endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// Issuer URL (e.g., "https://your-tenant.auth0.com/")
    ///
    /// Must match the `iss` claim in tokens exactly.
    /// Should include trailing slash if provider expects it.
    pub issuer: String,

    /// Expected audience claim (optional).
    ///
    /// If set, tokens must have this value in their `aud` claim.
    /// For Auth0, this is typically your API identifier.
    #[serde(default)]
    pub audience: Option<String>,

    /// Additional allowed audiences (optional).
    ///
    /// Some tokens may have multiple audiences. Add extras here.
    #[serde(default)]
    pub additional_audiences: Vec<String>,

    /// JWKS cache TTL in seconds.
    ///
    /// How long to cache the JWKS before refetching.
    /// Default: 3600 (1 hour)
    #[serde(default = "default_jwks_cache_ttl")]
    pub jwks_cache_ttl_secs: u64,

    /// Allowed token algorithms.
    ///
    /// Default: RS256 (most common for OIDC providers)
    #[serde(default = "default_algorithms")]
    pub allowed_algorithms: Vec<String>,

    /// Clock skew tolerance in seconds.
    ///
    /// Allow this many seconds of clock difference when
    /// validating exp/nbf/iat claims.
    /// Default: 60 seconds
    #[serde(default = "default_clock_skew")]
    pub clock_skew_secs: u64,

    /// Custom JWKS URI (optional).
    ///
    /// If set, skip OIDC discovery and use this URI directly.
    /// Useful for providers that don't support standard discovery.
    #[serde(default)]
    pub jwks_uri: Option<String>,

    /// Require authentication for all requests.
    ///
    /// If false, requests without tokens are allowed (anonymous access).
    /// Default: true
    #[serde(default = "default_required")]
    pub required: bool,

    /// Scope claim name.
    ///
    /// The claim containing user scopes/permissions.
    /// Default: "scope" (space-separated string)
    /// Some providers use "scp" or "permissions" (array)
    #[serde(default = "default_scope_claim")]
    pub scope_claim: String,
}

fn default_jwks_cache_ttl() -> u64 {
    3600
}

fn default_algorithms() -> Vec<String> {
    vec!["RS256".to_string()]
}

fn default_clock_skew() -> u64 {
    60
}

fn default_required() -> bool {
    true
}

fn default_scope_claim() -> String {
    "scope".to_string()
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            issuer: String::new(),
            audience: None,
            additional_audiences: Vec::new(),
            jwks_cache_ttl_secs: default_jwks_cache_ttl(),
            allowed_algorithms: default_algorithms(),
            clock_skew_secs: default_clock_skew(),
            jwks_uri: None,
            required: default_required(),
            scope_claim: default_scope_claim(),
        }
    }
}

impl OidcConfig {
    /// Create config for Auth0.
    ///
    /// # Arguments
    ///
    /// * `domain` - Your Auth0 domain (e.g., "your-tenant.auth0.com")
    /// * `audience` - Your API identifier
    #[must_use]
    pub fn auth0(domain: &str, audience: &str) -> Self {
        Self {
            issuer: format!("https://{domain}/"),
            audience: Some(audience.to_string()),
            ..Default::default()
        }
    }

    /// Create config for Keycloak.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Keycloak server URL (e.g., "https://keycloak.example.com")
    /// * `realm` - Realm name
    /// * `client_id` - Client ID (used as audience)
    #[must_use]
    pub fn keycloak(base_url: &str, realm: &str, client_id: &str) -> Self {
        Self {
            issuer: format!("{base_url}/realms/{realm}"),
            audience: Some(client_id.to_string()),
            ..Default::default()
        }
    }

    /// Create config for Okta.
    ///
    /// # Arguments
    ///
    /// * `domain` - Your Okta domain (e.g., "your-org.okta.com")
    /// * `audience` - Your API audience (often "api://default")
    #[must_use]
    pub fn okta(domain: &str, audience: &str) -> Self {
        Self {
            issuer: format!("https://{domain}"),
            audience: Some(audience.to_string()),
            ..Default::default()
        }
    }

    /// Create config for AWS Cognito.
    ///
    /// # Arguments
    ///
    /// * `region` - AWS region (e.g., "us-east-1")
    /// * `user_pool_id` - Cognito User Pool ID
    /// * `client_id` - App client ID (used as audience)
    #[must_use]
    pub fn cognito(region: &str, user_pool_id: &str, client_id: &str) -> Self {
        Self {
            issuer: format!("https://cognito-idp.{region}.amazonaws.com/{user_pool_id}"),
            audience: Some(client_id.to_string()),
            ..Default::default()
        }
    }

    /// Create config for Microsoft Entra ID (Azure AD).
    ///
    /// # Arguments
    ///
    /// * `tenant_id` - Azure AD tenant ID
    /// * `client_id` - Application (client) ID
    #[must_use]
    pub fn azure_ad(tenant_id: &str, client_id: &str) -> Self {
        Self {
            issuer: format!("https://login.microsoftonline.com/{tenant_id}/v2.0"),
            audience: Some(client_id.to_string()),
            ..Default::default()
        }
    }

    /// Create config for Google Identity.
    ///
    /// # Arguments
    ///
    /// * `client_id` - Google OAuth client ID
    #[must_use]
    pub fn google(client_id: &str) -> Self {
        Self {
            issuer: "https://accounts.google.com".to_string(),
            audience: Some(client_id.to_string()),
            ..Default::default()
        }
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<()> {
        if self.issuer.is_empty() {
            return Err(SecurityError::SecurityConfigError(
                "OIDC issuer URL is required".to_string(),
            ));
        }

        if !self.issuer.starts_with("https://") && !self.issuer.starts_with("http://localhost") {
            return Err(SecurityError::SecurityConfigError(
                "OIDC issuer must use HTTPS (except localhost for development)".to_string(),
            ));
        }

        if self.allowed_algorithms.is_empty() {
            return Err(SecurityError::SecurityConfigError(
                "At least one algorithm must be allowed".to_string(),
            ));
        }

        Ok(())
    }
}

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
struct CachedJwks {
    jwks: Jwks,
    fetched_at: Instant,
    ttl: Duration,
}

impl CachedJwks {
    fn is_expired(&self) -> bool {
        self.fetched_at.elapsed() > self.ttl
    }
}

// ============================================================================
// JWT Claims
// ============================================================================

/// Standard JWT claims for validation.
#[derive(Debug, Clone, Deserialize)]
pub struct JwtClaims {
    /// Subject (user ID)
    pub sub: Option<String>,

    /// Issuer
    pub iss: Option<String>,

    /// Audience (can be string or array)
    #[serde(default)]
    pub aud: Audience,

    /// Expiration time (Unix timestamp)
    pub exp: Option<i64>,

    /// Issued at (Unix timestamp)
    pub iat: Option<i64>,

    /// Not before (Unix timestamp)
    pub nbf: Option<i64>,

    /// Scope (space-separated string, common in Auth0/Okta)
    pub scope: Option<String>,

    /// Scopes (array, common in some providers)
    pub scp: Option<Vec<String>>,

    /// Permissions (array, common in Auth0)
    pub permissions: Option<Vec<String>>,

    /// Email claim
    pub email: Option<String>,

    /// Email verified
    pub email_verified: Option<bool>,

    /// Name claim
    pub name: Option<String>,
}

/// Audience can be a single string or array of strings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(untagged)]
pub enum Audience {
    /// No audience specified.
    #[default]
    None,
    /// Single audience string.
    Single(String),
    /// Multiple audiences as an array.
    Multiple(Vec<String>),
}

impl Audience {
    /// Check if the audience contains a specific value.
    pub fn contains(&self, value: &str) -> bool {
        match self {
            Self::None => false,
            Self::Single(s) => s == value,
            Self::Multiple(v) => v.iter().any(|s| s == value),
        }
    }

    /// Get all audience values as a vector.
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            Self::None => Vec::new(),
            Self::Single(s) => vec![s.clone()],
            Self::Multiple(v) => v.clone(),
        }
    }
}

// ============================================================================
// OIDC Validator
// ============================================================================

/// OIDC token validator with JWKS caching.
///
/// Validates JWT tokens against an OIDC provider's public keys.
/// Automatically fetches and caches the JWKS for efficiency.
pub struct OidcValidator {
    config: OidcConfig,
    http_client: reqwest::Client,
    jwks_cache: Arc<RwLock<Option<CachedJwks>>>,
    jwks_uri: String,
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
        config.validate()?;

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| SecurityError::SecurityConfigError(format!("HTTP client error: {e}")))?;

        // Determine JWKS URI
        let jwks_uri = if let Some(ref uri) = config.jwks_uri {
            uri.clone()
        } else {
            // Perform OIDC discovery
            let discovery_url = format!(
                "{}/.well-known/openid-configuration",
                config.issuer.trim_end_matches('/')
            );

            tracing::debug!(url = %discovery_url, "Performing OIDC discovery");

            let response = http_client
                .get(&discovery_url)
                .send()
                .await
                .map_err(|e| {
                    SecurityError::SecurityConfigError(format!("OIDC discovery failed: {e}"))
                })?;

            if !response.status().is_success() {
                return Err(SecurityError::SecurityConfigError(format!(
                    "OIDC discovery failed with status: {}",
                    response.status()
                )));
            }

            let discovery: OidcDiscoveryDocument = response.json().await.map_err(|e| {
                SecurityError::SecurityConfigError(format!("Invalid OIDC discovery response: {e}"))
            })?;

            tracing::info!(
                issuer = %discovery.issuer,
                jwks_uri = %discovery.jwks_uri,
                "OIDC discovery successful"
            );

            discovery.jwks_uri
        };

        Ok(Self {
            config,
            http_client,
            jwks_cache: Arc::new(RwLock::new(None)),
            jwks_uri,
        })
    }

    /// Create a validator without performing discovery.
    ///
    /// Use this for testing or when you have the JWKS URI directly.
    #[must_use]
    pub fn with_jwks_uri(config: OidcConfig, jwks_uri: String) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            jwks_cache: Arc::new(RwLock::new(None)),
            jwks_uri,
        }
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

        // Get the signing key
        let decoding_key = self.get_decoding_key(kid).await?;

        // Build validation
        let mut validation = Validation::new(self.get_algorithm(&header)?);
        validation.set_issuer(&[&self.config.issuer]);

        // Set audience validation
        if let Some(ref aud) = self.config.audience {
            let mut audiences = vec![aud.clone()];
            audiences.extend(self.config.additional_audiences.clone());
            validation.set_audience(&audiences);
        } else {
            validation.validate_aud = false;
        }

        // Set clock skew tolerance
        validation.leeway = self.config.clock_skew_secs;

        // Decode and validate token
        let token_data = decode::<JwtClaims>(token, &decoding_key, &validation).map_err(|e| {
            tracing::debug!(error = %e, "JWT validation failed");
            match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => SecurityError::TokenExpired {
                    expired_at: Utc::now(), // Approximate
                },
                jsonwebtoken::errors::ErrorKind::InvalidIssuer => SecurityError::InvalidToken,
                jsonwebtoken::errors::ErrorKind::InvalidAudience => SecurityError::InvalidToken,
                jsonwebtoken::errors::ErrorKind::InvalidSignature => SecurityError::InvalidToken,
                _ => SecurityError::InvalidToken,
            }
        })?;

        let claims = token_data.claims;

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

        let expires_at = DateTime::<Utc>::from_timestamp(exp, 0).ok_or(SecurityError::InvalidToken)?;

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
        })
    }

    /// Get the decoding key for a specific key ID.
    async fn get_decoding_key(&self, kid: &str) -> Result<DecodingKey> {
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

        // Find the key index first, then we can clone the key
        let key_index = jwks
            .keys
            .iter()
            .position(|k| k.kid.as_deref() == Some(kid))
            .ok_or_else(|| {
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
    async fn fetch_jwks(&self) -> Result<Jwks> {
        tracing::debug!(uri = %self.jwks_uri, "Fetching JWKS");

        let response = self
            .http_client
            .get(&self.jwks_uri)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to fetch JWKS");
                SecurityError::SecurityConfigError(format!("Failed to fetch JWKS: {e}"))
            })?;

        if !response.status().is_success() {
            return Err(SecurityError::SecurityConfigError(format!(
                "JWKS fetch failed with status: {}",
                response.status()
            )));
        }

        let jwks: Jwks = response.json().await.map_err(|e| {
            SecurityError::SecurityConfigError(format!("Invalid JWKS response: {e}"))
        })?;

        tracing::debug!(key_count = jwks.keys.len(), "JWKS fetched successfully");

        Ok(jwks)
    }

    /// Find a key in the JWKS by key ID.
    fn find_key<'a>(&self, jwks: &'a Jwks, kid: &str) -> Option<&'a Jwk> {
        jwks.keys.iter().find(|k| k.kid.as_deref() == Some(kid))
    }

    /// Convert a JWK to a jsonwebtoken DecodingKey.
    fn jwk_to_decoding_key(&self, jwk: &Jwk) -> Result<DecodingKey> {
        match jwk.kty.as_str() {
            "RSA" => {
                let n = jwk.n.as_ref().ok_or(SecurityError::InvalidToken)?;
                let e = jwk.e.as_ref().ok_or(SecurityError::InvalidToken)?;

                DecodingKey::from_rsa_components(n, e).map_err(|e| {
                    tracing::debug!(error = %e, "Failed to create RSA decoding key");
                    SecurityError::InvalidToken
                })
            }
            other => {
                tracing::debug!(key_type = %other, "Unsupported key type");
                Err(SecurityError::InvalidTokenAlgorithm {
                    algorithm: other.to_string(),
                })
            }
        }
    }

    /// Get the algorithm from the JWT header.
    fn get_algorithm(&self, header: &jsonwebtoken::Header) -> Result<Algorithm> {
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
    pub fn is_required(&self) -> bool {
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oidc_config_default() {
        let config = OidcConfig::default();
        assert!(config.issuer.is_empty());
        assert!(config.audience.is_none());
        assert_eq!(config.jwks_cache_ttl_secs, 3600);
        assert_eq!(config.allowed_algorithms, vec!["RS256"]);
        assert_eq!(config.clock_skew_secs, 60);
        assert!(config.required);
    }

    #[test]
    fn test_oidc_config_auth0() {
        let config = OidcConfig::auth0("my-tenant.auth0.com", "my-api");
        assert_eq!(config.issuer, "https://my-tenant.auth0.com/");
        assert_eq!(config.audience, Some("my-api".to_string()));
    }

    #[test]
    fn test_oidc_config_keycloak() {
        let config = OidcConfig::keycloak("https://keycloak.example.com", "myrealm", "myclient");
        assert_eq!(
            config.issuer,
            "https://keycloak.example.com/realms/myrealm"
        );
        assert_eq!(config.audience, Some("myclient".to_string()));
    }

    #[test]
    fn test_oidc_config_okta() {
        let config = OidcConfig::okta("myorg.okta.com", "api://default");
        assert_eq!(config.issuer, "https://myorg.okta.com");
        assert_eq!(config.audience, Some("api://default".to_string()));
    }

    #[test]
    fn test_oidc_config_cognito() {
        let config = OidcConfig::cognito("us-east-1", "us-east-1_abc123", "client123");
        assert_eq!(
            config.issuer,
            "https://cognito-idp.us-east-1.amazonaws.com/us-east-1_abc123"
        );
        assert_eq!(config.audience, Some("client123".to_string()));
    }

    #[test]
    fn test_oidc_config_azure_ad() {
        let config = OidcConfig::azure_ad("tenant-id-123", "client-id-456");
        assert_eq!(
            config.issuer,
            "https://login.microsoftonline.com/tenant-id-123/v2.0"
        );
        assert_eq!(config.audience, Some("client-id-456".to_string()));
    }

    #[test]
    fn test_oidc_config_google() {
        let config = OidcConfig::google("123456.apps.googleusercontent.com");
        assert_eq!(config.issuer, "https://accounts.google.com");
        assert_eq!(
            config.audience,
            Some("123456.apps.googleusercontent.com".to_string())
        );
    }

    #[test]
    fn test_oidc_config_validate_empty_issuer() {
        let config = OidcConfig::default();
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(SecurityError::SecurityConfigError(_))
        ));
    }

    #[test]
    fn test_oidc_config_validate_http_issuer() {
        let config = OidcConfig {
            issuer: "http://insecure.example.com".to_string(),
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_oidc_config_validate_localhost_allowed() {
        let config = OidcConfig {
            issuer: "http://localhost:8080".to_string(),
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_oidc_config_validate_https_required() {
        let config = OidcConfig {
            issuer: "https://secure.example.com".to_string(),
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_audience_none() {
        let aud = Audience::None;
        assert!(!aud.contains("test"));
        assert!(aud.to_vec().is_empty());
    }

    #[test]
    fn test_audience_single() {
        let aud = Audience::Single("my-api".to_string());
        assert!(aud.contains("my-api"));
        assert!(!aud.contains("other"));
        assert_eq!(aud.to_vec(), vec!["my-api"]);
    }

    #[test]
    fn test_audience_multiple() {
        let aud = Audience::Multiple(vec!["api1".to_string(), "api2".to_string()]);
        assert!(aud.contains("api1"));
        assert!(aud.contains("api2"));
        assert!(!aud.contains("api3"));
        assert_eq!(aud.to_vec(), vec!["api1", "api2"]);
    }

    #[test]
    fn test_jwk_deserialization() {
        let jwk_json = r#"{
            "kty": "RSA",
            "kid": "test-key-id",
            "alg": "RS256",
            "use": "sig",
            "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
            "e": "AQAB"
        }"#;

        let jwk: Jwk = serde_json::from_str(jwk_json).unwrap();
        assert_eq!(jwk.kty, "RSA");
        assert_eq!(jwk.kid, Some("test-key-id".to_string()));
        assert_eq!(jwk.alg, Some("RS256".to_string()));
        assert!(jwk.n.is_some());
        assert!(jwk.e.is_some());
    }

    #[test]
    fn test_jwks_deserialization() {
        let jwks_json = r#"{
            "keys": [
                {
                    "kty": "RSA",
                    "kid": "key1",
                    "n": "test_n",
                    "e": "AQAB"
                },
                {
                    "kty": "RSA",
                    "kid": "key2",
                    "n": "test_n2",
                    "e": "AQAB"
                }
            ]
        }"#;

        let jwks: Jwks = serde_json::from_str(jwks_json).unwrap();
        assert_eq!(jwks.keys.len(), 2);
        assert_eq!(jwks.keys[0].kid, Some("key1".to_string()));
        assert_eq!(jwks.keys[1].kid, Some("key2".to_string()));
    }

    #[test]
    fn test_jwt_claims_deserialization() {
        let claims_json = r#"{
            "sub": "user123",
            "iss": "https://issuer.example.com",
            "aud": "my-api",
            "exp": 1735689600,
            "iat": 1735686000,
            "scope": "read write",
            "email": "user@example.com"
        }"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        assert_eq!(claims.sub, Some("user123".to_string()));
        assert_eq!(claims.iss, Some("https://issuer.example.com".to_string()));
        assert!(claims.aud.contains("my-api"));
        assert_eq!(claims.exp, Some(1_735_689_600));
        assert_eq!(claims.scope, Some("read write".to_string()));
    }

    #[test]
    fn test_jwt_claims_array_audience() {
        let claims_json = r#"{
            "sub": "user123",
            "aud": ["api1", "api2"],
            "exp": 1735689600
        }"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        assert!(claims.aud.contains("api1"));
        assert!(claims.aud.contains("api2"));
    }

    #[test]
    fn test_oidc_discovery_document_deserialization() {
        let doc_json = r#"{
            "issuer": "https://issuer.example.com",
            "jwks_uri": "https://issuer.example.com/.well-known/jwks.json",
            "authorization_endpoint": "https://issuer.example.com/authorize",
            "token_endpoint": "https://issuer.example.com/oauth/token",
            "id_token_signing_alg_values_supported": ["RS256", "RS384", "RS512"]
        }"#;

        let doc: OidcDiscoveryDocument = serde_json::from_str(doc_json).unwrap();
        assert_eq!(doc.issuer, "https://issuer.example.com");
        assert_eq!(
            doc.jwks_uri,
            "https://issuer.example.com/.well-known/jwks.json"
        );
        assert_eq!(doc.id_token_signing_alg_values_supported.len(), 3);
    }
}
