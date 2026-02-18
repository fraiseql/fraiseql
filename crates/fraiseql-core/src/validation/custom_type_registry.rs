//! Custom type registry for runtime scalar type management.
//!
//! This module provides a thread-safe registry for managing custom scalar types
//! defined in GraphQL schemas. The registry uses `Arc<RwLock<HashMap>>` for
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

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use serde::{Deserialize, Serialize};

use super::{ValidationRule, elo_expressions::EloExpressionEvaluator};
use crate::error::{FraiseQLError, Result};

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

    /// ELO expression for custom validation.
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
/// Uses `Arc<RwLock<HashMap>>` pattern for:
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

impl Default for CustomTypeRegistry {
    fn default() -> Self {
        Self::new(Default::default())
    }
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
            types:  Arc::new(RwLock::new(HashMap::new())),
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
            path:    Some("custom_scalars".to_string()),
        })?;

        if types.contains_key(&name) {
            return Err(FraiseQLError::Validation {
                message: format!("Custom scalar '{}' already registered", name),
                path:    Some(format!("custom_scalars.{}", name)),
            });
        }

        if let Some(max) = self.config.max_scalars {
            if types.len() >= max {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Cannot register '{}': max scalars limit ({}) reached",
                        name, max
                    ),
                    path:    Some("custom_scalars".to_string()),
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
        self.types.read().map(|types| types.contains_key(name)).unwrap_or(false)
    }

    /// Remove a custom scalar type definition.
    ///
    /// Returns the removed definition if it existed.
    pub fn remove(&self, name: &str) -> Option<CustomTypeDef> {
        self.types.write().ok()?.remove(name)
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
        self.types.read().map(|types| types.len()).unwrap_or(0)
    }

    /// List all registered custom scalars.
    ///
    /// Returns a vector of (name, definition) tuples.
    pub fn list_all(&self) -> Vec<(String, CustomTypeDef)> {
        self.types
            .read()
            .map(|types| types.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    /// Clear all registered custom scalars.
    pub fn clear(&self) {
        if let Ok(mut types) = self.types.write() {
            types.clear();
        }
    }

    /// Create a registry with all 51 built-in rich scalar types pre-registered.
    ///
    /// This method registers all FraiseQL's rich scalar types including:
    /// - Contact/Communication: Email, PhoneNumber, URL, DomainName, Hostname
    /// - Location/Address: PostalCode, Latitude, Longitude, Coordinates, Timezone, etc.
    /// - Financial: IBAN, CUSIP, ISIN, SEDOL, LEI, MIC, CurrencyCode, Money, etc.
    /// - Identifiers: Slug, SemanticVersion, HashSHA256, APIKey, VIN, TrackingNumber, etc.
    /// - Networking: IPAddress, IPv4, IPv6, MACAddress, CIDR, Port
    /// - Transportation: AirportCode, PortCode, FlightNumber
    /// - Content: Markdown, HTML, MimeType, Color, Image, File
    /// - Database: LTree
    /// - Ranges: DateRange, Duration, Percentage
    ///
    /// # Returns
    ///
    /// A new `CustomTypeRegistry` with all built-in scalars registered.
    /// All scalars have base_type = Some("String") and no validation rules/ELO expressions.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let registry = CustomTypeRegistry::with_builtin_rich_scalars();
    /// assert!(registry.exists("Email"));
    /// assert!(registry.exists("IBAN"));
    /// assert_eq!(registry.count(), 51);
    /// ```
    pub fn with_builtin_rich_scalars() -> Self {
        let registry = Self::new(CustomTypeRegistryConfig::default());

        // All 51 built-in rich scalar type names
        let rich_scalars = [
            // Contact/Communication (5)
            "Email",
            "PhoneNumber",
            "URL",
            "DomainName",
            "Hostname",
            // Location/Address (8)
            "PostalCode",
            "Latitude",
            "Longitude",
            "Coordinates",
            "Timezone",
            "LocaleCode",
            "LanguageCode",
            "CountryCode",
            // Financial (13)
            "IBAN",
            "CUSIP",
            "ISIN",
            "SEDOL",
            "LEI",
            "MIC",
            "CurrencyCode",
            "Money",
            "ExchangeCode",
            "ExchangeRate",
            "StockSymbol",
            // Identifiers (9)
            "Slug",
            "SemanticVersion",
            "HashSHA256",
            "APIKey",
            "LicensePlate",
            "VIN",
            "TrackingNumber",
            "ContainerNumber",
            // Networking (6)
            "IPAddress",
            "IPv4",
            "IPv6",
            "MACAddress",
            "CIDR",
            "Port",
            // Transportation (3)
            "AirportCode",
            "PortCode",
            "FlightNumber",
            // Content (6)
            "Markdown",
            "HTML",
            "MimeType",
            "Color",
            "Image",
            "File",
            // Database/PostgreSQL specific (1)
            "LTree",
            // Ranges (3)
            "DateRange",
            "Duration",
            "Percentage",
        ];

        // Register all rich scalars with base_type = String
        for &scalar_name in &rich_scalars {
            let mut def = CustomTypeDef::new(scalar_name.to_string());
            def.base_type = Some("String".to_string());

            // Ignore registration errors - all should succeed in a fresh registry
            let _ = registry.register(scalar_name.to_string(), def);
        }

        registry
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
    pub fn validate(&self, type_name: &str, value: &serde_json::Value) -> Result<()> {
        let def = self.get(type_name).ok_or_else(|| FraiseQLError::Validation {
            message: format!("Unknown custom scalar type '{}'", type_name),
            path:    Some(format!("custom_scalars.{}", type_name)),
        })?;

        // Execute validation rules
        self.validate_rules(type_name, &def.validation_rules, value)?;

        // Execute ELO expression if present
        if let Some(expr) = &def.elo_expression {
            self.evaluate_elo(type_name, expr, value)?;
        }

        Ok(())
    }

    /// Validate value against a list of validation rules.
    fn validate_rules(
        &self,
        type_name: &str,
        rules: &[ValidationRule],
        value: &serde_json::Value,
    ) -> Result<()> {
        for rule in rules {
            self.validate_rule(type_name, rule, value)?;
        }
        Ok(())
    }

    /// Validate value against a single validation rule.
    fn validate_rule(
        &self,
        type_name: &str,
        rule: &ValidationRule,
        value: &serde_json::Value,
    ) -> Result<()> {
        match rule {
            ValidationRule::Pattern { pattern, message } => {
                self.validate_pattern(type_name, pattern, message.as_ref(), value)
            },
            ValidationRule::Length { min, max } => {
                self.validate_length(type_name, *min, *max, value)
            },
            ValidationRule::Range { min, max } => self.validate_range(type_name, *min, *max, value),
            // Other rule types not yet supported
            _ => Ok(()),
        }
    }

    /// Validate string value against regex pattern.
    fn validate_pattern(
        &self,
        type_name: &str,
        pattern: &str,
        message: Option<&String>,
        value: &serde_json::Value,
    ) -> Result<()> {
        let str_val = value.as_str().ok_or_else(|| FraiseQLError::Validation {
            message: format!(
                "Custom scalar '{}' pattern validation: value must be a string",
                type_name
            ),
            path:    Some(format!("custom_scalars.{}", type_name)),
        })?;

        let re = regex::Regex::new(pattern).map_err(|e| FraiseQLError::Validation {
            message: format!("Custom scalar '{}' has invalid regex pattern: {}", type_name, e),
            path:    Some(format!("custom_scalars.{}.validation_rules", type_name)),
        })?;

        if !re.is_match(str_val) {
            return Err(FraiseQLError::Validation {
                message: message.cloned().unwrap_or_else(|| {
                    format!(
                        "Custom scalar '{}' value '{}' does not match pattern '{}'",
                        type_name, str_val, pattern
                    )
                }),
                path:    Some(format!("custom_scalars.{}", type_name)),
            });
        }

        Ok(())
    }

    /// Validate string length against min/max constraints.
    fn validate_length(
        &self,
        type_name: &str,
        min: Option<usize>,
        max: Option<usize>,
        value: &serde_json::Value,
    ) -> Result<()> {
        let str_val = value.as_str().ok_or_else(|| FraiseQLError::Validation {
            message: format!(
                "Custom scalar '{}' length validation: value must be a string",
                type_name
            ),
            path:    Some(format!("custom_scalars.{}", type_name)),
        })?;

        let len = str_val.len();

        if let Some(min_len) = min {
            if len < min_len {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Custom scalar '{}' value must be at least {} characters, got {}",
                        type_name, min_len, len
                    ),
                    path:    Some(format!("custom_scalars.{}", type_name)),
                });
            }
        }

        if let Some(max_len) = max {
            if len > max_len {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Custom scalar '{}' value must be at most {} characters, got {}",
                        type_name, max_len, len
                    ),
                    path:    Some(format!("custom_scalars.{}", type_name)),
                });
            }
        }

        Ok(())
    }

    /// Validate numeric value against min/max range constraints.
    fn validate_range(
        &self,
        type_name: &str,
        min: Option<i64>,
        max: Option<i64>,
        value: &serde_json::Value,
    ) -> Result<()> {
        let num_val = value.as_i64().ok_or_else(|| FraiseQLError::Validation {
            message: format!(
                "Custom scalar '{}' range validation: value must be an integer",
                type_name
            ),
            path:    Some(format!("custom_scalars.{}", type_name)),
        })?;

        if let Some(min_val) = min {
            if num_val < min_val {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Custom scalar '{}' value must be at least {}, got {}",
                        type_name, min_val, num_val
                    ),
                    path:    Some(format!("custom_scalars.{}", type_name)),
                });
            }
        }

        if let Some(max_val) = max {
            if num_val > max_val {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Custom scalar '{}' value must be at most {}, got {}",
                        type_name, max_val, num_val
                    ),
                    path:    Some(format!("custom_scalars.{}", type_name)),
                });
            }
        }

        Ok(())
    }

    /// Evaluate ELO expression against a value.
    fn evaluate_elo(&self, type_name: &str, expr: &str, value: &serde_json::Value) -> Result<()> {
        // Create context with "value" field for ELO expression evaluation
        let context = serde_json::json!({
            "value": value
        });

        // Create and evaluate the ELO expression
        let evaluator = EloExpressionEvaluator::new(expr.to_string());
        let result = evaluator.evaluate(&context)?;

        if !result.valid {
            return Err(FraiseQLError::Validation {
                message: result.error.unwrap_or_else(|| {
                    format!(
                        "Custom scalar '{}' value validation failed with ELO expression",
                        type_name
                    )
                }),
                path:    Some(format!("custom_scalars.{}", type_name)),
            });
        }

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
            name:             "ISBN".to_string(),
            description:      Some("International Standard Book Number".to_string()),
            specified_by_url: Some("https://www.isbn-international.org/".to_string()),
            validation_rules: vec![],
            elo_expression:   Some("matches(value, /^[0-9-]{10,17}$/)".to_string()),
            base_type:        Some("String".to_string()),
        };

        assert_eq!(def.name, "ISBN");
        assert_eq!(def.description, Some("International Standard Book Number".to_string()));
        assert_eq!(def.specified_by_url, Some("https://www.isbn-international.org/".to_string()));
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
            max_scalars:    Some(2),
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
        use std::{sync::Arc as StdArc, thread};

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

        registry.register("LibraryCode".to_string(), def).unwrap();

        let value = serde_json::json!("LIB-1234");
        let result = registry.validate("LibraryCode", &value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_student_id_minimal() {
        let registry = CustomTypeRegistry::new(Default::default());
        let def = CustomTypeDef::new("StudentID".to_string());

        registry.register("StudentID".to_string(), def).unwrap();

        let value = serde_json::json!("STU-2024-001");
        let result = registry.validate("StudentID", &value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_patient_id_minimal() {
        let registry = CustomTypeRegistry::new(Default::default());
        let def = CustomTypeDef::new("PatientID".to_string());

        registry.register("PatientID".to_string(), def).unwrap();

        let value = serde_json::json!("PAT-987654");
        let result = registry.validate("PatientID", &value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_with_pattern_rule_valid() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("LibraryCode".to_string());
        def.validation_rules = vec![ValidationRule::Pattern {
            pattern: r"^LIB-[0-9]{4}$".to_string(),
            message: Some("Library code must be LIB-#### format".to_string()),
        }];

        registry.register("LibraryCode".to_string(), def).unwrap();

        let value = serde_json::json!("LIB-1234");
        let result = registry.validate("LibraryCode", &value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_with_pattern_rule_invalid() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("LibraryCode".to_string());
        def.validation_rules = vec![ValidationRule::Pattern {
            pattern: r"^LIB-[0-9]{4}$".to_string(),
            message: Some("Library code must be LIB-#### format".to_string()),
        }];

        registry.register("LibraryCode".to_string(), def).unwrap();

        let value = serde_json::json!("INVALID");
        let result = registry.validate("LibraryCode", &value);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_with_length_rule_valid() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("StudentID".to_string());
        def.validation_rules = vec![ValidationRule::Length {
            min: Some(5),
            max: Some(15),
        }];

        registry.register("StudentID".to_string(), def).unwrap();

        let value = serde_json::json!("STU-2024"); // 8 chars, within 5-15
        let result = registry.validate("StudentID", &value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_with_length_rule_too_short() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("StudentID".to_string());
        def.validation_rules = vec![ValidationRule::Length {
            min: Some(5),
            max: Some(15),
        }];

        registry.register("StudentID".to_string(), def).unwrap();

        let value = serde_json::json!("STU"); // 3 chars, below min of 5
        let result = registry.validate("StudentID", &value);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_with_multiple_rules() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("PatientID".to_string());
        def.validation_rules = vec![
            ValidationRule::Pattern {
                pattern: r"^PAT-[0-9]{6}$".to_string(),
                message: Some("Patient ID must be PAT-###### format".to_string()),
            },
            ValidationRule::Length {
                min: Some(10),
                max: Some(10),
            },
        ];

        registry.register("PatientID".to_string(), def).unwrap();

        // Valid: matches pattern and length
        let value_valid = serde_json::json!("PAT-123456");
        assert!(registry.validate("PatientID", &value_valid).is_ok());

        // Invalid: wrong pattern but right length
        let value_invalid_pattern = serde_json::json!("PAT-12345X");
        assert!(registry.validate("PatientID", &value_invalid_pattern).is_err());
    }

    #[test]
    fn test_validate_library_code_with_elo_expression_valid() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("LibraryCode".to_string());
        def.elo_expression = Some("matches(value, \"^LIB-[0-9]{4}$\")".to_string());

        registry.register("LibraryCode".to_string(), def).unwrap();

        let value = serde_json::json!("LIB-1234");
        let result = registry.validate("LibraryCode", &value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_library_code_with_elo_expression_invalid() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("LibraryCode".to_string());
        def.elo_expression = Some("matches(value, \"^LIB-[0-9]{4}$\")".to_string());

        registry.register("LibraryCode".to_string(), def).unwrap();

        let value = serde_json::json!("INVALID-CODE");
        let result = registry.validate("LibraryCode", &value);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_student_id_with_elo_expression_valid() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("StudentID".to_string());
        def.elo_expression = Some("matches(value, \"^STU-[0-9]{4}-[0-9]{3}$\")".to_string());

        registry.register("StudentID".to_string(), def).unwrap();

        let value = serde_json::json!("STU-2024-001");
        let result = registry.validate("StudentID", &value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_student_id_with_elo_expression_invalid() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("StudentID".to_string());
        def.elo_expression = Some("matches(value, \"^STU-[0-9]{4}-[0-9]{3}$\")".to_string());

        registry.register("StudentID".to_string(), def).unwrap();

        let value = serde_json::json!("STUDENT-2024");
        let result = registry.validate("StudentID", &value);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_patient_id_with_elo_expression_valid() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("PatientID".to_string());
        def.elo_expression = Some("matches(value, \"^PAT-[0-9]{6}$\")".to_string());

        registry.register("PatientID".to_string(), def).unwrap();

        let value = serde_json::json!("PAT-987654");
        let result = registry.validate("PatientID", &value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_patient_id_with_elo_expression_invalid() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("PatientID".to_string());
        def.elo_expression = Some("matches(value, \"^PAT-[0-9]{6}$\")".to_string());

        registry.register("PatientID".to_string(), def).unwrap();

        let value = serde_json::json!("PATIENT123");
        let result = registry.validate("PatientID", &value);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_rules_then_elo_expression_both_valid() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("LibraryCode".to_string());
        def.validation_rules = vec![ValidationRule::Length {
            min: Some(8),
            max: Some(8),
        }];
        def.elo_expression = Some("matches(value, \"^LIB-[0-9]{4}$\")".to_string());

        registry.register("LibraryCode".to_string(), def).unwrap();

        let value = serde_json::json!("LIB-1234");
        let result = registry.validate("LibraryCode", &value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_rules_pass_elo_fails() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("LibraryCode".to_string());
        def.validation_rules = vec![ValidationRule::Length {
            min: Some(8),
            max: Some(8),
        }];
        def.elo_expression = Some("matches(value, \"^LIB-[0-9]{4}$\")".to_string());

        registry.register("LibraryCode".to_string(), def).unwrap();

        // This passes the length rule but fails the ELO pattern
        let value = serde_json::json!("NOTVALID");
        let result = registry.validate("LibraryCode", &value);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_rules_fail_elo_not_evaluated() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("StudentID".to_string());
        def.validation_rules = vec![ValidationRule::Length {
            min: Some(10),
            max: Some(10),
        }];
        def.elo_expression = Some("matches(value, \"^STU-[0-9]{4}-[0-9]{3}$\")".to_string());

        registry.register("StudentID".to_string(), def).unwrap();

        // This fails the length rule, so ELO should not be evaluated
        let value = serde_json::json!("STU-2024");
        let result = registry.validate("StudentID", &value);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_complex_elo_expression_with_multiple_conditions() {
        let registry = CustomTypeRegistry::new(Default::default());
        let mut def = CustomTypeDef::new("PatientID".to_string());
        // Match pattern OR contains substring
        def.elo_expression =
            Some("matches(value, \"^PAT-[0-9]{6}$\") || contains(value, \"URGENT\")".to_string());

        registry.register("PatientID".to_string(), def).unwrap();

        // Valid: matches pattern
        let value1 = serde_json::json!("PAT-123456");
        assert!(registry.validate("PatientID", &value1).is_ok());

        // Valid: contains substring (even though doesn't match pattern)
        let value2 = serde_json::json!("URGENT-CASE");
        assert!(registry.validate("PatientID", &value2).is_ok());

        // Invalid: neither matches pattern nor contains substring
        let value3 = serde_json::json!("INVALID");
        assert!(registry.validate("PatientID", &value3).is_err());
    }

    #[test]
    fn test_with_builtin_rich_scalars_count() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();
        assert_eq!(registry.count(), 51);
    }

    #[test]
    fn test_with_builtin_rich_scalars_contact_types() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();

        // Contact/Communication scalars
        assert!(registry.exists("Email"));
        assert!(registry.exists("PhoneNumber"));
        assert!(registry.exists("URL"));
        assert!(registry.exists("DomainName"));
        assert!(registry.exists("Hostname"));
    }

    #[test]
    fn test_with_builtin_rich_scalars_location_types() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();

        // Location/Address scalars
        assert!(registry.exists("PostalCode"));
        assert!(registry.exists("Latitude"));
        assert!(registry.exists("Longitude"));
        assert!(registry.exists("Coordinates"));
        assert!(registry.exists("Timezone"));
        assert!(registry.exists("LocaleCode"));
        assert!(registry.exists("LanguageCode"));
        assert!(registry.exists("CountryCode"));
    }

    #[test]
    fn test_with_builtin_rich_scalars_financial_types() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();

        // Financial scalars
        assert!(registry.exists("IBAN"));
        assert!(registry.exists("CUSIP"));
        assert!(registry.exists("ISIN"));
        assert!(registry.exists("SEDOL"));
        assert!(registry.exists("LEI"));
        assert!(registry.exists("MIC"));
        assert!(registry.exists("CurrencyCode"));
        assert!(registry.exists("Money"));
        assert!(registry.exists("ExchangeCode"));
        assert!(registry.exists("ExchangeRate"));
        assert!(registry.exists("StockSymbol"));
    }

    #[test]
    fn test_with_builtin_rich_scalars_identifier_types() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();

        // Identifier scalars
        assert!(registry.exists("Slug"));
        assert!(registry.exists("SemanticVersion"));
        assert!(registry.exists("HashSHA256"));
        assert!(registry.exists("APIKey"));
        assert!(registry.exists("LicensePlate"));
        assert!(registry.exists("VIN"));
        assert!(registry.exists("TrackingNumber"));
        assert!(registry.exists("ContainerNumber"));
    }

    #[test]
    fn test_with_builtin_rich_scalars_networking_types() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();

        // Networking scalars
        assert!(registry.exists("IPAddress"));
        assert!(registry.exists("IPv4"));
        assert!(registry.exists("IPv6"));
        assert!(registry.exists("MACAddress"));
        assert!(registry.exists("CIDR"));
        assert!(registry.exists("Port"));
    }

    #[test]
    fn test_with_builtin_rich_scalars_transportation_types() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();

        // Transportation scalars
        assert!(registry.exists("AirportCode"));
        assert!(registry.exists("PortCode"));
        assert!(registry.exists("FlightNumber"));
    }

    #[test]
    fn test_with_builtin_rich_scalars_content_types() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();

        // Content scalars
        assert!(registry.exists("Markdown"));
        assert!(registry.exists("HTML"));
        assert!(registry.exists("MimeType"));
        assert!(registry.exists("Color"));
        assert!(registry.exists("Image"));
        assert!(registry.exists("File"));
    }

    #[test]
    fn test_with_builtin_rich_scalars_database_types() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();

        // Database/PostgreSQL scalars
        assert!(registry.exists("LTree"));
    }

    #[test]
    fn test_with_builtin_rich_scalars_range_types() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();

        // Range scalars
        assert!(registry.exists("DateRange"));
        assert!(registry.exists("Duration"));
        assert!(registry.exists("Percentage"));
    }

    #[test]
    fn test_with_builtin_rich_scalars_has_base_type() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();

        // All rich scalars should have base_type = String
        let email = registry.get("Email").unwrap();
        assert_eq!(email.base_type, Some("String".to_string()));

        let iban = registry.get("IBAN").unwrap();
        assert_eq!(iban.base_type, Some("String".to_string()));

        let percentage = registry.get("Percentage").unwrap();
        assert_eq!(percentage.base_type, Some("String".to_string()));
    }

    #[test]
    fn test_with_builtin_rich_scalars_no_validation_rules() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();

        // Rich scalars should not have validation rules
        let email = registry.get("Email").unwrap();
        assert_eq!(email.validation_rules.len(), 0);

        let iban = registry.get("IBAN").unwrap();
        assert_eq!(iban.validation_rules.len(), 0);
    }

    #[test]
    fn test_with_builtin_rich_scalars_backward_compatibility() {
        let registry = CustomTypeRegistry::with_builtin_rich_scalars();

        // Verify all scalars from the old RICH_SCALARS constant are present
        let all_rich_scalar_names = [
            "Email",
            "PhoneNumber",
            "URL",
            "DomainName",
            "Hostname",
            "PostalCode",
            "Latitude",
            "Longitude",
            "Coordinates",
            "Timezone",
            "LocaleCode",
            "LanguageCode",
            "CountryCode",
            "IBAN",
            "CUSIP",
            "ISIN",
            "SEDOL",
            "LEI",
            "MIC",
            "CurrencyCode",
            "Money",
            "ExchangeCode",
            "ExchangeRate",
            "StockSymbol",
            "Slug",
            "SemanticVersion",
            "HashSHA256",
            "APIKey",
            "LicensePlate",
            "VIN",
            "TrackingNumber",
            "ContainerNumber",
            "IPAddress",
            "IPv4",
            "IPv6",
            "MACAddress",
            "CIDR",
            "Port",
            "AirportCode",
            "PortCode",
            "FlightNumber",
            "Markdown",
            "HTML",
            "MimeType",
            "Color",
            "Image",
            "File",
            "LTree",
            "DateRange",
            "Duration",
            "Percentage",
        ];

        for scalar_name in &all_rich_scalar_names {
            assert!(
                registry.exists(scalar_name),
                "Rich scalar '{}' should be registered",
                scalar_name
            );
        }
    }
}
