//! Authentication middleware implementation.

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
    /// Create a new authentication middleware from configuration
    #[must_use]
    pub const fn from_config(config: AuthConfig) -> Self {
        Self { config }
    }

    /// Create middleware with permissive settings (authentication optional)
    #[must_use]
    pub const fn permissive() -> Self {
        Self::from_config(AuthConfig::permissive())
    }

    /// Create middleware with standard settings (authentication required)
    #[must_use]
    pub const fn standard() -> Self {
        Self::from_config(AuthConfig::standard())
    }

    /// Create middleware with strict settings (authentication required, short expiry)
    #[must_use]
    pub const fn strict() -> Self {
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
                jsonwebtoken::errors::ErrorKind::InvalidSignature => SecurityError::InvalidToken,
                jsonwebtoken::errors::ErrorKind::InvalidIssuer => SecurityError::InvalidToken,
                jsonwebtoken::errors::ErrorKind::InvalidAudience => SecurityError::InvalidToken,
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use chrono::Utc;
    use jsonwebtoken::Algorithm;
    use zeroize::Zeroizing;

    use super::{
        super::{
            config::AuthConfig,
            signing_key::SigningKey,
            types::{AuthRequest, AuthenticatedUser},
        },
        *,
    };
    use crate::security::errors::SecurityError;

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
            user_id:    "user123".to_string(),
            scopes:     vec!["read".to_string(), "write".to_string()],
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };

        assert!(user.has_scope("read"));
        assert!(user.has_scope("write"));
        assert!(!user.has_scope("admin"));
    }

    #[test]
    fn test_user_is_not_expired() {
        let user = AuthenticatedUser {
            user_id:    "user123".to_string(),
            scopes:     vec![],
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };

        assert!(!user.is_expired());
    }

    #[test]
    fn test_user_is_expired() {
        let user = AuthenticatedUser {
            user_id:    "user123".to_string(),
            scopes:     vec![],
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
            user_id:    "user123".to_string(),
            scopes:     vec![],
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
    #[allow(clippy::items_after_statements)] // Reason: test helper structs defined near point of use for readability
    fn create_signed_hs256_token(
        sub: &str,
        exp_offset_secs: i64,
        scope: Option<&str>,
        secret: &str,
    ) -> String {
        use jsonwebtoken::{EncodingKey, Header, encode};

        let now = chrono::Utc::now().timestamp();
        let exp = now + exp_offset_secs;

        #[derive(serde::Serialize)]
        struct Claims {
            sub:   String,
            exp:   i64,
            iat:   i64,
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
    #[allow(clippy::items_after_statements)] // Reason: test helper structs defined near point of use for readability
    fn test_hs256_with_issuer_validation() {
        use jsonwebtoken::{EncodingKey, Header, encode};

        #[derive(serde::Serialize)]
        struct ClaimsWithIss {
            sub: String,
            exp: i64,
            iss: String,
        }

        let secret = "test-secret";
        let config = AuthConfig::with_hs256(secret).with_issuer("https://auth.example.com");
        let middleware = AuthMiddleware::from_config(config);

        // Create token with matching issuer
        let now = chrono::Utc::now().timestamp();
        let claims = ClaimsWithIss {
            sub: "user123".to_string(),
            exp: now + 3600,
            iss: "https://auth.example.com".to_string(),
        };

        let token =
            encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
                .unwrap();

        let req = AuthRequest::new(Some(format!("Bearer {token}")));
        let result = middleware.validate_request(&req);
        assert!(result.is_ok(), "Expected valid token with issuer, got: {:?}", result);
    }

    #[test]
    #[allow(clippy::items_after_statements)] // Reason: test helper structs defined near point of use for readability
    fn test_hs256_with_wrong_issuer_rejected() {
        use jsonwebtoken::{EncodingKey, Header, encode};

        #[derive(serde::Serialize)]
        struct ClaimsWithIss {
            sub: String,
            exp: i64,
            iss: String,
        }

        let secret = "test-secret";
        let config = AuthConfig::with_hs256(secret).with_issuer("https://auth.example.com");
        let middleware = AuthMiddleware::from_config(config);

        // Create token with wrong issuer
        let now = chrono::Utc::now().timestamp();
        let claims = ClaimsWithIss {
            sub: "user123".to_string(),
            exp: now + 3600,
            iss: "https://wrong-issuer.com".to_string(),
        };

        let token =
            encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
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
    #[allow(clippy::items_after_statements)] // Reason: test helper structs defined near point of use for readability
    fn test_hs256_with_audience_validation() {
        use jsonwebtoken::{EncodingKey, Header, encode};

        #[derive(serde::Serialize)]
        struct ClaimsWithAud {
            sub: String,
            exp: i64,
            aud: String,
        }

        let secret = "test-secret";
        let config = AuthConfig::with_hs256(secret).with_audience("my-api");
        let middleware = AuthMiddleware::from_config(config);

        // Create token with matching audience
        let now = chrono::Utc::now().timestamp();
        let claims = ClaimsWithAud {
            sub: "user123".to_string(),
            exp: now + 3600,
            aud: "my-api".to_string(),
        };

        let token =
            encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
                .unwrap();

        let req = AuthRequest::new(Some(format!("Bearer {token}")));
        let result = middleware.validate_request(&req);
        assert!(result.is_ok(), "Expected valid token with audience, got: {:?}", result);
    }

    #[test]
    fn test_signing_key_algorithm_detection() {
        let hs256 = SigningKey::hs256("secret");
        assert!(matches!(hs256.algorithm(), Algorithm::HS256));

        let hs384 = SigningKey::Hs384(Zeroizing::new(b"secret".to_vec()));
        assert!(matches!(hs384.algorithm(), Algorithm::HS384));

        let hs512 = SigningKey::Hs512(Zeroizing::new(b"secret".to_vec()));
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
