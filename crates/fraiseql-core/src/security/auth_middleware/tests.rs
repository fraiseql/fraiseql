//! Tests for `security/auth_middleware/` modules.

mod middleware_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::collections::HashMap;

    use chrono::Utc;
    use jsonwebtoken::Algorithm;
    use zeroize::Zeroizing;

    use crate::security::{
        auth_middleware::{
            AuthMiddleware,
            config::AuthConfig,
            signing_key::SigningKey,
            types::{AuthRequest, AuthenticatedUser},
        },
        errors::SecurityError,
    };

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

        let extracted = req
            .extract_bearer_token()
            .unwrap_or_else(|e| panic!("expected bearer token extraction to succeed: {e}"));
        assert_eq!(extracted, token);
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
        result.unwrap_or_else(|e| panic!("expected valid token structure: {e}"));
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
        result.unwrap_or_else(|e| panic!("expected valid non-expired token to pass: {e}"));
    }

    #[test]
    fn test_expired_token_rejected() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user123", -3600, None); // 1 hour ago
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let result = middleware.validate_request(&req);
        assert!(matches!(result, Err(SecurityError::TokenExpired { .. })));
    }

    /// Sentinel: a token that expired 1 second ago must be rejected.
    ///
    /// Kills the `<= → >` and `<= → never-expire` mutations on the expiry check:
    /// `if expires_at <= Utc::now()`.
    ///
    /// Note: testing the exact `expires_at == now` boundary deterministically would
    /// require clock injection into `AuthMiddleware`, which is not yet supported.
    /// The ±1-second cases are tested here; the zero-offset case is inherently racy.
    #[test]
    fn test_token_expired_one_second_ago_is_rejected() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user123", -1, None); // expired 1 second ago
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        assert!(
            matches!(middleware.validate_request(&req), Err(SecurityError::TokenExpired { .. })),
            "token expired 1s ago must be rejected"
        );
    }

    /// Sentinel: a token expiring 60 seconds from now must be accepted.
    ///
    /// Complements `test_token_expired_one_second_ago_is_rejected` to pin the valid
    /// side of the expiry boundary.
    #[test]
    fn test_token_expiring_soon_is_accepted() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user123", 60, None); // expires in 60 seconds
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        assert!(
            middleware.validate_request(&req).is_ok(),
            "token expiring in 60s must be accepted"
        );
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

        let user = middleware
            .validate_request(&req)
            .unwrap_or_else(|e| panic!("expected user_id extraction to succeed: {e}"));
        assert_eq!(user.user_id.as_str(), "user_12345");
    }

    #[test]
    fn test_scopes_extracted_from_token() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user123", 3600, Some("read write admin"));
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let user = middleware
            .validate_request(&req)
            .unwrap_or_else(|e| panic!("expected scope extraction to succeed: {e}"));
        assert_eq!(user.scopes, vec!["read", "write", "admin"]);
    }

    #[test]
    fn test_empty_scopes_when_scope_claim_absent() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user123", 3600, None);
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let user = middleware
            .validate_request(&req)
            .unwrap_or_else(|e| panic!("expected token without scopes to be valid: {e}"));
        assert!(user.scopes.is_empty(), "expected empty scopes, got: {:?}", user.scopes);
    }

    #[test]
    fn test_expires_at_extracted_correctly() {
        let middleware = AuthMiddleware::standard();
        let offset_secs = 7200; // 2 hours

        let token = create_test_token("user123", offset_secs, None);
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let user = middleware
            .validate_request(&req)
            .unwrap_or_else(|e| panic!("expected expiry extraction to succeed: {e}"));
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
            user_id: "user123".into(),
            scopes: vec!["read".to_string(), "write".to_string()],
            expires_at: Utc::now() + chrono::Duration::hours(1),
            email: None,
            display_name: None,
            extra_claims: HashMap::new(),
        };

        assert!(user.has_scope("read"));
        assert!(user.has_scope("write"));
        assert!(!user.has_scope("admin"));
    }

    #[test]
    fn test_user_is_not_expired() {
        let user = AuthenticatedUser {
            user_id: "user123".into(),
            scopes: vec![],
            expires_at: Utc::now() + chrono::Duration::hours(1),
            email: None,
            display_name: None,
            extra_claims: HashMap::new(),
        };

        assert!(!user.is_expired());
    }

    #[test]
    fn test_user_is_expired() {
        let user = AuthenticatedUser {
            user_id: "user123".into(),
            scopes: vec![],
            expires_at: Utc::now() - chrono::Duration::hours(1),
            email: None,
            display_name: None,
            extra_claims: HashMap::new(),
        };

        assert!(user.is_expired());
    }

    #[test]
    fn test_user_ttl_calculation() {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::hours(2);
        let user = AuthenticatedUser {
            user_id: "user123".into(),
            scopes: vec![],
            expires_at,
            email: None,
            display_name: None,
            extra_claims: HashMap::new(),
        };

        let ttl = user.ttl_secs();
        // Should be approximately 7200 seconds (2 hours)
        assert!((7195..=7205).contains(&ttl));
    }

    #[test]
    fn test_user_display() {
        let user = AuthenticatedUser {
            user_id: "user123".into(),
            scopes: vec![],
            expires_at: Utc::now() + chrono::Duration::hours(1),
            email: None,
            display_name: None,
            extra_claims: HashMap::new(),
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

        let user = middleware
            .validate_request(&req)
            .unwrap_or_else(|e| panic!("expected whitespace-heavy scopes to parse: {e}"));
        // split_whitespace handles multiple spaces correctly
        assert_eq!(user.scopes.len(), 3);
    }

    #[test]
    fn test_single_scope_parsed_correctly() {
        let middleware = AuthMiddleware::standard();
        let token = create_test_token("user123", 3600, Some("read"));
        let req = AuthRequest::new(Some(format!("Bearer {token}")));

        let user = middleware
            .validate_request(&req)
            .unwrap_or_else(|e| panic!("expected single scope to parse: {e}"));
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
        assert_eq!(user.user_id.as_str(), "user123");
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
            matches!(result, Err(SecurityError::JwtSignatureInvalid)),
            "Expected JwtSignatureInvalid for wrong signature, got: {:?}",
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
            matches!(result, Err(SecurityError::JwtIssuerMismatch { .. })),
            "Expected JwtIssuerMismatch for wrong issuer, got: {:?}",
            result
        );
    }

    #[test]
    #[allow(clippy::items_after_statements)] // Reason: test helper structs defined near point of use for readability
    fn test_jwt_issuer_mismatch_error_contains_expected_issuer_and_word_issuer() {
        use jsonwebtoken::{EncodingKey, Header, encode};

        #[derive(serde::Serialize)]
        struct ClaimsWithIss {
            sub: String,
            exp: i64,
            iss: String,
        }

        let secret = "test-secret";
        let expected_issuer = "https://auth.example.com";
        let config = AuthConfig::with_hs256(secret).with_issuer(expected_issuer);
        let middleware = AuthMiddleware::from_config(config);

        // Token signed with the right secret but wrong issuer.
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
        let err = result.expect_err("expected error for wrong issuer");
        let msg = err.to_string();

        assert!(
            msg.contains(expected_issuer),
            "error message must contain expected issuer '{expected_issuer}': {msg}"
        );
        assert!(msg.contains("issuer"), "error message must contain 'issuer': {msg}");
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
            matches!(result, Err(SecurityError::JwtSignatureInvalid)),
            "Expected JwtSignatureInvalid for tampered payload, got: {:?}",
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
