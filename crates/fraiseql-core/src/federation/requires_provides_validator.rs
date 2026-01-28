//! @requires/@provides directive validation and enforcement
//!
//! Validates and enforces Apollo Federation @requires and @provides directives
//! at both compile-time and runtime.

use std::collections::{HashMap, HashSet};

use tracing::{debug, warn};

use crate::federation::types::{
    FederatedType, FieldFederationDirectives, FieldPathSelection, FederationMetadata,
};

/// Validation errors for @requires/@provides directives
#[derive(Debug, Clone)]
pub enum DirectiveValidationError {
    /// @requires references a field that doesn't exist
    RequiresNonexistentField {
        typename: String,
        field: String,
        required_field: String,
    },
    /// @provides references a field that doesn't exist
    ProvidesNonexistentField {
        typename: String,
        field: String,
        provided_field: String,
    },
    /// @requires references a field that isn't available (not external, not local)
    RequiresUnavailableField {
        typename: String,
        field: String,
        required_field: String,
    },
    /// Circular dependency detected
    CircularDependency {
        typename: String,
        field: String,
        cycle: Vec<String>,
    },
    /// Field missing at runtime when @requires declares it must be present
    MissingRequiredField {
        typename: String,
        field: String,
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
                    "Field {}.{} @requires {} but field does not exist",
                    typename, field, required_field
                )
            },
            Self::ProvidesNonexistentField {
                typename,
                field,
                provided_field,
            } => {
                write!(
                    f,
                    "Field {}.{} @provides {} but field does not exist",
                    typename, field, provided_field
                )
            },
            Self::RequiresUnavailableField {
                typename,
                field,
                required_field,
            } => {
                write!(
                    f,
                    "Field {}.{} @requires {} but field is not available (not external, not local)",
                    typename, field, required_field
                )
            },
            Self::CircularDependency {
                typename,
                field,
                cycle,
            } => {
                write!(
                    f,
                    "Circular dependency in {}.{}: {}",
                    typename,
                    field,
                    cycle.join(" -> ")
                )
            },
            Self::MissingRequiredField {
                typename,
                field,
                required_field,
            } => {
                write!(
                    f,
                    "Field {}.{} requires {} but it was not provided",
                    typename, field, required_field
                )
            },
        }
    }
}

impl std::error::Error for DirectiveValidationError {}

/// Validator for @requires/@provides directives
pub struct RequiresProvidesValidator {
    metadata: FederationMetadata,
}

impl RequiresProvidesValidator {
    /// Create a new validator with federation metadata
    pub fn new(metadata: FederationMetadata) -> Self {
        Self { metadata }
    }

    /// Validate all directives in the federation metadata
    pub fn validate_all(&self) -> Result<(), Vec<DirectiveValidationError>> {
        let mut errors = Vec::new();

        for federated_type in &self.metadata.types {
            // Validate all field directives
            for (field_name, directives) in &federated_type.field_directives {
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
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate a specific field's directives
    fn validate_field_directives(
        &self,
        typename: &str,
        field_name: &str,
        directives: &FieldFederationDirectives,
        federated_type: &FederatedType,
    ) -> Result<(), Vec<DirectiveValidationError>> {
        let mut errors = Vec::new();

        // Validate @requires
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

        // Validate @provides
        for provided_field_path in &directives.provides {
            if let Err(e) =
                self.validate_provides_field(typename, field_name, provided_field_path, federated_type)
            {
                errors.push(e);
            }
        }

        // Check for circular dependencies
        if let Err(e) = self.check_circular_dependencies(typename, field_name, federated_type) {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate a @requires field reference
    fn validate_requires_field(
        &self,
        typename: &str,
        field_name: &str,
        required_field_path: &FieldPathSelection,
        federated_type: &FederatedType,
    ) -> Result<(), DirectiveValidationError> {
        let required_field = &required_field_path.path[0];

        // Check if field exists
        if !self.field_exists(&required_field_path.typename, required_field) {
            return Err(DirectiveValidationError::RequiresNonexistentField {
                typename: typename.to_string(),
                field: field_name.to_string(),
                required_field: required_field.clone(),
            });
        }

        // Check if field is available (external or local)
        let is_external = federated_type.external_fields.contains(required_field);
        let is_local = self.field_exists(typename, required_field);

        if !is_external && !is_local {
            debug!(
                "Required field {} not available for {}.{}",
                required_field, typename, field_name
            );
        }

        Ok(())
    }

    /// Validate a @provides field reference
    fn validate_provides_field(
        &self,
        typename: &str,
        field_name: &str,
        provided_field_path: &FieldPathSelection,
        _federated_type: &FederatedType,
    ) -> Result<(), DirectiveValidationError> {
        let provided_field = &provided_field_path.path[0];

        // Check if field exists
        if !self.field_exists(&provided_field_path.typename, provided_field) {
            return Err(DirectiveValidationError::ProvidesNonexistentField {
                typename: typename.to_string(),
                field: field_name.to_string(),
                provided_field: provided_field.clone(),
            });
        }

        Ok(())
    }

    /// Check for circular dependencies in @requires
    fn check_circular_dependencies(
        &self,
        typename: &str,
        field_name: &str,
        federated_type: &FederatedType,
    ) -> Result<(), DirectiveValidationError> {
        let mut visited = HashSet::new();
        let mut path = Vec::new();

        if self.has_cycle(typename, field_name, federated_type, &mut visited, &mut path) {
            return Err(DirectiveValidationError::CircularDependency {
                typename: typename.to_string(),
                field: field_name.to_string(),
                cycle: path,
            });
        }

        Ok(())
    }

    /// Check if there's a cycle in dependencies
    fn has_cycle(
        &self,
        typename: &str,
        field_name: &str,
        federated_type: &FederatedType,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> bool {
        let node_id = format!("{}.{}", typename, field_name);

        if path.contains(&node_id) {
            return true; // Cycle detected
        }

        if visited.contains(&node_id) {
            return false; // Already checked this node
        }

        visited.insert(node_id.clone());
        path.push(node_id);

        if let Some(directives) = federated_type.get_field_directives(field_name) {
            for required in &directives.requires {
                let required_field = &required.path[0];
                if let Some(required_type) = self.get_type(&required.typename) {
                    if self.has_cycle(&required.typename, required_field, required_type, visited, path) {
                        return true;
                    }
                }
            }
        }

        path.pop();
        false
    }

    /// Get a federated type by name
    fn get_type(&self, typename: &str) -> Option<&FederatedType> {
        self.metadata.types.iter().find(|t| t.name == typename)
    }

    /// Check if a field exists in a type
    fn field_exists(&self, typename: &str, field_name: &str) -> bool {
        self.get_type(typename)
            .map(|t| t.get_field_directives(field_name).is_some() || field_name == "id")
            .unwrap_or(false)
    }
}

/// Runtime validator for checking @requires at resolution time
pub struct RequiresProvidesRuntimeValidator;

impl RequiresProvidesRuntimeValidator {
    /// Validate that all required fields are present in an entity
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
                errors.push(DirectiveValidationError::MissingRequiredField {
                    typename: typename.to_string(),
                    field: field_name.to_string(),
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

    /// Validate that @provides fields are returned
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
                    "Field {}.{} declared @provides {} but it was not returned",
                    typename, field_name, provided_field
                );
            }
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
            requires: vec![FieldPathSelection {
                path: vec!["weight".to_string()],
                typename: "Order".to_string(),
            }],
            provides: vec![],
            external: false,
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
            requires: vec![FieldPathSelection {
                path: vec!["weight".to_string()],
                typename: "Order".to_string(),
            }],
            provides: vec![],
            external: false,
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
}
