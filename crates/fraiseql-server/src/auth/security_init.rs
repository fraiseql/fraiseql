//! Security system initialization from compiled schema configuration
//!
//! Loads security configuration from schema.compiled.json and initializes
//! all security subsystems with proper environment variable overrides.

use serde_json::Value as JsonValue;
use tracing::{debug, info, warn};

use super::security_config::SecurityConfigFromSchema;
use crate::auth::{error::Result, AuthError};

/// Initialize security configuration from compiled schema JSON string
///
/// Loads security settings from the schema.compiled.json and applies
/// environment variable overrides. This function should be called during
/// server startup after loading the compiled schema.
///
/// # Arguments
///
/// * `schema_json_str` - The compiled schema as a JSON string
///
/// # Returns
///
/// Returns a configured `SecurityConfigFromSchema` with environment overrides applied
///
/// # Errors
///
/// Returns error if:
/// - JSON parsing fails
/// - Security configuration section is invalid or missing required fields
///
/// # Example
///
/// ```rust,ignore
/// let schema = schema_loader.load().await?;
/// let json_str = schema.to_json()?;
/// let security_config = init_security_config(&json_str)?;
/// ```
pub fn init_security_config(schema_json_str: &str) -> Result<SecurityConfigFromSchema> {
    debug!("Parsing schema JSON for security configuration");

    // Parse JSON string to JsonValue
    let schema_json: JsonValue = serde_json::from_str(schema_json_str).map_err(|e| {
        warn!("Failed to parse schema JSON: {e}");
        AuthError::ConfigError {
            message: format!("Invalid schema JSON: {e}"),
        }
    })?;

    init_security_config_from_value(&schema_json)
}

/// Initialize security configuration from compiled schema JSON value
///
/// Internal function that works with parsed JsonValue. Use `init_security_config` for strings.
///
/// # Arguments
///
/// * `schema_json` - The compiled schema as a JsonValue
///
/// # Returns
///
/// Returns a configured `SecurityConfigFromSchema` with environment overrides applied
///
/// # Errors
///
/// Returns error if security configuration section is invalid or missing required fields
fn init_security_config_from_value(schema_json: &JsonValue) -> Result<SecurityConfigFromSchema> {
    debug!("Initializing security configuration from schema");

    // Extract security section from schema
    let security_value = schema_json
        .get("security")
        .ok_or_else(|| {
            warn!("No security configuration found in schema, using defaults");
            AuthError::ConfigError {
                message: "Missing security configuration in schema".to_string(),
            }
        })?;

    // Parse security configuration from schema
    let mut config = SecurityConfigFromSchema::from_json(security_value).map_err(|e| {
        warn!("Failed to parse security configuration: {e}");
        AuthError::ConfigError {
            message: format!("Invalid security configuration: {e}"),
        }
    })?;

    info!("Security configuration loaded from schema");

    // Apply environment variable overrides
    config.apply_env_overrides();
    debug!("Security environment variable overrides applied");

    Ok(config)
}

/// Initialize security configuration with default values if schema doesn't have security config
///
/// This is useful for backward compatibility when the schema doesn't include
/// a security section. It loads defaults and applies environment overrides.
///
/// # Returns
///
/// A default `SecurityConfigFromSchema` with environment overrides applied
pub fn init_default_security_config() -> SecurityConfigFromSchema {
    info!("Initializing default security configuration");
    let mut config = SecurityConfigFromSchema::default();
    config.apply_env_overrides();
    debug!("Default security configuration applied with environment overrides");
    config
}

/// Log the active security configuration (sanitized for safe logging)
///
/// Outputs current security settings to logs, excluding sensitive values
/// like encryption keys.
///
/// # Arguments
///
/// * `config` - The security configuration to log
pub fn log_security_config(config: &SecurityConfigFromSchema) {
    info!(
        audit_logging_enabled = config.audit_logging.enabled,
        audit_log_level = %config.audit_logging.log_level,
        audit_async_logging = config.audit_logging.async_logging,
        audit_buffer_size = config.audit_logging.buffer_size,
        "Audit logging configuration"
    );

    info!(
        error_sanitization_enabled = config.error_sanitization.enabled,
        error_generic_messages = config.error_sanitization.generic_messages,
        error_internal_logging = config.error_sanitization.internal_logging,
        error_leak_sensitive = config.error_sanitization.leak_sensitive_details,
        "Error sanitization configuration"
    );

    info!(
        rate_limiting_enabled = config.rate_limiting.enabled,
        auth_start_max = config.rate_limiting.auth_start_max_requests,
        auth_callback_max = config.rate_limiting.auth_callback_max_requests,
        auth_refresh_max = config.rate_limiting.auth_refresh_max_requests,
        failed_login_max = config.rate_limiting.failed_login_max_requests,
        "Rate limiting configuration"
    );

    info!(
        state_encryption_enabled = config.state_encryption.enabled,
        state_encryption_algorithm = %config.state_encryption.algorithm,
        state_encryption_nonce_size = config.state_encryption.nonce_size,
        state_encryption_key_size = config.state_encryption.key_size,
        "State encryption configuration"
    );
}

/// Verify security configuration consistency
///
/// Performs validation checks to ensure the loaded security configuration
/// doesn't have dangerous or conflicting settings.
///
/// # Arguments
///
/// * `config` - The security configuration to validate
///
/// # Returns
///
/// Returns Ok(()) if configuration is valid, Err with description if not
pub fn validate_security_config(config: &SecurityConfigFromSchema) -> Result<()> {
    // Check if sensitive data leaking is disabled (security requirement)
    if config.error_sanitization.leak_sensitive_details {
        warn!("SECURITY WARNING: leak_sensitive_details is enabled! This is a security risk.");
        return Err(AuthError::ConfigError {
            message: "leak_sensitive_details must be false in production".to_string(),
        });
    }

    // Check rate limits are reasonable
    if config.rate_limiting.enabled {
        if config.rate_limiting.auth_start_max_requests == 0 {
            return Err(AuthError::ConfigError {
                message: "auth_start_max_requests must be greater than 0".to_string(),
            });
        }
        if config.rate_limiting.auth_start_window_secs == 0 {
            return Err(AuthError::ConfigError {
                message: "auth_start_window_secs must be greater than 0".to_string(),
            });
        }
    }

    // Check state encryption key size if enabled
    if config.state_encryption.enabled && config.state_encryption.key_size != 32 {
        warn!(
            "State encryption key size is {} bytes, expected 32 bytes for ChaCha20-Poly1305",
            config.state_encryption.key_size
        );
    }

    info!("Security configuration validation passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_default_security_config() {
        let config = init_default_security_config();
        assert!(config.audit_logging.enabled);
        assert!(config.error_sanitization.enabled);
        assert!(config.rate_limiting.enabled);
        assert!(config.state_encryption.enabled);
    }

    #[test]
    fn test_validate_security_config_success() {
        let config = SecurityConfigFromSchema::default();
        assert!(validate_security_config(&config).is_ok());
    }

    #[test]
    fn test_validate_security_config_leak_sensitive_fails() {
        let mut config = SecurityConfigFromSchema::default();
        config.error_sanitization.leak_sensitive_details = true;
        assert!(validate_security_config(&config).is_err());
    }

    #[test]
    fn test_log_security_config() {
        let config = SecurityConfigFromSchema::default();
        // Just verify the function doesn't panic
        log_security_config(&config);
    }

    #[test]
    fn test_init_security_config_from_json() {
        let json = serde_json::json!({
            "security": {
                "auditLogging": {
                    "enabled": true,
                    "logLevel": "debug"
                },
                "rateLimiting": {
                    "enabled": true,
                    "authStart": {
                        "maxRequests": 200,
                        "windowSecs": 60
                    }
                }
            }
        });

        let config = init_security_config_from_value(&json);
        assert!(config.is_ok());
        let cfg = config.unwrap();
        assert_eq!(cfg.audit_logging.log_level, "debug");
        assert_eq!(cfg.rate_limiting.auth_start_max_requests, 200);
    }

    #[test]
    fn test_init_security_config_from_string() {
        let json_str = r#"{
            "security": {
                "auditLogging": {
                    "enabled": true,
                    "logLevel": "info"
                },
                "errorSanitization": {
                    "enabled": true,
                    "genericMessages": true
                }
            }
        }"#;

        let config = init_security_config(json_str);
        assert!(config.is_ok());
        let cfg = config.unwrap();
        assert_eq!(cfg.audit_logging.log_level, "info");
        assert!(cfg.error_sanitization.generic_messages);
    }

    #[test]
    fn test_init_security_config_missing_section() {
        let json = serde_json::json!({});
        let config = init_security_config_from_value(&json);
        // Should return error because security section is required
        assert!(config.is_err());
    }
}
