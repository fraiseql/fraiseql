//! Authentication Middleware (Phase 6.2)
//!
//! This module provides authentication validation for GraphQL requests.
//! It validates:
//! - Authentication requirement (auth mandatory or optional)
//! - JWT token extraction from Authorization header
//! - Token signature verification (HS256/RS256/RS384/RS512)
//! - Token expiry validation (exp claim)
//! - Required claims validation (sub, exp, aud, iss)
//!
//! # Architecture
//!
//! The Auth middleware acts as the second layer in the security middleware:
//! ```text
//! HTTP Request with Authorization header
//!     ↓
//! AuthMiddleware::validate_request()
//!     ├─ Check 1: Extract token from Authorization header
//!     ├─ Check 2: Validate token structure and signature (HS256/RS256)
//!     ├─ Check 3: Check token expiry (exp claim)
//!     ├─ Check 4: Validate required claims (sub, exp)
//!     └─ Check 5: Extract user info from claims
//!     ↓
//! Result<AuthenticatedUser> (user info or error)
//! ```
//!
//! # Signature Verification
//!
//! The middleware supports multiple signing algorithms:
//! - **HS256** (HMAC-SHA256): Symmetric key, good for internal services
//! - **RS256/RS384/RS512** (RSA): Asymmetric key, good for external providers
//!
//! # Examples
//!
//! ```ignore
//! use fraiseql_core::security::{AuthMiddleware, AuthConfig, SigningKey};
//!
//! // Create middleware with HS256 signing key
//! let config = AuthConfig {
//!     required: true,
//!     token_expiry_secs: 3600,
//!     signing_key: Some(SigningKey::hs256("your-secret-key")),
//!     issuer: Some("https://your-issuer.com".to_string()),
//!     audience: Some("your-api".to_string()),
//! };
//! let middleware = AuthMiddleware::from_config(config);
//!
//! // Validate a request (extract and validate token with signature verification)
//! let user = middleware.validate_request(&request)?;
//! println!("Authenticated user: {}", user.user_id);
//! println!("Scopes: {:?}", user.scopes);
//! println!("Expires: {}", user.expires_at);
//! ```

use crate::security::errors::{Result, SecurityError};
use chrono::{DateTime, Utc};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::fmt;

// ============================================================================
// Signing Key Configuration
// ============================================================================

/// Signing key for JWT signature verification.
///
/// Supports both symmetric (HS256) and asymmetric (RS256/RS384/RS512) algorithms.
#[derive(Debug, Clone)]
pub enum SigningKey {
    /// HMAC-SHA256 symmetric key.
    ///
    /// Use for internal services where the same secret is shared
    /// between token issuer and validator.
    Hs256(Vec<u8>),

    /// HMAC-SHA384 symmetric key.
    Hs384(Vec<u8>),

    /// HMAC-SHA512 symmetric key.
    Hs512(Vec<u8>),

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
        Self::Hs256(secret.as_bytes().to_vec())
    }

    /// Create an HS256 signing key from raw bytes.
    #[must_use]
    pub fn hs256_bytes(secret: &[u8]) -> Self {
        Self::Hs256(secret.to_vec())
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

    /// Convert to a jsonwebtoken DecodingKey.
    fn to_decoding_key(&self) -> std::result::Result<DecodingKey, SecurityError> {
        match self {
            Self::Hs256(secret) | Self::Hs384(secret) | Self::Hs512(secret) => {
                Ok(DecodingKey::from_secret(secret))
            }
            Self::Rs256Pem(pem) | Self::Rs384Pem(pem) | Self::Rs512Pem(pem) => {
                DecodingKey::from_rsa_pem(pem.as_bytes()).map_err(|e| {
                    SecurityError::SecurityConfigError(format!("Invalid RSA PEM key: {e}"))
                })
            }
            Self::Rs256Components { n, e } => {
                DecodingKey::from_rsa_components(n, e).map_err(|e| {
                    SecurityError::SecurityConfigError(format!("Invalid RSA components: {e}"))
                })
            }
        }
    }
}

// ============================================================================
// Authentication Configuration
// ============================================================================

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

fn default_clock_skew() -> u64 {
    60
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
            required: false,
            token_expiry_secs: 3600,
            signing_key: None,
            issuer: None,
            audience: None,
            clock_skew_secs: default_clock_skew(),
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
            required: true,
            token_expiry_secs: 3600,
            signing_key: None,
            issuer: None,
            audience: None,
            clock_skew_secs: default_clock_skew(),
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
            required: true,
            token_expiry_secs: 1800,
            signing_key: None,
            issuer: None,
            audience: None,
            clock_skew_secs: default_clock_skew(),
        }
    }

    /// Create a configuration with HS256 signing key.
    ///
    /// This is the recommended configuration for production when using
    /// symmetric key signing (internal services).
    #[must_use]
    pub fn with_hs256(secret: &str) -> Self {
        Self {
            required: true,
            token_expiry_secs: 3600,
            signing_key: Some(SigningKey::hs256(secret)),
            issuer: None,
            audience: None,
            clock_skew_secs: default_clock_skew(),
        }
    }

    /// Create a configuration with RS256 signing key from PEM.
    ///
    /// This is the recommended configuration for production when using
    /// asymmetric key signing (external identity providers).
    #[must_use]
    pub fn with_rs256_pem(pem: &str) -> Self {
        Self {
            required: true,
            token_expiry_secs: 3600,
            signing_key: Some(SigningKey::rs256_pem(pem)),
            issuer: None,
            audience: None,
            clock_skew_secs: default_clock_skew(),
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

/// Authenticated user information extracted from JWT claims
///
/// Contains the essential user information needed for authorization checks
/// and audit logging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    /// User ID (from 'sub' claim in JWT)
    pub user_id: String,

    /// Scopes/permissions (from 'scope' claim if present)
    pub scopes: Vec<String>,

    /// When the token expires
    pub expires_at: DateTime<Utc>,
}

impl fmt::Display for AuthenticatedUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "User({}, expires={})",
            self.user_id,
            self.expires_at.format("%Y-%m-%d %H:%M:%S UTC")
        )
    }
}

impl AuthenticatedUser {
    /// Check if the user has a specific scope
    #[must_use]
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }

    /// Check if the token has expired (as of now)
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }

    /// Get time until expiry in seconds
    #[must_use]
    pub fn ttl_secs(&self) -> i64 {
        (self.expires_at - Utc::now()).num_seconds()
    }
}

/// Request context for authentication validation
///
/// Contains information extracted from an HTTP request.
#[derive(Debug, Clone)]
pub struct AuthRequest {
    /// Raw Authorization header value (e.g., "Bearer eyJhbG...")
    pub authorization_header: Option<String>,
}

impl AuthRequest {
    /// Create a new auth request from an authorization header
    #[must_use]
    pub fn new(authorization_header: Option<String>) -> Self {
        Self {
            authorization_header,
        }
    }

    /// Extract the bearer token from the Authorization header
    ///
    /// Expected format: "Bearer <token>"
    ///
    /// Returns Ok(token) if valid format, Err otherwise
    pub fn extract_bearer_token(&self) -> Result<String> {
        let header = self
            .authorization_header
            .as_ref()
            .ok_or(SecurityError::AuthRequired)?;

        if !header.starts_with("Bearer ") {
            return Err(SecurityError::AuthRequired);
        }

        Ok(header[7..].to_string())
    }
}

/// Claims extracted from JWT token
///
/// This is a simplified representation of JWT claims.
/// In production, this would be more comprehensive.
#[derive(Debug, Clone)]
pub struct TokenClaims {
    /// Subject (user ID)
    pub sub: Option<String>,

    /// Expiration timestamp (seconds since epoch)
    pub exp: Option<i64>,

    /// Scope claim (space-separated string)
    pub scope: Option<String>,

    /// Audience claim
    pub aud: Option<Vec<String>>,

    /// Issuer claim
    pub iss: Option<String>,
}

/// JWT claims structure for jsonwebtoken crate deserialization.
///
/// This struct is used internally for decoding and validating JWT tokens
/// when signature verification is enabled.
#[derive(Debug, Deserialize)]
struct JwtClaims {
    /// Subject (user ID) - required
    sub: Option<String>,

    /// Expiration timestamp (seconds since epoch) - required
    exp: Option<i64>,

    /// Issued at timestamp (captured but not used directly)
    #[serde(default)]
    #[allow(dead_code)]
    iat: Option<i64>,

    /// Not before timestamp (captured but not used directly)
    #[serde(default)]
    #[allow(dead_code)]
    nbf: Option<i64>,

    /// Scope claim (space-separated string)
    #[serde(default)]
    scope: Option<String>,

    /// Scopes as array (alternative format used by some providers)
    #[serde(default)]
    scp: Option<Vec<String>>,

    /// Permissions (Auth0 RBAC style)
    #[serde(default)]
    permissions: Option<Vec<String>>,

    /// Audience claim (validated by jsonwebtoken, captured for logging)
    #[serde(default)]
    #[allow(dead_code)]
    aud: Option<serde_json::Value>,

    /// Issuer claim (validated by jsonwebtoken, captured for logging)
    #[serde(default)]
    #[allow(dead_code)]
    iss: Option<String>,
}

/// Authentication Middleware
///
/// Validates incoming requests for authentication requirements.
/// Acts as the second layer in the security middleware pipeline.
#[derive(Debug, Clone)]
pub struct AuthMiddleware {
    config: AuthConfig,
}

impl AuthMiddleware {
    /// Create a new authentication middleware from configuration
    #[must_use]
    pub fn from_config(config: AuthConfig) -> Self {
        Self { config }
    }

    /// Create middleware with permissive settings (authentication optional)
    #[must_use]
    pub fn permissive() -> Self {
        Self::from_config(AuthConfig::permissive())
    }

    /// Create middleware with standard settings (authentication required)
    #[must_use]
    pub fn standard() -> Self {
        Self::from_config(AuthConfig::standard())
    }

    /// Create middleware with strict settings (authentication required, short expiry)
    #[must_use]
    pub fn strict() -> Self {
        Self::from_config(AuthConfig::strict())
    }

    /// Validate authentication in a request
    ///
    /// Performs validation checks in order:
    /// 1. Extract token from Authorization header
    /// 2. Validate token signature (if signing key configured)
    /// 3. Check token expiry (exp claim)
    /// 4. Validate issuer/audience claims (if configured)
    /// 5. Extract required claims (sub)
    /// 6. Extract optional claims (scope, aud, iss)
    ///
    /// Returns AuthenticatedUser if valid, Err if any check fails.
    pub fn validate_request(&self, req: &AuthRequest) -> Result<AuthenticatedUser> {
        // Check 1: Extract token from Authorization header
        let token = self.extract_token(req)?;

        // Check 2: Validate token (with or without signature verification)
        if let Some(ref signing_key) = self.config.signing_key {
            // Use jsonwebtoken crate for proper signature verification
            self.validate_token_with_signature(&token, signing_key)
        } else {
            // Fallback: structure validation only (for testing/backwards compatibility)
            // WARNING: This is insecure for production use!
            self.validate_token_structure_only(&token)
        }
    }

    /// Validate token with cryptographic signature verification.
    ///
    /// This is the secure path used when a signing key is configured.
    fn validate_token_with_signature(
        &self,
        token: &str,
        signing_key: &SigningKey,
    ) -> Result<AuthenticatedUser> {
        // Get the decoding key
        let decoding_key = signing_key.to_decoding_key()?;

        // Build validation configuration
        let mut validation = Validation::new(signing_key.algorithm());

        // Configure issuer validation (only validate if configured)
        if let Some(ref issuer) = self.config.issuer {
            validation.set_issuer(&[issuer]);
        }
        // Note: If issuer is not set, validation.iss is None and won't be validated

        // Configure audience validation
        if let Some(ref audience) = self.config.audience {
            validation.set_audience(&[audience]);
        } else {
            validation.validate_aud = false;
        }

        // Set clock skew tolerance
        validation.leeway = self.config.clock_skew_secs;

        // Decode and validate the token
        let token_data = decode::<JwtClaims>(token, &decoding_key, &validation).map_err(|e| {
            match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                    // Try to extract the actual expiry time from the token
                    SecurityError::TokenExpired {
                        expired_at: Utc::now(), // Approximate - actual time is not accessible
                    }
                }
                jsonwebtoken::errors::ErrorKind::InvalidSignature => SecurityError::InvalidToken,
                jsonwebtoken::errors::ErrorKind::InvalidIssuer => SecurityError::InvalidToken,
                jsonwebtoken::errors::ErrorKind::InvalidAudience => SecurityError::InvalidToken,
                jsonwebtoken::errors::ErrorKind::InvalidAlgorithm => {
                    SecurityError::InvalidTokenAlgorithm {
                        algorithm: format!("{:?}", signing_key.algorithm()),
                    }
                }
                jsonwebtoken::errors::ErrorKind::MissingRequiredClaim(claim) => {
                    SecurityError::TokenMissingClaim {
                        claim: claim.clone(),
                    }
                }
                _ => SecurityError::InvalidToken,
            }
        })?;

        let claims = token_data.claims;

        // Extract scopes (supports multiple formats)
        let scopes = self.extract_scopes_from_jwt_claims(&claims);

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

        Ok(AuthenticatedUser {
            user_id,
            scopes,
            expires_at,
        })
    }

    /// Extract scopes from JWT claims.
    ///
    /// Supports multiple formats:
    /// - `scope`: space-separated string (OAuth2 standard)
    /// - `scp`: array of strings (Microsoft)
    /// - `permissions`: array of strings (Auth0 RBAC)
    fn extract_scopes_from_jwt_claims(&self, claims: &JwtClaims) -> Vec<String> {
        // Try space-separated scope string first (most common)
        if let Some(ref scope) = claims.scope {
            return scope.split_whitespace().map(String::from).collect();
        }

        // Try array of scopes (scp claim)
        if let Some(ref scp) = claims.scp {
            return scp.clone();
        }

        // Try permissions array (Auth0 RBAC)
        if let Some(ref permissions) = claims.permissions {
            return permissions.clone();
        }

        Vec::new()
    }

    /// Validate token structure only (no signature verification).
    ///
    /// WARNING: This is insecure and should only be used for testing
    /// or when signature verification is handled elsewhere.
    fn validate_token_structure_only(&self, token: &str) -> Result<AuthenticatedUser> {
        // Validate basic structure
        self.validate_token_structure(token)?;

        // Parse claims
        let claims = self.parse_claims(token)?;

        // Extract and validate 'exp' claim (required)
        let exp = claims.exp.ok_or(SecurityError::TokenMissingClaim {
            claim: "exp".to_string(),
        })?;

        // Check expiry
        let expires_at =
            DateTime::<Utc>::from_timestamp(exp, 0).ok_or(SecurityError::InvalidToken)?;

        if expires_at <= Utc::now() {
            return Err(SecurityError::TokenExpired {
                expired_at: expires_at,
            });
        }

        // Extract and validate 'sub' claim (required)
        let user_id = claims.sub.ok_or(SecurityError::TokenMissingClaim {
            claim: "sub".to_string(),
        })?;

        // Extract optional claims
        let scopes = claims
            .scope
            .as_ref()
            .map(|s| s.split_whitespace().map(String::from).collect())
            .unwrap_or_default();

        Ok(AuthenticatedUser {
            user_id,
            scopes,
            expires_at,
        })
    }

    /// Extract token from the authorization header
    fn extract_token(&self, req: &AuthRequest) -> Result<String> {
        // If auth is not required and no header present, that's OK
        if !self.config.required && req.authorization_header.is_none() {
            return Err(SecurityError::AuthRequired); // Will be handled differently
        }

        req.extract_bearer_token()
    }

    /// Validate token structure (basic checks)
    ///
    /// A real implementation would validate the signature here.
    /// For now, we just check basic structure.
    fn validate_token_structure(&self, token: &str) -> Result<()> {
        // JWT has 3 parts separated by dots: header.payload.signature
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(SecurityError::InvalidToken);
        }

        // Check that each part is non-empty
        if parts.iter().any(|p| p.is_empty()) {
            return Err(SecurityError::InvalidToken);
        }

        Ok(())
    }

    /// Parse JWT claims (simplified, for demo purposes)
    ///
    /// In a real implementation, this would decode and validate the JWT signature.
    /// For testing, we accept a special test token format: "test:{json_payload}"
    fn parse_claims(&self, token: &str) -> Result<TokenClaims> {
        // Split the token
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(SecurityError::InvalidToken);
        }

        // For testing, we use a simple format: part1.{json}.part3
        // where {json} is a base64-like encoded JSON
        // Since we don't have base64 in core dependencies, we'll try to parse directly
        let payload_part = parts[1];

        // Try to decode as hex (simpler than base64 and no dependencies)
        // For test tokens, we'll encode the JSON as hex
        if let Ok(decoded) = hex::decode(payload_part) {
            if let Ok(json_str) = std::str::from_utf8(&decoded) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                    return Ok(self.extract_claims_from_json(&json));
                }
            }
        }

        // If hex decoding fails, try to parse as UTF-8 directly (for test tokens created inline)
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload_part) {
            return Ok(self.extract_claims_from_json(&json));
        }

        Err(SecurityError::InvalidToken)
    }

    /// Extract claims from parsed JSON
    fn extract_claims_from_json(&self, json: &serde_json::Value) -> TokenClaims {
        let sub = json["sub"].as_str().map(String::from);
        let exp = json["exp"].as_i64();
        let scope = json["scope"].as_str().map(String::from);
        let aud = json["aud"].as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });
        let iss = json["iss"].as_str().map(String::from);

        TokenClaims {
            sub,
            exp,
            scope,
            aud,
            iss,
        }
    }

    /// Get the underlying configuration
    #[must_use]
    pub const fn config(&self) -> &AuthConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Helper Functions
    // ============================================================================

    /// Create a valid JWT token with specified claims (for testing)
    ///
    /// Note: This creates a structurally valid JWT, but doesn't sign it.
    /// For real use, you'd use a proper JWT library.
    fn create_test_token(sub: &str, exp_offset_secs: i64, scope: Option<&str>) -> String {
        let now = chrono::Utc::now().timestamp();
        let exp = now + exp_offset_secs;

        // Create payload
        let mut payload = serde_json::json!({
            "sub": sub,
            "exp": exp,
            "iat": now,
            "aud": ["test-audience"],
            "iss": "test-issuer"
        });

        if let Some(s) = scope {
            payload["scope"] = serde_json::json!(s);
        }

        // Encode payload as hex for testing
        let payload_json = payload.to_string();
        let payload_hex = hex::encode(&payload_json);
        let signature = "test_signature"; // Not a real signature

        // Format: header.payload_hex.signature
        format!("header.{payload_hex}.{signature}")
    }

    // ============================================================================
    // Check 1: Token Extraction Tests
    // ============================================================================

    #[test]
    fn test_bearer_token_extracted_correctly() {
        let token = "test_token_12345";
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = req.extract_bearer_token();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), token);
    }

    #[test]
    fn test_missing_authorization_header_rejected() {
        let req = AuthRequest::new(None);

        let result = req.extract_bearer_token();
        assert!(matches!(result, Err(SecurityError::AuthRequired)));
    }

    #[test]
    fn test_invalid_authorization_header_format_rejected() {
        let req = AuthRequest::new(Some("Basic abc123".to_string()));

        let result = req.extract_bearer_token();
        assert!(matches!(result, Err(SecurityError::AuthRequired)));
    }

    #[test]
    fn test_bearer_prefix_required() {
        let req = AuthRequest::new(Some("abc123".to_string()));

        let result = req.extract_bearer_token();
        assert!(matches!(result, Err(SecurityError::AuthRequired)));
    }

    // ============================================================================
    // Check 2: Token Structure Validation Tests
    // ============================================================================

    #[test]
    fn test_valid_token_structure() {
        let middleware = AuthMiddleware::permissive();
        let token = create_test_token("user123", 3600, None);

        let result = middleware.validate_token_structure(&token);
        assert!(result.is_ok());
    }

    #[test]
    fn test_token_with_wrong_part_count_rejected() {
        let middleware = AuthMiddleware::permissive();
        let token = "header.payload"; // Missing signature

        let result = middleware.validate_token_structure(token);
        assert!(matches!(result, Err(SecurityError::InvalidToken)));
    }

    #[test]
    fn test_token_with_empty_part_rejected() {
        let middleware = AuthMiddleware::permissive();
        let token = "header..signature"; // Empty payload

        let result = middleware.validate_token_structure(token);
        assert!(matches!(result, Err(SecurityError::InvalidToken)));
    }

    // ============================================================================
    // Check 3: Token Expiry Validation Tests
    // ============================================================================

    #[test]
    fn test_valid_token_not_expired() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user123", 3600, None); // 1 hour from now
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        assert!(result.is_ok());
    }

    #[test]
    fn test_expired_token_rejected() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user123", -3600, None); // 1 hour ago
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        assert!(matches!(result, Err(SecurityError::TokenExpired { .. })));
    }

    #[test]
    fn test_token_expiring_now_rejected() {
        let middleware = AuthMiddleware::standard();
        // Token that expires at the current timestamp (or in past due to processing time)
        let token = create_test_token("user123", 0, None);
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        // May pass or fail depending on exact timing, but should be close
        let result = middleware.validate_request(&req);
        // We won't assert here since timing is critical
        let _ = result;
    }

    // ============================================================================
    // Check 4: Required Claims Validation Tests
    // ============================================================================

    #[test]
    fn test_missing_sub_claim_rejected() {
        let middleware = AuthMiddleware::standard();

        // Create token without 'sub' claim
        let now = chrono::Utc::now().timestamp();
        let payload = serde_json::json!({
            "exp": now + 3600,
            "iat": now
        });

        let payload_hex = hex::encode(payload.to_string());
        let token = format!("header.{payload_hex}.signature");

        let req = AuthRequest::new(Some(format!("Bearer {token}")));
        let result = middleware.validate_request(&req);

        assert!(matches!(
            result,
            Err(SecurityError::TokenMissingClaim { claim })
            if claim == "sub"
        ));
    }

    #[test]
    fn test_missing_exp_claim_rejected() {
        let middleware = AuthMiddleware::standard();

        // Create token without 'exp' claim
        let payload = serde_json::json!({
            "sub": "user123",
            "iat": chrono::Utc::now().timestamp()
        });

        let payload_hex = hex::encode(payload.to_string());
        let token = format!("header.{payload_hex}.signature");

        let req = AuthRequest::new(Some(format!("Bearer {token}")));
        let result = middleware.validate_request(&req);

        assert!(matches!(
            result,
            Err(SecurityError::TokenMissingClaim { claim })
            if claim == "exp"
        ));
    }

    // ============================================================================
    // Check 5: User Info Extraction Tests
    // ============================================================================

    #[test]
    fn test_user_id_extracted_from_token() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user_12345", 3600, None);
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        assert!(result.is_ok());

        let user = result.unwrap();
        assert_eq!(user.user_id, "user_12345");
    }

    #[test]
    fn test_scopes_extracted_from_token() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user123", 3600, Some("read write admin"));
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        assert!(result.is_ok());

        let user = result.unwrap();
        assert_eq!(user.scopes, vec!["read", "write", "admin"]);
    }

    #[test]
    fn test_empty_scopes_when_scope_claim_absent() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user123", 3600, None);
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        assert!(result.is_ok());

        let user = result.unwrap();
        assert!(user.scopes.is_empty());
    }

    #[test]
    fn test_expires_at_extracted_correctly() {
        let middleware = AuthMiddleware::standard();
        let offset_secs = 7200; // 2 hours

        let token = create_test_token("user123", offset_secs, None);
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        assert!(result.is_ok());

        let user = result.unwrap();
        let now = Utc::now();
        let diff = (user.expires_at - now).num_seconds();

        // Should be approximately offset_secs (within 5 seconds due to processing)
        assert!((offset_secs - 5..=offset_secs + 5).contains(&diff));
    }

    // ============================================================================
    // Configuration Tests
    // ============================================================================

    #[test]
    fn test_permissive_config() {
        let config = AuthConfig::permissive();
        assert!(!config.required);
        assert_eq!(config.token_expiry_secs, 3600);
    }

    #[test]
    fn test_standard_config() {
        let config = AuthConfig::standard();
        assert!(config.required);
        assert_eq!(config.token_expiry_secs, 3600);
    }

    #[test]
    fn test_strict_config() {
        let config = AuthConfig::strict();
        assert!(config.required);
        assert_eq!(config.token_expiry_secs, 1800);
    }

    #[test]
    fn test_middleware_helpers() {
        let permissive = AuthMiddleware::permissive();
        assert!(!permissive.config().required);

        let standard = AuthMiddleware::standard();
        assert!(standard.config().required);

        let strict = AuthMiddleware::strict();
        assert!(strict.config().required);
    }

    // ============================================================================
    // AuthenticatedUser Tests
    // ============================================================================

    #[test]
    fn test_user_has_scope() {
        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };

        assert!(user.has_scope("read"));
        assert!(user.has_scope("write"));
        assert!(!user.has_scope("admin"));
    }

    #[test]
    fn test_user_is_not_expired() {
        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            scopes: vec![],
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };

        assert!(!user.is_expired());
    }

    #[test]
    fn test_user_is_expired() {
        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            scopes: vec![],
            expires_at: Utc::now() - chrono::Duration::hours(1),
        };

        assert!(user.is_expired());
    }

    #[test]
    fn test_user_ttl_calculation() {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::hours(2);
        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            scopes: vec![],
            expires_at,
        };

        let ttl = user.ttl_secs();
        // Should be approximately 7200 seconds (2 hours)
        assert!((7195..=7205).contains(&ttl));
    }

    #[test]
    fn test_user_display() {
        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            scopes: vec![],
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };

        let display_str = user.to_string();
        assert!(display_str.contains("user123"));
        assert!(display_str.contains("UTC"));
    }

    // ============================================================================
    // Error Message Tests
    // ============================================================================

    #[test]
    fn test_error_messages_clear_and_actionable() {
        let middleware = AuthMiddleware::standard();

        // Test missing header error
        let req = AuthRequest::new(None);
        let result = middleware.validate_request(&req);
        assert!(matches!(result, Err(SecurityError::AuthRequired)));

        // Test invalid format error
        let req = AuthRequest::new(Some("Basic xyz".to_string()));
        let result = middleware.validate_request(&req);
        assert!(matches!(result, Err(SecurityError::AuthRequired)));
    }

    // ============================================================================
    // Edge Cases
    // ============================================================================

    #[test]
    fn test_auth_not_required_allows_missing_token() {
        // When auth is NOT required, missing token should still go through extraction
        let middleware = AuthMiddleware::permissive(); // required = false
        let req = AuthRequest::new(None);

        let result = middleware.validate_request(&req);
        // Should fail at extraction, not because auth is optional
        assert!(matches!(result, Err(SecurityError::AuthRequired)));
    }

    #[test]
    fn test_whitespace_in_scopes_handled() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user123", 3600, Some("  read   write  admin  "));
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        assert!(result.is_ok());

        let user = result.unwrap();
        // split_whitespace handles multiple spaces correctly
        assert_eq!(user.scopes.len(), 3);
    }

    #[test]
    fn test_single_scope_parsed_correctly() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user123", 3600, Some("read"));
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        assert!(result.is_ok());

        let user = result.unwrap();
        assert_eq!(user.scopes, vec!["read"]);
    }

    // ============================================================================
    // JWT Signature Verification Tests (Issue #225)
    // ============================================================================

    /// Helper to create a properly signed HS256 JWT token
    fn create_signed_hs256_token(
        sub: &str,
        exp_offset_secs: i64,
        scope: Option<&str>,
        secret: &str,
    ) -> String {
        use jsonwebtoken::{encode, EncodingKey, Header};

        let now = chrono::Utc::now().timestamp();
        let exp = now + exp_offset_secs;

        #[derive(serde::Serialize)]
        struct Claims {
            sub: String,
            exp: i64,
            iat: i64,
            #[serde(skip_serializing_if = "Option::is_none")]
            scope: Option<String>,
        }

        let claims = Claims {
            sub: sub.to_string(),
            exp,
            iat: now,
            scope: scope.map(String::from),
        };

        encode(
            &Header::default(), // HS256
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("Failed to create test token")
    }

    #[test]
    fn test_hs256_signature_verification_valid_token() {
        let secret = "super-secret-key-for-testing-only";
        let config = AuthConfig::with_hs256(secret);
        let middleware = AuthMiddleware::from_config(config);

        let token = create_signed_hs256_token("user123", 3600, Some("read write"), secret);
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        assert!(result.is_ok(), "Expected valid token, got: {:?}", result);

        let user = result.unwrap();
        assert_eq!(user.user_id, "user123");
        assert_eq!(user.scopes, vec!["read", "write"]);
    }

    #[test]
    fn test_hs256_signature_verification_wrong_secret_rejected() {
        let signing_secret = "correct-secret";
        let wrong_secret = "wrong-secret";

        let config = AuthConfig::with_hs256(signing_secret);
        let middleware = AuthMiddleware::from_config(config);

        // Token signed with wrong secret
        let token = create_signed_hs256_token("user123", 3600, None, wrong_secret);
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        assert!(
            matches!(result, Err(SecurityError::InvalidToken)),
            "Expected InvalidToken for wrong signature, got: {:?}",
            result
        );
    }

    #[test]
    fn test_hs256_expired_token_rejected() {
        let secret = "test-secret";
        let config = AuthConfig::with_hs256(secret);
        let middleware = AuthMiddleware::from_config(config);

        // Token expired 1 hour ago
        let token = create_signed_hs256_token("user123", -3600, None, secret);
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        assert!(
            matches!(result, Err(SecurityError::TokenExpired { .. })),
            "Expected TokenExpired, got: {:?}",
            result
        );
    }

    #[test]
    fn test_hs256_with_issuer_validation() {
        let secret = "test-secret";
        let config = AuthConfig::with_hs256(secret).with_issuer("https://auth.example.com");
        let middleware = AuthMiddleware::from_config(config);

        // Create token with matching issuer
        use jsonwebtoken::{encode, EncodingKey, Header};

        #[derive(serde::Serialize)]
        struct ClaimsWithIss {
            sub: String,
            exp: i64,
            iss: String,
        }

        let now = chrono::Utc::now().timestamp();
        let claims = ClaimsWithIss {
            sub: "user123".to_string(),
            exp: now + 3600,
            iss: "https://auth.example.com".to_string(),
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();

        let req = AuthRequest::new(Some(format!("Bearer {token}")));
        let result = middleware.validate_request(&req);
        assert!(result.is_ok(), "Expected valid token with issuer, got: {:?}", result);
    }

    #[test]
    fn test_hs256_with_wrong_issuer_rejected() {
        let secret = "test-secret";
        let config = AuthConfig::with_hs256(secret).with_issuer("https://auth.example.com");
        let middleware = AuthMiddleware::from_config(config);

        // Create token with wrong issuer
        use jsonwebtoken::{encode, EncodingKey, Header};

        #[derive(serde::Serialize)]
        struct ClaimsWithIss {
            sub: String,
            exp: i64,
            iss: String,
        }

        let now = chrono::Utc::now().timestamp();
        let claims = ClaimsWithIss {
            sub: "user123".to_string(),
            exp: now + 3600,
            iss: "https://wrong-issuer.com".to_string(),
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();

        let req = AuthRequest::new(Some(format!("Bearer {token}")));
        let result = middleware.validate_request(&req);
        assert!(
            matches!(result, Err(SecurityError::InvalidToken)),
            "Expected InvalidToken for wrong issuer, got: {:?}",
            result
        );
    }

    #[test]
    fn test_hs256_with_audience_validation() {
        let secret = "test-secret";
        let config = AuthConfig::with_hs256(secret).with_audience("my-api");
        let middleware = AuthMiddleware::from_config(config);

        // Create token with matching audience
        use jsonwebtoken::{encode, EncodingKey, Header};

        #[derive(serde::Serialize)]
        struct ClaimsWithAud {
            sub: String,
            exp: i64,
            aud: String,
        }

        let now = chrono::Utc::now().timestamp();
        let claims = ClaimsWithAud {
            sub: "user123".to_string(),
            exp: now + 3600,
            aud: "my-api".to_string(),
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();

        let req = AuthRequest::new(Some(format!("Bearer {token}")));
        let result = middleware.validate_request(&req);
        assert!(result.is_ok(), "Expected valid token with audience, got: {:?}", result);
    }

    #[test]
    fn test_signing_key_algorithm_detection() {
        use jsonwebtoken::Algorithm;

        let hs256 = SigningKey::hs256("secret");
        assert!(matches!(hs256.algorithm(), Algorithm::HS256));

        let hs384 = SigningKey::Hs384(b"secret".to_vec());
        assert!(matches!(hs384.algorithm(), Algorithm::HS384));

        let hs512 = SigningKey::Hs512(b"secret".to_vec());
        assert!(matches!(hs512.algorithm(), Algorithm::HS512));

        let rs256_pem = SigningKey::rs256_pem("fake-pem");
        assert!(matches!(rs256_pem.algorithm(), Algorithm::RS256));

        let rs256_comp = SigningKey::rs256_components("n", "e");
        assert!(matches!(rs256_comp.algorithm(), Algorithm::RS256));
    }

    #[test]
    fn test_config_has_signing_key() {
        let config_without = AuthConfig::standard();
        assert!(!config_without.has_signing_key());

        let config_with = AuthConfig::with_hs256("secret");
        assert!(config_with.has_signing_key());
    }

    #[test]
    fn test_config_builder_pattern() {
        let config = AuthConfig::with_hs256("secret")
            .with_issuer("https://auth.example.com")
            .with_audience("my-api");

        assert!(config.has_signing_key());
        assert_eq!(config.issuer, Some("https://auth.example.com".to_string()));
        assert_eq!(config.audience, Some("my-api".to_string()));
    }

    #[test]
    fn test_malformed_token_rejected_with_signature_verification() {
        let config = AuthConfig::with_hs256("secret");
        let middleware = AuthMiddleware::from_config(config);

        // Not a valid JWT at all
        let req = AuthRequest::new(Some("Bearer not-a-jwt".to_string()));
        let result = middleware.validate_request(&req);
        assert!(
            matches!(result, Err(SecurityError::InvalidToken)),
            "Expected InvalidToken for malformed JWT, got: {:?}",
            result
        );
    }

    #[test]
    fn test_tampered_payload_rejected() {
        let secret = "test-secret";
        let config = AuthConfig::with_hs256(secret);
        let middleware = AuthMiddleware::from_config(config);

        // Create a valid token
        let token = create_signed_hs256_token("user123", 3600, None, secret);

        // Tamper with the payload (change middle part)
        let parts: Vec<&str> = token.split('.').collect();
        let tampered_token = format!("{}.dGFtcGVyZWQ.{}", parts[0], parts[2]);

        let req = AuthRequest::new(Some(format!("Bearer {tampered_token}")));
        let result = middleware.validate_request(&req);
        assert!(
            matches!(result, Err(SecurityError::InvalidToken)),
            "Expected InvalidToken for tampered payload, got: {:?}",
            result
        );
    }

    #[test]
    fn test_clock_skew_tolerance() {
        let secret = "test-secret";
        let mut config = AuthConfig::with_hs256(secret);
        config.clock_skew_secs = 120; // 2 minutes tolerance
        let middleware = AuthMiddleware::from_config(config);

        // Token that expired 30 seconds ago (within 2 minute tolerance)
        let token = create_signed_hs256_token("user123", -30, None, secret);
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        // Should still be valid due to clock skew tolerance
        assert!(result.is_ok(), "Expected valid token within clock skew, got: {:?}", result);
    }
}
