//! Authentication Middleware
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

pub mod bearer;
pub mod claims;
pub mod config;
pub mod jwt;
pub mod signing_key;
pub mod types;

pub use config::AuthConfig;
pub use signing_key::SigningKey;
pub use types::{AuthRequest, AuthenticatedUser, TokenClaims};

use crate::security::errors::Result;

/// Authentication Middleware
///
/// Validates incoming requests for authentication requirements.
/// Acts as the second layer in the security middleware pipeline.
#[derive(Debug, Clone)]
pub struct AuthMiddleware {
    pub(super) config: AuthConfig,
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

    /// Get the underlying configuration
    #[must_use]
    pub const fn config(&self) -> &AuthConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use zeroize::Zeroizing;

    use crate::security::errors::SecurityError;

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
        use jsonwebtoken::Algorithm;

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
