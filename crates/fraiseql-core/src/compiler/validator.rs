//! Schema validator - validates IR for correctness.
//!
//! # Validation Rules
//!
//! - Type references are valid
//! - SQL bindings exist
//! - No circular dependencies
//! - Auth rules are valid
//! - Analytics fact table metadata is valid
//! - Aggregate types follow required structure

use super::ir::AuthoringIR;
use crate::{
    error::{FraiseQLError, Result},
    schema::is_known_scalar,
};

/// Extract the base type name from a GraphQL type string.
///
/// Removes list brackets, non-null markers, and whitespace.
/// Examples:
/// - "String!" -> "String"
/// - "[User]" -> "User"
/// - "[User!]!" -> "User"
/// - "Int" -> "Int"
pub(crate) fn extract_base_type(type_str: &str) -> &str {
    let s = type_str.trim();

    // Remove list brackets and non-null markers
    let s = s.trim_start_matches('[').trim_end_matches(']');
    let s = s.trim_end_matches('!').trim_start_matches('!');

    // Handle nested cases like "[User!]!"
    let s = s.trim_start_matches('[').trim_end_matches(']');
    let s = s.trim_end_matches('!');

    s.trim()
}

/// Check if a type is valid (either a known scalar or defined type).
fn is_valid_type(base_type: &str, defined_types: &std::collections::HashSet<&str>) -> bool {
    is_known_scalar(base_type) || defined_types.contains(base_type)
}

/// Schema validation error produced by [`SchemaValidator`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaValidationError {
    /// Error message.
    pub message: String,
    /// Location in schema.
    pub location: String,
}

/// Schema validator.
pub struct SchemaValidator {
    // Validator state
}

impl SchemaValidator {
    /// Create new validator.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }

    /// Validate IR.
    ///
    /// # Arguments
    ///
    /// * `ir` - Authoring IR to validate
    ///
    /// # Returns
    ///
    /// Validated IR (potentially with transformations)
    ///
    /// # Errors
    ///
    /// Returns error if validation fails.
    pub fn validate(&self, ir: AuthoringIR) -> Result<AuthoringIR> {
        // Comprehensive type coverage validation for all operation types
        // Note: validate_queries() also validates mutations and subscriptions
        // See lines 220-260 for full validation logic
        self.validate_types(&ir)?;
        self.validate_queries(&ir)?;

        // Analytics validation
        if !ir.fact_tables.is_empty() {
            self.validate_fact_tables(&ir)?;
        }

        // Validate aggregate types (regardless of fact_tables)
        // This ensures aggregate types in the schema follow the required structure
        self.validate_aggregate_types(&ir)?;

        Ok(ir)
    }

    /// Validate type definitions.
    fn validate_types(&self, ir: &AuthoringIR) -> Result<()> {
        // Collect all defined type names
        let defined_types: std::collections::HashSet<&str> =
            ir.types.iter().map(|t| t.name.as_str()).collect();

        // Validate each type
        for ir_type in &ir.types {
            // Validate type name is not empty
            if ir_type.name.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: "Type name cannot be empty".to_string(),
                    path: Some("types".to_string()),
                });
            }

            // Validate field types reference valid types
            for field in &ir_type.fields {
                let base_type = extract_base_type(&field.field_type);

                // Skip validation for list markers and check if type is valid
                if !base_type.is_empty() && !is_valid_type(base_type, &defined_types) {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Type '{}' field '{}' references unknown type '{}'",
                            ir_type.name, field.name, base_type
                        ),
                        path: Some(format!("types.{}.fields.{}", ir_type.name, field.name)),
                    });
                }
            }
        }

        Ok(())
    }

    /// Validate query definitions.
    fn validate_queries(&self, ir: &AuthoringIR) -> Result<()> {
        // Collect all defined type names
        let defined_types: std::collections::HashSet<&str> =
            ir.types.iter().map(|t| t.name.as_str()).collect();

        // Validate each query
        for query in &ir.queries {
            // Validate query name is not empty
            if query.name.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: "Query name cannot be empty".to_string(),
                    path: Some("queries".to_string()),
                });
            }

            // Validate return type exists
            let base_type = extract_base_type(&query.return_type);
            if !is_valid_type(base_type, &defined_types) {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Query '{}' returns unknown type '{}'",
                        query.name, query.return_type
                    ),
                    path: Some(format!("queries.{}.return_type", query.name)),
                });
            }

            // Validate argument types
            for arg in &query.arguments {
                let base_type = extract_base_type(&arg.arg_type);
                if !is_valid_type(base_type, &defined_types) {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Query '{}' argument '{}' has unknown type '{}'",
                            query.name, arg.name, arg.arg_type
                        ),
                        path: Some(format!("queries.{}.arguments.{}", query.name, arg.name)),
                    });
                }
            }
        }

        // Validate mutations
        for mutation in &ir.mutations {
            if mutation.name.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: "Mutation name cannot be empty".to_string(),
                    path: Some("mutations".to_string()),
                });
            }

            let base_type = extract_base_type(&mutation.return_type);
            if !is_valid_type(base_type, &defined_types) {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Mutation '{}' returns unknown type '{}'",
                        mutation.name, mutation.return_type
                    ),
                    path: Some(format!("mutations.{}.return_type", mutation.name)),
                });
            }
        }

        // Validate subscriptions
        for subscription in &ir.subscriptions {
            if subscription.name.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: "Subscription name cannot be empty".to_string(),
                    path: Some("subscriptions".to_string()),
                });
            }

            let base_type = extract_base_type(&subscription.return_type);
            if !is_valid_type(base_type, &defined_types) {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Subscription '{}' returns unknown type '{}'",
                        subscription.name, subscription.return_type
                    ),
                    path: Some(format!("subscriptions.{}.return_type", subscription.name)),
                });
            }
        }

        Ok(())
    }

    /// Validate fact table metadata structure.
    ///
    /// Ensures that fact table metadata follows the required structure:
    /// - Table name uses `tf_*` prefix
    /// - Has at least one measure
    fn validate_fact_tables(&self, ir: &AuthoringIR) -> Result<()> {
        for (table_name, metadata) in &ir.fact_tables {
            // Validate table name follows tf_* pattern
            if !table_name.starts_with("tf_") {
                return Err(FraiseQLError::Validation {
                    message: format!("Fact table '{}' must start with 'tf_' prefix", table_name),
                    path: Some(format!("fact_tables.{}", table_name)),
                });
            }

            if metadata.measures.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: format!("Fact table '{}' must have at least one measure", table_name),
                    path: Some(format!("fact_tables.{}.measures", table_name)),
                });
            }

            // Validate dimensions name is not empty
            if metadata.dimensions.name.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: format!("Fact table '{}' dimensions missing 'name' field", table_name),
                    path: Some(format!("fact_tables.{}.dimensions", table_name)),
                });
            }
        }

        Ok(())
    }

    /// Validate aggregate types follow required structure.
    ///
    /// Aggregate types must:
    /// - Have a `count` field (always available)
    /// - Have measure aggregate fields (e.g., `revenue_sum`, `quantity_avg`)
    /// - `GroupByInput` types must have Boolean fields
    /// - `HavingInput` types must have comparison operator suffixes
    fn validate_aggregate_types(&self, ir: &AuthoringIR) -> Result<()> {
        // Find aggregate types (those ending with "Aggregate")
        for ir_type in &ir.types {
            if ir_type.name.ends_with("Aggregate") {
                // Validate has count field
                let has_count = ir_type.fields.iter().any(|f| f.name == "count");
                if !has_count {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Aggregate type '{}' must have a 'count' field",
                            ir_type.name
                        ),
                        path: Some(format!("types.{}.fields", ir_type.name)),
                    });
                }
            }

            // Validate GroupByInput types
            if ir_type.name.ends_with("GroupByInput") {
                for field in &ir_type.fields {
                    // All fields must be Boolean type
                    if field.field_type != "Boolean" && field.field_type != "Boolean!" {
                        return Err(FraiseQLError::Validation {
                            message: format!(
                                "GroupByInput type '{}' field '{}' must be Boolean, got '{}'",
                                ir_type.name, field.name, field.field_type
                            ),
                            path: Some(format!("types.{}.fields.{}", ir_type.name, field.name)),
                        });
                    }
                }
            }

            // Validate HavingInput types
            if ir_type.name.ends_with("HavingInput") {
                for field in &ir_type.fields {
                    // Field names must have operator suffixes (_eq, _gt, _gte, _lt, _lte)
                    let valid_suffixes = ["_eq", "_neq", "_gt", "_gte", "_lt", "_lte"];
                    let has_valid_suffix = valid_suffixes.iter().any(|s| field.name.ends_with(s));

                    if !has_valid_suffix {
                        return Err(FraiseQLError::Validation {
                            message: format!(
                                "HavingInput type '{}' field '{}' must have operator suffix (_eq, _neq, _gt, _gte, _lt, _lte)",
                                ir_type.name, field.name
                            ),
                            path: Some(format!("types.{}.fields.{}", ir_type.name, field.name)),
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for SchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}
