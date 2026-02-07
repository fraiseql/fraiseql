// Phase 12.5 Cycle 2: Database Schema for Secrets & Keys - GREEN
//! Database schema definitions for secrets management, encryption keys,
//! external authentication providers, and OAuth sessions.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Secret rotation audit record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretRotationAudit {
    /// Unique audit record ID
    pub id:                 String,
    /// Secret name that was rotated
    pub secret_name:        String,
    /// When rotation occurred
    pub rotation_timestamp: DateTime<Utc>,
    /// User or system that performed rotation
    pub rotated_by:         Option<String>,
    /// ID of previous secret version
    pub previous_secret_id: Option<String>,
    /// ID of new secret version
    pub new_secret_id:      Option<String>,
    /// Rotation status: "success", "failed"
    pub status:             String,
    /// Error message if failed
    pub error_message:      Option<String>,
    /// Additional metadata (JSON)
    pub metadata:           HashMap<String, String>,
}

impl SecretRotationAudit {
    /// Create new secret rotation audit record
    pub fn new(secret_name: impl Into<String>, status: impl Into<String>) -> Self {
        Self {
            id:                 uuid::Uuid::new_v4().to_string(),
            secret_name:        secret_name.into(),
            rotation_timestamp: Utc::now(),
            rotated_by:         None,
            previous_secret_id: None,
            new_secret_id:      None,
            status:             status.into(),
            error_message:      None,
            metadata:           HashMap::new(),
        }
    }

    /// Set who performed the rotation
    pub fn with_rotated_by(mut self, user_id: impl Into<String>) -> Self {
        self.rotated_by = Some(user_id.into());
        self
    }

    /// Set secret version IDs
    pub fn with_secret_ids(
        mut self,
        previous_id: impl Into<String>,
        new_id: impl Into<String>,
    ) -> Self {
        self.previous_secret_id = Some(previous_id.into());
        self.new_secret_id = Some(new_id.into());
        self
    }

    /// Set error message
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error_message = Some(error.into());
        self
    }

    /// Add metadata
    pub fn add_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Check if rotation was successful
    pub fn is_successful(&self) -> bool {
        self.status == "success"
    }
}

/// Encryption key record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptionKey {
    /// Unique key ID
    pub id:                     String,
    /// Key name (e.g., "fraiseql/database-encryption")
    pub name:                   String,
    /// Encrypted key material (stored as encrypted bytes)
    pub key_material_encrypted: Vec<u8>,
    /// Encryption algorithm (e.g., "AES-256-GCM")
    pub algorithm:              String,
    /// Version number (incremented on rotation)
    pub version:                u16,
    /// When key was created
    pub created_at:             DateTime<Utc>,
    /// When key was last rotated
    pub rotated_at:             Option<DateTime<Utc>>,
    /// Key status: "active", "rotating", "retired"
    pub status:                 String,
    /// Additional metadata (JSON)
    pub metadata:               HashMap<String, String>,
}

impl EncryptionKey {
    /// Create new encryption key
    pub fn new(
        name: impl Into<String>,
        key_material_encrypted: Vec<u8>,
        algorithm: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            key_material_encrypted,
            algorithm: algorithm.into(),
            version: 1,
            created_at: Utc::now(),
            rotated_at: None,
            status: "active".to_string(),
            metadata: HashMap::new(),
        }
    }

    /// Mark key as rotating
    pub fn start_rotation(mut self) -> Self {
        self.status = "rotating".to_string();
        self
    }

    /// Complete rotation and increment version
    pub fn complete_rotation(mut self, new_key_material: Vec<u8>) -> Self {
        self.version += 1;
        self.key_material_encrypted = new_key_material;
        self.rotated_at = Some(Utc::now());
        self.status = "active".to_string();
        self
    }

    /// Retire key (for historical decryption only)
    pub fn retire(mut self) -> Self {
        self.status = "retired".to_string();
        self
    }

    /// Check if key is active
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    /// Check if key is rotating
    pub fn is_rotating(&self) -> bool {
        self.status == "rotating"
    }

    /// Add metadata
    pub fn add_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// External authentication provider
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalAuthProviderRecord {
    /// Unique provider ID
    pub id: String,
    /// Tenant ID (for multi-tenancy)
    pub tenant_id: String,
    /// Provider type: "oauth2", "oidc"
    pub provider_type: String,
    /// Provider name: "auth0", "google", "microsoft", "okta"
    pub provider_name: String,
    /// Client ID from provider
    pub client_id: String,
    /// Vault path to client secret
    pub client_secret_vault_path: String,
    /// Provider configuration (JSON)
    pub configuration: HashMap<String, String>,
    /// Is provider enabled
    pub enabled: bool,
    /// When provider was configured
    pub created_at: DateTime<Utc>,
}

impl ExternalAuthProviderRecord {
    /// Create new external auth provider
    pub fn new(
        tenant_id: impl Into<String>,
        provider_type: impl Into<String>,
        provider_name: impl Into<String>,
        client_id: impl Into<String>,
        client_secret_vault_path: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant_id.into(),
            provider_type: provider_type.into(),
            provider_name: provider_name.into(),
            client_id: client_id.into(),
            client_secret_vault_path: client_secret_vault_path.into(),
            configuration: HashMap::new(),
            enabled: true,
            created_at: Utc::now(),
        }
    }

    /// Enable provider
    pub fn enable(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// Disable provider
    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Add configuration
    pub fn add_configuration(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.configuration.insert(key.into(), value.into());
        self
    }

    /// Check if provider is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Check if provider is OIDC
    pub fn is_oidc(&self) -> bool {
        self.provider_type == "oidc"
    }

    /// Check if provider is OAuth2
    pub fn is_oauth2(&self) -> bool {
        self.provider_type == "oauth2"
    }
}

/// OAuth session record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthSessionRecord {
    /// Unique session ID
    pub id: String,
    /// Local user ID
    pub user_id: String,
    /// Provider type: "oauth2", "oidc"
    pub provider_type: String,
    /// Provider's user ID (e.g., "auth0|user123")
    pub provider_user_id: String,
    /// Encrypted access token
    pub access_token_encrypted: String,
    /// Encrypted refresh token (if available)
    pub refresh_token_encrypted: Option<String>,
    /// When access token expires
    pub token_expiry: DateTime<Utc>,
    /// When session was created
    pub created_at: DateTime<Utc>,
    /// When token was last refreshed
    pub last_refreshed: Option<DateTime<Utc>>,
}

impl OAuthSessionRecord {
    /// Create new OAuth session
    pub fn new(
        user_id: impl Into<String>,
        provider_type: impl Into<String>,
        provider_user_id: impl Into<String>,
        access_token_encrypted: impl Into<String>,
        token_expiry: DateTime<Utc>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            provider_type: provider_type.into(),
            provider_user_id: provider_user_id.into(),
            access_token_encrypted: access_token_encrypted.into(),
            refresh_token_encrypted: None,
            token_expiry,
            created_at: Utc::now(),
            last_refreshed: None,
        }
    }

    /// Set refresh token
    pub fn with_refresh_token(mut self, refresh_token: impl Into<String>) -> Self {
        self.refresh_token_encrypted = Some(refresh_token.into());
        self
    }

    /// Update tokens after refresh
    pub fn refresh_tokens(
        mut self,
        access_token: impl Into<String>,
        token_expiry: DateTime<Utc>,
    ) -> Self {
        self.access_token_encrypted = access_token.into();
        self.token_expiry = token_expiry;
        self.last_refreshed = Some(Utc::now());
        self
    }

    /// Check if token is expired
    pub fn is_token_expired(&self) -> bool {
        self.token_expiry <= Utc::now()
    }

    /// Check if token will expire soon (within grace period)
    pub fn is_token_expiring_soon(&self, grace_seconds: i64) -> bool {
        let grace_deadline = Utc::now() + chrono::Duration::seconds(grace_seconds);
        self.token_expiry <= grace_deadline
    }
}

/// Database schema migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaMigration {
    /// Migration file name (e.g., "0013_secrets_audit.sql")
    pub filename:    String,
    /// Full SQL migration script
    pub sql_content: String,
    /// When migration was created
    pub created_at:  DateTime<Utc>,
    /// Migration description
    pub description: Option<String>,
}

impl SchemaMigration {
    /// Create new schema migration
    pub fn new(
        filename: impl Into<String>,
        sql_content: impl Into<String>,
        description: Option<String>,
    ) -> Self {
        Self {
            filename: filename.into(),
            sql_content: sql_content.into(),
            created_at: Utc::now(),
            description,
        }
    }

    /// Get migration 0013 for secrets audit schema
    pub fn secrets_audit_migration() -> Self {
        let sql = r"
CREATE TABLE IF NOT EXISTS secret_rotation_audit (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    secret_name VARCHAR(255) NOT NULL,
    rotation_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    rotated_by VARCHAR(255),
    previous_secret_id UUID,
    new_secret_id UUID,
    status VARCHAR(50),
    error_message TEXT,
    metadata JSONB
);

CREATE TABLE IF NOT EXISTS encryption_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL UNIQUE,
    key_material_encrypted BYTEA NOT NULL,
    algorithm VARCHAR(50),
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    rotated_at TIMESTAMPTZ,
    status VARCHAR(50),
    metadata JSONB
);

CREATE TABLE IF NOT EXISTS external_auth_providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    provider_type VARCHAR(50) NOT NULL,
    provider_name VARCHAR(255) NOT NULL,
    client_id VARCHAR(255) NOT NULL,
    client_secret_vault_path VARCHAR(255),
    configuration JSONB,
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, provider_name)
);

CREATE TABLE IF NOT EXISTS oauth_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    provider_type VARCHAR(50),
    provider_user_id VARCHAR(255) NOT NULL,
    access_token_encrypted VARCHAR(2048),
    refresh_token_encrypted VARCHAR(2048),
    token_expiry TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_refreshed TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_secret_rotation_audit_name
    ON secret_rotation_audit(secret_name, rotation_timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_encryption_keys_status
    ON encryption_keys(status);

CREATE INDEX IF NOT EXISTS idx_external_auth_providers_tenant
    ON external_auth_providers(tenant_id, enabled);

CREATE INDEX IF NOT EXISTS idx_oauth_sessions_user
    ON oauth_sessions(user_id);

CREATE INDEX IF NOT EXISTS idx_oauth_sessions_provider_user
    ON oauth_sessions(provider_user_id, provider_type);

CREATE INDEX IF NOT EXISTS idx_oauth_sessions_expiry
    ON oauth_sessions(token_expiry);
";

        Self {
            filename:    "0013_secrets_audit.sql".to_string(),
            sql_content: sql.to_string(),
            created_at:  Utc::now(),
            description: Some(
                "Create secrets audit, encryption keys, auth providers, and OAuth sessions tables"
                    .to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_rotation_audit_creation() {
        let audit = SecretRotationAudit::new("database/creds/fraiseql", "success");
        assert_eq!(audit.secret_name, "database/creds/fraiseql");
        assert_eq!(audit.status, "success");
        assert!(audit.is_successful());
    }

    #[test]
    fn test_secret_rotation_audit_with_ids() {
        let audit = SecretRotationAudit::new("database/creds/fraiseql", "success")
            .with_secret_ids("prev_id", "new_id");
        assert_eq!(audit.previous_secret_id, Some("prev_id".to_string()));
        assert_eq!(audit.new_secret_id, Some("new_id".to_string()));
    }

    #[test]
    fn test_secret_rotation_audit_with_error() {
        let audit = SecretRotationAudit::new("database/creds/fraiseql", "failed")
            .with_error("Vault unavailable");
        assert!(!audit.is_successful());
        assert_eq!(audit.error_message, Some("Vault unavailable".to_string()));
    }

    #[test]
    fn test_encryption_key_creation() {
        let key = EncryptionKey::new("fraiseql/database-encryption", vec![1, 2, 3], "AES-256-GCM");
        assert_eq!(key.name, "fraiseql/database-encryption");
        assert_eq!(key.version, 1);
        assert_eq!(key.status, "active");
        assert!(key.is_active());
    }

    #[test]
    fn test_encryption_key_rotation() {
        let key = EncryptionKey::new("test-key", vec![1, 2, 3], "AES-256-GCM");
        let rotated = key.complete_rotation(vec![4, 5, 6]);
        assert_eq!(rotated.version, 2);
        assert!(rotated.rotated_at.is_some());
        assert!(rotated.is_active());
    }

    #[test]
    fn test_encryption_key_retire() {
        let key = EncryptionKey::new("test-key", vec![1, 2, 3], "AES-256-GCM");
        let retired = key.retire();
        assert_eq!(retired.status, "retired");
        assert!(!retired.is_active());
    }

    #[test]
    fn test_external_auth_provider_creation() {
        let provider = ExternalAuthProviderRecord::new(
            "tenant_id",
            "oidc",
            "auth0",
            "client_id",
            "vault/path/to/secret",
        );
        assert_eq!(provider.provider_name, "auth0");
        assert!(provider.is_oidc());
        assert!(provider.is_enabled());
    }

    #[test]
    fn test_external_auth_provider_disable_enable() {
        let provider = ExternalAuthProviderRecord::new(
            "tenant_id",
            "oauth2",
            "google",
            "client_id",
            "vault/path",
        );
        let disabled = provider.disable();
        assert!(!disabled.is_enabled());

        let enabled = disabled.enable();
        assert!(enabled.is_enabled());
    }

    #[test]
    fn test_external_auth_provider_is_oauth2() {
        let provider = ExternalAuthProviderRecord::new(
            "tenant_id",
            "oauth2",
            "google",
            "client_id",
            "vault/path",
        );
        assert!(provider.is_oauth2());
        assert!(!provider.is_oidc());
    }

    #[test]
    fn test_oauth_session_creation() {
        let session = OAuthSessionRecord::new(
            "user_123",
            "oidc",
            "auth0|user_id",
            "access_token",
            Utc::now() + chrono::Duration::hours(1),
        );
        assert_eq!(session.user_id, "user_123");
        assert!(!session.is_token_expired());
    }

    #[test]
    fn test_oauth_session_with_refresh_token() {
        let session = OAuthSessionRecord::new(
            "user_123",
            "oidc",
            "auth0|user_id",
            "access_token",
            Utc::now() + chrono::Duration::hours(1),
        )
        .with_refresh_token("refresh_token");
        assert_eq!(session.refresh_token_encrypted, Some("refresh_token".to_string()));
    }

    #[test]
    fn test_oauth_session_token_refresh() {
        let session = OAuthSessionRecord::new(
            "user_123",
            "oidc",
            "auth0|user_id",
            "old_token",
            Utc::now() + chrono::Duration::hours(1),
        );
        let refreshed =
            session.refresh_tokens("new_token", Utc::now() + chrono::Duration::hours(2));
        assert_eq!(refreshed.access_token_encrypted, "new_token");
        assert!(refreshed.last_refreshed.is_some());
    }

    #[test]
    fn test_oauth_session_expiry_check() {
        let expired = OAuthSessionRecord::new(
            "user_123",
            "oidc",
            "auth0|user_id",
            "access_token",
            Utc::now() - chrono::Duration::hours(1),
        );
        assert!(expired.is_token_expired());
    }

    #[test]
    fn test_oauth_session_expiring_soon() {
        let session = OAuthSessionRecord::new(
            "user_123",
            "oidc",
            "auth0|user_id",
            "access_token",
            Utc::now() + chrono::Duration::seconds(30),
        );
        assert!(session.is_token_expiring_soon(60));
    }

    #[test]
    fn test_schema_migration_creation() {
        let migration = SchemaMigration::secrets_audit_migration();
        assert_eq!(migration.filename, "0013_secrets_audit.sql");
        assert!(!migration.sql_content.is_empty());
        assert!(migration.sql_content.contains("secret_rotation_audit"));
    }

    #[test]
    fn test_schema_migration_contains_all_tables() {
        let migration = SchemaMigration::secrets_audit_migration();
        assert!(migration.sql_content.contains("secret_rotation_audit"));
        assert!(migration.sql_content.contains("encryption_keys"));
        assert!(migration.sql_content.contains("external_auth_providers"));
        assert!(migration.sql_content.contains("oauth_sessions"));
    }

    #[test]
    fn test_schema_migration_contains_indexes() {
        let migration = SchemaMigration::secrets_audit_migration();
        assert!(migration.sql_content.contains("CREATE INDEX"));
        assert!(migration.sql_content.contains("idx_secret_rotation_audit_name"));
        assert!(migration.sql_content.contains("idx_oauth_sessions_user"));
    }
}
