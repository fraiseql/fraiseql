//! ELO Rust target integration for compiled validators.
//!
//! Provides infrastructure for compiling ELO expressions to Rust validators,
//! caching compiled validators, and executing them with <1µs latency targets.

use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;

/// Configuration for the Rust validator registry
#[derive(Debug, Clone)]
pub struct RustValidatorRegistryConfig {
    /// Enable ELO Rust validator compilation and caching
    pub enabled:          bool,
    /// Cache compiled validators for reuse
    pub cache_validators: bool,
    /// Maximum number of validators to cache
    pub max_cache_size:   usize,
}

impl Default for RustValidatorRegistryConfig {
    fn default() -> Self {
        Self {
            enabled:          true,
            cache_validators: true,
            max_cache_size:   1000,
        }
    }
}

/// A single ELO Rust validator with generated code
#[derive(Debug, Clone)]
pub struct EloRustValidator {
    /// Name/identifier of the validator
    pub name:           String,
    /// ELO expression source
    pub elo_expression: String,
    /// Generated Rust code (if compiled)
    pub generated_code: Option<String>,
}

/// Registry for managing ELO Rust validators
#[derive(Clone)]
pub struct RustValidatorRegistry {
    config:     Arc<RustValidatorRegistryConfig>,
    validators: Arc<RwLock<HashMap<String, EloRustValidator>>>,
}

impl RustValidatorRegistry {
    /// Create a new validator registry with the given configuration
    pub fn new(config: RustValidatorRegistryConfig) -> Self {
        Self {
            config:     Arc::new(config),
            validators: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new validator
    pub fn register(&self, validator: EloRustValidator) {
        let mut validators = self.validators.write();
        validators.insert(validator.name.clone(), validator);
    }

    /// Get a registered validator by name
    pub fn get(&self, name: &str) -> Option<EloRustValidator> {
        let validators = self.validators.read();
        validators.get(name).cloned()
    }

    /// Check if a validator exists
    pub fn exists(&self, name: &str) -> bool {
        let validators = self.validators.read();
        validators.contains_key(name)
    }

    /// Remove a validator by name
    pub fn remove(&self, name: &str) {
        let mut validators = self.validators.write();
        validators.remove(name);
    }

    /// Get the number of registered validators
    pub fn count(&self) -> usize {
        let validators = self.validators.read();
        validators.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        let validators = self.validators.read();
        validators.is_empty()
    }

    /// List all validators
    pub fn list_all(&self) -> Vec<EloRustValidator> {
        let validators = self.validators.read();
        validators.values().cloned().collect()
    }

    /// Clear all validators
    pub fn clear(&self) {
        let mut validators = self.validators.write();
        validators.clear();
    }

    /// Check if registry is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get configuration reference
    pub fn config(&self) -> &RustValidatorRegistryConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = RustValidatorRegistryConfig::default();
        assert!(config.enabled);
        assert!(config.cache_validators);
        assert_eq!(config.max_cache_size, 1000);
    }

    #[test]
    fn test_validator_creation() {
        let validator = EloRustValidator {
            name:           "test".to_string(),
            elo_expression: "x > 0".to_string(),
            generated_code: None,
        };
        assert_eq!(validator.name, "test");
    }

    #[test]
    fn test_registry_register_get() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validator = EloRustValidator {
            name:           "test".to_string(),
            elo_expression: "x > 0".to_string(),
            generated_code: None,
        };

        registry.register(validator.clone());
        let retrieved = registry.get("test");
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_registry_exists() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validator = EloRustValidator {
            name:           "test".to_string(),
            elo_expression: "x > 0".to_string(),
            generated_code: None,
        };

        registry.register(validator);
        assert!(registry.exists("test"));
        assert!(!registry.exists("nonexistent"));
    }

    #[test]
    fn test_registry_count() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        for i in 0..5 {
            let validator = EloRustValidator {
                name:           format!("v{}", i),
                elo_expression: "true".to_string(),
                generated_code: None,
            };
            registry.register(validator);
        }

        assert_eq!(registry.count(), 5);
    }

    #[test]
    fn test_registry_remove() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validator = EloRustValidator {
            name:           "test".to_string(),
            elo_expression: "true".to_string(),
            generated_code: None,
        };

        registry.register(validator);
        assert_eq!(registry.count(), 1);

        registry.remove("test");
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_registry_is_enabled() {
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
}
