//! Tests for `security/auth_middleware/types.rs`.

mod authenticated_user_tests {
    use std::collections::HashMap;

    use chrono::Utc;

    use crate::security::auth_middleware::types::AuthenticatedUser;

    #[test]
    fn test_user_has_scope() {
        let user = AuthenticatedUser {
            user_id:      "user123".into(),
            scopes:       vec!["read".to_string(), "write".to_string()],
            expires_at:   Utc::now() + chrono::Duration::hours(1),
            email:        None,
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
            user_id:      "user123".into(),
            scopes:       vec![],
            expires_at:   Utc::now() + chrono::Duration::hours(1),
            email:        None,
            display_name: None,
            extra_claims: HashMap::new(),
        };

        assert!(!user.is_expired());
    }

    #[test]
    fn test_user_is_expired() {
        let user = AuthenticatedUser {
            user_id:      "user123".into(),
            scopes:       vec![],
            expires_at:   Utc::now() - chrono::Duration::hours(1),
            email:        None,
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
            user_id:      "user123".into(),
            scopes:       vec![],
            expires_at:   Utc::now() + chrono::Duration::hours(1),
            email:        None,
            display_name: None,
            extra_claims: HashMap::new(),
        };

        let display_str = user.to_string();
        assert!(display_str.contains("user123"));
        assert!(display_str.contains("UTC"));
    }
}
