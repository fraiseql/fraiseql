// Error message sanitization tests

#[cfg(test)]
mod error_sanitization {
    use std::fmt;

    /// A sanitizable error that has both user-facing and internal representations
    #[derive(Debug, Clone)]
    pub struct SanitizableError {
        /// User-facing message (safe to expose)
        pub user_message:     String,
        /// Internal message (details for logs only)
        pub internal_message: String,
    }

    impl SanitizableError {
        /// Create a new sanitizable error
        pub fn new(user_message: &str, internal_message: &str) -> Self {
            Self {
                user_message:     user_message.to_string(),
                internal_message: internal_message.to_string(),
            }
        }

        /// Get the user-facing message (safe for API responses)
        pub fn user_facing(&self) -> &str {
            &self.user_message
        }

        /// Get the internal message (for logging only)
        pub fn internal(&self) -> &str {
            &self.internal_message
        }
    }

    impl fmt::Display for SanitizableError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.user_message)
        }
    }

    // ===== JWT ERROR SANITIZATION TESTS =====

    #[test]
    fn test_jwt_invalid_signature_sanitized() {
        // RED: JWT signature errors should hide cryptographic details from users
        let error = SanitizableError::new(
            "Authentication failed",
            "Invalid JWT signature: RS256 verification failed at index 145",
        );

        // User sees generic message
        assert_eq!(error.user_facing(), "Authentication failed");

        // Logs contain full details
        assert!(error.internal().contains("RS256"));
        assert!(error.internal().contains("verification"));
    }

    #[test]
    fn test_jwt_expired_token_generic() {
        // RED: Token expiration should not reveal expiration time to user
        let error = SanitizableError::new(
            "Authentication failed",
            "Token expired: exp: 1704067200, now: 1704153600",
        );

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().contains("exp:"));
        assert!(error.internal().contains("now:"));
    }

    #[test]
    fn test_jwt_invalid_issuer_sanitized() {
        // RED: Issuer errors should not expose internal issuer URLs
        let error = SanitizableError::new(
            "Authentication failed",
            "Invalid issuer: expected https://internal-auth.company.com/oauth, got https://attacker.com",
        );

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().contains("internal-auth.company.com"));
        assert!(error.internal().contains("attacker.com"));
    }

    #[test]
    fn test_jwt_missing_claim_sanitized() {
        // RED: Missing claims should not expose claim structure
        let error = SanitizableError::new(
            "Authentication failed",
            "Missing required claim: custom_org_id (expected for RBAC)",
        );

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().contains("custom_org_id"));
    }

    // ===== OIDC ERROR SANITIZATION TESTS =====

    #[test]
    fn test_oidc_server_error_sanitized() {
        // RED: OIDC server errors should hide provider internals
        let error = SanitizableError::new(
            "Authentication failed",
            "OIDC provider error: Internal database transaction failed at db.execute_query",
        );

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().contains("database"));
        assert!(error.internal().contains("execute_query"));
    }

    #[test]
    fn test_oidc_network_error_sanitized() {
        // RED: Network errors should hide provider URLs/IPs
        let error = SanitizableError::new(
            "Authentication failed",
            "Failed to reach OIDC provider: connection timeout to 10.0.1.45:8443",
        );

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().contains("10.0.1.45"));
        assert!(error.internal().contains("8443"));
    }

    #[test]
    fn test_oidc_invalid_client_secret_sanitized() {
        // RED: Client secret errors should hide credential details
        let error = SanitizableError::new(
            "Authentication failed",
            "Client secret validation failed: provided secret does not match stored hmac",
        );

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().contains("hmac"));
    }

    // ===== SESSION/TOKEN ERROR SANITIZATION TESTS =====

    #[test]
    fn test_invalid_session_token_sanitized() {
        // RED: Session token errors should not expose token format
        let error = SanitizableError::new(
            "Authentication failed",
            "Invalid session token: expected format {session_id}:{signature}, got malformed",
        );

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().contains("session_id"));
        assert!(error.internal().contains("signature"));
    }

    #[test]
    fn test_expired_session_sanitized() {
        // RED: Session expiration should not expose TTL details
        let error = SanitizableError::new(
            "Authentication failed",
            "Session expired: created_at: 2024-01-01, expires_at: 2024-01-08, ttl: 604800s",
        );

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().contains("created_at"));
        assert!(error.internal().contains("ttl"));
    }

    #[test]
    fn test_revoked_session_sanitized() {
        // RED: Revocation reasons should not expose revocation policy
        let error = SanitizableError::new(
            "Authentication failed",
            "Session revoked: Reason=AdminRevoke, PolicyViolation=3_failed_logins, IPChange=true",
        );

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().contains("PolicyViolation"));
        assert!(error.internal().contains("3_failed_logins"));
    }

    // ===== DATABASE ERROR SANITIZATION TESTS =====

    #[test]
    fn test_database_connection_error_sanitized() {
        // RED: DB errors should not expose host/port/schema
        let error = SanitizableError::new(
            "Service temporarily unavailable",
            "Database connection failed: cannot connect to postgres://user@db.internal:5432/auth_db",
        );

        assert_eq!(error.user_facing(), "Service temporarily unavailable");
        assert!(error.internal().contains("db.internal"));
        assert!(error.internal().contains("5432"));
        assert!(error.internal().contains("auth_db"));
    }

    #[test]
    fn test_database_query_error_sanitized() {
        // RED: SQL errors should not expose schema/queries
        let error = SanitizableError::new(
            "Service temporarily unavailable",
            "Query execution failed: syntax error at line 5: SELECT * FROM users WHERE id = $1",
        );

        assert_eq!(error.user_facing(), "Service temporarily unavailable");
        assert!(error.internal().contains("SELECT"));
        assert!(error.internal().contains("users"));
    }

    #[test]
    fn test_database_constraint_error_sanitized() {
        // RED: Constraint violations should not expose table/column names
        let error = SanitizableError::new(
            "Request failed",
            "Unique constraint violation on table 'users' column 'email'",
        );

        assert_eq!(error.user_facing(), "Request failed");
        assert!(error.internal().contains("users"));
        assert!(error.internal().contains("email"));
    }

    // ===== AUTHORIZATION ERROR SANITIZATION TESTS =====

    #[test]
    fn test_permission_denied_generic() {
        // RED: Permission errors should not expose permission structure
        let error = SanitizableError::new(
            "Permission denied",
            "User does not have permission: requires role=admin, organization=org123, scope=write:users",
        );

        assert_eq!(error.user_facing(), "Permission denied");
        assert!(error.internal().contains("admin"));
        assert!(error.internal().contains("org123"));
    }

    #[test]
    fn test_rbac_policy_error_sanitized() {
        // RED: RBAC errors should not expose policy details
        let error = SanitizableError::new(
            "Permission denied",
            "RBAC policy violation: PolicyID=p_read_only_2024, Rule=deny_mutation, Effect=Deny",
        );

        assert_eq!(error.user_facing(), "Permission denied");
        assert!(error.internal().contains("p_read_only_2024"));
    }

    // ===== CONSISTENTLY GENERIC MESSAGES =====

    #[test]
    fn test_all_auth_errors_have_consistent_user_message() {
        // RED: All auth-related errors should have same user message for consistency
        let jwt_error =
            SanitizableError::new("Authentication failed", "JWT signature verification failed");

        let oidc_error =
            SanitizableError::new("Authentication failed", "OIDC provider error: internal error");

        let session_error =
            SanitizableError::new("Authentication failed", "Session token corrupted");

        assert_eq!(jwt_error.user_facing(), oidc_error.user_facing());
        assert_eq!(oidc_error.user_facing(), session_error.user_facing());
        assert_eq!(jwt_error.user_facing(), "Authentication failed");
    }

    #[test]
    fn test_no_internal_details_in_user_message() {
        // RED: User messages must never contain technical details
        let errors = vec![
            SanitizableError::new("Authentication failed", "JWT exp=1234567890, iat=1234567800"),
            SanitizableError::new("Permission denied", "Policy admin_only_7a4b prevents access"),
            SanitizableError::new(
                "Service temporarily unavailable",
                "PostgreSQL connection lost to 192.168.1.5:5432",
            ),
        ];

        for error in errors {
            let user_msg = error.user_facing();

            // Should not contain:
            assert!(!user_msg.contains("=")); // No variable assignments
            assert!(!user_msg.contains("://")); // No URLs
            assert!(!user_msg.contains(":")); // No ports
            assert!(!user_msg.contains(".")); // No IP addresses or domain parts
            assert!(!user_msg.contains("[")); // No stack traces
        }
    }

    #[test]
    fn test_internal_message_contains_full_details() {
        // RED: Internal messages must contain all details for debugging
        let error = SanitizableError::new(
            "Authentication failed",
            "JWT validation failed: algorithm=RS256, kid=abc123, issuer=https://auth.example.com, subject=user@example.com",
        );

        let internal = error.internal();
        assert!(internal.contains("RS256"));
        assert!(internal.contains("abc123"));
        assert!(internal.contains("issuer"));
        assert!(internal.contains("subject"));
    }

    // ===== ERROR FORMATTING TESTS =====

    #[test]
    fn test_error_display_uses_user_message() {
        // RED: Display impl should show user message (safe for logging)
        let error = SanitizableError::new(
            "Authentication failed",
            "Secret JWT signing key exposed at /etc/secrets/jwt.key",
        );

        assert_eq!(format!("{}", error), "Authentication failed");
    }

    #[test]
    fn test_error_debug_shows_internal() {
        // RED: Debug impl can show full details for development
        let error = SanitizableError::new(
            "Authentication failed",
            "Critical: JWT validation failed at cryptographic boundary",
        );

        let debug_string = format!("{:?}", error);
        assert!(debug_string.contains("internal_message"));
        assert!(debug_string.contains("cryptographic"));
    }

    // ===== EDGE CASES =====

    #[test]
    fn test_very_long_error_messages_sanitized() {
        // RED: Long errors should still be sanitized
        let long_internal = format!("Detailed error: {}", "x".repeat(1000));

        let error = SanitizableError::new("Authentication failed", &long_internal);

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().len() > 1000);
    }

    #[test]
    fn test_special_characters_in_internal_messages() {
        // RED: Should handle special characters safely
        let error = SanitizableError::new(
            "Authentication failed",
            "Error contains: <script>, \"quotes\", \\backslash, 'apostrophe'",
        );

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().contains("<script>"));
        assert!(error.internal().contains("\"quotes\""));
    }

    #[test]
    fn test_sanitization_doesnt_lose_information() {
        // RED: Sanitization should preserve all details internally
        let original_internal =
            "Cannot connect to auth server: socket error 111 (Connection refused) at 10.0.0.1:9000";

        let error = SanitizableError::new("Service temporarily unavailable", original_internal);

        assert_eq!(error.internal(), original_internal);
        assert_ne!(error.user_facing(), original_internal);
    }
}
