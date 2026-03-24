use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use super::config::{CustomTypeDef, CustomTypeRegistryConfig};
use crate::{
    error::{FraiseQLError, Result},
    validation::{ValidationRule, elo_expressions::EloExpressionEvaluator},
};

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
    /// ```
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
    /// ```
    /// use fraiseql_core::validation::{CustomTypeRegistry, CustomTypeDef};
    /// let registry = CustomTypeRegistry::new(Default::default());
    /// registry.register("Email".to_string(), CustomTypeDef::new("Email".to_string())).unwrap();
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
    /// ```
    /// use fraiseql_core::validation::{CustomTypeRegistry, CustomTypeDef};
    /// let registry = CustomTypeRegistry::new(Default::default());
    /// registry.register("Email".to_string(), CustomTypeDef::new("Email".to_string())).unwrap();
    /// if let Some(def) = registry.get("Email") {
    ///     let _ = def.description.unwrap_or_default();
    /// }
    /// ```
    pub fn get(&self, name: &str) -> Option<CustomTypeDef> {
        let types = self.types.read().unwrap_or_else(|e| {
            tracing::error!(
                "CustomTypeRegistry RwLock poisoned during get(); recovering. \
                 A previous thread panicked while holding this lock. \
                 Registry contents may be inconsistent."
            );
            e.into_inner()
        });
        types.get(name).cloned()
    }

    /// Check if a custom scalar type is registered.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::validation::{CustomTypeRegistry, CustomTypeDef};
    /// let registry = CustomTypeRegistry::new(Default::default());
    /// registry.register("Email".to_string(), CustomTypeDef::new("Email".to_string())).unwrap();
    /// assert!(registry.exists("Email"));
    /// assert!(!registry.exists("UnknownType"));
    /// ```
    #[inline]
    pub fn exists(&self, name: &str) -> bool {
        self.types
            .read()
            .unwrap_or_else(|e| {
                tracing::error!(
                    "CustomTypeRegistry RwLock poisoned during exists() check; recovering."
                );
                e.into_inner()
            })
            .contains_key(name)
    }

    /// Remove a custom scalar type definition.
    ///
    /// Returns the removed definition if it existed.
    ///
    /// # Panics
    ///
    /// Panics if the registry `RwLock` is poisoned (a thread panicked while holding the lock).
    pub fn remove(&self, name: &str) -> Option<CustomTypeDef> {
        self.types
            .write()
            .expect(
                "CustomTypeRegistry RwLock poisoned during remove(); \
                 the registry is in an unrecoverable state",
            )
            .remove(name)
    }

    /// Get the number of registered custom scalars.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::validation::{CustomTypeRegistry, CustomTypeDef};
    /// let registry = CustomTypeRegistry::new(Default::default());
    /// assert_eq!(registry.count(), 0);
    /// registry.register("Email".to_string(), CustomTypeDef::new("Email".to_string())).unwrap();
    /// assert_eq!(registry.count(), 1);
    /// ```
    pub fn count(&self) -> usize {
        self.types
            .read()
            .unwrap_or_else(|e| {
                tracing::error!("CustomTypeRegistry RwLock poisoned during count(); recovering.");
                e.into_inner()
            })
            .len()
    }

    /// List all registered custom scalars.
    ///
    /// Returns a vector of (name, definition) tuples.
    pub fn list_all(&self) -> Vec<(String, CustomTypeDef)> {
        self.types
            .read()
            .unwrap_or_else(|e| {
                tracing::error!(
                    "CustomTypeRegistry RwLock poisoned during list_all(); recovering."
                );
                e.into_inner()
            })
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Clear all registered custom scalars.
    ///
    /// # Panics
    ///
    /// Panics if the registry `RwLock` is poisoned (a thread panicked while holding the lock).
    pub fn clear(&self) {
        self.types
            .write()
            .expect(
                "CustomTypeRegistry RwLock poisoned during clear(); \
                 the registry is in an unrecoverable state",
            )
            .clear();
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
    /// ```
    /// use fraiseql_core::validation::CustomTypeRegistry;
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
    /// ```
    /// use fraiseql_core::validation::{CustomTypeRegistry, CustomTypeDef, ValidationRule};
    /// use serde_json::json;
    ///
    /// let registry = CustomTypeRegistry::new(Default::default());
    /// let mut def = CustomTypeDef::new("LibraryCode".to_string());
    /// def.validation_rules = vec![
    ///     ValidationRule::Pattern {
    ///         pattern: r"^LIB-[0-9]{4}$".to_string(),
    ///         message: Some("Must match LIB-NNNN format".to_string()),
    ///     },
    /// ];
    /// registry.register("LibraryCode".to_string(), def).unwrap();
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
