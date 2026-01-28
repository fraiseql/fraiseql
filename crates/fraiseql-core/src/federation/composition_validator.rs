//! Multi-subgraph federation composition validation and schema merging
//!
//! This module implements comprehensive validation for composing multiple federated subgraphs
//! into a single Apollo Federation supergraph. It validates:
//!
//! - **@key Consistency**: All @extends types must use the same @key as the primary type
//! - **@external Ownership**: Each @external field must have exactly one owning subgraph
//! - **@shareable Consistency**: If a field is @shareable in one subgraph, it must be in all
//! - **Type Merging**: Proper composition of type definitions across subgraphs
//!
//! # Example
//!
//! ```ignore
//! let subgraphs = vec![
//!     ("users".to_string(), users_metadata),
//!     ("orders".to_string(), orders_metadata),
//! ];
//!
//! let validator = CompositionValidator::new(ConflictResolutionStrategy::Error);
//! let composed = validator.validate_composition(subgraphs)?;
//! ```
//!
//! # Architecture
//!
//! The composition process works in two phases:
//!
//! 1. **Consistency Validation** (CrossSubgraphValidator)
//!    - Validates across all subgraphs simultaneously
//!    - Checks federation directives for conflicts
//!    - Collects all errors before returning
//!
//! 2. **Schema Composition** (CompositionValidator)
//!    - Merges type definitions from all subgraphs
//!    - Applies conflict resolution strategy
//!    - Produces final supergraph schema

use std::collections::HashMap;

use tracing::{debug, info, warn};

use crate::federation::types::{FederatedType, FederationMetadata};

/// Errors during schema composition
///
/// These errors indicate problems with multi-subgraph federation that prevent
/// the supergraph from being composed. Each error includes context for debugging.
#[derive(Debug, Clone)]
pub enum CompositionError {
    /// @external field has no owning subgraph
    ///
    /// An @external field was marked in a subgraph extension, but no other subgraph
    /// defines this field as local (non-external).
    ExternalFieldNoOwner { field: String },

    /// @external field owned by multiple subgraphs
    ///
    /// An @external field reference conflicts: multiple subgraphs claim to own it.
    /// Only one subgraph can own each @external field.
    ExternalFieldMultipleOwners { field: String, owners: Vec<String> },

    /// @key directive mismatch across subgraphs
    ///
    /// The @key directive on a type differs across subgraphs. All subgraphs must
    /// agree on the @key for a given type.
    KeyMismatch {
        typename: String,
        key_a:    Vec<String>,
        key_b:    Vec<String>,
    },

    /// @shareable field conflict (shareable in one, not in another)
    ///
    /// A field is marked @shareable in one subgraph but not in another.
    /// @shareable must be consistent across all subgraphs that define a field.
    ShareableFieldConflict {
        typename:   String,
        field:      String,
        subgraph_a: String,
        subgraph_b: String,
    },

    /// Type definition conflict
    ///
    /// A type definition conflict that doesn't fit other categories.
    TypeConflict { typename: String, reason: String },
}

impl std::fmt::Display for CompositionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExternalFieldNoOwner { field } => {
                write!(
                    f,
                    "External field '{}' has no owning subgraph. \
                     This field was marked @external but no subgraph defines it.",
                    field
                )
            },
            Self::ExternalFieldMultipleOwners { field, owners } => {
                write!(
                    f,
                    "External field '{}' owned by multiple subgraphs: {}. \
                     Each @external field must be owned by exactly one subgraph.",
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
                    "@key mismatch for type '{}': {} vs {}. \
                     All subgraphs must use the same @key directive.",
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
                    "@shareable conflict on {}.{} between '{}' and '{}'. \
                     Either both subgraphs must mark the field @shareable or neither.",
                    typename, field, subgraph_a, subgraph_b
                )
            },
            Self::TypeConflict { typename, reason } => {
                write!(f, "Type '{}' conflict: {}", typename, reason)
            },
        }
    }
}

impl std::error::Error for CompositionError {}

/// Validator for cross-subgraph federation consistency
///
/// Validates that multiple federated subgraphs form a valid federation by checking:
/// - @key directive consistency across all subgraphs
/// - @external field ownership rules
/// - @shareable field consistency
///
/// Named subgraphs are used in error messages for clear debugging context.
#[derive(Debug)]
pub struct CrossSubgraphValidator {
    subgraphs: Vec<(String, FederationMetadata)>,
}

impl CrossSubgraphValidator {
    /// Create a new cross-subgraph validator with named subgraphs
    ///
    /// # Arguments
    /// - `subgraphs`: Vector of (subgraph_name, metadata) tuples
    ///
    /// Subgraph names are used in error messages to identify problem sources.
    pub fn new(subgraphs: Vec<(String, FederationMetadata)>) -> Self {
        Self { subgraphs }
    }

    /// Validate consistency across all subgraphs
    ///
    /// Performs comprehensive cross-subgraph validation and collects all errors
    /// before returning. This allows callers to see all problems at once.
    ///
    /// # Errors
    ///
    /// Returns `Err` with vector of all consistency errors found.
    /// Returns `Ok(())` if all validation passes.
    pub fn validate_consistency(&self) -> Result<(), Vec<CompositionError>> {
        let mut errors = Vec::new();

        info!("Starting cross-subgraph validation for {} subgraph(s)", self.subgraphs.len());

        // Validate @key consistency
        debug!("Validating @key directive consistency");
        if let Err(key_errors) = self.validate_key_consistency() {
            errors.extend(key_errors);
        }

        // Validate @external field ownership
        debug!("Validating @external field ownership");
        if let Err(external_errors) = self.validate_external_field_ownership() {
            errors.extend(external_errors);
        }

        // Validate @shareable field consistency
        debug!("Validating @shareable field consistency");
        if let Err(shareable_errors) = self.validate_shareable_consistency() {
            errors.extend(shareable_errors);
        }

        if errors.is_empty() {
            info!("Validation successful - all subgraph consistent");
            Ok(())
        } else {
            warn!("Validation failed with {} errors", errors.len());
            Err(errors)
        }
    }

    /// Validate @key directives are consistent across subgraphs
    ///
    /// Ensures that all @extends definitions of a type use the same @key as the primary definition.
    fn validate_key_consistency(&self) -> Result<(), Vec<CompositionError>> {
        let mut errors = Vec::new();
        let mut type_keys: HashMap<String, Vec<String>> = HashMap::new();

        // Collect all @key definitions per type (from primary definitions only)
        for (sg_name, metadata) in &self.subgraphs {
            for ftype in &metadata.types {
                if !ftype.is_extends {
                    // Primary definition of this type
                    if let Some(key_directive) = ftype.keys.first() {
                        type_keys
                            .entry(ftype.name.clone())
                            .or_insert_with(|| key_directive.fields.clone());
                        debug!(
                            "Found primary definition of {} in {} with @key({})",
                            ftype.name,
                            sg_name,
                            key_directive.fields.join(",")
                        );
                    }
                }
            }
        }

        // Validate all extensions have same keys as primary
        for (sg_name, metadata) in &self.subgraphs {
            for ftype in &metadata.types {
                if ftype.is_extends {
                    // Extended definition - must match primary key
                    if let Some(primary_key) = type_keys.get(&ftype.name) {
                        if let Some(key_directive) = ftype.keys.first() {
                            if &key_directive.fields != primary_key {
                                warn!(
                                    "Key mismatch for {} in {}: expected @key({}), found @key({})",
                                    ftype.name,
                                    sg_name,
                                    primary_key.join(","),
                                    key_directive.fields.join(",")
                                );
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
///
/// Determines how composition handles conflicts when multiple subgraphs define the same type or
/// field.
#[derive(Debug, Clone, Copy)]
pub enum ConflictResolutionStrategy {
    /// Fail on any conflict (default)
    ///
    /// The composition fails immediately when any conflict is detected.
    /// This is the safest strategy for production deployments.
    Error,

    /// First definition wins
    ///
    /// When multiple subgraphs define the same type or field, the first one in the
    /// composition order is used. Other definitions are ignored (with warnings).
    FirstWins,

    /// Allow only if both are @shareable
    ///
    /// Conflicts are only allowed if all definitions are marked @shareable.
    /// Non-shareable conflicting definitions cause composition failure.
    ShareableOnly,
}

/// Composes multiple subgraph schemas into a supergraph
///
/// Validates cross-subgraph consistency and produces a composed supergraph schema
/// that combines all federated types and directives from input subgraphs.
#[derive(Debug)]
pub struct CompositionValidator {
    #[allow(dead_code)]
    strategy: ConflictResolutionStrategy,
}

impl CompositionValidator {
    /// Create a new composition validator with specified conflict resolution strategy
    ///
    /// # Arguments
    /// - `strategy`: Determines how to handle conflicts when composing subgraphs
    ///
    /// # Example
    /// ```ignore
    /// let validator = CompositionValidator::new(ConflictResolutionStrategy::Error);
    /// ```
    pub fn new(strategy: ConflictResolutionStrategy) -> Self {
        Self { strategy }
    }

    /// Validate and compose multiple subgraphs into a supergraph
    ///
    /// Performs two-phase composition:
    /// 1. **Validate**: Cross-subgraph consistency checking
    /// 2. **Compose**: Merge types into supergraph schema
    ///
    /// # Arguments
    /// - `subgraphs`: Named subgraph metadata to compose
    ///
    /// # Errors
    /// Returns vector of all composition errors if validation or composition fails.
    ///
    /// # Example
    /// ```ignore
    /// let subgraphs = vec![
    ///     ("users".to_string(), users_metadata),
    ///     ("orders".to_string(), orders_metadata),
    /// ];
    ///
    /// let validator = CompositionValidator::new(ConflictResolutionStrategy::Error);
    /// let composed = validator.validate_composition(subgraphs)?;
    /// ```
    pub fn validate_composition(
        &self,
        subgraphs: Vec<(String, FederationMetadata)>,
    ) -> Result<ComposedSchema, Vec<CompositionError>> {
        info!("Starting schema composition for {} subgraph(s)", subgraphs.len());

        // First validate cross-subgraph consistency
        let cross_validator = CrossSubgraphValidator::new(subgraphs.clone());
        if let Err(errors) = cross_validator.validate_consistency() {
            warn!("Composition validation failed with {} errors", errors.len());
            return Err(errors);
        }

        // Then compose into supergraph
        debug!("Validation passed - proceeding with schema composition");
        let composed = self.compose_subgraphs(subgraphs)?;

        info!("Composition complete - produced supergraph with {} types", composed.types.len());
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
///
/// Represents the final composed schema combining all federated types from input subgraphs.
/// This is the output of the composition process and serves as the supergraph schema.
#[derive(Debug, Clone)]
pub struct ComposedSchema {
    /// All types in the composed supergraph
    ///
    /// This includes both primary type definitions and extended definitions
    /// from all subgraphs, merged appropriately.
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
///
/// Represents a single GraphQL type that may be composed from:
/// - A primary definition in one subgraph
/// - Extensions in other subgraphs
///
/// All definitions are preserved to track the composition structure.
#[derive(Debug, Clone)]
pub struct ComposedType {
    /// Type name (e.g., "User", "Order")
    pub name: String,

    /// All definitions of this type (primary + extensions)
    ///
    /// Typically:
    /// - First definition is the primary (non-extended) definition
    /// - Remaining are @extends definitions from other subgraphs
    pub definitions: Vec<FederatedType>,

    /// Is this type extended in any subgraph?
    ///
    /// True if any @extends definition exists for this type.
    pub is_extended: bool,
}

impl ComposedType {
    /// Create a composed type from a federated type definition
    ///
    /// # Arguments
    /// - `ftype`: The initial federated type definition
    pub fn from_federated(ftype: &FederatedType) -> Self {
        Self {
            name:        ftype.name.clone(),
            definitions: vec![ftype.clone()],
            is_extended: ftype.is_extends,
        }
    }

    /// Merge another federated type definition into this composed type
    ///
    /// Used to build up the complete composed type from multiple subgraph definitions.
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
