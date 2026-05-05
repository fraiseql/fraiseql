//! Elo Rust target integration for compiled validators.
//!
//! Provides infrastructure for compiling Elo expressions to Rust validators,
//! caching compiled validators, and executing them with <1µs latency targets.
//!
//! Elo is an expression language by Bernard Lambeau: <https://elo-lang.org/>

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
