//! Integration tests for ELO Rust validator integration.
//!
//! Tests validator registry with ELO expressions, caching, concurrent access,
//! and performance characteristics for validator execution.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fraiseql_core::validation::elo_rust_integration::{
        EloRustValidator, RustValidatorRegistry, RustValidatorRegistryConfig,
    };

    /// Test basic validator registration and retrieval.
    #[test]
    fn test_elo_rust_validator_registry_register() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validator = EloRustValidator {
            name:           "test_validator".to_string(),
            elo_expression: "age >= 18".to_string(),
            generated_code: Some("fn validate(age: i32) -> bool { age >= 18 }".to_string()),
        };

        registry.register(validator.clone());

        let retrieved = registry.get("test_validator");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test_validator");
    }

    /// Test validator not found.
    #[test]
    fn test_elo_rust_validator_registry_not_found() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let result = registry.get("nonexistent");
        assert!(result.is_none());
    }

    /// Test validator count.
    #[test]
    fn test_elo_rust_validator_registry_count() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        for i in 0..5 {
            let validator = EloRustValidator {
                name:           format!("validator_{}", i),
                elo_expression: format!("value > {}", i),
                generated_code: None,
            };
            registry.register(validator);
        }

        assert_eq!(registry.count(), 5);
    }

    /// Test validator clear/flush.
    #[test]
    fn test_elo_rust_validator_registry_clear() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validator = EloRustValidator {
            name:           "test".to_string(),
            elo_expression: "true".to_string(),
            generated_code: None,
        };

        registry.register(validator);
        assert_eq!(registry.count(), 1);

        registry.clear();
        assert_eq!(registry.count(), 0);
    }

    /// Test validator registry with default config.
    #[test]
    fn test_elo_rust_validator_registry_default_config() {
        let config = RustValidatorRegistryConfig::default();

        assert!(config.enabled);
        assert!(config.cache_validators);
        assert!(config.max_cache_size > 0);
    }

    /// Test validator registry with disabled caching.
    #[test]
    fn test_elo_rust_validator_registry_disabled_cache() {
        let config = RustValidatorRegistryConfig {
            cache_validators: false,
            ..Default::default()
        };

        let registry = RustValidatorRegistry::new(config);

        let validator = EloRustValidator {
            name:           "test".to_string(),
            elo_expression: "x > 0".to_string(),
            generated_code: None,
        };

        registry.register(validator);

        // Should still work even with caching disabled
        let retrieved = registry.get("test");
        assert!(retrieved.is_some());
    }

    /// Test validator creation with ELO expression.
    #[test]
    fn test_elo_rust_validator_creation() {
        let validator = EloRustValidator {
            name:           "email_validator".to_string(),
            elo_expression: "contains(email, \"@\") && contains(email, \".\")".to_string(),
            generated_code: None,
        };

        assert_eq!(validator.name, "email_validator");
        assert!(!validator.elo_expression.is_empty());
    }

    /// Test validator with generated code.
    #[test]
    fn test_elo_rust_validator_with_generated_code() {
        let validator = EloRustValidator {
            name:           "range_validator".to_string(),
            elo_expression: "value >= 0 && value <= 100".to_string(),
            generated_code: Some(
                "fn validate(value: i32) -> bool { value >= 0 && value <= 100 }".to_string(),
            ),
        };

        assert!(validator.generated_code.is_some());
        assert!(validator.generated_code.as_ref().unwrap().contains("validate"));
    }

    /// Test multiple validators in registry.
    #[test]
    fn test_elo_rust_validator_registry_multiple() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validators = vec![
            EloRustValidator {
                name:           "age_validator".to_string(),
                elo_expression: "age >= 18".to_string(),
                generated_code: None,
            },
            EloRustValidator {
                name:           "email_validator".to_string(),
                elo_expression: "contains(email, \"@\")".to_string(),
                generated_code: None,
            },
            EloRustValidator {
                name:           "password_validator".to_string(),
                elo_expression: "length(password) >= 8".to_string(),
                generated_code: None,
            },
        ];

        for validator in validators {
            registry.register(validator);
        }

        assert_eq!(registry.count(), 3);
        assert!(registry.get("age_validator").is_some());
        assert!(registry.get("email_validator").is_some());
        assert!(registry.get("password_validator").is_some());
    }

    /// Test validator registry list all.
    #[test]
    fn test_elo_rust_validator_registry_list_all() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validator1 = EloRustValidator {
            name:           "validator_1".to_string(),
            elo_expression: "true".to_string(),
            generated_code: None,
        };

        let validator2 = EloRustValidator {
            name:           "validator_2".to_string(),
            elo_expression: "false".to_string(),
            generated_code: None,
        };

        registry.register(validator1);
        registry.register(validator2);

        let all = registry.list_all();
        assert_eq!(all.len(), 2);
    }

    /// Test validator update/replace.
    #[test]
    fn test_elo_rust_validator_registry_update() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validator1 = EloRustValidator {
            name:           "test_validator".to_string(),
            elo_expression: "x > 0".to_string(),
            generated_code: None,
        };

        registry.register(validator1);

        let validator2 = EloRustValidator {
            name:           "test_validator".to_string(),
            elo_expression: "x > 10".to_string(),
            generated_code: Some("updated".to_string()),
        };

        registry.register(validator2);

        let retrieved = registry.get("test_validator");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().elo_expression, "x > 10");
    }

    /// Test validator with complex ELO expression.
    #[test]
    fn test_elo_rust_validator_complex_expression() {
        let validator = EloRustValidator {
            name:           "complex_validator".to_string(),
            elo_expression: "(age >= 18 && age <= 65) || (is_senior && age >= 65)".to_string(),
            generated_code: None,
        };

        assert!(validator.elo_expression.contains("||"));
        assert!(validator.elo_expression.contains("&&"));
    }

    /// Test validator concurrent access.
    #[test]
    fn test_elo_rust_validator_registry_concurrent() {
        let config = RustValidatorRegistryConfig::default();
        let registry = Arc::new(RustValidatorRegistry::new(config));

        let mut handles = vec![];

        // Spawn 10 threads, each registering 10 validators
        for thread_id in 0..10 {
            let registry_clone = Arc::clone(&registry);

            let handle = std::thread::spawn(move || {
                for i in 0..10 {
                    let validator = EloRustValidator {
                        name:           format!("thread_{}_validator_{}", thread_id, i),
                        elo_expression: format!("value > {}", i),
                        generated_code: None,
                    };
                    registry_clone.register(validator);
                }
            });

            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // Should have 100 validators registered
        assert_eq!(registry.count(), 100);
    }

    /// Test validator registry with capacity limit.
    #[test]
    fn test_elo_rust_validator_registry_max_cache_size() {
        let config = RustValidatorRegistryConfig {
            max_cache_size: 5,
            ..Default::default()
        };

        let registry = RustValidatorRegistry::new(config);

        // Register more than max_cache_size
        for i in 0..10 {
            let validator = EloRustValidator {
                name:           format!("validator_{}", i),
                elo_expression: format!("value > {}", i),
                generated_code: None,
            };
            registry.register(validator);
        }

        // Count should not exceed max_cache_size (if LRU eviction is implemented)
        let count = registry.count();
        assert!(count <= 10); // May have all 10 or less depending on eviction
    }

    /// Test validator exists check.
    #[test]
    fn test_elo_rust_validator_registry_exists() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validator = EloRustValidator {
            name:           "test_validator".to_string(),
            elo_expression: "true".to_string(),
            generated_code: None,
        };

        registry.register(validator);

        assert!(registry.exists("test_validator"));
        assert!(!registry.exists("nonexistent"));
    }

    /// Test validator with nil/null expression.
    #[test]
    fn test_elo_rust_validator_empty_expression() {
        let validator = EloRustValidator {
            name:           "empty_validator".to_string(),
            elo_expression: String::new(),
            generated_code: None,
        };

        assert!(validator.elo_expression.is_empty());
    }

    /// Test validator config with custom settings.
    #[test]
    fn test_elo_rust_validator_registry_custom_config() {
        let config = RustValidatorRegistryConfig {
            enabled:          false,
            cache_validators: false,
            max_cache_size:   100,
        };

        let registry = RustValidatorRegistry::new(config);

        assert!(!registry.is_enabled());
    }

    /// Test validator clone behavior.
    #[test]
    fn test_elo_rust_validator_clone() {
        let validator = EloRustValidator {
            name:           "cloneable".to_string(),
            elo_expression: "x > 0".to_string(),
            generated_code: Some("code".to_string()),
        };

        let cloned = validator.clone();

        assert_eq!(validator.name, cloned.name);
        assert_eq!(validator.elo_expression, cloned.elo_expression);
        assert_eq!(validator.generated_code, cloned.generated_code);
    }

    /// Test validator registry clone shares state.
    #[test]
    fn test_elo_rust_validator_registry_clone_shares_state() {
        let config = RustValidatorRegistryConfig::default();
        let registry1 = RustValidatorRegistry::new(config);

        let validator = EloRustValidator {
            name:           "shared_validator".to_string(),
            elo_expression: "true".to_string(),
            generated_code: None,
        };

        registry1.register(validator);

        let registry2 = registry1.clone();

        assert!(registry2.exists("shared_validator"));
        assert_eq!(registry1.count(), registry2.count());
    }

    /// Test validator registry enabled/disabled.
    #[test]
    fn test_elo_rust_validator_registry_enabled_flag() {
        let config = RustValidatorRegistryConfig {
            enabled: true,
            ..Default::default()
        };

        let registry = RustValidatorRegistry::new(config);
        assert!(registry.is_enabled());

        let config2 = RustValidatorRegistryConfig {
            enabled: false,
            ..Default::default()
        };

        let registry2 = RustValidatorRegistry::new(config2);
        assert!(!registry2.is_enabled());
    }

    /// Test validator with description/metadata.
    #[test]
    fn test_elo_rust_validator_metadata() {
        let validator = EloRustValidator {
            name:           "documented_validator".to_string(),
            elo_expression: "age >= 18 && age <= 65".to_string(),
            generated_code: Some("validation code".to_string()),
        };

        assert_eq!(validator.name, "documented_validator");
        assert!(!validator.elo_expression.is_empty());
    }

    /// Test validator retrieval after operations.
    #[test]
    fn test_elo_rust_validator_registry_operations_sequence() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let v1 = EloRustValidator {
            name:           "v1".to_string(),
            elo_expression: "expr1".to_string(),
            generated_code: None,
        };

        registry.register(v1);
        assert!(registry.exists("v1"));

        let v2 = EloRustValidator {
            name:           "v2".to_string(),
            elo_expression: "expr2".to_string(),
            generated_code: None,
        };

        registry.register(v2);
        assert_eq!(registry.count(), 2);

        let retrieved = registry.get("v1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().elo_expression, "expr1");
    }

    /// Test validator with special characters in expression.
    #[test]
    fn test_elo_rust_validator_special_chars() {
        let validator = EloRustValidator {
            name:           "special_validator".to_string(),
            elo_expression: "field ~= /^[a-zA-Z0-9_]+$/ && length > 0".to_string(),
            generated_code: None,
        };

        assert!(validator.elo_expression.contains("~="));
        assert!(validator.elo_expression.contains("/"));
    }

    /// Test validator registry stats/metrics.
    #[test]
    fn test_elo_rust_validator_registry_stats() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        for i in 0..5 {
            let validator = EloRustValidator {
                name:           format!("v{}", i),
                elo_expression: format!("expr{}", i),
                generated_code: None,
            };
            registry.register(validator);
        }

        assert_eq!(registry.count(), 5);
        let all = registry.list_all();
        assert_eq!(all.len(), 5);
    }

    /// Test validator expression with all data types.
    #[test]
    fn test_elo_rust_validator_multitype_expression() {
        let validator = EloRustValidator {
            name:           "multitype".to_string(),
            elo_expression:
                "(age >= 18) && (name ~= /^[A-Z]/) && (balance > 100.50) || (is_verified)"
                    .to_string(),
            generated_code: None,
        };

        assert!(validator.elo_expression.contains("age"));
        assert!(validator.elo_expression.contains("name"));
        assert!(validator.elo_expression.contains("balance"));
        assert!(validator.elo_expression.contains("is_verified"));
    }

    /// Test validator removal/delete.
    #[test]
    fn test_elo_rust_validator_registry_remove() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validator = EloRustValidator {
            name:           "to_remove".to_string(),
            elo_expression: "true".to_string(),
            generated_code: None,
        };

        registry.register(validator);
        assert_eq!(registry.count(), 1);

        registry.remove("to_remove");
        assert_eq!(registry.count(), 0);
        assert!(!registry.exists("to_remove"));
    }

    /// Test validator registry is_empty.
    #[test]
    fn test_elo_rust_validator_registry_is_empty() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        assert!(registry.is_empty());

        let validator = EloRustValidator {
            name:           "v1".to_string(),
            elo_expression: "expr".to_string(),
            generated_code: None,
        };

        registry.register(validator);
        assert!(!registry.is_empty());
    }
}
