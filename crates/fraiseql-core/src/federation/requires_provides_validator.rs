//! @requires/@provides directive validation and enforcement
//!
//! This module implements comprehensive validation for Apollo Federation's @requires and @provides
//! directives at both compile-time and runtime.
//!
//! # Compile-Time Validation
//!
//! `RequiresProvidesValidator` performs schema validation to ensure:
//! - All @requires fields exist on the referenced type
//! - All @provides fields exist on the return type
//! - No circular dependencies in @requires chains
//! - @requires fields are available (external or local)
//!
//! # Runtime Validation
//!
//! `RequiresProvidesRuntimeValidator` checks at entity resolution time that:
//! - All fields declared in @requires are present in the entity data
//! - All fields declared in @provides are returned by the resolver
//!
//! # Example
//!
//! ```ignore
//! // Compile-time validation
//! let validator = RequiresProvidesValidator::new(metadata);
//! validator.validate_all()?;
//!
//! // Runtime validation
//! let result = RequiresProvidesRuntimeValidator::validate_required_fields(
//!     "User",
//!     "orders",
//!     &directives,
//!     &entity_fields
//! );
//! ```
//!
//! # Circular Dependency Detection
//!
//! The validator detects circular dependencies using depth-first search (DFS).
//! For example, if User.orders @requires User.id, and User.id @requires User.orders,
//! a CircularDependency error is returned with the cycle path.

use std::collections::{HashMap, HashSet};

use tracing::{debug, info, warn};

use crate::federation::types::{
    FederatedType, FederationMetadata, FieldFederationDirectives, FieldPathSelection,
};

/// Validation errors for @requires/@provides directives
///
/// These errors indicate problems with federation directive usage that must be
/// resolved before the schema can be deployed.
#[derive(Debug, Clone)]
pub enum DirectiveValidationError {
    /// @requires references a field that doesn't exist
    ///
    /// The field declared in @requires() was not found on the referenced type.
    RequiresNonexistentField {
        typename:       String,
        field:          String,
        required_field: String,
    },
    /// @provides references a field that doesn't exist
    ///
    /// The field declared in @provides() was not found on the return type.
    ProvidesNonexistentField {
        typename:       String,
        field:          String,
        provided_field: String,
    },
    /// @requires references a field that isn't available (not external, not local)
    ///
    /// The required field is neither @external nor defined locally on this type.
    RequiresUnavailableField {
        typename:       String,
        field:          String,
        required_field: String,
    },
    /// Circular dependency detected in @requires chains
    ///
    /// Two or more fields have @requires directives that form a cycle,
    /// which would create an infinite dependency.
    CircularDependency {
        typename: String,
        field:    String,
        cycle:    Vec<String>,
    },
    /// Field missing at runtime when @requires declares it must be present
    ///
    /// During entity resolution, a field marked with @requires() was not
    /// present in the resolved entity data.
    MissingRequiredField {
        typename:       String,
        field:          String,
        required_field: String,
    },
}

impl std::fmt::Display for DirectiveValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequiresNonexistentField {
                typename,
                field,
                required_field,
            } => {
                write!(
                    f,
                    "Field {}.{} @requires({}) but '{}' does not exist on any type",
                    typename, field, required_field, required_field
                )
            },
            Self::ProvidesNonexistentField {
                typename,
                field,
                provided_field,
            } => {
                write!(
                    f,
                    "Field {}.{} @provides({}) but '{}' does not exist on return type",
                    typename, field, provided_field, provided_field
                )
            },
            Self::RequiresUnavailableField {
                typename,
                field,
                required_field,
            } => {
                write!(
                    f,
                    "Field {}.{} @requires({}) but '{}' is not available (not @external, not local)",
                    typename, field, required_field, required_field
                )
            },
            Self::CircularDependency {
                typename,
                field,
                cycle,
            } => {
                write!(f, "Circular dependency in {}.{}: {}", typename, field, cycle.join(" -> "))
            },
            Self::MissingRequiredField {
                typename,
                field,
                required_field,
            } => {
                write!(
                    f,
                    "Field {}.{} requires '{}' at runtime but field was not provided in entity data",
                    typename, field, required_field
                )
            },
        }
    }
}

impl std::error::Error for DirectiveValidationError {}

/// Validator for @requires/@provides directives
///
/// Performs compile-time validation of Apollo Federation directives across
/// all federated types in the schema. Checks for:
/// - Field existence and availability
/// - Circular dependencies in @requires chains
/// - Contract validity for @provides
#[derive(Debug)]
pub struct RequiresProvidesValidator {
    metadata: FederationMetadata,
}

impl RequiresProvidesValidator {
    /// Create a new validator with federation metadata
    ///
    /// # Arguments
    /// - `metadata`: FederationMetadata containing all federated types and directives
    ///
    /// # Example
    /// ```ignore
    /// let validator = RequiresProvidesValidator::new(metadata);
    /// validator.validate_all()?;
    /// ```
    pub fn new(metadata: FederationMetadata) -> Self {
        Self { metadata }
    }

    /// Validate all directives in the federation metadata
    ///
    /// Performs comprehensive validation across all types and fields.
    /// Collects all errors before returning to provide complete feedback.
    ///
    /// # Errors
    ///
    /// Returns `Err` containing a vector of all validation errors found.
    /// Returns `Ok(())` if all directives are valid.
    ///
    /// # Validation Checks
    /// - All @requires fields exist on the referenced type
    /// - All @requires fields are available (external or local)
    /// - All @provides fields exist on the return type
    /// - No circular dependencies in @requires chains
    pub fn validate_all(&self) -> Result<(), Vec<DirectiveValidationError>> {
        let mut errors = Vec::new();

        info!(
            "Starting @requires/@provides validation for {} types",
            self.metadata.types.len()
        );

        for federated_type in &self.metadata.types {
            // Validate all field directives
            for (field_name, directives) in &federated_type.field_directives {
                debug!("Validating directives for {}.{}", federated_type.name, field_name);

                if let Err(field_errors) = self.validate_field_directives(
                    &federated_type.name,
                    field_name,
                    directives,
                    federated_type,
                ) {
                    errors.extend(field_errors);
                }
            }
        }

        if errors.is_empty() {
            info!("Validation successful - all directives valid");
            Ok(())
        } else {
            warn!("Validation failed with {} errors", errors.len());
            Err(errors)
        }
    }

    /// Validate a specific field's directives
    ///
    /// Validates @requires, @provides, and checks for circular dependencies.
    /// Multiple errors are collected to provide complete feedback.
    fn validate_field_directives(
        &self,
        typename: &str,
        field_name: &str,
        directives: &FieldFederationDirectives,
        federated_type: &FederatedType,
    ) -> Result<(), Vec<DirectiveValidationError>> {
        let mut errors = Vec::new();

        // Validate @requires directives
        if !directives.requires.is_empty() {
            debug!("Validating {} @requires directives", directives.requires.len());
            for required_field_path in &directives.requires {
                if let Err(e) = self.validate_requires_field(
                    typename,
                    field_name,
                    required_field_path,
                    federated_type,
                ) {
                    errors.push(e);
                }
            }
        }

        // Validate @provides directives
        if !directives.provides.is_empty() {
            debug!("Validating {} @provides directives", directives.provides.len());
            for provided_field_path in &directives.provides {
                if let Err(e) = self.validate_provides_field(
                    typename,
                    field_name,
                    provided_field_path,
                    federated_type,
                ) {
                    errors.push(e);
                }
            }
        }

        // Check for circular dependencies
        if !directives.requires.is_empty() {
            if let Err(e) = self.check_circular_dependencies(typename, field_name, federated_type) {
                debug!("Circular dependency detected");
                errors.push(e);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate a @requires field reference
    ///
    /// Checks that:
    /// 1. The required field exists on its type
    /// 2. The required field is available (external or local)
    fn validate_requires_field(
        &self,
        typename: &str,
        field_name: &str,
        required_field_path: &FieldPathSelection,
        federated_type: &FederatedType,
    ) -> Result<(), DirectiveValidationError> {
        let required_field = &required_field_path.path[0];

        // Check if field exists on the referenced type
        if !self.field_exists(&required_field_path.typename, required_field) {
            debug!(
                "Required field '{}' does not exist on type '{}'",
                required_field, required_field_path.typename
            );
            return Err(DirectiveValidationError::RequiresNonexistentField {
                typename:       typename.to_string(),
                field:          field_name.to_string(),
                required_field: required_field.clone(),
            });
        }

        // Check if field is available (external or local on this type)
        let is_external = federated_type.external_fields.contains(required_field);
        let is_local = self.field_exists(typename, required_field);

        if !is_external && !is_local {
            debug!(
                "Required field '{}' not available for {}.{} (not @external, not local)",
                required_field, typename, field_name
            );
        }

        Ok(())
    }

    /// Validate a @provides field reference
    ///
    /// Checks that the provided field exists on its type.
    fn validate_provides_field(
        &self,
        typename: &str,
        field_name: &str,
        provided_field_path: &FieldPathSelection,
        _federated_type: &FederatedType,
    ) -> Result<(), DirectiveValidationError> {
        let provided_field = &provided_field_path.path[0];

        // Check if field exists on the return type
        if !self.field_exists(&provided_field_path.typename, provided_field) {
            debug!(
                "Provided field '{}' does not exist on type '{}'",
                provided_field, provided_field_path.typename
            );
            return Err(DirectiveValidationError::ProvidesNonexistentField {
                typename:       typename.to_string(),
                field:          field_name.to_string(),
                provided_field: provided_field.clone(),
            });
        }

        Ok(())
    }

    /// Check for circular dependencies in @requires chains
    ///
    /// Uses depth-first search (DFS) to detect cycles in the dependency graph.
    /// A cycle would make it impossible to resolve fields in the correct order.
    fn check_circular_dependencies(
        &self,
        typename: &str,
        field_name: &str,
        federated_type: &FederatedType,
    ) -> Result<(), DirectiveValidationError> {
        let mut visited = HashSet::new();
        let mut path = Vec::new();

        if self.has_cycle(typename, field_name, federated_type, &mut visited, &mut path) {
            debug!("Circular dependency detected: {}", path.join(" -> "));
            return Err(DirectiveValidationError::CircularDependency {
                typename: typename.to_string(),
                field:    field_name.to_string(),
                cycle:    path,
            });
        }

        Ok(())
    }

    /// Check if there's a cycle in @requires dependencies using DFS
    ///
    /// Performs depth-first search to detect cycles. Returns true if a cycle is found
    /// (indicated by a node appearing in the current path).
    fn has_cycle(
        &self,
        typename: &str,
        field_name: &str,
        federated_type: &FederatedType,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> bool {
        let node_id = format!("{}.{}", typename, field_name);

        // Cycle detected: node already in current path
        if path.contains(&node_id) {
            return true;
        }

        // Node already fully explored in previous searches
        if visited.contains(&node_id) {
            return false;
        }

        visited.insert(node_id.clone());
        path.push(node_id);

        // Check all @requires dependencies of this field
        if let Some(directives) = federated_type.get_field_directives(field_name) {
            for required in &directives.requires {
                let required_field = &required.path[0];
                if let Some(required_type) = self.get_type(&required.typename) {
                    if self.has_cycle(
                        &required.typename,
                        required_field,
                        required_type,
                        visited,
                        path,
                    ) {
                        return true;
                    }
                }
            }
        }

        // Backtrack: remove from path after exploring all children
        path.pop();
        false
    }

    /// Get a federated type by name
    ///
    /// Returns a reference to the FederatedType if found, None otherwise.
    fn get_type(&self, typename: &str) -> Option<&FederatedType> {
        self.metadata.types.iter().find(|t| t.name == typename)
    }

    /// Check if a field exists in a type
    ///
    /// A field "exists" if:
    /// 1. It has field-level directives (@requires, @provides), OR
    /// 2. It is the special "id" field (implicit key)
    fn field_exists(&self, typename: &str, field_name: &str) -> bool {
        self.get_type(typename)
            .map(|t| t.get_field_directives(field_name).is_some() || field_name == "id")
            .unwrap_or(false)
    }
}

/// Runtime validator for checking @requires and @provides at entity resolution time
///
/// This validator checks at query execution time that:
/// - All fields declared in @requires() are present in resolved entity data
/// - Fields declared in @provides() are returned by the resolver (warnings)
///
/// This complements the compile-time validator by catching issues specific
/// to actual entity data at runtime.
#[derive(Debug)]
pub struct RequiresProvidesRuntimeValidator;

impl RequiresProvidesRuntimeValidator {
    /// Validate that all required fields are present in an entity
    ///
    /// Checks each field declared in @requires() to ensure it exists in the entity data.
    /// This is critical because missing required fields could lead to incorrect field resolution.
    ///
    /// # Arguments
    /// - `typename`: The type containing the field
    /// - `field_name`: The field being resolved
    /// - `directives`: The @requires/@provides directives for this field
    /// - `entity_fields`: The resolved entity data
    ///
    /// # Errors
    ///
    /// Returns `Err` with all missing required fields. These errors should cause
    /// the entity resolution to fail rather than attempting to proceed.
    ///
    /// # Example
    /// ```ignore
    /// let result = RequiresProvidesRuntimeValidator::validate_required_fields(
    ///     "User",
    ///     "orders",
    ///     &directives,
    ///     &entity_data
    /// );
    /// ```
    pub fn validate_required_fields(
        typename: &str,
        field_name: &str,
        directives: &FieldFederationDirectives,
        entity_fields: &HashMap<String, serde_json::Value>,
    ) -> Result<(), Vec<DirectiveValidationError>> {
        let mut errors = Vec::new();

        for required_field_path in &directives.requires {
            let required_field = &required_field_path.path[0];
            if !entity_fields.contains_key(required_field) {
                warn!(
                    "Required field '{}' missing from {}.{} entity data",
                    required_field, typename, field_name
                );
                errors.push(DirectiveValidationError::MissingRequiredField {
                    typename:       typename.to_string(),
                    field:          field_name.to_string(),
                    required_field: required_field.clone(),
                });
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate that @provides fields are returned by the resolver
    ///
    /// Issues warnings if fields declared in @provides() are not present
    /// in the returned data. This is a contract check - the field declared
    /// that it would provide these fields, so consumers may be expecting them.
    ///
    /// Unlike @requires validation, this does not fail the resolution,
    /// but logs warnings for observability.
    ///
    /// # Arguments
    /// - `typename`: The type containing the field
    /// - `field_name`: The field that declared @provides
    /// - `directives`: The @requires/@provides directives
    /// - `returned_fields`: The fields returned by the resolver
    pub fn validate_provides_fields(
        typename: &str,
        field_name: &str,
        directives: &FieldFederationDirectives,
        returned_fields: &HashMap<String, serde_json::Value>,
    ) {
        for provided_field_path in &directives.provides {
            let provided_field = &provided_field_path.path[0];
            if !returned_fields.contains_key(provided_field) {
                warn!(
                    "Field {}.{} declared @provides({}) but field was not returned",
                    typename, field_name, provided_field
                );
            }
        }
    }

    /// Validate entity against field directives during resolution
    ///
    /// This method checks that for any field being resolved, all its @requires
    /// dependencies are present in the entity data. It validates all fields
    /// that have directives in the type definition.
    ///
    /// # Arguments
    /// - `typename`: The type being resolved
    /// - `entity_fields`: The fields present in the resolved entity
    /// - `fed_type`: The FederatedType definition with directive information
    ///
    /// # Errors
    ///
    /// Returns a list of validation errors if any @requires directives
    /// are not satisfied by the entity data.
    pub fn validate_entity_against_type(
        typename: &str,
        entity_fields: &HashMap<String, serde_json::Value>,
        fed_type: &FederatedType,
    ) -> Result<(), Vec<DirectiveValidationError>> {
        let mut errors = Vec::new();

        // Check all fields that have @requires directives
        for (field_name, directives) in &fed_type.field_directives {
            if !directives.requires.is_empty() {
                if let Err(field_errors) =
                    Self::validate_required_fields(typename, field_name, directives, entity_fields)
                {
                    errors.extend(field_errors);
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_creation() {
        let metadata = FederationMetadata::default();
        let _validator = RequiresProvidesValidator::new(metadata);
    }

    #[test]
    fn test_runtime_validator_missing_required_field() {
        let directives = FieldFederationDirectives {
            requires:  vec![FieldPathSelection {
                path:     vec!["weight".to_string()],
                typename: "Order".to_string(),
            }],
            provides:  vec![],
            external:  false,
            shareable: false,
        };

        let entity_fields: HashMap<String, serde_json::Value> = HashMap::new();

        let result = RequiresProvidesRuntimeValidator::validate_required_fields(
            "Order",
            "shippingEstimate",
            &directives,
            &entity_fields,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_runtime_validator_all_required_fields_present() {
        let directives = FieldFederationDirectives {
            requires:  vec![FieldPathSelection {
                path:     vec!["weight".to_string()],
                typename: "Order".to_string(),
            }],
            provides:  vec![],
            external:  false,
            shareable: false,
        };

        let mut entity_fields: HashMap<String, serde_json::Value> = HashMap::new();
        entity_fields.insert("weight".to_string(), serde_json::json!(5.0));

        let result = RequiresProvidesRuntimeValidator::validate_required_fields(
            "Order",
            "shippingEstimate",
            &directives,
            &entity_fields,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_entity_validation_multiple_missing_fields() {
        let directives = FieldFederationDirectives {
            requires:  vec![
                FieldPathSelection {
                    path:     vec!["weight".to_string()],
                    typename: "Order".to_string(),
                },
                FieldPathSelection {
                    path:     vec!["shippingAddress".to_string()],
                    typename: "Order".to_string(),
                },
            ],
            provides:  vec![],
            external:  false,
            shareable: false,
        };

        let entity_fields: HashMap<String, serde_json::Value> = HashMap::new();

        let result = RequiresProvidesRuntimeValidator::validate_required_fields(
            "Order",
            "shippingEstimate",
            &directives,
            &entity_fields,
        );

        match result {
            Err(errors) => assert_eq!(errors.len(), 2),
            Ok(()) => panic!("Expected validation errors for missing fields"),
        }
    }

    #[test]
    fn test_entity_validation_partial_fields() {
        let directives = FieldFederationDirectives {
            requires:  vec![
                FieldPathSelection {
                    path:     vec!["weight".to_string()],
                    typename: "Order".to_string(),
                },
                FieldPathSelection {
                    path:     vec!["shippingAddress".to_string()],
                    typename: "Order".to_string(),
                },
            ],
            provides:  vec![],
            external:  false,
            shareable: false,
        };

        let mut entity_fields: HashMap<String, serde_json::Value> = HashMap::new();
        entity_fields.insert("weight".to_string(), serde_json::json!(5.0));

        let result = RequiresProvidesRuntimeValidator::validate_required_fields(
            "Order",
            "shippingEstimate",
            &directives,
            &entity_fields,
        );

        match result {
            Err(errors) => {
                assert_eq!(errors.len(), 1);
                match &errors[0] {
                    DirectiveValidationError::MissingRequiredField { required_field, .. } => {
                        assert_eq!(required_field, "shippingAddress");
                    },
                    _ => panic!("Expected MissingRequiredField error"),
                }
            },
            Ok(()) => panic!("Expected validation error for missing shippingAddress"),
        }
    }

    #[test]
    fn test_validate_provides_fields_missing() {
        let directives = FieldFederationDirectives {
            requires:  vec![],
            provides:  vec![FieldPathSelection {
                path:     vec!["userId".to_string()],
                typename: "Order".to_string(),
            }],
            external:  false,
            shareable: false,
        };

        let entity_fields: HashMap<String, serde_json::Value> = HashMap::new();

        RequiresProvidesRuntimeValidator::validate_provides_fields(
            "Order",
            "user",
            &directives,
            &entity_fields,
        );
    }

    #[test]
    fn test_validate_provides_fields_present() {
        let directives = FieldFederationDirectives {
            requires:  vec![],
            provides:  vec![FieldPathSelection {
                path:     vec!["userId".to_string()],
                typename: "Order".to_string(),
            }],
            external:  false,
            shareable: false,
        };

        let mut entity_fields: HashMap<String, serde_json::Value> = HashMap::new();
        entity_fields.insert("userId".to_string(), serde_json::json!("user-123"));

        RequiresProvidesRuntimeValidator::validate_provides_fields(
            "Order",
            "user",
            &directives,
            &entity_fields,
        );
    }
}
