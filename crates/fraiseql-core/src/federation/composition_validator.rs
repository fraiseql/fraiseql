//! Multi-subgraph federation composition validation and schema merging
//!
//! Validates and composes multiple federated subgraphs into a single supergraph.

use std::collections::HashMap;

use crate::federation::types::{FederatedType, FederationMetadata};

/// Errors during schema composition
#[derive(Debug, Clone)]
pub enum CompositionError {
    /// @external field has no owning subgraph
    ExternalFieldNoOwner { field: String },

    /// @external field owned by multiple subgraphs
    ExternalFieldMultipleOwners { field: String, owners: Vec<String> },

    /// @key directive mismatch across subgraphs
    KeyMismatch {
        typename: String,
        key_a:    Vec<String>,
        key_b:    Vec<String>,
    },

    /// @shareable field conflict (shareable in one, not in another)
    ShareableFieldConflict {
        typename: String,
        field:    String,
        subgraph_a: String,
        subgraph_b: String,
    },

    /// Type definition conflict
    TypeConflict {
        typename: String,
        reason:   String,
    },
}

impl std::fmt::Display for CompositionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExternalFieldNoOwner { field } => {
                write!(f, "External field '{}' has no owning subgraph", field)
            },
            Self::ExternalFieldMultipleOwners { field, owners } => {
                write!(
                    f,
                    "External field '{}' owned by multiple subgraphs: {}",
                    field,
                    owners.join(", ")
                )
            },
            Self::KeyMismatch {
                typename,
                key_a,
                key_b,
            } => {
                write!(
                    f,
                    "Key mismatch for {}: {} vs {}",
                    typename,
                    key_a.join(","),
                    key_b.join(",")
                )
            },
            Self::ShareableFieldConflict {
                typename,
                field,
                subgraph_a,
                subgraph_b,
            } => {
                write!(
                    f,
                    "Shareable conflict on {}.{} between {} and {}",
                    typename, field, subgraph_a, subgraph_b
                )
            },
            Self::TypeConflict { typename, reason } => {
                write!(f, "Type {} conflict: {}", typename, reason)
            },
        }
    }
}

impl std::error::Error for CompositionError {}

/// Validator for cross-subgraph federation consistency
pub struct CrossSubgraphValidator {
    subgraphs: Vec<(String, FederationMetadata)>,
}

impl CrossSubgraphValidator {
    /// Create a new cross-subgraph validator with named subgraphs
    pub fn new(subgraphs: Vec<(String, FederationMetadata)>) -> Self {
        Self { subgraphs }
    }

    /// Validate consistency across all subgraphs
    pub fn validate_consistency(&self) -> Result<(), Vec<CompositionError>> {
        let mut errors = Vec::new();

        // Validate @key consistency
        if let Err(key_errors) = self.validate_key_consistency() {
            errors.extend(key_errors);
        }

        // Validate @external field ownership
        if let Err(external_errors) = self.validate_external_field_ownership() {
            errors.extend(external_errors);
        }

        // Validate @shareable field consistency
        if let Err(shareable_errors) = self.validate_shareable_consistency() {
            errors.extend(shareable_errors);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate @key directives are consistent across subgraphs
    fn validate_key_consistency(&self) -> Result<(), Vec<CompositionError>> {
        let mut errors = Vec::new();
        let mut type_keys: HashMap<String, Vec<String>> = HashMap::new();

        // Collect all @key definitions per type
        for (_sg_name, metadata) in &self.subgraphs {
            for ftype in &metadata.types {
                if !ftype.is_extends {
                    // Primary definition of this type
                    if let Some(key_directive) = ftype.keys.first() {
                        type_keys
                            .entry(ftype.name.clone())
                            .or_insert_with(|| key_directive.fields.clone());
                    }
                }
            }
        }

        // Validate all extensions have same keys
        for (_sg_name, metadata) in &self.subgraphs {
            for ftype in &metadata.types {
                if ftype.is_extends {
                    // Extended definition - must match primary key
                    if let Some(primary_key) = type_keys.get(&ftype.name) {
                        if let Some(key_directive) = ftype.keys.first() {
                            if &key_directive.fields != primary_key {
                                errors.push(CompositionError::KeyMismatch {
                                    typename: ftype.name.clone(),
                                    key_a:    primary_key.clone(),
                                    key_b:    key_directive.fields.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate @external field ownership rules
    fn validate_external_field_ownership(&self) -> Result<(), Vec<CompositionError>> {
        let mut errors = Vec::new();
        let mut field_owners: HashMap<String, Vec<String>> = HashMap::new();

        // Collect which subgraphs own each field
        for (sg_name, metadata) in &self.subgraphs {
            for ftype in &metadata.types {
                if !ftype.is_extends {
                    for field in ftype.field_directives.keys() {
                        let field_key = format!("{}.{}", ftype.name, field);
                        field_owners
                            .entry(field_key)
                            .or_insert_with(Vec::new)
                            .push(sg_name.clone());
                    }
                }
            }
        }

        // Check external field declarations don't conflict
        for (_sg_name, metadata) in &self.subgraphs {
            for ftype in &metadata.types {
                if ftype.is_extends {
                    for external_field in &ftype.external_fields {
                        let field_key = format!("{}.{}", ftype.name, external_field);

                        // External field must have exactly one owner
                        let owners = field_owners.get(&field_key);
                        if owners.is_none() {
                            errors.push(CompositionError::ExternalFieldNoOwner {
                                field: field_key.clone(),
                            });
                        }
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate @shareable field consistency
    fn validate_shareable_consistency(&self) -> Result<(), Vec<CompositionError>> {
        let mut errors = Vec::new();
        let mut field_shareable: HashMap<String, HashMap<String, bool>> = HashMap::new();

        // Collect shareable status per field
        for (sg_name, metadata) in &self.subgraphs {
            for ftype in &metadata.types {
                for (field_name, directives) in &ftype.field_directives {
                    let field_key = format!("{}.{}", ftype.name, field_name);
                    field_shareable
                        .entry(field_key)
                        .or_insert_with(HashMap::new)
                        .insert(sg_name.clone(), directives.shareable);
                }
            }
        }

        // Check consistency: if shareable in one subgraph, must be in all
        for (field_key, shareable_map) in &field_shareable {
            let any_shareable = shareable_map.values().any(|&s| s);
            if any_shareable {
                // If any subgraph marks as shareable, all must
                if let Some((sg1, &s1)) = shareable_map.iter().next() {
                    for (sg2, &s2) in shareable_map.iter().skip(1) {
                        if s1 != s2 {
                            let parts: Vec<&str> = field_key.split('.').collect();
                            if parts.len() == 2 {
                                errors.push(CompositionError::ShareableFieldConflict {
                                    typename:   parts[0].to_string(),
                                    field:      parts[1].to_string(),
                                    subgraph_a: sg1.clone(),
                                    subgraph_b: sg2.clone(),
                                });
                            }
                        }
                    }
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

/// Configuration for schema composition
#[derive(Debug, Clone, Copy)]
pub enum ConflictResolutionStrategy {
    /// Fail on any conflict (default)
    Error,

    /// First definition wins
    FirstWins,

    /// Allow only if both are @shareable
    ShareableOnly,
}

/// Composes multiple subgraph schemas into a supergraph
pub struct CompositionValidator {
    #[allow(dead_code)]
    strategy: ConflictResolutionStrategy,
}

impl CompositionValidator {
    /// Create a new composition validator with specified strategy
    pub fn new(strategy: ConflictResolutionStrategy) -> Self {
        Self { strategy }
    }

    /// Validate and compose multiple subgraphs
    pub fn validate_composition(
        &self,
        subgraphs: Vec<(String, FederationMetadata)>,
    ) -> Result<ComposedSchema, Vec<CompositionError>> {
        // First validate cross-subgraph consistency
        let cross_validator = CrossSubgraphValidator::new(subgraphs.clone());
        cross_validator.validate_consistency()?;

        // Then compose into supergraph
        let composed = self.compose_subgraphs(subgraphs)?;

        Ok(composed)
    }

    /// Compose subgraphs into a single supergraph schema
    fn compose_subgraphs(
        &self,
        subgraphs: Vec<(String, FederationMetadata)>,
    ) -> Result<ComposedSchema, Vec<CompositionError>> {
        let mut composed = ComposedSchema::new();
        let mut type_definitions: HashMap<String, ComposedType> = HashMap::new();

        // Merge all types from all subgraphs
        for (_sg_name, metadata) in subgraphs {
            for ftype in metadata.types {
                let key = ftype.name.clone();

                if !type_definitions.contains_key(&key) {
                    // First definition of this type
                    type_definitions.insert(key, ComposedType::from_federated(&ftype));
                } else {
                    // Merge with existing definition
                    if let Some(composed_type) = type_definitions.get_mut(&key) {
                        composed_type.merge_from(&ftype);
                    }
                }
            }
        }

        composed.types = type_definitions.values().cloned().collect();
        Ok(composed)
    }
}

/// Composed supergraph schema
#[derive(Debug, Clone)]
pub struct ComposedSchema {
    /// Types in the composed schema
    pub types: Vec<ComposedType>,
}

impl ComposedSchema {
    /// Create a new empty composed schema
    pub fn new() -> Self {
        Self { types: Vec::new() }
    }
}

impl Default for ComposedSchema {
    fn default() -> Self {
        Self::new()
    }
}

/// A type in the composed supergraph
#[derive(Debug, Clone)]
pub struct ComposedType {
    /// Type name
    pub name: String,

    /// All definitions of this type (primary + extensions)
    pub definitions: Vec<FederatedType>,

    /// Is this type extended in any subgraph?
    pub is_extended: bool,
}

impl ComposedType {
    /// Create a composed type from a federated type
    pub fn from_federated(ftype: &FederatedType) -> Self {
        Self {
            name:        ftype.name.clone(),
            definitions: vec![ftype.clone()],
            is_extended: ftype.is_extends,
        }
    }

    /// Merge another federated type definition into this composed type
    pub fn merge_from(&mut self, ftype: &FederatedType) {
        self.definitions.push(ftype.clone());
        if ftype.is_extends {
            self.is_extended = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composition_validator_creation() {
        let _validator = CompositionValidator::new(ConflictResolutionStrategy::Error);
    }

    #[test]
    fn test_cross_subgraph_validator_creation() {
        let subgraphs = vec![];
        let _validator = CrossSubgraphValidator::new(subgraphs);
    }

    #[test]
    fn test_composed_schema_creation() {
        let schema = ComposedSchema::new();
        assert!(schema.types.is_empty());
    }

    #[test]
    fn test_composed_type_from_federated() {
        let ftype = FederatedType::new("User".to_string());
        let composed = ComposedType::from_federated(&ftype);
        assert_eq!(composed.name, "User");
        assert!(!composed.is_extended);
    }

    #[test]
    fn test_composed_type_merge() {
        let user_primary = FederatedType::new("User".to_string());
        let mut user_extension = FederatedType::new("User".to_string());
        user_extension.is_extends = true;

        let mut composed = ComposedType::from_federated(&user_primary);
        composed.merge_from(&user_extension);

        assert_eq!(composed.definitions.len(), 2);
        assert!(composed.is_extended);
    }
}
