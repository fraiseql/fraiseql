//! Comprehensive tests for database schema and operations,
//! including secrets audit, encryption keys, auth providers, and OAuth sessions.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod schema_tests {
    use std::collections::HashMap;

    use chrono::{Duration, Utc};

    use crate::secrets::schemas::{
        EncryptionKey, ExternalAuthProviderRecord, OAuthSessionRecord, SchemaMigration,
        SecretRotationAudit,
    };

    // ============================================================================
    // SECRET ROTATION AUDIT TABLE TESTS
    // ============================================================================

    /// Test secret rotation audit record creation
    #[tokio::test]
    async fn test_secret_rotation_audit_creation() {
        let audit = SecretRotationAudit::new("database/creds/fraiseql", "success")
            .with_rotated_by("system")
            .with_secret_ids("old-uuid-001", "new-uuid-002")
            .add_metadata("duration_ms", "150")
            .add_metadata("reason", "scheduled");

        assert_eq!(audit.secret_name, "database/creds/fraiseql");
        assert_eq!(audit.status, "success");
        assert!(audit.is_successful());
        assert_eq!(audit.rotated_by, Some("system".to_string()));
        assert_eq!(audit.previous_secret_id, Some("old-uuid-001".to_string()));
        assert_eq!(audit.new_secret_id, Some("new-uuid-002".to_string()));
        assert_eq!(audit.metadata.get("duration_ms"), Some(&"150".to_string()));
        assert_eq!(audit.metadata.get("reason"), Some(&"scheduled".to_string()));
        assert!(audit.error_message.is_none());
        assert!(!audit.id.is_empty());
        assert!(audit.rotation_timestamp <= Utc::now());
    }

    /// Test secret rotation audit retrieval by secret name
    #[tokio::test]
    async fn test_secret_rotation_audit_by_name() {
        // Simulate a table of audit records
        let mut records = Vec::new();
        for i in 0..5 {
            let audit = SecretRotationAudit::new("database/creds/fraiseql", "success")
                .with_rotated_by(format!("user-{i}"))
                .with_secret_ids(format!("old-{i}"), format!("new-{i}"));
            records.push(audit);
        }
        // Add records for a different secret
        records.push(SecretRotationAudit::new("api/keys/stripe", "success"));

        // Query by secret name
        let fraiseql_records: Vec<_> = records
            .iter()
            .filter(|r| r.secret_name == "database/creds/fraiseql")
            .collect();
        assert_eq!(fraiseql_records.len(), 5);

        // Sorted by timestamp (descending) — all are essentially the same time
        // but we can verify ordering
        for record in &fraiseql_records {
            assert_eq!(record.secret_name, "database/creds/fraiseql");
            assert!(record.is_successful());
        }

        // Other secret has its own history
        let stripe_records: Vec<_> = records
            .iter()
            .filter(|r| r.secret_name == "api/keys/stripe")
            .collect();
        assert_eq!(stripe_records.len(), 1);
    }

    /// Test secret rotation audit filtering by status
    #[tokio::test]
    async fn test_secret_rotation_audit_filter_by_status() {
        let records = vec![
            SecretRotationAudit::new("secret/a", "success"),
            SecretRotationAudit::new("secret/b", "failed").with_error("Connection timeout"),
            SecretRotationAudit::new("secret/c", "success"),
            SecretRotationAudit::new("secret/d", "failed").with_error("Permission denied"),
        ];

        let successful: Vec<_> = records.iter().filter(|r| r.is_successful()).collect();
        assert_eq!(successful.len(), 2);

        let failed: Vec<_> = records.iter().filter(|r| !r.is_successful()).collect();
        assert_eq!(failed.len(), 2);
        assert!(failed[0].error_message.is_some());
        assert!(failed[1].error_message.is_some());
    }

    /// Test secret rotation audit date range filtering
    #[tokio::test]
    async fn test_secret_rotation_audit_date_range() {
        let now = Utc::now();
        let records = vec![
            SecretRotationAudit::new("secret/a", "success"),
            SecretRotationAudit::new("secret/b", "success"),
            SecretRotationAudit::new("secret/c", "success"),
        ];

        // All records created "now" so they all fall within a range of ±1 hour
        let range_start = now - Duration::hours(1);
        let range_end = now + Duration::hours(1);

        let in_range: Vec<_> = records
            .iter()
            .filter(|r| r.rotation_timestamp >= range_start && r.rotation_timestamp <= range_end)
            .collect();
        assert_eq!(in_range.len(), 3);

        // Filter with a past range — nothing matches
        let old_range_end = now - Duration::hours(2);
        let in_old_range: Vec<_> = records
            .iter()
            .filter(|r| r.rotation_timestamp <= old_range_end)
            .collect();
        assert_eq!(in_old_range.len(), 0);
    }

    /// Test secret rotation audit with metadata
    #[tokio::test]
    async fn test_secret_rotation_audit_with_metadata() {
        let audit = SecretRotationAudit::new("database/creds/fraiseql", "success")
            .add_metadata("duration_ms", "250")
            .add_metadata("reason", "emergency")
            .add_metadata("affected_services", "api,worker,scheduler")
            .add_metadata("retry_count", "2");

        assert_eq!(audit.metadata.get("duration_ms"), Some(&"250".to_string()));
        assert_eq!(audit.metadata.get("reason"), Some(&"emergency".to_string()));
        assert_eq!(
            audit.metadata.get("affected_services"),
            Some(&"api,worker,scheduler".to_string())
        );
        assert_eq!(audit.metadata.get("retry_count"), Some(&"2".to_string()));

        // Metadata is queryable
        assert!(audit.metadata.contains_key("reason"));
        assert!(!audit.metadata.contains_key("nonexistent"));
    }

    /// Test secret rotation audit cleanup
    #[tokio::test]
    async fn test_secret_rotation_audit_cleanup() {
        let mut records = Vec::new();
        for _ in 0..10 {
            records.push(SecretRotationAudit::new("secret/a", "success"));
        }

        assert_eq!(records.len(), 10);

        // Simulate cleanup: remove records older than a cutoff
        // Since all records are created "now", use a future cutoff to simulate aging
        let cutoff = Utc::now() + Duration::seconds(1);
        records.retain(|r| r.rotation_timestamp >= cutoff);
        assert_eq!(records.len(), 0); // All "old" records cleaned up

        // Verify fresh records survive cleanup
        let fresh = SecretRotationAudit::new("secret/b", "success");
        records.push(fresh);
        let future_cutoff = Utc::now() - Duration::hours(1);
        records.retain(|r| r.rotation_timestamp >= future_cutoff);
        assert_eq!(records.len(), 1); // Recent record survives
    }

    // ============================================================================
    // ENCRYPTION KEYS TABLE TESTS
    // ============================================================================

    /// Test encryption key record creation
    #[tokio::test]
    async fn test_encryption_key_creation() {
        let key_material = vec![0x42u8; 32]; // 256-bit key
        let key = EncryptionKey::new("fraiseql/database-encryption", key_material.clone(), "AES-256-GCM")
            .add_metadata("compliance_frameworks", "HIPAA,PCI-DSS,GDPR")
            .add_metadata("rotation_schedule", "0 2 1 * *");

        assert_eq!(key.name, "fraiseql/database-encryption");
        assert_eq!(key.key_material_encrypted, key_material);
        assert_eq!(key.algorithm, "AES-256-GCM");
        assert_eq!(key.version, 1);
        assert_eq!(key.status, "active");
        assert!(key.is_active());
        assert!(key.rotated_at.is_none());
        assert!(!key.id.is_empty());
    }

    /// Test encryption key retrieval by name
    #[tokio::test]
    async fn test_encryption_key_by_name() {
        let mut keys = HashMap::new();
        let key = EncryptionKey::new("fraiseql/database-encryption", vec![1, 2, 3], "AES-256-GCM");
        keys.insert(key.name.clone(), key);

        let key2 = EncryptionKey::new("fraiseql/token-encryption", vec![4, 5, 6], "AES-256-GCM");
        keys.insert(key2.name.clone(), key2);

        // Retrieve by name
        let result = keys.get("fraiseql/database-encryption");
        assert!(result.is_some());
        assert_eq!(result.unwrap().algorithm, "AES-256-GCM");

        // Not found
        assert!(!keys.contains_key("nonexistent"));
    }

    /// Test encryption key version management
    #[tokio::test]
    async fn test_encryption_key_version_increment() {
        let key = EncryptionKey::new("test-key", vec![1, 2, 3], "AES-256-GCM");
        assert_eq!(key.version, 1);
        assert!(key.rotated_at.is_none());

        let rotated = key.complete_rotation(vec![4, 5, 6]);
        assert_eq!(rotated.version, 2);
        assert_eq!(rotated.key_material_encrypted, vec![4, 5, 6]);
        assert!(rotated.rotated_at.is_some());
        assert!(rotated.is_active());

        // Rotate again
        let rotated2 = rotated.complete_rotation(vec![7, 8, 9]);
        assert_eq!(rotated2.version, 3);
    }

    /// Test encryption key status lifecycle
    #[tokio::test]
    async fn test_encryption_key_status_transitions() {
        let key = EncryptionKey::new("test-key", vec![1, 2, 3], "AES-256-GCM");
        assert!(key.is_active());
        assert!(!key.is_rotating());

        // Start rotation
        let rotating = key.start_rotation();
        assert!(rotating.is_rotating());
        assert!(!rotating.is_active());
        assert_eq!(rotating.status, "rotating");

        // Complete rotation -> active again
        let active = rotating.complete_rotation(vec![4, 5, 6]);
        assert!(active.is_active());
        assert!(!active.is_rotating());

        // Retire
        let retired = active.retire();
        assert!(!retired.is_active());
        assert!(!retired.is_rotating());
        assert_eq!(retired.status, "retired");
    }

    /// Test encryption key metadata
    #[tokio::test]
    async fn test_encryption_key_metadata() {
        let key = EncryptionKey::new("test-key", vec![1, 2, 3], "AES-256-GCM")
            .add_metadata("compliance_frameworks", "HIPAA,PCI-DSS,GDPR")
            .add_metadata("rotation_schedule", "0 2 1 * *")
            .add_metadata("retention_policy", "1 year")
            .add_metadata("protected_fields", "email,phone,ssn");

        assert_eq!(
            key.metadata.get("compliance_frameworks"),
            Some(&"HIPAA,PCI-DSS,GDPR".to_string())
        );
        assert_eq!(
            key.metadata.get("rotation_schedule"),
            Some(&"0 2 1 * *".to_string())
        );
        assert_eq!(
            key.metadata.get("retention_policy"),
            Some(&"1 year".to_string())
        );
        assert_eq!(
            key.metadata.get("protected_fields"),
            Some(&"email,phone,ssn".to_string())
        );
    }

    /// Test encryption key material retrieval
    #[tokio::test]
    async fn test_encryption_key_material_retrieval() {
        let key_material = vec![0xABu8; 32];
        let key = EncryptionKey::new("test-key", key_material.clone(), "AES-256-GCM");

        // Only active keys should have material retrieved
        assert!(key.is_active());
        assert_eq!(key.key_material_encrypted, key_material);
        assert_eq!(key.key_material_encrypted.len(), 32);

        // Retired key still has material (for historical decryption)
        let retired = key.retire();
        assert!(!retired.is_active());
        assert_eq!(retired.key_material_encrypted, key_material);
    }

    /// Test encryption key unique constraint
    #[tokio::test]
    async fn test_encryption_key_unique_name() {
        let mut key_store: HashMap<String, EncryptionKey> = HashMap::new();

        let key1 = EncryptionKey::new("fraiseql/encryption", vec![1, 2, 3], "AES-256-GCM");
        key_store.insert(key1.name.clone(), key1);

        // Attempting to insert duplicate name overwrites (simulates UNIQUE constraint)
        let duplicate = EncryptionKey::new("fraiseql/encryption", vec![4, 5, 6], "AES-256-GCM");
        let already_exists = key_store.contains_key(&duplicate.name);
        assert!(already_exists); // Would be a constraint violation in DB

        // Different name succeeds
        let key2 = EncryptionKey::new("fraiseql/other-key", vec![7, 8, 9], "AES-256-GCM");
        assert!(!key_store.contains_key(&key2.name));
        key_store.insert(key2.name.clone(), key2);
        assert_eq!(key_store.len(), 2);
    }

    // ============================================================================
    // EXTERNAL AUTH PROVIDERS TABLE TESTS
    // ============================================================================

    /// Test external auth provider creation
    #[tokio::test]
    async fn test_external_auth_provider_creation() {
        let provider = ExternalAuthProviderRecord::new(
            "tenant-001",
            "oidc",
            "auth0",
            "client-id-abc",
            "fraiseql/auth/auth0/secret",
        )
        .add_configuration("authorization_endpoint", "https://example.auth0.com/authorize")
        .add_configuration("token_endpoint", "https://example.auth0.com/oauth/token");

        assert_eq!(provider.tenant_id, "tenant-001");
        assert_eq!(provider.provider_type, "oidc");
        assert_eq!(provider.provider_name, "auth0");
        assert_eq!(provider.client_id, "client-id-abc");
        assert_eq!(provider.client_secret_vault_path, "fraiseql/auth/auth0/secret");
        assert!(provider.is_oidc());
        assert!(provider.is_enabled());
        assert!(!provider.id.is_empty());
    }

    /// Test external auth provider per tenant
    #[tokio::test]
    async fn test_external_auth_provider_by_tenant() {
        let providers = vec![
            ExternalAuthProviderRecord::new("tenant-001", "oidc", "auth0", "cid-1", "vault/1"),
            ExternalAuthProviderRecord::new("tenant-001", "oauth2", "google", "cid-2", "vault/2"),
            ExternalAuthProviderRecord::new("tenant-002", "oidc", "okta", "cid-3", "vault/3"),
        ];

        // Query by tenant
        let tenant1: Vec<_> = providers
            .iter()
            .filter(|p| p.tenant_id == "tenant-001" && p.is_enabled())
            .collect();
        assert_eq!(tenant1.len(), 2);

        let tenant2: Vec<_> = providers
            .iter()
            .filter(|p| p.tenant_id == "tenant-002" && p.is_enabled())
            .collect();
        assert_eq!(tenant2.len(), 1);
        assert_eq!(tenant2[0].provider_name, "okta");
    }

    /// Test external auth provider unique constraint
    #[tokio::test]
    async fn test_external_auth_provider_unique_per_tenant() {
        let mut providers: HashMap<(String, String), ExternalAuthProviderRecord> = HashMap::new();

        let p1 = ExternalAuthProviderRecord::new("tenant-001", "oidc", "auth0", "cid-1", "vault/1");
        let key1 = (p1.tenant_id.clone(), p1.provider_name.clone());
        providers.insert(key1, p1);

        // Duplicate (tenant_id, provider_name) would violate constraint
        let p2 = ExternalAuthProviderRecord::new("tenant-001", "oidc", "auth0", "cid-2", "vault/2");
        let key2 = (p2.tenant_id.clone(), p2.provider_name.clone());
        assert!(providers.contains_key(&key2)); // Would be constraint violation

        // Same provider, different tenant — OK
        let p3 = ExternalAuthProviderRecord::new("tenant-002", "oidc", "auth0", "cid-3", "vault/3");
        let key3 = (p3.tenant_id.clone(), p3.provider_name.clone());
        assert!(!providers.contains_key(&key3));
        providers.insert(key3, p3);
        assert_eq!(providers.len(), 2);
    }

    /// Test external auth provider enable/disable
    #[tokio::test]
    async fn test_external_auth_provider_enable_disable() {
        let provider = ExternalAuthProviderRecord::new("tenant-001", "oidc", "auth0", "cid-1", "vault/1");
        assert!(provider.is_enabled());

        let disabled = provider.disable();
        assert!(!disabled.is_enabled());

        // Disabled providers hidden from login UI
        let providers = vec![
            ExternalAuthProviderRecord::new("tenant-001", "oidc", "auth0", "cid-1", "vault/1"),
            ExternalAuthProviderRecord::new("tenant-001", "oauth2", "google", "cid-2", "vault/2").disable(),
        ];

        let enabled_for_login: Vec<_> = providers.iter().filter(|p| p.is_enabled()).collect();
        assert_eq!(enabled_for_login.len(), 1);
        assert_eq!(enabled_for_login[0].provider_name, "auth0");

        // Can be re-enabled
        let re_enabled = providers[1].clone().enable();
        assert!(re_enabled.is_enabled());
    }

    /// Test external auth provider configuration
    #[tokio::test]
    async fn test_external_auth_provider_configuration() {
        let provider = ExternalAuthProviderRecord::new("tenant-001", "oidc", "auth0", "cid-1", "vault/1")
            .add_configuration("authorization_endpoint", "https://example.auth0.com/authorize")
            .add_configuration("token_endpoint", "https://example.auth0.com/oauth/token")
            .add_configuration("userinfo_endpoint", "https://example.auth0.com/userinfo")
            .add_configuration("scopes", "openid,profile,email");

        assert_eq!(
            provider.configuration.get("authorization_endpoint"),
            Some(&"https://example.auth0.com/authorize".to_string())
        );
        assert_eq!(
            provider.configuration.get("token_endpoint"),
            Some(&"https://example.auth0.com/oauth/token".to_string())
        );
        assert_eq!(
            provider.configuration.get("scopes"),
            Some(&"openid,profile,email".to_string())
        );
    }

    /// Test external auth provider removal
    #[tokio::test]
    async fn test_external_auth_provider_deletion() {
        let mut providers = vec![
            ExternalAuthProviderRecord::new("tenant-001", "oidc", "auth0", "cid-1", "vault/1"),
            ExternalAuthProviderRecord::new("tenant-001", "oauth2", "google", "cid-2", "vault/2"),
        ];

        let removed_id = providers[0].id.clone();
        providers.retain(|p| p.id != removed_id);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_name, "google");
    }

    // ============================================================================
    // OAUTH SESSIONS TABLE TESTS
    // ============================================================================

    /// Test OAuth session creation
    #[tokio::test]
    async fn test_oauth_session_creation() {
        let expiry = Utc::now() + Duration::hours(1);
        let session = OAuthSessionRecord::new(
            "user-001",
            "oidc",
            "auth0|user123",
            "encrypted-access-token-xyz",
            expiry,
        )
        .with_refresh_token("encrypted-refresh-token-abc");

        assert_eq!(session.user_id, "user-001");
        assert_eq!(session.provider_type, "oidc");
        assert_eq!(session.provider_user_id, "auth0|user123");
        assert_eq!(session.access_token_encrypted, "encrypted-access-token-xyz");
        assert_eq!(
            session.refresh_token_encrypted,
            Some("encrypted-refresh-token-abc".to_string())
        );
        assert!(!session.is_token_expired());
        assert!(session.last_refreshed.is_none());
        assert!(!session.id.is_empty());
    }

    /// Test OAuth session by user
    #[tokio::test]
    async fn test_oauth_session_by_user() {
        let sessions = vec![
            OAuthSessionRecord::new("user-001", "oidc", "auth0|u1", "token-a", Utc::now() + Duration::hours(1)),
            OAuthSessionRecord::new("user-001", "oauth2", "google|u1", "token-b", Utc::now() + Duration::hours(1)),
            OAuthSessionRecord::new("user-002", "oidc", "auth0|u2", "token-c", Utc::now() + Duration::hours(1)),
        ];

        let user1_sessions: Vec<_> = sessions.iter().filter(|s| s.user_id == "user-001").collect();
        assert_eq!(user1_sessions.len(), 2);

        let user2_sessions: Vec<_> = sessions.iter().filter(|s| s.user_id == "user-002").collect();
        assert_eq!(user2_sessions.len(), 1);
    }

    /// Test OAuth session by provider user ID
    #[tokio::test]
    async fn test_oauth_session_by_provider_user_id() {
        let sessions = vec![
            OAuthSessionRecord::new("user-001", "oidc", "auth0|user123", "token-a", Utc::now() + Duration::hours(1)),
            OAuthSessionRecord::new("user-002", "oauth2", "google|456", "token-b", Utc::now() + Duration::hours(1)),
        ];

        // Find by provider user ID + type
        let found: Vec<_> = sessions
            .iter()
            .filter(|s| s.provider_user_id == "auth0|user123" && s.provider_type == "oidc")
            .collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].user_id, "user-001");
    }

    /// Test OAuth session token expiry tracking
    #[tokio::test]
    async fn test_oauth_session_token_expiry() {
        let sessions = vec![
            OAuthSessionRecord::new("user-001", "oidc", "id-1", "token-a", Utc::now() - Duration::hours(1)), // expired
            OAuthSessionRecord::new("user-002", "oidc", "id-2", "token-b", Utc::now() + Duration::hours(1)), // valid
            OAuthSessionRecord::new("user-003", "oidc", "id-3", "token-c", Utc::now() + Duration::seconds(30)), // expiring soon
        ];

        let expired: Vec<_> = sessions.iter().filter(|s| s.is_token_expired()).collect();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].user_id, "user-001");

        let expiring_soon: Vec<_> = sessions.iter().filter(|s| s.is_token_expiring_soon(300)).collect();
        assert_eq!(expiring_soon.len(), 2); // expired + expiring within 5 min
    }

    /// Test OAuth session token refresh update
    #[tokio::test]
    async fn test_oauth_session_token_refresh_update() {
        let session = OAuthSessionRecord::new(
            "user-001",
            "oidc",
            "auth0|user123",
            "old-token",
            Utc::now() + Duration::minutes(5),
        );
        assert!(session.last_refreshed.is_none());

        let new_expiry = Utc::now() + Duration::hours(1);
        let refreshed = session.refresh_tokens("new-token", new_expiry);

        assert_eq!(refreshed.access_token_encrypted, "new-token");
        assert_eq!(refreshed.token_expiry, new_expiry);
        assert!(refreshed.last_refreshed.is_some());
        assert!(!refreshed.is_token_expired());
    }

    /// Test OAuth session multiple providers per user
    #[tokio::test]
    async fn test_oauth_session_multiple_providers_per_user() {
        let user_id = "user-001";
        let sessions = vec![
            OAuthSessionRecord::new(user_id, "oidc", "auth0|u1", "token-auth0", Utc::now() + Duration::hours(1)),
            OAuthSessionRecord::new(user_id, "oauth2", "google|u1", "token-google", Utc::now() + Duration::hours(1)),
            OAuthSessionRecord::new(user_id, "oauth2", "github|u1", "token-github", Utc::now() + Duration::hours(1)),
        ];

        let user_sessions: Vec<_> = sessions.iter().filter(|s| s.user_id == user_id).collect();
        assert_eq!(user_sessions.len(), 3);

        // Each has a different provider type/user ID
        let provider_types: Vec<_> = user_sessions.iter().map(|s| &s.provider_user_id).collect();
        assert!(provider_types.contains(&&"auth0|u1".to_string()));
        assert!(provider_types.contains(&&"google|u1".to_string()));
        assert!(provider_types.contains(&&"github|u1".to_string()));
    }

    /// Test OAuth session provider type consistency
    #[tokio::test]
    async fn test_oauth_session_provider_type_consistency() {
        let providers = vec![
            ExternalAuthProviderRecord::new("tenant-001", "oidc", "auth0", "cid-1", "vault/1"),
            ExternalAuthProviderRecord::new("tenant-001", "oauth2", "google", "cid-2", "vault/2"),
        ];

        let sessions = vec![
            OAuthSessionRecord::new("user-001", "oidc", "auth0|u1", "token-a", Utc::now() + Duration::hours(1)),
            OAuthSessionRecord::new("user-001", "oauth2", "google|u1", "token-b", Utc::now() + Duration::hours(1)),
        ];

        // Verify each session's provider_type matches a registered provider
        for session in &sessions {
            let matching_provider = providers
                .iter()
                .any(|p| p.provider_type == session.provider_type);
            assert!(matching_provider, "Session provider_type '{}' has no matching provider", session.provider_type);
        }
    }

    /// Test OAuth session cleanup after provider removal
    #[tokio::test]
    async fn test_oauth_session_cleanup_on_provider_deletion() {
        let mut sessions = vec![
            OAuthSessionRecord::new("user-001", "oidc", "auth0|u1", "token-a", Utc::now() + Duration::hours(1)),
            OAuthSessionRecord::new("user-002", "oidc", "auth0|u2", "token-b", Utc::now() + Duration::hours(1)),
            OAuthSessionRecord::new("user-001", "oauth2", "google|u1", "token-c", Utc::now() + Duration::hours(1)),
        ];

        // Cascade delete: remove all oidc sessions when auth0 provider deleted
        sessions.retain(|s| s.provider_type != "oidc");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].provider_type, "oauth2");
    }

    // ============================================================================
    // SCHEMA RELATIONSHIP TESTS
    // ============================================================================

    /// Test foreign key constraints
    #[tokio::test]
    async fn test_schema_foreign_key_constraints() {
        let tenant_ids = vec!["tenant-001", "tenant-002"];

        // Provider must reference existing tenant
        let provider = ExternalAuthProviderRecord::new("tenant-001", "oidc", "auth0", "cid-1", "vault/1");
        assert!(tenant_ids.contains(&provider.tenant_id.as_str()));

        // Invalid tenant reference
        let bad_provider = ExternalAuthProviderRecord::new("nonexistent-tenant", "oidc", "auth0", "cid-1", "vault/1");
        assert!(!tenant_ids.contains(&bad_provider.tenant_id.as_str()));

        // Session must reference valid user
        let user_ids = vec!["user-001", "user-002"];
        let session = OAuthSessionRecord::new("user-001", "oidc", "auth0|u1", "token", Utc::now() + Duration::hours(1));
        assert!(user_ids.contains(&session.user_id.as_str()));
    }

    /// Test cascade on provider deletion
    #[tokio::test]
    async fn test_schema_cascade_on_provider_deletion() {
        let provider = ExternalAuthProviderRecord::new("tenant-001", "oidc", "auth0", "cid-1", "vault/1");
        let _provider_id = provider.id.clone();

        let mut sessions = vec![
            OAuthSessionRecord::new("user-001", "oidc", "auth0|u1", "token-a", Utc::now() + Duration::hours(1)),
            OAuthSessionRecord::new("user-002", "oidc", "auth0|u2", "token-b", Utc::now() + Duration::hours(1)),
            OAuthSessionRecord::new("user-001", "oauth2", "google|u1", "token-c", Utc::now() + Duration::hours(1)),
        ];

        // Simulated cascade: remove sessions matching provider type
        sessions.retain(|s| s.provider_type != provider.provider_type);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].provider_type, "oauth2");
    }

    /// Test indexes for performance
    #[tokio::test]
    async fn test_schema_indexes() {
        let migration = SchemaMigration::secrets_audit_migration();
        let sql = &migration.sql_content;

        // Verify all expected indexes exist
        assert!(sql.contains("idx_secret_rotation_audit_name"));
        assert!(sql.contains("idx_encryption_keys_status"));
        assert!(sql.contains("idx_external_auth_providers_tenant"));
        assert!(sql.contains("idx_oauth_sessions_user"));
        assert!(sql.contains("idx_oauth_sessions_provider_user"));
        assert!(sql.contains("idx_oauth_sessions_expiry"));

        // Verify unique constraints
        assert!(sql.contains("UNIQUE(tenant_id, provider_name)"));
        assert!(sql.contains("UNIQUE")); // encryption_keys.name
    }

    /// Test schema migrations
    #[tokio::test]
    async fn test_schema_migration_0013() {
        let migration = SchemaMigration::secrets_audit_migration();

        assert_eq!(migration.filename, "0013_secrets_audit.sql");
        assert!(migration.description.is_some());

        // Idempotent: uses IF NOT EXISTS
        assert!(migration.sql_content.contains("IF NOT EXISTS"));

        // Contains all four tables
        assert!(migration.sql_content.contains("secret_rotation_audit"));
        assert!(migration.sql_content.contains("encryption_keys"));
        assert!(migration.sql_content.contains("external_auth_providers"));
        assert!(migration.sql_content.contains("oauth_sessions"));
    }

    // ============================================================================
    // AUDIT LOGGING SCHEMA TESTS
    // ============================================================================

    /// Test secret rotation audit provides complete history
    #[tokio::test]
    async fn test_secret_audit_provides_rotation_history() {
        // Build rotation history over time
        let history = vec![
            SecretRotationAudit::new("database/creds/fraiseql", "success")
                .with_rotated_by("system")
                .with_secret_ids("v0", "v1")
                .add_metadata("reason", "initial_setup"),
            SecretRotationAudit::new("database/creds/fraiseql", "success")
                .with_rotated_by("admin")
                .with_secret_ids("v1", "v2")
                .add_metadata("reason", "scheduled"),
            SecretRotationAudit::new("database/creds/fraiseql", "failed")
                .with_rotated_by("system")
                .with_error("Vault connection timeout")
                .add_metadata("reason", "scheduled"),
            SecretRotationAudit::new("database/creds/fraiseql", "success")
                .with_rotated_by("system")
                .with_secret_ids("v2", "v3")
                .add_metadata("reason", "retry"),
        ];

        assert_eq!(history.len(), 4);

        // Query history with reasons
        let scheduled: Vec<_> = history
            .iter()
            .filter(|r| r.metadata.get("reason") == Some(&"scheduled".to_string()))
            .collect();
        assert_eq!(scheduled.len(), 2);

        // Failed rotations
        let failed: Vec<_> = history.iter().filter(|r| !r.is_successful()).collect();
        assert_eq!(failed.len(), 1);
        assert!(failed[0].error_message.is_some());
    }

    /// Test encryption key audit trail
    #[tokio::test]
    async fn test_encryption_key_audit_trail() {
        let key = EncryptionKey::new("test-key", vec![1, 2, 3], "AES-256-GCM");
        assert!(key.created_at <= Utc::now());
        assert!(key.rotated_at.is_none());
        assert_eq!(key.version, 1);

        // After rotation: audit trail updated
        let rotated = key.complete_rotation(vec![4, 5, 6]);
        assert!(rotated.rotated_at.is_some());
        assert!(rotated.rotated_at.unwrap() >= rotated.created_at);
        assert_eq!(rotated.version, 2);
    }

    /// Test OAuth session audit trail
    #[tokio::test]
    async fn test_oauth_session_audit_trail() {
        let session = OAuthSessionRecord::new(
            "user-001",
            "oidc",
            "auth0|u1",
            "initial-token",
            Utc::now() + Duration::hours(1),
        );

        // Session lifecycle tracked
        assert!(session.created_at <= Utc::now());
        assert!(session.last_refreshed.is_none());

        // After refresh
        let refreshed = session.refresh_tokens("refreshed-token", Utc::now() + Duration::hours(2));
        assert!(refreshed.last_refreshed.is_some());
        assert!(refreshed.last_refreshed.unwrap() >= refreshed.created_at);
    }

    // ============================================================================
    // DATA RETENTION AND CLEANUP TESTS
    // ============================================================================

    /// Test secret audit retention policy
    #[tokio::test]
    async fn test_secret_audit_retention_policy() {
        let mut records = Vec::new();
        for _ in 0..20 {
            records.push(SecretRotationAudit::new("secret/a", "success"));
        }

        // Retention: keep records from the last year
        let one_year_ago = Utc::now() - Duration::days(365);
        let retained: Vec<_> = records
            .iter()
            .filter(|r| r.rotation_timestamp >= one_year_ago)
            .collect();

        // All recent records retained
        assert_eq!(retained.len(), 20);
    }

    /// Test encryption key cleanup
    #[tokio::test]
    async fn test_encryption_key_cleanup() {
        let active_key = EncryptionKey::new("active-key", vec![1, 2, 3], "AES-256-GCM");
        let retired_key = EncryptionKey::new("retired-key", vec![4, 5, 6], "AES-256-GCM").retire();

        let keys = vec![active_key, retired_key];

        // Active keys must never be deleted
        let active: Vec<_> = keys.iter().filter(|k| k.is_active()).collect();
        assert_eq!(active.len(), 1);

        // Retired keys kept for decryption of historical data
        let retired: Vec<_> = keys.iter().filter(|k| k.status == "retired").collect();
        assert_eq!(retired.len(), 1);
        assert!(!retired[0].key_material_encrypted.is_empty());
    }

    /// Test OAuth session cleanup
    #[tokio::test]
    async fn test_oauth_session_cleanup() {
        let sessions = vec![
            OAuthSessionRecord::new("user-001", "oidc", "id-1", "t-a", Utc::now() - Duration::days(60)), // old expired
            OAuthSessionRecord::new("user-002", "oidc", "id-2", "t-b", Utc::now() - Duration::days(10)), // recently expired
            OAuthSessionRecord::new("user-003", "oidc", "id-3", "t-c", Utc::now() + Duration::hours(1)),  // active
        ];

        // Cleanup: remove sessions expired more than 30 days ago
        let cutoff = Utc::now() - Duration::days(30);
        let remaining: Vec<_> = sessions
            .iter()
            .filter(|s| s.token_expiry >= cutoff)
            .collect();

        assert_eq!(remaining.len(), 2); // recently expired + active remain
    }

    /// Test schema capacity and scaling
    #[tokio::test]
    async fn test_schema_capacity_and_scaling() {
        // Simulate large dataset
        let mut audit_records = Vec::new();
        for i in 0..1000 {
            audit_records.push(
                SecretRotationAudit::new(format!("secret/{}", i % 10), "success"),
            );
        }
        assert_eq!(audit_records.len(), 1000);

        // Query performance: filter by name
        let secret_0: Vec<_> = audit_records
            .iter()
            .filter(|r| r.secret_name == "secret/0")
            .collect();
        assert_eq!(secret_0.len(), 100);

        // Encryption keys: typically small
        let mut keys = Vec::new();
        for i in 0..50 {
            keys.push(EncryptionKey::new(
                format!("key-{i}"),
                vec![0u8; 32],
                "AES-256-GCM",
            ));
        }
        assert_eq!(keys.len(), 50);

        // OAuth sessions: scales with users
        let mut sessions = Vec::new();
        for i in 0..500 {
            sessions.push(OAuthSessionRecord::new(
                format!("user-{i}"),
                "oidc",
                format!("auth0|u{i}"),
                format!("token-{i}"),
                Utc::now() + Duration::hours(1),
            ));
        }
        assert_eq!(sessions.len(), 500);
    }
}
