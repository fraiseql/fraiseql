# Phase 12: Enterprise Features - Part 2 (Secrets & Encryption)

**Objective**: Implement secrets management, encryption-at-rest, and credential rotation

**Duration**: 1-2 weeks

**Estimated LOC**: 1000-1500

**Dependencies**: Phase 11 complete

---

## Success Criteria

- [ ] HashiCorp Vault integration for dynamic secrets
- [ ] Encryption-at-rest for sensitive database fields
- [ ] Credential rotation automation
- [ ] Secrets in environment variables with validation
- [ ] External auth provider integration (OAuth, OIDC)
- [ ] Secrets audit trail
- [ ] All tests passing
- [ ] Zero clippy warnings

---

## TDD Cycles

### Cycle 12.1: Secrets Manager Interface

**Objective**: Create abstraction for multiple secrets backends

#### Files
- `crates/fraiseql-server/src/secrets/mod.rs`
- `crates/fraiseql-server/src/secrets/manager.rs`
- `crates/fraiseql-server/src/secrets/backends/vault.rs`
- `crates/fraiseql-server/src/secrets/backends/env.rs`
- `crates/fraiseql-server/src/secrets/backends/file.rs`

#### RED: Tests
```rust
#[tokio::test]
async fn test_secrets_manager_interface() {
    let manager = SecretsManager::new(SecretsBackend::Env);

    // Get secret
    let secret = manager.get_secret("db_password").await.unwrap();
    assert!(!secret.is_empty());

    // Get secret with expiry
    let (secret, expiry) = manager.get_secret_with_expiry("jwt_key").await.unwrap();
    assert!(expiry > Utc::now());
}

#[tokio::test]
async fn test_vault_backend() {
    // Requires Vault running on localhost:8200
    let manager = SecretsManager::new(SecretsBackend::Vault {
        addr: "http://localhost:8200".to_string(),
        token: "dev-only-token".to_string(),
    });

    let secret = manager.get_secret("database/creds/fraiseql").await.unwrap();
    assert!(!secret.is_empty());
}

#[test]
fn test_env_backend() {
    std::env::set_var("FRAISEQL_DB_PASSWORD", "test_password");

    let manager = SecretsManager::new(SecretsBackend::Env);
    let secret = manager
        .get_secret("FRAISEQL_DB_PASSWORD")
        .await
        .unwrap();

    assert_eq!(secret, "test_password");
}

#[test]
fn test_secrets_not_logged() {
    // Ensure secrets are redacted from logs
    let secret = Secret::new("super_secret");
    let debug_str = format!("{:?}", secret);

    assert!(debug_str.contains("***"));
    assert!(!debug_str.contains("super_secret"));
}
```

#### GREEN: Implement secrets manager
```rust
pub trait SecretsBackend: Send + Sync {
    async fn get_secret(&self, name: &str) -> Result<String>;
    async fn get_secret_with_expiry(&self, name: &str) -> Result<(String, DateTime<Utc>)>;
    async fn rotate_secret(&self, name: &str) -> Result<String>;
}

pub struct SecretsManager {
    backend: Arc<dyn SecretsBackend>,
    cache: Arc<Mutex<HashMap<String, CachedSecret>>>,
}

pub struct CachedSecret {
    value: String,
    expires_at: DateTime<Utc>,
}

impl SecretsManager {
    pub async fn get_secret(&self, name: &str) -> Result<String> {
        // Check cache first
        if let Some(cached) = self.cache.lock().await.get(name) {
            if cached.expires_at > Utc::now() {
                return Ok(cached.value.clone());
            }
        }

        // Fetch from backend
        let (secret, expiry) = self.backend.get_secret_with_expiry(name).await?;

        // Cache for 80% of TTL
        let cache_until = Utc::now() + (expiry - Utc::now()) * 0.8;
        self.cache.lock().await.insert(
            name.to_string(),
            CachedSecret {
                value: secret.clone(),
                expires_at: cache_until,
            },
        );

        Ok(secret)
    }

    pub async fn rotate_secret(&self, name: &str) -> Result<String> {
        let new_secret = self.backend.rotate_secret(name).await?;
        self.cache.lock().await.remove(name);
        Ok(new_secret)
    }
}

// Secret wrapper that redacts on Display/Debug
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Self {
        Secret(value)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Secret(***)")
    }
}

impl Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "***")
    }
}
```

#### CLEANUP
- Ensure no secrets in logs
- Verify cache invalidation

---

### Cycle 12.2: Vault Integration

**Objective**: Implement HashiCorp Vault backend for dynamic secrets

#### Files
- `crates/fraiseql-server/src/secrets/backends/vault.rs`

#### Features
- Dynamic database credentials
- Secret rotation schedules
- Lease management
- Audit logging integration

#### Tests
```rust
#[tokio::test]
#[ignore]  // Requires Vault running
async fn test_vault_dynamic_db_credentials() {
    let vault = VaultBackend::new(
        "http://localhost:8200",
        "dev-only-token",
    ).await.unwrap();

    // Request dynamic credentials for PostgreSQL
    let creds = vault.get_db_credentials("fraiseql-role").await.unwrap();

    assert!(!creds.username.is_empty());
    assert!(!creds.password.is_empty());
    assert!(creds.lease_duration > 0);

    // Verify they expire
    sleep(Duration::from_secs(creds.lease_duration)).await;
    assert!(vault.get_db_credentials("fraiseql-role").await.is_err());
}

#[tokio::test]
#[ignore]
async fn test_vault_secret_rotation() {
    let vault = VaultBackend::new(
        "http://localhost:8200",
        "dev-only-token",
    ).await.unwrap();

    let secret1 = vault.get_secret("transit/encrypt/data/apikey").await.unwrap();
    sleep(Duration::from_secs(2)).await;

    let secret2 = vault.rotate_secret("transit/encrypt/data/apikey").await.unwrap();

    assert_ne!(secret1, secret2);
}
```

#### Implementation
- Use `vaultrs` crate for Vault API
- Dynamic secrets for database
- Key encryption with Vault Transit engine
- Lease renewal background task

---

### Cycle 12.3: Field-Level Encryption

**Objective**: Encrypt sensitive database fields

#### Files
- `crates/fraiseql-server/src/encryption/mod.rs`
- `crates/fraiseql-core/src/db/encryption.rs` (for adapters)

#### Fields to Encrypt
- User emails
- Phone numbers
- SSN/tax IDs
- Credit card data
- API keys
- OAuth tokens

#### Tests
```rust
#[test]
fn test_field_encryption() {
    let cipher = FieldEncryption::new("encryption-key-from-vault");

    let plaintext = "user@example.com";
    let encrypted = cipher.encrypt(plaintext).unwrap();
    let decrypted = cipher.decrypt(&encrypted).unwrap();

    assert_ne!(plaintext, encrypted);
    assert_eq!(plaintext, decrypted);
}

#[tokio::test]
async fn test_encrypted_field_in_database() {
    let pool = setup_test_db().await;
    let cipher = FieldEncryption::new("key");

    // Insert with encryption
    let email = "alice@example.com";
    let encrypted = cipher.encrypt(email).unwrap();

    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(Uuid::new_v4())
        .bind(&encrypted)
        .execute(&pool)
        .await
        .unwrap();

    // Verify encrypted in database
    let row: (String,) = sqlx::query_as("SELECT email FROM users LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_ne!(row.0, email);

    // Decrypt on retrieval
    let decrypted = cipher.decrypt(&row.0).unwrap();
    assert_eq!(decrypted, email);
}
```

#### Implementation
- Use `aes-gcm` for encryption
- Key stored in Vault
- Transparent encryption/decryption in query layer

---

### Cycle 12.4: Credential Rotation Automation (Configurable)

**Objective**: Automatic rotation of database credentials and API keys with configurable timing

#### Files
- `crates/fraiseql-server/src/secrets/rotation.rs`
- `crates/fraiseql-server/src/config/rotation_config.rs`

#### Configuration

```toml
# fraiseql.toml

[secrets.rotation]
# Default: 7 days (conservative for production)
# Shorter intervals = higher security but more operational churn
interval_days = 7

# Grace period: time old credentials remain valid after rotation (1-60 min recommended)
grace_period_minutes = 30

# Per-credential overrides
[secrets.rotation.database]
interval_days = 7    # Standard database rotation

[secrets.rotation.api_key]
interval_days = 90   # Less frequent for stable API integrations

[secrets.rotation.oauth_token]
interval_days = 1    # Very frequent for sensitive tokens
```

#### Tests
```rust
#[tokio::test]
async fn test_rotation_config_loading() {
    let config_str = r#"
    [secrets.rotation]
    interval_days = 7
    grace_period_minutes = 30
    "#;

    let config: RotationConfig = toml::from_str(config_str).unwrap();
    assert_eq!(config.interval_days, 7);
    assert_eq!(config.grace_period_minutes, 30);
}

#[tokio::test]
async fn test_credential_rotation_scheduled_with_config() {
    let config = RotationConfig {
        interval_days: 7,
        grace_period_minutes: 30,
        per_credential: Default::default(),
    };

    let rotation = RotationScheduler::new(
        secrets_manager.clone(),
        config,
    );

    rotation.start().await.unwrap();

    // First credential
    let cred1 = rotation.current_credential("db").await.unwrap();

    // Wait for configured interval
    let interval = Duration::from_secs(7 * 24 * 3600);
    sleep(interval + Duration::from_secs(1)).await;

    // Should be different (rotated)
    let cred2 = rotation.current_credential("db").await.unwrap();
    assert_ne!(cred1.id, cred2.id);
}

#[tokio::test]
async fn test_per_credential_rotation_override() {
    let mut config = RotationConfig::default();
    config.per_credential.insert(
        "oauth_token".to_string(),
        Duration::from_secs(1 * 24 * 3600),  // 1 day override
    );

    let rotation = RotationScheduler::new(secrets_manager.clone(), config);

    let oauth_interval = rotation.get_rotation_interval("oauth_token").await;
    assert_eq!(oauth_interval, Duration::from_secs(1 * 24 * 3600));
}

#[tokio::test]
async fn test_old_credentials_valid_during_grace_period() {
    let config = RotationConfig {
        interval_days: 7,
        grace_period_minutes: 30,
        per_credential: Default::default(),
    };

    let rotation = RotationScheduler::new(secrets_manager.clone(), config);

    let old_cred = rotation.current_credential("db").await.unwrap();

    // Trigger rotation
    rotation.rotate("db").await.unwrap();

    // Old credential should still be valid within grace period
    assert!(rotation.is_valid(&old_cred).await.unwrap());

    // Wait past grace period
    sleep(Duration::from_secs(30 * 60 + 1)).await;

    // Now should be invalid
    assert!(!rotation.is_valid(&old_cred).await.unwrap());
}

#[tokio::test]
async fn test_rotation_failure_with_retry() {
    let config = RotationConfig::default();
    let rotation = RotationScheduler::new(secrets_manager.clone(), config);

    // Simulate Vault unavailable
    // Should retry with exponential backoff
    let result = rotation.rotate("db").await;
    assert!(result.is_err());

    // Verify retry scheduled
    let retry_scheduled = rotation.is_retry_scheduled("db").await.unwrap();
    assert!(retry_scheduled);
}
```

#### Implementation
```rust
pub struct RotationConfig {
    pub interval_days: i32,
    pub grace_period_minutes: i32,
    pub per_credential: std::collections::HashMap<String, Duration>,
}

impl RotationConfig {
    pub fn get_interval(&self, credential_name: &str) -> Duration {
        self.per_credential
            .get(credential_name)
            .copied()
            .unwrap_or_else(|| Duration::from_secs(self.interval_days as u64 * 24 * 3600))
    }
}

pub struct RotationScheduler {
    config: RotationConfig,
    secrets: Arc<SecretsManager>,
    current_credentials: Arc<Mutex<HashMap<String, RotatedCredential>>>,
    next_rotation: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
    audit_logger: Arc<AuditLogger>,
}

#[derive(Clone, Debug)]
pub struct RotatedCredential {
    pub current: Secret,
    pub previous: Option<Secret>,
    pub current_id: String,
    pub previous_id: Option<String>,
    pub rotated_at: DateTime<Utc>,
    pub grace_until: DateTime<Utc>,
}

impl RotationScheduler {
    pub async fn get_rotation_interval(&self, credential_name: &str) -> Duration {
        self.config.get_interval(credential_name)
    }

    pub async fn rotate(&self, credential_name: &str) -> Result<()> {
        let interval = self.get_rotation_interval(credential_name).await;
        let grace_period = Duration::from_secs(self.config.grace_period_minutes as u64 * 60);

        // Get new credential from backend
        let new_secret = self.secrets.get_secret(credential_name).await?;

        // Maintain old credential during grace period
        let mut creds = self.current_credentials.lock().await;
        let old_cred = creds.get(credential_name).cloned();

        creds.insert(
            credential_name.to_string(),
            RotatedCredential {
                current: Secret::new(new_secret),
                previous: old_cred.as_ref().map(|c| c.current.clone()),
                current_id: uuid::Uuid::new_v4().to_string(),
                previous_id: old_cred.as_ref().map(|c| c.current_id.clone()),
                rotated_at: Utc::now(),
                grace_until: Utc::now() + grace_period,
            },
        );

        // Update next rotation time
        let mut next = self.next_rotation.lock().await;
        next.insert(credential_name.to_string(), Utc::now() + interval);

        // Audit log
        self.audit_logger.log_rotation(credential_name).await?;

        Ok(())
    }

    pub async fn is_valid(&self, cred: &RotatedCredential) -> Result<bool> {
        // Valid if current OR within grace period
        Ok(cred.grace_until > Utc::now())
    }
}
```

#### CLEANUP
- Verify rotation intervals configurable
- Test grace period behavior
- Audit all rotations

---

### Cycle 12.5: External Auth Provider Integration

**Objective**: Support OAuth2 and OIDC for user authentication

#### Files
- `crates/fraiseql-server/src/auth/oauth.rs`
- `crates/fraiseql-server/src/auth/oidc.rs`

#### Providers
- Auth0
- Google
- Microsoft
- Okta

#### Tests
```rust
#[tokio::test]
async fn test_oauth_authorization_code_flow() {
    let oauth = OAuth2Client::new(
        "client_id",
        "client_secret",
        "https://auth0.com/authorize",
        "https://auth0.com/oauth/token",
    );

    // Authorization URL
    let auth_url = oauth.authorization_url("https://localhost/callback").unwrap();
    assert!(auth_url.contains("client_id"));

    // Exchange code for token
    let token = oauth.exchange_code("auth_code", "https://localhost/callback")
        .await
        .unwrap();

    assert!(!token.access_token.is_empty());
    assert!(!token.refresh_token.is_empty());
}

#[tokio::test]
async fn test_oidc_id_token_verification() {
    let oidc = OIDCClient::new(
        "https://auth0.com/.well-known/openid-configuration",
        "client_id",
        "client_secret",
    );

    let id_token = "eyJhbGc..."; // Real JWT
    let claims = oidc.verify_id_token(id_token).await.unwrap();

    assert_eq!(claims.aud, "client_id");
    assert!(claims.exp > Utc::now().timestamp());
}
```

#### Implementation
- OAuth2 authorization code flow
- OIDC provider discovery
- JWT validation
- User provisioning on first login

---

### Cycle 12.6: Database Schema for Secrets & Keys

#### Files
- `crates/fraiseql-server/migrations/0013_secrets_audit.sql`

#### Schema
```sql
-- 0013_secrets_audit.sql
CREATE TABLE secret_rotation_audit (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    secret_name VARCHAR(255) NOT NULL,
    rotation_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    rotated_by VARCHAR(255),
    previous_secret_id UUID,
    new_secret_id UUID,
    status VARCHAR(50),  -- success, failed
    error_message TEXT,
    metadata JSONB
);

CREATE TABLE encryption_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL UNIQUE,
    key_material BYTEA NOT NULL,  -- Encrypted with Vault
    algorithm VARCHAR(50),
    version INT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    rotated_at TIMESTAMPTZ,
    status VARCHAR(50),  -- active, rotating, retired
    metadata JSONB
);

CREATE TABLE external_auth_providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    provider_type VARCHAR(50) NOT NULL,  -- oauth2, oidc
    provider_name VARCHAR(255) NOT NULL,
    client_id VARCHAR(255) NOT NULL,
    client_secret_vault_path VARCHAR(255),
    configuration JSONB,
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, provider_name)
);

CREATE TABLE oauth_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id),
    provider_type VARCHAR(50),
    provider_user_id VARCHAR(255) NOT NULL,
    access_token VARCHAR(2048),
    refresh_token VARCHAR(2048),
    token_expiry TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_refreshed TIMESTAMPTZ
);
```

---

## Configuration

### Environment Variables
```bash
# Vault configuration
VAULT_ADDR=https://vault.example.com:8200
VAULT_TOKEN=s.xxxxxxxxxxxxxxxx  # Or use VAULT_SKIP_VERIFY for testing
VAULT_NAMESPACE=fraiseql/prod

# Encryption keys
ENCRYPTION_KEY_NAME=fraiseql/database-encryption
FIELD_ENCRYPTION_KEY_ID=uuid-here

# Credential rotation
ROTATION_INTERVAL_HOURS=24
ROTATION_GRACE_PERIOD_MINUTES=30

# OAuth providers (if using)
AUTH0_DOMAIN=tenant.auth0.com
AUTH0_CLIENT_ID=xxxxxxxxxxxx
AUTH0_CLIENT_SECRET=xxxxxxxxxxxx  # Or from Vault

# External services
SENTRY_DSN=https://key@sentry.io/project
```

### Helm Values
```yaml
# deploy/kubernetes/helm/fraiseql/values.yaml additions

secrets:
  backend: vault  # or env, file
  vault:
    addr: https://vault.example.com:8200
    namespace: fraiseql/prod
    # Token from secret mount
    tokenSecretName: vault-token
    tokenSecretKey: token
  encryption:
    algorithm: aes-256-gcm
    keyPath: fraiseql/encryption/database

rotation:
  enabled: true
  intervalHours: 24
  gracePeriodMinutes: 30

oauth:
  providers:
    - name: auth0
      type: oidc
      enabled: true
      clientIdSecret: auth0-client-id
      clientSecretSecret: auth0-client-secret
```

---

## Verification

```bash
# Unit tests
cargo test --lib secrets
cargo test --lib encryption

# Integration tests (requires Vault)
docker run -p 8200:8200 vault:latest server -dev
export VAULT_ADDR=http://localhost:8200
export VAULT_TOKEN=myroot
cargo test --test integration_secrets -- --ignored

# Vault setup script
./tools/setup-vault.sh
```

---

## Status

- [ ] Not Started
- [ ] In Progress (Cycle X)
- [ ] Complete

---

## Next Phase

→ Phase 13: Configuration Placeholders Wiring
