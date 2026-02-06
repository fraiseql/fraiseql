//! Security configuration parsing from fraiseql.toml

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Audit logging configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
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
#[serde(default)]
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

/// Rate limiting per endpoint
///
/// Reason: Included for forward compatibility with per-endpoint rate limiting.
/// Currently unused but provided for API completeness in security configuration.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitingPerEndpoint {
    /// Maximum requests per window
    pub max_requests: u32,
    /// Time window in seconds
    pub window_secs:  u64,
}

#[allow(dead_code)]
impl RateLimitingPerEndpoint {
    /// Convert to JSON representation for schema
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "maxRequests": self.max_requests,
            "windowSecs": self.window_secs,
        })
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
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
#[serde(default)]
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

impl StateEncryptionConfig {
    /// Validate state encryption configuration
    pub fn validate(&self) -> Result<()> {
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
#[serde(default)]
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
pub struct RoleDefinitionConfig {
    /// Role name identifier
    pub name:        String,
    /// Role description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Permission scopes assigned to this role
    pub scopes:      Vec<String>,
}

impl RoleDefinitionConfig {
    /// Convert to core RoleDefinition for schema compilation
    /// Used in runtime field filtering (Cycle 5)
    #[allow(dead_code)]
    pub fn to_core_role_definition(&self) -> fraiseql_core::schema::RoleDefinition {
        fraiseql_core::schema::RoleDefinition {
            name:        self.name.clone(),
            description: self.description.clone(),
            scopes:      self.scopes.clone(),
        }
    }
}

/// Complete security configuration from fraiseql.toml
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
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
}

impl SecurityConfig {
    /// Validate all security configurations
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

    /// Find a role definition by name
    /// Used in runtime field filtering (Cycle 5)
    #[allow(dead_code)]
    pub fn find_role(&self, name: &str) -> Option<&RoleDefinitionConfig> {
        self.role_definitions.iter().find(|r| r.name == name)
    }

    /// Get all scopes for a role
    /// Used in runtime field filtering (Cycle 5)
    #[allow(dead_code)]
    pub fn get_role_scopes(&self, role_name: &str) -> Vec<String> {
        self.find_role(role_name).map(|role| role.scopes.clone()).unwrap_or_default()
    }

    /// Convert to JSON representation for schema.json
    pub fn to_json(&self) -> serde_json::Value {
        let mut json = serde_json::json!({
            "auditLogging": self.audit_logging.to_json(),
            "errorSanitization": self.error_sanitization.to_json(),
            "rateLimiting": self.rate_limiting.to_json(),
            "stateEncryption": self.state_encryption.to_json(),
            "constantTime": self.constant_time.to_json(),
        });

        // Add role definitions if present
        if !self.role_definitions.is_empty() {
            json["roleDefinitions"] = serde_json::to_value(
                self.role_definitions
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "name": r.name,
                            "description": r.description,
                            "scopes": r.scopes,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_default();
        }

        // Add default role if present
        if let Some(default_role) = &self.default_role {
            json["defaultRole"] = serde_json::json!(default_role);
        }

        json
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_security_config() {
        let config = SecurityConfig::default();
        assert!(config.audit_logging.enabled);
        assert!(config.error_sanitization.enabled);
        assert!(config.rate_limiting.enabled);
        assert!(config.state_encryption.enabled);
        assert!(config.constant_time.enabled);
    }

    #[test]
    fn test_error_sanitization_validation() {
        let mut config = ErrorSanitizationConfig::default();
        assert!(config.validate().is_ok());

        config.leak_sensitive_details = true;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_rate_limiting_validation() {
        let mut config = RateLimitConfig::default();
        assert!(config.validate().is_ok());

        config.auth_start_window_secs = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_state_encryption_validation() {
        let mut config = StateEncryptionConfig::default();
        assert!(config.validate().is_ok());

        config.key_size = 20;
        assert!(config.validate().is_err());

        config.key_size = 32;
        config.nonce_size = 16;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_security_config_serialization() {
        let config = SecurityConfig::default();
        let json = config.to_json();
        assert!(json["auditLogging"]["enabled"].is_boolean());
        assert!(json["rateLimiting"]["authStart"]["maxRequests"].is_number());
        assert!(json["stateEncryption"]["algorithm"].is_string());
    }
}
