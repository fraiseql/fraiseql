//! Authentication middleware implementation.

use chrono::{DateTime, Utc};
use jsonwebtoken::{Validation, decode};

use super::{
    config::AuthConfig,
    signing_key::SigningKey,
    types::{AuthRequest, AuthenticatedUser, JwtClaims, extract_claim_string, extract_name_string},
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
    #[must_use]
    pub const fn from_config(config: AuthConfig) -> Self {
        Self { config }
    }

    /// Validate authentication in a request.
    ///
    /// Performs validation checks in order:
    /// 1. Extract token from Authorization header
    /// 2. Validate token signature
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

        // Check 2: Validate the token against the configured signing key
        self.validate_token_with_signature(&token, &self.config.signing_key)
    }

    /// Validate token with cryptographic signature verification.
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

        let email = claims.extra.get("email").and_then(extract_claim_string);
        let display_name = claims.extra.get("name").and_then(extract_name_string);

        Ok(AuthenticatedUser {
            user_id,
            scopes,
            expires_at,
            email,
            display_name,
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

    /// Extract token from the authorization header
    fn extract_token(&self, req: &AuthRequest) -> Result<String> {
        // If auth is not required and no header present, that's OK
        if !self.config.required && req.authorization_header.is_none() {
            return Err(SecurityError::AuthRequired); // Will be handled differently
        }

        req.extract_bearer_token()
    }

    /// Get the underlying configuration
    #[must_use]
    pub const fn config(&self) -> &AuthConfig {
        &self.config
    }
}
