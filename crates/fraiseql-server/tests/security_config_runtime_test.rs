//! Integration tests for runtime security configuration loading
//!
//! Tests the complete flow:
//! 1. TOML configuration in schema.compiled.json
//! 2. Server loads and parses configuration
//! 3. Environment variable overrides are applied
//! 4. Security subsystems are initialized

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
                "rateLimiting": {
                    "enabled": true,
                    "authStart": {
                        "maxRequests": 100,
                        "windowSecs": 60
                    },
                    "authCallback": {
                        "maxRequests": 50,
                        "windowSecs": 60
                    },
                    "authRefresh": {
                        "maxRequests": 10,
                        "windowSecs": 60
                    },
                    "authLogout": {
                        "maxRequests": 20,
                        "windowSecs": 60
                    },
                    "failedLogin": {
                        "maxRequests": 5,
                        "windowSecs": 3600
                    }
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

        assert!(config.is_ok(), "Failed to parse security config from schema JSON");
        let cfg = config.unwrap();

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

        // Verify rate limiting settings
        assert!(cfg.rate_limiting.enabled);
        assert_eq!(cfg.rate_limiting.auth_start_max_requests, 100);
        assert_eq!(cfg.rate_limiting.auth_start_window_secs, 60);
        assert_eq!(cfg.rate_limiting.auth_callback_max_requests, 50);
        assert_eq!(cfg.rate_limiting.failed_login_max_requests, 5);
        assert_eq!(cfg.rate_limiting.failed_login_window_secs, 3600);

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
                },
                "rateLimiting": {
                    "enabled": true,
                    "authStart": {
                        "maxRequests": 200,
                        "windowSecs": 120
                    }
                }
            }
        }"#;

        let config = fraiseql_server::auth::init_security_config(schema_json_str);
        assert!(config.is_ok(), "Failed to initialize security config from schema");

        let cfg = config.unwrap();
        // Verify custom values were loaded
        assert_eq!(cfg.audit_logging.log_level, "debug");
        assert_eq!(cfg.rate_limiting.auth_start_max_requests, 200);
        assert_eq!(cfg.rate_limiting.auth_start_window_secs, 120);

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
        assert!(fraiseql_server::auth::validate_security_config(&config).is_ok());

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

        assert!(config.rate_limiting.enabled);
        assert_eq!(config.rate_limiting.auth_start_max_requests, 100);

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
                "rateLimiting": {
                    "enabled": true,
                    "authStart": {
                        "maxRequests": 150,
                        "windowSecs": 60
                    }
                },
                "stateEncryption": {
                    "enabled": true,
                    "algorithm": "chacha20-poly1305"
                }
            }
        });

        let security_value = full_schema.get("security").unwrap();
        let config = fraiseql_server::auth::SecurityConfigFromSchema::from_json(security_value);

        assert!(config.is_ok());
        let cfg = config.unwrap();

        // Verify custom config values from security section
        assert_eq!(cfg.audit_logging.log_level, "warn");
        assert_eq!(cfg.rate_limiting.auth_start_max_requests, 150);
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

        assert!(config.is_ok());
        let cfg = config.unwrap();

        // Explicit setting is used
        assert!(!cfg.audit_logging.enabled);

        // Other settings use defaults
        assert!(cfg.error_sanitization.enabled);
        assert!(cfg.rate_limiting.enabled);
        assert!(cfg.state_encryption.enabled);
    }

    #[test]
    fn test_security_config_rate_limit_windows() {
        // Test various rate limit window configurations
        let schema_json = json!({
            "security": {
                "rateLimiting": {
                    "enabled": true,
                    "authStart": {
                        "maxRequests": 50,
                        "windowSecs": 120
                    },
                    "authCallback": {
                        "maxRequests": 25,
                        "windowSecs": 180
                    },
                    "failedLogin": {
                        "maxRequests": 3,
                        "windowSecs": 1800
                    }
                }
            }
        });

        let security_value = schema_json.get("security").unwrap();
        let config = fraiseql_server::auth::SecurityConfigFromSchema::from_json(security_value);

        assert!(config.is_ok());
        let cfg = config.unwrap();

        // Verify rate limit configuration
        assert_eq!(cfg.rate_limiting.auth_start_max_requests, 50);
        assert_eq!(cfg.rate_limiting.auth_start_window_secs, 120);

        assert_eq!(cfg.rate_limiting.auth_callback_max_requests, 25);
        assert_eq!(cfg.rate_limiting.auth_callback_window_secs, 180);

        assert_eq!(cfg.rate_limiting.failed_login_max_requests, 3);
        assert_eq!(cfg.rate_limiting.failed_login_window_secs, 1800);
    }
}
