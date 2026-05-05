//! Authentication middleware implementation.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use jsonwebtoken::{Validation, decode};

use super::{
    config::AuthConfig,
    signing_key::SigningKey,
    types::{AuthRequest, AuthenticatedUser, JwtClaims, TokenClaims},
};
use crate::security::errors::{Result, SecurityError};

/// Authentication Middleware
///
/// Validates incoming requests for authentication requirements.
/// Acts as the second layer in the security middleware pipeline.
#[derive(Debug, Clone)]
pub struct AuthMiddleware {
    config: AuthConfig,
}

impl AuthMiddleware {
    /// Create a new authentication middleware from configuration.
    ///
    /// Emits a warning when `required = true` but no signing key is configured,
    /// because JWT signature verification will be disabled in that case.
    #[must_use]
    pub fn from_config(config: AuthConfig) -> Self {
        if config.required && config.signing_key.is_none() {
            tracing::warn!(
                "AuthMiddleware: required=true but no signing_key configured — \
                 JWT signatures will NOT be verified"
            );
        }
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

    /// Validate authentication in a request.
    ///
    /// Performs validation checks in order:
    /// 1. Extract token from Authorization header
    /// 2. Validate token signature (if signing key configured)
    /// 3. Check token expiry (exp claim)
    /// 4. Validate issuer/audience claims (if configured)
    /// 5. Extract required claims (sub)
    /// 6. Extract optional claims (scope, aud, iss)
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError::AuthRequired`] if no Authorization header or
    /// token is missing. Returns [`SecurityError::InvalidToken`] if the token
    /// signature, expiry, issuer, or audience is invalid. Returns
    /// [`SecurityError::TokenMissingClaim`] if a required claim is absent.
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

        // Configure audience validation.
        // SECURITY: `validate_aud = true` is the default in jsonwebtoken; we must
        // NOT override it to `false` when no audience is configured, as that would
        // silently accept tokens issued for any service (cross-service token replay).
        // When no audience is pinned, any non-empty `aud` claim is accepted — callers
        // should set `audience` in config to restrict this further.
        if let Some(ref audience) = self.config.audience {
            validation.set_audience(&[audience]);
        }
        // `validation.validate_aud` remains `true` (the library default) when no
        // specific audience is configured.

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
                },
                jsonwebtoken::errors::ErrorKind::InvalidSignature => {
                    SecurityError::JwtSignatureInvalid
                },
                jsonwebtoken::errors::ErrorKind::InvalidIssuer => {
                    SecurityError::JwtIssuerMismatch {
                        expected: self
                            .config
                            .issuer
                            .clone()
                            .unwrap_or_else(|| "(not configured)".to_string()),
                    }
                },
                jsonwebtoken::errors::ErrorKind::InvalidAudience => {
                    SecurityError::JwtAudienceMismatch {
                        expected: self
                            .config
                            .audience
                            .clone()
                            .unwrap_or_else(|| "(not configured)".to_string()),
                    }
                },
                jsonwebtoken::errors::ErrorKind::InvalidAlgorithm => {
                    SecurityError::InvalidTokenAlgorithm {
                        algorithm: format!("{:?}", signing_key.algorithm()),
                    }
                },
                jsonwebtoken::errors::ErrorKind::MissingRequiredClaim(claim) => {
                    SecurityError::TokenMissingClaim {
                        claim: claim.clone(),
                    }
                },
                _ => SecurityError::InvalidToken,
            }
        })?;

        let claims = token_data.claims;

        // Extract scopes (supports multiple formats)
        let scopes = self.extract_scopes_from_jwt_claims(&claims);

        // Extract user ID (required)
        let user_id_str = claims.sub.ok_or(SecurityError::TokenMissingClaim {
            claim: "sub".to_string(),
        })?;
        let user_id = crate::types::UserId::new(user_id_str);

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
            extra_claims: claims.extra,
        })
    }

    /// Extract scopes from JWT claims.
    ///
    /// Supports multiple formats:
    /// - `scope`: space-separated string (`OAuth2` standard)
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
        let user_id_str = claims.sub.ok_or(SecurityError::TokenMissingClaim {
            claim: "sub".to_string(),
        })?;
        let user_id = crate::types::UserId::new(user_id_str);

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
            extra_claims: HashMap::new(),
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
    pub(crate) fn validate_token_structure(&self, token: &str) -> Result<()> {
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
    /// For testing, we accept a special test token format: "`test:{json_payload`}"
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
        let aud = json["aud"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());
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
