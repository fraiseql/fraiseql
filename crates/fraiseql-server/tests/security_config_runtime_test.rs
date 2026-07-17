//! Integration tests for runtime security configuration loading
//!
//! Tests the complete flow:
//! 1. TOML configuration in schema.compiled.json
//! 2. Server loads and parses configuration
//! 3. Environment variable overrides are applied
//! 4. Security subsystems are initialized
//!
//! Rate limiting is **not** exercised here: the live rate-limit config is read by
//! the server middleware from the compiled schema's flat `security.rate_limiting`
//! `snake_case` key (`RateLimitingSecurityConfig`), not by `SecurityConfigFromSchema`.
//! The nested-`camelCase` reader that used to own it here was removed under #612
//! (item 5b) because it never matched the merger's emitted shape.
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn test_security_config_parsing_from_schema() {
        // Simulate a compiled schema with security configuration
        let schema_json = json!({
            "version": "2.0.0",
            "types": [],
            "queries": [],
            "security": {
                "auditLogging": {
                    "enabled": true,
                    "logLevel": "info",
                    "includeSensitiveData": false,
                    "asyncLogging": true,
                    "bufferSize": 1000,
                    "flushIntervalSecs": 5
                },
                "errorSanitization": {
                    "enabled": true,
                    "genericMessages": true,
                    "internalLogging": true,
                    "leakSensitiveDetails": false,
                    "userFacingFormat": "generic"
                },
                "stateEncryption": {
                    "enabled": true,
                    "algorithm": "chacha20-poly1305",
                    "keyRotationEnabled": false,
                    "nonceSize": 12,
                    "keySize": 32
                }
            }
        });

        // Test parsing from JSON
        let security_section = schema_json.get("security").unwrap();
        let config = fraiseql_server::auth::SecurityConfigFromSchema::from_json(security_section);

        let cfg = config
            .unwrap_or_else(|e| panic!("Failed to parse security config from schema JSON: {e}"));

        // Verify audit logging settings
        assert!(cfg.audit_logging.enabled);
        assert_eq!(cfg.audit_logging.log_level, "info");
        assert!(!cfg.audit_logging.include_sensitive_data);
        assert!(cfg.audit_logging.async_logging);
        assert_eq!(cfg.audit_logging.buffer_size, 1000);
        assert_eq!(cfg.audit_logging.flush_interval_secs, 5);

        // Verify error sanitization settings
        assert!(cfg.error_sanitization.enabled);
        assert!(cfg.error_sanitization.generic_messages);
        assert!(cfg.error_sanitization.internal_logging);
        assert!(!cfg.error_sanitization.leak_sensitive_details);
        assert_eq!(cfg.error_sanitization.user_facing_format, "generic");

        // Verify state encryption settings
        assert!(cfg.state_encryption.enabled);
        assert_eq!(cfg.state_encryption.algorithm, "chacha20-poly1305");
        assert!(!cfg.state_encryption.key_rotation_enabled);
        assert_eq!(cfg.state_encryption.nonce_size, 12);
        assert_eq!(cfg.state_encryption.key_size, 32);
    }

    #[test]
    fn test_security_config_initialization_with_defaults() {
        // Test initialization when schema has security config
        let schema_json_str = r#"{
            "security": {
                "auditLogging": {
                    "enabled": true,
                    "logLevel": "debug"
                }
            }
        }"#;

        let config = fraiseql_server::auth::init_security_config(schema_json_str);
        let cfg = config
            .unwrap_or_else(|e| panic!("Failed to initialize security config from schema: {e}"));
        // Verify custom values were loaded
        assert_eq!(cfg.audit_logging.log_level, "debug");

        // Verify other values use defaults
        assert!(cfg.audit_logging.enabled);
        assert!(cfg.error_sanitization.enabled);
        assert!(cfg.state_encryption.enabled);
    }

    #[test]
    fn test_security_config_validation() {
        // Test that validation rejects dangerous configurations
        let mut config = fraiseql_server::auth::SecurityConfigFromSchema::default();

        // Should be valid by default
        fraiseql_server::auth::validate_security_config(&config)
            .unwrap_or_else(|e| panic!("expected Ok for default security config: {e}"));

        // Enable sensitive data leaking - should fail
        config.error_sanitization.leak_sensitive_details = true;
        assert!(
            fraiseql_server::auth::validate_security_config(&config).is_err(),
            "Should reject leak_sensitive_details = true"
        );
    }

    #[test]
    fn test_security_config_default_values() {
        let config = fraiseql_server::auth::init_default_security_config();

        // Verify all defaults are sensible
        assert!(config.audit_logging.enabled);
        assert_eq!(config.audit_logging.log_level, "info");

        assert!(config.error_sanitization.enabled);
        assert!(config.error_sanitization.generic_messages);
        assert!(!config.error_sanitization.leak_sensitive_details);

        assert!(config.state_encryption.enabled);
        assert_eq!(config.state_encryption.algorithm, "chacha20-poly1305");
    }

    #[test]
    fn test_security_config_complete_schema() {
        // Test parsing a more complete schema document
        let full_schema = json!({
            "version": "2.0.0",
            "types": [
                {
                    "name": "User",
                    "fields": [
                        {
                            "name": "id",
                            "type": "ID",
                            "nullable": false
                        }
                    ]
                }
            ],
            "queries": [
                {
                    "name": "users",
                    "return_type": "User",
                    "return_is_list": true,
                    "sql_source": "SELECT * FROM users",
                    "arguments": []
                }
            ],
            "security": {
                "auditLogging": {
                    "enabled": true,
                    "logLevel": "warn"
                },
                "errorSanitization": {
                    "enabled": true
                },
                "stateEncryption": {
                    "enabled": true,
                    "algorithm": "chacha20-poly1305"
                }
            }
        });

        let security_value = full_schema.get("security").unwrap();
        let config = fraiseql_server::auth::SecurityConfigFromSchema::from_json(security_value);

        let cfg = config
            .unwrap_or_else(|e| panic!("expected Ok parsing complete schema security config: {e}"));

        // Verify custom config values from security section
        assert_eq!(cfg.audit_logging.log_level, "warn");
    }

    #[test]
    fn test_security_config_missing_optional_fields() {
        // Test that missing optional fields use defaults
        let minimal_schema = json!({
            "security": {
                "auditLogging": {
                    "enabled": false
                }
            }
        });

        let security_value = minimal_schema.get("security").unwrap();
        let config = fraiseql_server::auth::SecurityConfigFromSchema::from_json(security_value);

        let cfg = config
            .unwrap_or_else(|e| panic!("expected Ok parsing minimal schema security config: {e}"));

        // Explicit setting is used
        assert!(!cfg.audit_logging.enabled);

        // Other settings use defaults
        assert!(cfg.error_sanitization.enabled);
        assert!(cfg.state_encryption.enabled);
    }
}
