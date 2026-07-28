//! Security configuration parsing from fraiseql.toml

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Audit logging configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuditLoggingConfig {
    /// Enable audit logging
    pub enabled:                bool,
    /// Log level threshold ("debug", "info", "warn")
    pub log_level:              String,
    /// Include sensitive data in audit logs
    pub include_sensitive_data: bool,
    /// Use asynchronous logging
    pub async_logging:          bool,
    /// Buffer size for async logging
    pub buffer_size:            u32,
    /// Interval to flush logs in seconds
    pub flush_interval_secs:    u32,
}

impl Default for AuditLoggingConfig {
    fn default() -> Self {
        Self {
            enabled:                true,
            log_level:              "info".to_string(),
            include_sensitive_data: false,
            async_logging:          true,
            buffer_size:            1000,
            flush_interval_secs:    5,
        }
    }
}

impl AuditLoggingConfig {
    /// Convert to JSON representation for schema
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "logLevel": self.log_level,
            "includeSensitiveData": self.include_sensitive_data,
            "asyncLogging": self.async_logging,
            "bufferSize": self.buffer_size,
            "flushIntervalSecs": self.flush_interval_secs,
        })
    }
}

/// Error sanitization configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ErrorSanitizationConfig {
    /// Enable error sanitization
    pub enabled:                bool,
    /// Use generic error messages for users
    pub generic_messages:       bool,
    /// Log full errors internally
    pub internal_logging:       bool,
    /// Never leak sensitive details (security flag)
    pub leak_sensitive_details: bool,
    /// User-facing error format ("generic", "simple", "detailed")
    pub user_facing_format:     String,
}

impl Default for ErrorSanitizationConfig {
    fn default() -> Self {
        Self {
            enabled:                true,
            generic_messages:       true,
            internal_logging:       true,
            leak_sensitive_details: false,
            user_facing_format:     "generic".to_string(),
        }
    }
}

impl ErrorSanitizationConfig {
    /// Validate error sanitization configuration
    ///
    /// # Errors
    ///
    /// Returns an error if `leak_sensitive_details` is `true`, which is a
    /// security risk that must not be enabled in production.
    pub fn validate(&self) -> Result<()> {
        if self.leak_sensitive_details {
            anyhow::bail!(
                "leak_sensitive_details=true is a security risk! Never enable in production."
            );
        }
        Ok(())
    }

    /// Convert to JSON representation for schema
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "genericMessages": self.generic_messages,
            "internalLogging": self.internal_logging,
            "leakSensitiveDetails": self.leak_sensitive_details,
            "userFacingFormat": self.user_facing_format,
        })
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enabled: bool,

    /// Max requests for auth start endpoint (per IP)
    pub auth_start_max_requests: u32,
    /// Time window for auth start in seconds
    pub auth_start_window_secs:  u64,

    /// Max requests for auth callback endpoint (per IP)
    pub auth_callback_max_requests: u32,
    /// Time window for auth callback in seconds
    pub auth_callback_window_secs:  u64,

    /// Max requests for auth refresh endpoint (per user)
    pub auth_refresh_max_requests: u32,
    /// Time window for auth refresh in seconds
    pub auth_refresh_window_secs:  u64,

    /// Max requests for auth logout endpoint (per user)
    pub auth_logout_max_requests: u32,
    /// Time window for auth logout in seconds
    pub auth_logout_window_secs:  u64,

    /// Max failed login attempts per IP
    pub failed_login_max_requests: u32,
    /// Time window for failed login tracking in seconds
    pub failed_login_window_secs:  u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled:                    true,
            auth_start_max_requests:    100,
            auth_start_window_secs:     60,
            auth_callback_max_requests: 50,
            auth_callback_window_secs:  60,
            auth_refresh_max_requests:  10,
            auth_refresh_window_secs:   60,
            auth_logout_max_requests:   20,
            auth_logout_window_secs:    60,
            failed_login_max_requests:  5,
            failed_login_window_secs:   3600,
        }
    }
}

impl RateLimitConfig {
    /// Validate rate limiting configuration
    ///
    /// # Errors
    ///
    /// Returns an error if any time-window is zero or if any `max_requests`
    /// value is zero (which would permanently block all requests).
    pub fn validate(&self) -> Result<()> {
        for (name, window) in &[
            ("auth_start_window_secs", self.auth_start_window_secs),
            ("auth_callback_window_secs", self.auth_callback_window_secs),
            ("auth_refresh_window_secs", self.auth_refresh_window_secs),
            ("auth_logout_window_secs", self.auth_logout_window_secs),
            ("failed_login_window_secs", self.failed_login_window_secs),
        ] {
            if *window == 0 {
                anyhow::bail!("{name} must be positive");
            }
        }
        for (name, max_req) in &[
            ("auth_start_max_requests", self.auth_start_max_requests),
            ("auth_callback_max_requests", self.auth_callback_max_requests),
            ("auth_refresh_max_requests", self.auth_refresh_max_requests),
            ("auth_logout_max_requests", self.auth_logout_max_requests),
            ("failed_login_max_requests", self.failed_login_max_requests),
        ] {
            if *max_req == 0 {
                anyhow::bail!(
                    "{name} must be at least 1; \
                     setting it to 0 blocks all requests permanently"
                );
            }
        }
        Ok(())
    }

    /// Convert to JSON representation for schema
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "authStart": {
                "maxRequests": self.auth_start_max_requests,
                "windowSecs": self.auth_start_window_secs,
            },
            "authCallback": {
                "maxRequests": self.auth_callback_max_requests,
                "windowSecs": self.auth_callback_window_secs,
            },
            "authRefresh": {
                "maxRequests": self.auth_refresh_max_requests,
                "windowSecs": self.auth_refresh_window_secs,
            },
            "authLogout": {
                "maxRequests": self.auth_logout_max_requests,
                "windowSecs": self.auth_logout_window_secs,
            },
            "failedLogin": {
                "maxRequests": self.failed_login_max_requests,
                "windowSecs": self.failed_login_window_secs,
            },
        })
    }
}

/// State encryption configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct StateEncryptionConfig {
    /// Enable state encryption
    pub enabled:              bool,
    /// Encryption algorithm ("chacha20-poly1305")
    pub algorithm:            String,
    /// Enable automatic key rotation
    pub key_rotation_enabled: bool,
    /// Nonce size in bytes (typically 12 for 96-bit)
    pub nonce_size:           u32,
    /// Key size in bytes (16, 24, or 32)
    pub key_size:             u32,
}

impl Default for StateEncryptionConfig {
    fn default() -> Self {
        Self {
            enabled:              true,
            algorithm:            "chacha20-poly1305".to_string(),
            key_rotation_enabled: false,
            nonce_size:           12,
            key_size:             32,
        }
    }
}

/// Supported encryption algorithms for `[security.state_encryption]`.
const SUPPORTED_ALGORITHMS: &[&str] = &["chacha20-poly1305", "aes-256-gcm"];

impl StateEncryptionConfig {
    /// Validate state encryption configuration
    ///
    /// # Errors
    ///
    /// Returns an error if `algorithm` is not a supported value, if `key_size`
    /// is not 16, 24, or 32 bytes, or if `nonce_size` is not 12 bytes.
    pub fn validate(&self) -> Result<()> {
        if !SUPPORTED_ALGORITHMS.contains(&self.algorithm.as_str()) {
            anyhow::bail!(
                "algorithm {:?} is not supported; must be one of: {}",
                self.algorithm,
                SUPPORTED_ALGORITHMS.join(", ")
            );
        }
        if ![16, 24, 32].contains(&self.key_size) {
            anyhow::bail!("key_size must be 16, 24, or 32 bytes");
        }
        if self.nonce_size != 12 {
            anyhow::bail!("nonce_size must be 12 bytes (96-bit)");
        }
        Ok(())
    }

    /// Convert to JSON representation for schema
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "algorithm": self.algorithm,
            "keyRotationEnabled": self.key_rotation_enabled,
            "nonceSize": self.nonce_size,
            "keySize": self.key_size,
        })
    }
}

/// Constant-time comparison configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConstantTimeConfig {
    /// Enable constant-time comparisons
    pub enabled:                 bool,
    /// Apply constant-time comparison to JWT tokens
    pub apply_to_jwt:            bool,
    /// Apply constant-time comparison to session tokens
    pub apply_to_session_tokens: bool,
    /// Apply constant-time comparison to CSRF tokens
    pub apply_to_csrf_tokens:    bool,
    /// Apply constant-time comparison to refresh tokens
    pub apply_to_refresh_tokens: bool,
}

impl Default for ConstantTimeConfig {
    fn default() -> Self {
        Self {
            enabled:                 true,
            apply_to_jwt:            true,
            apply_to_session_tokens: true,
            apply_to_csrf_tokens:    true,
            apply_to_refresh_tokens: true,
        }
    }
}

impl ConstantTimeConfig {
    /// Convert to JSON representation for schema
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "applyToJwt": self.apply_to_jwt,
            "applyToSessionTokens": self.apply_to_session_tokens,
            "applytoCsrfTokens": self.apply_to_csrf_tokens,
            "applyToRefreshTokens": self.apply_to_refresh_tokens,
        })
    }
}

/// Field-level RBAC role definition from fraiseql.toml
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleDefinitionConfig {
    /// Role name identifier
    pub name:        String,
    /// Role description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Permission scopes assigned to this role
    pub scopes:      Vec<String>,
}

/// Tenancy isolation mode from fraiseql.toml.
///
/// Determines how tenant data is separated at the database level.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TenancyModeConfig {
    /// Single-tenant deployment, no isolation machinery.
    #[default]
    None,
    /// Row-level isolation via `@tenant_id` column injection.
    Row,
    /// Schema-level isolation via PostgreSQL schemas.
    Schema,
}

/// Tenancy configuration from `[fraiseql.tenancy]` in fraiseql.toml.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TenancyTomlConfig {
    /// Isolation strategy: `"none"`, `"row"`, or `"schema"`.
    pub mode:         TenancyModeConfig,
    /// JWT claim name that carries the tenant identifier.
    pub tenant_claim: String,
}

impl Default for TenancyTomlConfig {
    fn default() -> Self {
        Self {
            mode:         TenancyModeConfig::None,
            tenant_claim: "tenant_id".to_string(),
        }
    }
}

impl TenancyTomlConfig {
    /// Validate tenancy configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if `tenant_claim` is empty when mode is not `none`.
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.mode, TenancyModeConfig::None) && self.tenant_claim.is_empty() {
            anyhow::bail!("tenancy.tenant_claim must not be empty when mode is not 'none'");
        }
        Ok(())
    }

    /// Convert to JSON representation for compiled schema.
    ///
    /// Built by serializing the **runtime** type rather than by hand: this used to
    /// emit `tenantClaim`, which `fraiseql_core::schema::TenancyConfig` does not
    /// declare, so a configured claim vanished into the untyped catch-all and the
    /// runtime silently reverted to its `"tenant_id"` default (#757). Compile-time
    /// `@tenant_id` validation read the camelCase key, so the compiler and the
    /// server disagreed about which claim carries the tenant.
    ///
    /// Going through `TenancyConfig` makes the two agree by construction: a rename
    /// on either side is a compile error here, not a silent default at runtime.
    pub fn to_json(&self) -> serde_json::Value {
        let runtime = fraiseql_core::schema::TenancyConfig {
            mode:         match self.mode {
                TenancyModeConfig::None => fraiseql_core::schema::TenancyMode::None,
                TenancyModeConfig::Row => fraiseql_core::schema::TenancyMode::Row,
                TenancyModeConfig::Schema => fraiseql_core::schema::TenancyMode::Schema,
            },
            tenant_claim: self.tenant_claim.clone(),
        };
        serde_json::to_value(runtime).unwrap_or_else(|_| serde_json::json!({}))
    }
}

/// Complete security configuration from fraiseql.toml
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecurityConfig {
    /// Audit logging configuration
    #[serde(rename = "audit_logging")]
    pub audit_logging:      AuditLoggingConfig,
    /// Error sanitization configuration
    #[serde(rename = "error_sanitization")]
    pub error_sanitization: ErrorSanitizationConfig,
    /// Rate limiting configuration
    #[serde(rename = "rate_limiting")]
    pub rate_limiting:      RateLimitConfig,
    /// State encryption configuration
    #[serde(rename = "state_encryption")]
    pub state_encryption:   StateEncryptionConfig,
    /// Constant-time comparison configuration
    #[serde(rename = "constant_time")]
    pub constant_time:      ConstantTimeConfig,
    /// Field-level RBAC role definitions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_definitions:   Vec<RoleDefinitionConfig>,
    /// Default role when user has no explicit role assignment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_role:       Option<String>,
    /// Declare that this schema serves multiple tenants (`multi_tenant = true`).
    ///
    /// A non-`none` `[fraiseql.tenancy] mode` implies it. Activates the
    /// subscription tenant fail-closed gate and the cache+RLS boot gate — both of
    /// which named this key in their error messages while `deny_unknown_fields`
    /// made it a compile error to set (#758).
    pub multi_tenant:       bool,
    /// Declare that database Row-Level Security isolates this schema's data
    /// (`[fraiseql.security.rls] enabled = true`). Verified against the live
    /// catalog at boot.
    pub rls:                fraiseql_core::schema::RlsConfig,
}

impl SecurityConfig {
    /// Validate all security configurations
    ///
    /// # Errors
    ///
    /// Returns an error if any sub-configuration is invalid, if a role name is
    /// empty, or if a role definition contains no scopes.
    pub fn validate(&self) -> Result<()> {
        self.error_sanitization.validate()?;
        self.rate_limiting.validate()?;
        self.state_encryption.validate()?;

        // Validate role definitions if present
        for role in &self.role_definitions {
            if role.name.is_empty() {
                anyhow::bail!("Role name cannot be empty");
            }
            if role.scopes.is_empty() {
                anyhow::bail!("Role '{}' must have at least one scope", role.name);
            }
        }

        Ok(())
    }

    /// Convert to JSON representation for schema.json
    pub fn to_json(&self) -> serde_json::Value {
        // `multi_tenant` and `rls` are spelled exactly as
        // `fraiseql_core::schema::SecurityConfig` names them. The camelCase keys
        // around them land in that struct's `#[serde(flatten)] additional` map and
        // are read back out by name; these two bind to typed fields, so a rename
        // here would silently restore the #758 default rather than fail.
        let mut json = serde_json::json!({
            "multi_tenant": self.multi_tenant,
            "rls": self.rls,
            "auditLogging": self.audit_logging.to_json(),
            "errorSanitization": self.error_sanitization.to_json(),
            "rateLimiting": self.rate_limiting.to_json(),
            "stateEncryption": self.state_encryption.to_json(),
            "constantTime": self.constant_time.to_json(),
        });

        // Field-level RBAC grants. Emitted as `role_definitions` — the name
        // `fraiseql_core::schema::SecurityConfig` declares — and built by serializing
        // the runtime `RoleDefinition` type rather than by hand.
        //
        // These were emitted as `roleDefinitions` with hand-written keys, so they
        // deserialized into the consumer's `#[serde(flatten)] additional` map, nothing
        // read them back out, and `role_definitions` stayed empty. Since
        // `role_has_scope` is the *only* input to `can_access_scope`, that made the
        // documented field-level RBAC feature deny-all on every project-TOML compile:
        // a member of a role granted a scope was refused the field the role was
        // created to unlock (#757).
        if !self.role_definitions.is_empty() {
            let runtime: Vec<fraiseql_core::schema::RoleDefinition> = self
                .role_definitions
                .iter()
                .map(|r| fraiseql_core::schema::RoleDefinition {
                    description: r.description.clone(),
                    ..fraiseql_core::schema::RoleDefinition::new(r.name.clone(), r.scopes.clone())
                })
                .collect();
            json["role_definitions"] = serde_json::to_value(runtime).unwrap_or_default();
        }

        // Same rename, same consequence: `defaultRole` never reached the typed field.
        if let Some(default_role) = &self.default_role {
            json["default_role"] = serde_json::json!(default_role);
        }

        json
    }
}
