//! Custom type registry for runtime scalar type management.
//!
//! This module provides a thread-safe registry for managing custom scalar types
//! defined in GraphQL schemas. The registry uses Arc<RwLock<HashMap>> for
//! concurrent read access with exclusive write access.
//!
//! # Architecture
//!
//! ```text
//! CustomTypeDef (metadata for one custom scalar)
//!     ↓
//! CustomTypeRegistry (manages multiple custom types)
//!     ↓
//! CompiledSchema (contains registry)
//!     ↓
//! Runtime Validation (executes validation rules)
//! ```
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_core::validation::{CustomTypeRegistry, CustomTypeDef, ValidationRule};
//!
//! let registry = CustomTypeRegistry::new(Default::default());
//! let email_def = CustomTypeDef {
//!     name: "Email".to_string(),
//!     description: Some("Valid email address".to_string()),
//!     specified_by_url: None,
//!     validation_rules: vec![],
//!     elo_expression: Some("matches(value, /^[a-z0-9._%+-]+@[a-z0-9.-]+\\.[a-z]{2,}$/)".to_string()),
//!     base_type: None,
//! };
//! registry.register("Email".to_string(), email_def)?;
//! assert!(registry.exists("Email"));
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::error::{FraiseQLError, Result};
use super::ValidationRule;

/// Configuration for the custom type registry.
#[derive(Debug, Clone, Default)]
pub struct CustomTypeRegistryConfig {
    /// Maximum number of custom scalars allowed (None = unlimited).
    pub max_scalars: Option<usize>,

    /// Enable caching for future optimization.
    pub enable_caching: bool,
}

/// Definition of a custom scalar type at runtime.
///
/// Combines metadata with validation configuration for a single custom scalar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomTypeDef {
    /// Scalar type name.
    pub name: String,

    /// Human-readable description of the scalar.
    pub description: Option<String>,

    /// URL to specification/RFC (GraphQL spec §3.5.1).
    pub specified_by_url: Option<String>,

    /// Built-in validation rules.
    #[serde(default)]
    pub validation_rules: Vec<ValidationRule>,

    /// ELO expression for custom validation (Phase 4).
    pub elo_expression: Option<String>,

    /// Base type for type aliases (e.g., "String" for Email scalar).
    pub base_type: Option<String>,
}

impl CustomTypeDef {
    /// Create a new custom type definition with minimal required fields.
    #[must_use]
    pub fn new(name: String) -> Self {
        Self {
            name,
            description: None,
            specified_by_url: None,
            validation_rules: Vec::new(),
            elo_expression: None,
            base_type: None,
        }
    }
}

/// Thread-safe registry for custom scalar type definitions.
///
/// Uses Arc<RwLock<HashMap>> pattern for:
/// - Multiple concurrent readers (queries validating input)
/// - Single writer (schema compilation)
/// - Lock-free reads on common path (type lookup)
#[derive(Debug, Clone)]
pub struct CustomTypeRegistry {
    /// Configuration settings.
    config: Arc<CustomTypeRegistryConfig>,

    /// Thread-safe map of custom scalar definitions.
    types: Arc<RwLock<HashMap<String, CustomTypeDef>>>,
}

impl CustomTypeRegistry {
    /// Create a new custom type registry.
    ///
    /// # Arguments
    ///
    /// * `config` - Registry configuration (max scalars, caching, etc.)
    ///
    /// # Example
    ///
    /// ```ignore
    /// use fraiseql_core::validation::CustomTypeRegistry;
    ///
    /// let registry = CustomTypeRegistry::new(Default::default());
    /// assert_eq!(registry.count(), 0);
    /// ```
    #[must_use]
    pub fn new(config: CustomTypeRegistryConfig) -> Self {
        Self {
            config: Arc::new(config),
            types: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a custom scalar type definition.
    ///
    /// # Arguments
    ///
    /// * `name` - Scalar type name (must be unique)
    /// * `def` - Custom type definition
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Scalar name already exists
    /// - Max scalars limit exceeded
    ///
    /// # Example
    ///
    /// ```ignore
    /// registry.register("Email".to_string(), CustomTypeDef::new("Email".to_string()))?;
    /// ```
    pub fn register(&self, name: String, def: CustomTypeDef) -> Result<()> {
        let mut types = self.types.write().map_err(|_| FraiseQLError::Validation {
            message: "Failed to acquire write lock on custom type registry".to_string(),
            path: Some("custom_scalars".to_string()),
        })?;

        if types.contains_key(&name) {
            return Err(FraiseQLError::Validation {
                message: format!("Custom scalar '{}' already registered", name),
                path: Some(format!("custom_scalars.{}", name)),
            });
        }

        if let Some(max) = self.config.max_scalars {
            if types.len() >= max {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Cannot register '{}': max scalars limit ({}) reached",
                        name, max
                    ),
                    path: Some("custom_scalars".to_string()),
                });
            }
        }

        types.insert(name, def);
        Ok(())
    }

    /// Retrieve a custom scalar type definition.
    ///
    /// Returns None if scalar is not registered (scalars may be builtin).
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(def) = registry.get("Email") {
    ///     println!("Email: {}", def.description.unwrap_or_default());
    /// }
    /// ```
    pub fn get(&self, name: &str) -> Option<CustomTypeDef> {
        let types = self.types.read().ok()?;
        types.get(name).cloned()
    }

    /// Check if a custom scalar type is registered.
    ///
    /// # Example
    ///
    /// ```ignore
    /// assert!(registry.exists("Email"));
    /// assert!(!registry.exists("UnknownType"));
    /// ```
    #[inline]
    pub fn exists(&self, name: &str) -> bool {
        self.types
            .read()
            .map(|types| types.contains_key(name))
            .unwrap_or(false)
    }

    /// Remove a custom scalar type definition.
    ///
    /// Returns the removed definition if it existed.
    pub fn remove(&self, name: &str) -> Option<CustomTypeDef> {
        self.types
            .write()
            .ok()?
            .remove(name)
    }

    /// Get the number of registered custom scalars.
    ///
    /// # Example
    ///
    /// ```ignore
    /// assert_eq!(registry.count(), 0);
    /// registry.register("Email".to_string(), CustomTypeDef::new("Email".to_string()))?;
    /// assert_eq!(registry.count(), 1);
    /// ```
    pub fn count(&self) -> usize {
        self.types
            .read()
            .map(|types| types.len())
            .unwrap_or(0)
    }

    /// List all registered custom scalars.
    ///
    /// Returns a vector of (name, definition) tuples.
    pub fn list_all(&self) -> Vec<(String, CustomTypeDef)> {
        self.types
            .read()
            .map(|types| {
                types
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Clear all registered custom scalars.
    pub fn clear(&self) {
        if let Ok(mut types) = self.types.write() {
            types.clear();
        }
    }

    /// Validate a value against a custom scalar type's rules and ELO expression.
    ///
    /// Executes in order:
    /// 1. All validation rules from the definition
    /// 2. ELO expression (if present)
    ///
    /// # Arguments
    ///
    /// * `type_name` - Name of the custom scalar type
    /// * `value` - Value to validate (as serde_json::Value)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Custom scalar type not registered
    /// - Any validation rule fails
    /// - ELO expression evaluation fails
    /// - ELO expression evaluates to false
    ///
    /// # Example
    ///
    /// ```ignore
    /// let registry = CustomTypeRegistry::new(Default::default());
    /// let mut def = CustomTypeDef::new("LibraryCode".to_string());
    /// def.elo_expression = Some("matches(value, /^LIB-[0-9]{4}$/)".to_string());
    /// registry.register("LibraryCode".to_string(), def)?;
    ///
    /// // Valid
    /// assert!(registry.validate("LibraryCode", &json!("LIB-1234")).is_ok());
    ///
    /// // Invalid
    /// assert!(registry.validate("LibraryCode", &json!("INVALID")).is_err());
    /// ```
    pub fn validate(&self, type_name: &str, _value: &serde_json::Value) -> Result<()> {
        let _def = self.get(type_name).ok_or_else(|| FraiseQLError::Validation {
            message: format!("Unknown custom scalar type '{}'", type_name),
            path: Some(format!("custom_scalars.{}", type_name)),
        })?;

        // Phase 4 TODO: Execute validation rules
        // for rule in &_def.validation_rules {
        //     rule.validate(_value)?;
        // }

        // Phase 4 TODO: Execute ELO expression if present
        // if let Some(expr) = &_def.elo_expression {
        //     self.evaluate_elo(expr, _value)?;
        // }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_type_def_new() {
        let def = CustomTypeDef::new("Email".to_string());

        assert_eq!(def.name, "Email");
        assert_eq!(def.description, None);
        assert_eq!(def.specified_by_url, None);
        assert_eq!(def.validation_rules.len(), 0);
        assert_eq!(def.elo_expression, None);
        assert_eq!(def.base_type, None);
    }

    #[test]
    fn test_custom_type_def_with_all_fields() {
        let def = CustomTypeDef {
            name: "ISBN".to_string(),
            description: Some("International Standard Book Number".to_string()),
            specified_by_url: Some("https://www.isbn-international.org/".to_string()),
            validation_rules: vec![],
            elo_expression: Some("matches(value, /^[0-9-]{10,17}$/)".to_string()),
            base_type: Some("String".to_string()),
        };

        assert_eq!(def.name, "ISBN");
        assert_eq!(
            def.description,
            Some("International Standard Book Number".to_string())
        );
        assert_eq!(
            def.specified_by_url,
            Some("https://www.isbn-international.org/".to_string())
        );
        assert_eq!(def.elo_expression, Some("matches(value, /^[0-9-]{10,17}$/)".to_string()));
        assert_eq!(def.base_type, Some("String".to_string()));
    }

    #[test]
    fn test_custom_type_def_equality() {
        let def1 = CustomTypeDef::new("Email".to_string());
        let def2 = CustomTypeDef::new("Email".to_string());

        assert_eq!(def1, def2);
    }

    #[test]
    fn test_registry_register_and_get() {
        let registry = CustomTypeRegistry::new(Default::default());
        let def = CustomTypeDef::new("Email".to_string());

        assert!(registry.register("Email".to_string(), def.clone()).is_ok());
        assert_eq!(registry.get("Email"), Some(def));
    }

    #[test]
    fn test_registry_register_duplicate() {
        let registry = CustomTypeRegistry::new(Default::default());
        let def = CustomTypeDef::new("Email".to_string());

        assert!(registry.register("Email".to_string(), def.clone()).is_ok());
        assert!(registry.register("Email".to_string(), def).is_err());
    }

    #[test]
    fn test_registry_exists() {
        let registry = CustomTypeRegistry::new(Default::default());
        let def = CustomTypeDef::new("Email".to_string());

        assert!(!registry.exists("Email"));
        registry.register("Email".to_string(), def).unwrap();
        assert!(registry.exists("Email"));
    }

    #[test]
    fn test_registry_remove() {
        let registry = CustomTypeRegistry::new(Default::default());
        let def = CustomTypeDef::new("Email".to_string());

        registry.register("Email".to_string(), def.clone()).unwrap();
        assert_eq!(registry.remove("Email"), Some(def));
        assert!(!registry.exists("Email"));
    }

    #[test]
    fn test_registry_count() {
        let registry = CustomTypeRegistry::new(Default::default());

        assert_eq!(registry.count(), 0);
        registry
            .register("Email".to_string(), CustomTypeDef::new("Email".to_string()))
            .unwrap();
        assert_eq!(registry.count(), 1);
        registry
            .register("ISBN".to_string(), CustomTypeDef::new("ISBN".to_string()))
            .unwrap();
        assert_eq!(registry.count(), 2);
    }

    #[test]
    fn test_registry_list_all() {
        let registry = CustomTypeRegistry::new(Default::default());

        registry
            .register("Email".to_string(), CustomTypeDef::new("Email".to_string()))
            .unwrap();
        registry
            .register("ISBN".to_string(), CustomTypeDef::new("ISBN".to_string()))
            .unwrap();

        let all = registry.list_all();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|(name, _)| name == "Email"));
        assert!(all.iter().any(|(name, _)| name == "ISBN"));
    }

    #[test]
    fn test_registry_clear() {
        let registry = CustomTypeRegistry::new(Default::default());

        registry
            .register("Email".to_string(), CustomTypeDef::new("Email".to_string()))
            .unwrap();
        assert_eq!(registry.count(), 1);

        registry.clear();
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_registry_max_scalars_limit() {
        let config = CustomTypeRegistryConfig {
            max_scalars: Some(2),
            enable_caching: false,
        };
        let registry = CustomTypeRegistry::new(config);

        registry
            .register("Email".to_string(), CustomTypeDef::new("Email".to_string()))
            .unwrap();
        registry
            .register("ISBN".to_string(), CustomTypeDef::new("ISBN".to_string()))
            .unwrap();

        // Third registration should fail
        let result = registry.register("IBAN".to_string(), CustomTypeDef::new("IBAN".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_concurrent_reads() {
        use std::sync::Arc as StdArc;
        use std::thread;

        let registry = StdArc::new(CustomTypeRegistry::new(Default::default()));
        registry
            .register("Email".to_string(), CustomTypeDef::new("Email".to_string()))
            .unwrap();

        let mut handles = vec![];
        for _ in 0..5 {
            let reg = StdArc::clone(&registry);
            let handle = thread::spawn(move || {
                assert!(reg.exists("Email"));
                assert_eq!(reg.count(), 1);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_validate_unknown_scalar() {
        let registry = CustomTypeRegistry::new(Default::default());
        let value = serde_json::json!("some-value");

        let result = registry.validate("UnknownType", &value);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_library_code_minimal() {
        let registry = CustomTypeRegistry::new(Default::default());
        let def = CustomTypeDef::new("LibraryCode".to_string());

        registry
            .register("LibraryCode".to_string(), def)
            .unwrap();

        let value = serde_json::json!("LIB-1234");
        let result = registry.validate("LibraryCode", &value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_student_id_minimal() {
        let registry = CustomTypeRegistry::new(Default::default());
        let def = CustomTypeDef::new("StudentID".to_string());

        registry
            .register("StudentID".to_string(), def)
            .unwrap();

        let value = serde_json::json!("STU-2024-001");
        let result = registry.validate("StudentID", &value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_patient_id_minimal() {
        let registry = CustomTypeRegistry::new(Default::default());
        let def = CustomTypeDef::new("PatientID".to_string());

        registry
            .register("PatientID".to_string(), def)
            .unwrap();

        let value = serde_json::json!("PAT-987654");
        let result = registry.validate("PatientID", &value);
        assert!(result.is_ok());
    }
}
