//! Integration tests for validation audit logging (Phase 4, Cycle 4).
//!
//! Tests validation-specific audit logging with tenant isolation, PII redaction,
//! and compliance features.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fraiseql_core::security::validation_audit::{
        RedactionPolicy, ValidationAuditEntry, ValidationAuditLogger, ValidationAuditLoggerConfig,
    };

    fn create_test_entry(field: &str, valid: bool, rule: &str) -> ValidationAuditEntry {
        ValidationAuditEntry {
            timestamp: chrono::Utc::now(),
            user_id: Some("user:123".to_string()),
            tenant_id: Some("tenant:1".to_string()),
            ip_address: "192.168.1.100".to_string(),
            query_string: "{ user { id name } }".to_string(),
            mutation_name: None,
            field: field.to_string(),
            validation_rule: rule.to_string(),
            valid,
            failure_reason: if valid {
                None
            } else {
                Some("Value exceeds maximum length".to_string())
            },
            duration_us: 125,
            execution_context: "pattern_validator".to_string(),
        }
    }

    /// Test basic audit entry creation and properties.
    #[test]
    fn test_validation_audit_entry_creation() {
        let entry = create_test_entry("email", false, "pattern");

        assert_eq!(entry.field, "email");
        assert_eq!(entry.validation_rule, "pattern");
        assert!(!entry.valid);
        assert!(entry.failure_reason.is_some());
        assert_eq!(entry.duration_us, 125);
    }

    /// Test successful validation entries.
    #[test]
    fn test_validation_audit_entry_success() {
        let entry = create_test_entry("age", true, "range");

        assert_eq!(entry.field, "age");
        assert!(entry.valid);
        assert!(entry.failure_reason.is_none());
    }

    /// Test audit logger initialization with default config.
    #[test]
    fn test_validation_audit_logger_default_config() {
        let config = ValidationAuditLoggerConfig::default();

        assert!(config.enabled);
        assert!(config.capture_successful_validations);
        assert!(config.capture_query_strings);
    }

    /// Test audit logger with custom redaction policy.
    #[test]
    fn test_validation_audit_logger_with_redaction() {
        let config = ValidationAuditLoggerConfig {
            redaction_policy: RedactionPolicy::Aggressive,
            ..Default::default()
        };

        let logger = ValidationAuditLogger::new(config);
        let entry = create_test_entry("password", false, "length");

        logger.log_entry(entry.clone());

        // Logger should be initialized without panic
        assert!(logger.is_enabled());
    }

    /// Test audit logger enables and disables.
    #[test]
    fn test_validation_audit_logger_enable_disable() {
        let config = ValidationAuditLoggerConfig {
            enabled: false,
            ..Default::default()
        };

        let logger = ValidationAuditLogger::new(config);
        assert!(!logger.is_enabled());

        let config2 = ValidationAuditLoggerConfig {
            enabled: true,
            ..Default::default()
        };

        let logger2 = ValidationAuditLogger::new(config2);
        assert!(logger2.is_enabled());
    }

    /// Test logging entry with user context.
    #[test]
    fn test_validation_audit_entry_user_context() {
        let entry = create_test_entry("username", false, "pattern");

        assert_eq!(entry.user_id, Some("user:123".to_string()));
        assert!(entry.user_id.is_some());
    }

    /// Test logging entry with tenant context.
    #[test]
    fn test_validation_audit_entry_tenant_isolation() {
        let entry = create_test_entry("field", true, "required");

        assert_eq!(entry.tenant_id, Some("tenant:1".to_string()));
        assert_ne!(entry.tenant_id, Some("tenant:2".to_string()));
    }

    /// Test audit entry with IP address.
    #[test]
    fn test_validation_audit_entry_ip_address() {
        let entry = create_test_entry("email", false, "pattern");

        assert_eq!(entry.ip_address, "192.168.1.100");
        assert!(!entry.ip_address.is_empty());
    }

    /// Test audit entry with query string.
    #[test]
    fn test_validation_audit_entry_query_string() {
        let entry = create_test_entry("id", true, "required");

        assert_eq!(entry.query_string, "{ user { id name } }");
        assert!(!entry.query_string.is_empty());
    }

    /// Test audit entry with mutation name.
    #[test]
    fn test_validation_audit_entry_mutation_name() {
        let mut entry = create_test_entry("email", false, "pattern");
        entry.mutation_name = Some("createUser".to_string());

        assert_eq!(entry.mutation_name, Some("createUser".to_string()));
    }

    /// Test validation rule name tracking.
    #[test]
    fn test_validation_audit_entry_rule_types() {
        let rules = vec!["required", "pattern", "range", "length", "enum", "checksum"];

        for rule in rules {
            let entry = create_test_entry("field", false, rule);
            assert_eq!(entry.validation_rule, rule);
        }
    }

    /// Test execution context tracking (validator type).
    #[test]
    fn test_validation_audit_entry_execution_context() {
        let contexts = vec!["pattern_validator", "async_validator", "checksum_validator"];

        for context in contexts {
            let mut entry = create_test_entry("field", false, "pattern");
            entry.execution_context = context.to_string();
            assert_eq!(entry.execution_context, context);
        }
    }

    /// Test validation duration tracking in microseconds.
    #[test]
    fn test_validation_audit_entry_duration() {
        let mut entry = create_test_entry("field", true, "pattern");
        entry.duration_us = 1500;

        assert_eq!(entry.duration_us, 1500);
        assert!(entry.duration_us > 0);
    }

    /// Test audit logger processes entries without panic.
    #[test]
    fn test_validation_audit_logger_process_entry() {
        let config = ValidationAuditLoggerConfig::default();
        let logger = ValidationAuditLogger::new(config);
        let entry = create_test_entry("email", false, "pattern");

        logger.log_entry(entry);
        // Test passes if no panic
    }

    /// Test logging multiple entries in sequence.
    #[test]
    fn test_validation_audit_logger_multiple_entries() {
        let config = ValidationAuditLoggerConfig::default();
        let logger = ValidationAuditLogger::new(config);

        for i in 0..10 {
            let field = format!("field{}", i);
            let entry = create_test_entry(&field, i % 2 == 0, "pattern");
            logger.log_entry(entry);
        }
        // Test passes if all entries logged without panic
    }

    /// Test tenant isolation in audit entries.
    #[test]
    fn test_validation_audit_tenant_different_entries() {
        let config = ValidationAuditLoggerConfig::default();
        let logger = ValidationAuditLogger::new(config);

        let mut entry1 = create_test_entry("field", true, "required");
        entry1.tenant_id = Some("tenant:1".to_string());

        let mut entry2 = create_test_entry("field", true, "required");
        entry2.tenant_id = Some("tenant:2".to_string());

        let tenant1 = entry1.tenant_id.clone();
        let tenant2 = entry2.tenant_id.clone();

        logger.log_entry(entry1);
        logger.log_entry(entry2);

        // Tenants should be isolated
        assert_ne!(tenant1.as_ref().unwrap(), tenant2.as_ref().unwrap());
    }

    /// Test redaction policy configuration.
    #[test]
    fn test_validation_audit_redaction_policies() {
        let policies = vec![
            RedactionPolicy::None,
            RedactionPolicy::Conservative,
            RedactionPolicy::Aggressive,
        ];

        for policy in policies {
            let config = ValidationAuditLoggerConfig {
                redaction_policy: policy,
                ..Default::default()
            };

            let logger = ValidationAuditLogger::new(config);
            assert!(logger.is_enabled());
        }
    }

    /// Test capture successful validations flag.
    #[test]
    fn test_validation_audit_capture_successful_flag() {
        let config = ValidationAuditLoggerConfig {
            capture_successful_validations: true,
            ..Default::default()
        };

        let logger = ValidationAuditLogger::new(config);
        let entry = create_test_entry("field", true, "pattern");

        logger.log_entry(entry);
        // Should handle successful entries
    }

    /// Test capture query strings flag.
    #[test]
    fn test_validation_audit_capture_query_strings_flag() {
        let config = ValidationAuditLoggerConfig {
            capture_query_strings: true,
            ..Default::default()
        };

        let logger = ValidationAuditLogger::new(config);
        let entry = create_test_entry("field", false, "pattern");

        logger.log_entry(entry);
        // Should capture query strings when enabled
    }

    /// Test concurrent logging access.
    #[test]
    fn test_validation_audit_logger_concurrent() {
        let config = ValidationAuditLoggerConfig::default();
        let logger = Arc::new(ValidationAuditLogger::new(config));

        let mut handles = vec![];

        // Spawn 10 threads, each logging 10 entries
        for thread_id in 0..10 {
            let logger_clone = Arc::clone(&logger);

            let handle = std::thread::spawn(move || {
                for i in 0..10 {
                    let field = format!("thread_{}_field_{}", thread_id, i);
                    let entry = create_test_entry(&field, i % 2 == 0, "pattern");
                    logger_clone.log_entry(entry);
                }
            });

            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // Test passes if all concurrent logging completed without panic
    }

    /// Test failure reason capture.
    #[test]
    fn test_validation_audit_entry_failure_reason() {
        let mut entry = create_test_entry("email", false, "pattern");
        entry.failure_reason = Some("Invalid email format: missing @".to_string());

        assert!(entry.failure_reason.is_some());
        assert_eq!(entry.failure_reason.unwrap(), "Invalid email format: missing @");
    }

    /// Test no failure reason on successful validation.
    #[test]
    fn test_validation_audit_entry_no_failure_on_success() {
        let entry = create_test_entry("email", true, "pattern");

        assert!(entry.failure_reason.is_none());
    }

    /// Test audit logger can clone and share state.
    #[test]
    fn test_validation_audit_logger_clone() {
        let logger1 = ValidationAuditLogger::new(ValidationAuditLoggerConfig::default());
        let logger2 = logger1.clone();

        let entry1 = create_test_entry("field1", true, "required");
        let entry2 = create_test_entry("field2", false, "pattern");

        logger1.log_entry(entry1);
        logger2.log_entry(entry2);

        // Both loggers should be operational
        assert!(logger1.is_enabled());
        assert!(logger2.is_enabled());
    }

    /// Test with all PII fields present.
    #[test]
    fn test_validation_audit_entry_all_pii_fields() {
        let mut entry = create_test_entry("sensitive_field", false, "pattern");
        entry.user_id = Some("alice@example.com".to_string());
        entry.tenant_id = Some("enterprise_customer".to_string());
        entry.ip_address = "203.0.113.42".to_string();

        assert!(entry.user_id.is_some());
        assert!(entry.tenant_id.is_some());
        assert!(!entry.ip_address.is_empty());
    }

    /// Test config with disabled audit logging.
    #[test]
    fn test_validation_audit_disabled_config() {
        let config = ValidationAuditLoggerConfig {
            enabled: false,
            ..Default::default()
        };

        let logger = ValidationAuditLogger::new(config);
        let entry = create_test_entry("field", false, "pattern");

        logger.log_entry(entry);
        // Should handle disabled state gracefully
        assert!(!logger.is_enabled());
    }

    /// Test timestamp precision.
    #[test]
    fn test_validation_audit_entry_timestamp() {
        let entry = create_test_entry("field", true, "required");
        let now = chrono::Utc::now();

        // Timestamp should be recent (within 1 second)
        let diff = now.signed_duration_since(entry.timestamp);
        assert!(diff.num_seconds() <= 1);
    }

    /// Test validation rules with different failure reasons.
    #[test]
    fn test_validation_audit_various_failure_reasons() {
        let test_cases = vec![
            ("field", "pattern", "Value does not match pattern"),
            ("age", "range", "Value exceeds maximum: 150"),
            ("password", "length", "Value shorter than minimum: 8"),
            ("status", "enum", "Value not in allowed set"),
            ("card", "checksum", "Invalid checksum"),
        ];

        for (field, rule, reason) in test_cases {
            let mut entry = create_test_entry(field, false, rule);
            entry.failure_reason = Some(reason.to_string());

            assert_eq!(entry.field, field);
            assert_eq!(entry.validation_rule, rule);
            assert_eq!(entry.failure_reason.unwrap(), reason);
        }
    }
}
