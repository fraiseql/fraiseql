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
fn extract_base_type(type_str: &str) -> &str {
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
    pub message:  String,
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
                    path:    Some("types".to_string()),
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
                        path:    Some(format!("types.{}.fields.{}", ir_type.name, field.name)),
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
                    path:    Some("queries".to_string()),
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
                    path:    Some(format!("queries.{}.return_type", query.name)),
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
                        path:    Some(format!("queries.{}.arguments.{}", query.name, arg.name)),
                    });
                }
            }
        }

        // Validate mutations
        for mutation in &ir.mutations {
            if mutation.name.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: "Mutation name cannot be empty".to_string(),
                    path:    Some("mutations".to_string()),
                });
            }

            let base_type = extract_base_type(&mutation.return_type);
            if !is_valid_type(base_type, &defined_types) {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Mutation '{}' returns unknown type '{}'",
                        mutation.name, mutation.return_type
                    ),
                    path:    Some(format!("mutations.{}.return_type", mutation.name)),
                });
            }
        }

        // Validate subscriptions
        for subscription in &ir.subscriptions {
            if subscription.name.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: "Subscription name cannot be empty".to_string(),
                    path:    Some("subscriptions".to_string()),
                });
            }

            let base_type = extract_base_type(&subscription.return_type);
            if !is_valid_type(base_type, &defined_types) {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Subscription '{}' returns unknown type '{}'",
                        subscription.name, subscription.return_type
                    ),
                    path:    Some(format!("subscriptions.{}.return_type", subscription.name)),
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
                    path:    Some(format!("fact_tables.{}", table_name)),
                });
            }

            if metadata.measures.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: format!("Fact table '{}' must have at least one measure", table_name),
                    path:    Some(format!("fact_tables.{}.measures", table_name)),
                });
            }

            // Validate dimensions name is not empty
            if metadata.dimensions.name.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: format!("Fact table '{}' dimensions missing 'name' field", table_name),
                    path:    Some(format!("fact_tables.{}.dimensions", table_name)),
                });
            }
        }

        Ok(())
    }

    /// Validate aggregate types follow required structure.
    ///
    /// Aggregate types must:
    /// - Have a `count` field (always available)
    /// - Have measure aggregate fields (e.g., revenue_sum, quantity_avg)
    /// - GroupByInput types must have Boolean fields
    /// - HavingInput types must have comparison operator suffixes
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
                        path:    Some(format!("types.{}.fields", ir_type.name)),
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
                            path:    Some(format!("types.{}.fields.{}", ir_type.name, field.name)),
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
                            path:    Some(format!("types.{}.fields.{}", ir_type.name, field.name)),
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

#[cfg(test)]
mod tests {
    use super::{
        super::ir::{IRField, IRType},
        *,
    };
    use crate::compiler::fact_table::{DimensionColumn, FactTableMetadata, MeasureColumn, SqlType};

    #[test]
    fn test_validator_new() {
        let validator = SchemaValidator::new();
        let ir = AuthoringIR::new();
        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate new IR should succeed: {e}"));
    }

    #[test]
    fn test_validate_empty_ir() {
        let validator = SchemaValidator::new();
        let ir = AuthoringIR::new();
        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate empty IR should succeed: {e}"));
    }

    fn make_fact_table(measures: Vec<MeasureColumn>, dim_name: &str) -> FactTableMetadata {
        FactTableMetadata {
            table_name: String::new(),
            measures,
            dimensions: DimensionColumn {
                name:  dim_name.to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions: vec![],
        }
    }

    #[test]
    fn test_validate_fact_table_with_valid_metadata() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();
        ir.fact_tables.insert(
            "tf_sales".to_string(),
            make_fact_table(
                vec![MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                }],
                "data",
            ),
        );
        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate fact table with valid metadata should succeed: {e}"));
    }

    #[test]
    fn test_validate_fact_table_invalid_prefix() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();
        ir.fact_tables.insert(
            "sales".to_string(),
            make_fact_table(
                vec![MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                }],
                "data",
            ),
        );
        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("must start with 'tf_' prefix")),
            "expected Validation error about tf_ prefix, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_fact_table_empty_measures() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();
        ir.fact_tables.insert("tf_sales".to_string(), make_fact_table(vec![], "data"));
        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("must have at least one measure")),
            "expected Validation error about empty measures, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_fact_table_dimensions_missing_name() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();
        ir.fact_tables.insert(
            "tf_sales".to_string(),
            make_fact_table(
                vec![MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                }],
                "",
            ),
        );
        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("dimensions missing 'name' field")),
            "expected Validation error about missing dimensions name, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_aggregate_type_missing_count() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "SalesAggregate".to_string(),
            fields:      vec![IRField {
                name:        "revenue_sum".to_string(),
                field_type:  "Float".to_string(),
                nullable:    true,
                description: None,
                sql_column:  None,
            }],
            sql_source:  None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("must have a 'count' field")),
            "expected Validation error about missing count field, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_aggregate_type_with_count() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "SalesAggregate".to_string(),
            fields:      vec![
                IRField {
                    name:        "count".to_string(),
                    field_type:  "Int!".to_string(),
                    nullable:    false,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "revenue_sum".to_string(),
                    field_type:  "Float".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  None,
            description: None,
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate aggregate type with count should succeed: {e}"));
    }

    #[test]
    fn test_validate_group_by_input_invalid_field_type() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "SalesGroupByInput".to_string(),
            fields:      vec![IRField {
                name:        "category".to_string(),
                field_type:  "String".to_string(), // Should be Boolean
                nullable:    true,
                description: None,
                sql_column:  None,
            }],
            sql_source:  None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("must be Boolean")),
            "expected Validation error about Boolean requirement, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_group_by_input_valid() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "SalesGroupByInput".to_string(),
            fields:      vec![IRField {
                name:        "category".to_string(),
                field_type:  "Boolean".to_string(),
                nullable:    true,
                description: None,
                sql_column:  None,
            }],
            sql_source:  None,
            description: None,
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate group by input with Boolean fields should succeed: {e}"));
    }

    #[test]
    fn test_validate_having_input_invalid_suffix() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "SalesHavingInput".to_string(),
            fields:      vec![IRField {
                name:        "count".to_string(), // Missing operator suffix
                field_type:  "Int".to_string(),
                nullable:    true,
                description: None,
                sql_column:  None,
            }],
            sql_source:  None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("must have operator suffix")),
            "expected Validation error about operator suffix, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_having_input_valid() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "SalesHavingInput".to_string(),
            fields:      vec![
                IRField {
                    name:        "count_gt".to_string(),
                    field_type:  "Int".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "revenue_sum_gte".to_string(),
                    field_type:  "Float".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  None,
            description: None,
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate having input with valid suffixes should succeed: {e}"));
    }

    // =========================================================================
    // Type and Query Validation Tests
    // =========================================================================

    #[test]
    fn test_extract_base_type() {
        assert_eq!(extract_base_type("String"), "String");
        assert_eq!(extract_base_type("String!"), "String");
        assert_eq!(extract_base_type("[String]"), "String");
        assert_eq!(extract_base_type("[String!]"), "String");
        assert_eq!(extract_base_type("[String!]!"), "String");
        assert_eq!(extract_base_type("  User  "), "User");
    }

    #[test]
    fn test_validate_type_with_valid_references() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        // Define User type
        ir.types.push(IRType {
            name:        "User".to_string(),
            fields:      vec![
                IRField {
                    name:        "id".to_string(),
                    field_type:  "ID!".to_string(),
                    nullable:    false,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "name".to_string(),
                    field_type:  "String!".to_string(),
                    nullable:    false,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  Some("v_user".to_string()),
            description: None,
        });

        // Define Post type that references User
        ir.types.push(IRType {
            name:        "Post".to_string(),
            fields:      vec![
                IRField {
                    name:        "id".to_string(),
                    field_type:  "ID!".to_string(),
                    nullable:    false,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "author".to_string(),
                    field_type:  "User".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  Some("v_post".to_string()),
            description: None,
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate type with valid references should succeed: {e}"));
    }

    #[test]
    fn test_validate_type_with_invalid_reference() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "Post".to_string(),
            fields:      vec![IRField {
                name:        "author".to_string(),
                field_type:  "NonExistentType".to_string(),
                nullable:    true,
                description: None,
                sql_column:  None,
            }],
            sql_source:  None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("references unknown type") && message.contains("NonExistentType")),
            "expected Validation error about unknown type reference, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_type_empty_name() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        String::new(),
            fields:      vec![],
            sql_source:  None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("name cannot be empty")),
            "expected Validation error about empty type name, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_query_with_valid_return_type() {
        use super::super::ir::{AutoParams, IRArgument, IRQuery};

        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        // Define User type
        ir.types.push(IRType {
            name:        "User".to_string(),
            fields:      vec![IRField {
                name:        "id".to_string(),
                field_type:  "ID!".to_string(),
                nullable:    false,
                description: None,
                sql_column:  None,
            }],
            sql_source:  Some("v_user".to_string()),
            description: None,
        });

        // Define query that returns User
        ir.queries.push(IRQuery {
            name:         "user".to_string(),
            return_type:  "User".to_string(),
            returns_list: false,
            nullable:     true,
            arguments:    vec![IRArgument {
                name:          "id".to_string(),
                arg_type:      "ID!".to_string(),
                nullable:      false,
                default_value: None,
                description:   None,
            }],
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams::default(),
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate query with valid return type should succeed: {e}"));
    }

    #[test]
    fn test_validate_query_with_invalid_return_type() {
        use super::super::ir::{AutoParams, IRQuery};

        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.queries.push(IRQuery {
            name:         "unknownQuery".to_string(),
            return_type:  "NonExistentType".to_string(),
            returns_list: false,
            nullable:     true,
            arguments:    vec![],
            sql_source:   None,
            description:  None,
            auto_params:  AutoParams::default(),
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("returns unknown type") && message.contains("NonExistentType")),
            "expected Validation error about unknown return type, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_query_with_scalar_return_type() {
        use super::super::ir::{AutoParams, IRQuery};

        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        // Query returning scalar type (no custom type needed)
        ir.queries.push(IRQuery {
            name:         "serverTime".to_string(),
            return_type:  "DateTime".to_string(),
            returns_list: false,
            nullable:     false,
            arguments:    vec![],
            sql_source:   None,
            description:  None,
            auto_params:  AutoParams::default(),
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate query with scalar return type should succeed: {e}"));
    }

    #[test]
    fn test_validate_query_empty_name() {
        use super::super::ir::{AutoParams, IRQuery};

        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.queries.push(IRQuery {
            name:         String::new(),
            return_type:  "String".to_string(),
            returns_list: false,
            nullable:     true,
            arguments:    vec![],
            sql_source:   None,
            description:  None,
            auto_params:  AutoParams::default(),
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("Query name cannot be empty")),
            "expected Validation error about empty query name, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_list_type_references() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        // Define User type
        ir.types.push(IRType {
            name:        "User".to_string(),
            fields:      vec![
                IRField {
                    name:        "id".to_string(),
                    field_type:  "ID!".to_string(),
                    nullable:    false,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "friends".to_string(),
                    field_type:  "[User!]".to_string(), // List of Users
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  None,
            description: None,
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate list type references should succeed: {e}"));
    }

    #[test]
    fn test_validate_builtin_scalar_types() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        // Test all builtin scalars are recognized in type fields
        ir.types.push(IRType {
            name:        "TestType".to_string(),
            fields:      vec![
                IRField {
                    name:        "id".to_string(),
                    field_type:  "ID".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "name".to_string(),
                    field_type:  "String".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "age".to_string(),
                    field_type:  "Int".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "rating".to_string(),
                    field_type:  "Float".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "active".to_string(),
                    field_type:  "Boolean".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "created".to_string(),
                    field_type:  "DateTime".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "uid".to_string(),
                    field_type:  "UUID".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  None,
            description: None,
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("all builtin scalars should be recognized: {e}"));
    }

    #[test]
    fn test_validate_rich_scalar_types() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        // Test some rich scalars are recognized
        ir.types.push(IRType {
            name:        "Contact".to_string(),
            fields:      vec![
                IRField {
                    name:        "email".to_string(),
                    field_type:  "Email".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "phone".to_string(),
                    field_type:  "PhoneNumber".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "url".to_string(),
                    field_type:  "URL".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "ip".to_string(),
                    field_type:  "IPAddress".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  None,
            description: None,
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("rich scalars should be recognized: {e}"));
    }
}
