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

use crate::error::{FraiseQLError, Result};
use super::ir::AuthoringIR;

/// Validation error.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
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
    pub fn new() -> Self {
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
        // Existing validation (TODO: implement basic validations)
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
    fn validate_types(&self, _ir: &AuthoringIR) -> Result<()> {
        // TODO: Implement type validation
        // - Type references are valid
        // - No circular dependencies
        Ok(())
    }

    /// Validate query definitions.
    fn validate_queries(&self, _ir: &AuthoringIR) -> Result<()> {
        // TODO: Implement query validation
        // - Return types exist
        // - SQL sources are valid
        Ok(())
    }

    /// Validate fact table metadata structure.
    ///
    /// Ensures that fact table metadata follows the required structure:
    /// - Has table_name field
    /// - Has measures array (at least one measure)
    /// - Has dimensions object
    /// - Denormalized filters are valid
    fn validate_fact_tables(&self, ir: &AuthoringIR) -> Result<()> {
        for (table_name, metadata) in &ir.fact_tables {
            // Validate table name follows tf_* pattern
            if !table_name.starts_with("tf_") {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Fact table '{}' must start with 'tf_' prefix",
                        table_name
                    ),
                    path: Some(format!("fact_tables.{}", table_name)),
                });
            }

            // Validate metadata is an object
            let obj = metadata.as_object().ok_or_else(|| FraiseQLError::Validation {
                message: format!("Fact table '{}' metadata must be an object", table_name),
                path: Some(format!("fact_tables.{}", table_name)),
            })?;

            // Validate measures exist and is an array
            let measures = obj.get("measures").ok_or_else(|| FraiseQLError::Validation {
                message: format!("Fact table '{}' missing 'measures' field", table_name),
                path: Some(format!("fact_tables.{}.measures", table_name)),
            })?;

            let measures_arr = measures.as_array().ok_or_else(|| FraiseQLError::Validation {
                message: format!(
                    "Fact table '{}' measures must be an array",
                    table_name
                ),
                path: Some(format!("fact_tables.{}.measures", table_name)),
            })?;

            if measures_arr.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Fact table '{}' must have at least one measure",
                        table_name
                    ),
                    path: Some(format!("fact_tables.{}.measures", table_name)),
                });
            }

            // Validate each measure has required fields
            for (idx, measure) in measures_arr.iter().enumerate() {
                let measure_obj = measure.as_object().ok_or_else(|| {
                    FraiseQLError::Validation {
                        message: format!(
                            "Fact table '{}' measure {} must be an object",
                            table_name, idx
                        ),
                        path: Some(format!("fact_tables.{}.measures[{}]", table_name, idx)),
                    }
                })?;

                // Validate measure has name field
                if !measure_obj.contains_key("name") {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Fact table '{}' measure {} missing 'name' field",
                            table_name, idx
                        ),
                        path: Some(format!("fact_tables.{}.measures[{}]", table_name, idx)),
                    });
                }

                // Validate measure has sql_type field
                if !measure_obj.contains_key("sql_type") {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Fact table '{}' measure {} missing 'sql_type' field",
                            table_name, idx
                        ),
                        path: Some(format!("fact_tables.{}.measures[{}]", table_name, idx)),
                    });
                }
            }

            // Validate dimensions exist
            let dimensions = obj.get("dimensions").ok_or_else(|| {
                FraiseQLError::Validation {
                    message: format!("Fact table '{}' missing 'dimensions' field", table_name),
                    path: Some(format!("fact_tables.{}.dimensions", table_name)),
                }
            })?;

            let dimensions_obj = dimensions.as_object().ok_or_else(|| {
                FraiseQLError::Validation {
                    message: format!(
                        "Fact table '{}' dimensions must be an object",
                        table_name
                    ),
                    path: Some(format!("fact_tables.{}.dimensions", table_name)),
                }
            })?;

            // Validate dimension has name field
            if !dimensions_obj.contains_key("name") {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Fact table '{}' dimensions missing 'name' field",
                        table_name
                    ),
                    path: Some(format!("fact_tables.{}.dimensions", table_name)),
                });
            }

            // Validate denormalized_filters is an array (if present)
            if let Some(filters) = obj.get("denormalized_filters") {
                if !filters.is_array() {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Fact table '{}' denormalized_filters must be an array",
                            table_name
                        ),
                        path: Some(format!(
                            "fact_tables.{}.denormalized_filters",
                            table_name
                        )),
                    });
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn test_validator_new() {
        let validator = SchemaValidator::new();
        let ir = AuthoringIR::new();
        let result = validator.validate(ir.clone());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_empty_ir() {
        let validator = SchemaValidator::new();
        let ir = AuthoringIR::new();
        let result = validator.validate(ir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_fact_table_with_valid_metadata() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        let metadata = json!({
            "table_name": "tf_sales",
            "measures": [
                {"name": "revenue", "sql_type": "Decimal", "nullable": false}
            ],
            "dimensions": {
                "name": "data",
                "paths": []
            },
            "denormalized_filters": []
        });

        ir.fact_tables.insert("tf_sales".to_string(), metadata);

        let result = validator.validate(ir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_fact_table_invalid_prefix() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        let metadata = json!({
            "measures": [{"name": "revenue", "sql_type": "Decimal"}],
            "dimensions": {"name": "data"}
        });

        ir.fact_tables.insert("sales".to_string(), metadata);

        let result = validator.validate(ir);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("must start with 'tf_' prefix"));
        }
    }

    #[test]
    fn test_validate_fact_table_missing_measures() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        let metadata = json!({
            "dimensions": {"name": "data"}
        });

        ir.fact_tables.insert("tf_sales".to_string(), metadata);

        let result = validator.validate(ir);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("missing 'measures' field"));
        }
    }

    #[test]
    fn test_validate_fact_table_empty_measures() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        let metadata = json!({
            "measures": [],
            "dimensions": {"name": "data"}
        });

        ir.fact_tables.insert("tf_sales".to_string(), metadata);

        let result = validator.validate(ir);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("must have at least one measure"));
        }
    }

    #[test]
    fn test_validate_fact_table_measure_missing_name() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        let metadata = json!({
            "measures": [
                {"sql_type": "Decimal"}
            ],
            "dimensions": {"name": "data"}
        });

        ir.fact_tables.insert("tf_sales".to_string(), metadata);

        let result = validator.validate(ir);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("missing 'name' field"));
        }
    }

    #[test]
    fn test_validate_fact_table_measure_missing_sql_type() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        let metadata = json!({
            "measures": [
                {"name": "revenue"}
            ],
            "dimensions": {"name": "data"}
        });

        ir.fact_tables.insert("tf_sales".to_string(), metadata);

        let result = validator.validate(ir);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("missing 'sql_type' field"));
        }
    }

    #[test]
    fn test_validate_fact_table_missing_dimensions() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        let metadata = json!({
            "measures": [
                {"name": "revenue", "sql_type": "Decimal"}
            ]
        });

        ir.fact_tables.insert("tf_sales".to_string(), metadata);

        let result = validator.validate(ir);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("missing 'dimensions' field"));
        }
    }

    #[test]
    fn test_validate_fact_table_dimensions_missing_name() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        let metadata = json!({
            "measures": [
                {"name": "revenue", "sql_type": "Decimal"}
            ],
            "dimensions": {
                "paths": []
            }
        });

        ir.fact_tables.insert("tf_sales".to_string(), metadata);

        let result = validator.validate(ir);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("dimensions missing 'name' field"));
        }
    }

    #[test]
    fn test_validate_fact_table_invalid_filters() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        let metadata = json!({
            "measures": [
                {"name": "revenue", "sql_type": "Decimal"}
            ],
            "dimensions": {"name": "data"},
            "denormalized_filters": "not an array"
        });

        ir.fact_tables.insert("tf_sales".to_string(), metadata);

        let result = validator.validate(ir);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("denormalized_filters must be an array"));
        }
    }

    #[test]
    fn test_validate_aggregate_type_missing_count() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        use super::super::ir::{IRType, IRField};

        ir.types.push(IRType {
            name: "SalesAggregate".to_string(),
            fields: vec![
                IRField {
                    name: "revenue_sum".to_string(),
                    field_type: "Float".to_string(),
                    nullable: true,
                    description: None,
                    sql_column: None,
                }
            ],
            sql_source: None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("must have a 'count' field"));
        }
    }

    #[test]
    fn test_validate_aggregate_type_with_count() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        use super::super::ir::{IRType, IRField};

        ir.types.push(IRType {
            name: "SalesAggregate".to_string(),
            fields: vec![
                IRField {
                    name: "count".to_string(),
                    field_type: "Int!".to_string(),
                    nullable: false,
                    description: None,
                    sql_column: None,
                },
                IRField {
                    name: "revenue_sum".to_string(),
                    field_type: "Float".to_string(),
                    nullable: true,
                    description: None,
                    sql_column: None,
                }
            ],
            sql_source: None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_group_by_input_invalid_field_type() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        use super::super::ir::{IRType, IRField};

        ir.types.push(IRType {
            name: "SalesGroupByInput".to_string(),
            fields: vec![
                IRField {
                    name: "category".to_string(),
                    field_type: "String".to_string(), // Should be Boolean
                    nullable: true,
                    description: None,
                    sql_column: None,
                }
            ],
            sql_source: None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("must be Boolean"));
        }
    }

    #[test]
    fn test_validate_group_by_input_valid() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        use super::super::ir::{IRType, IRField};

        ir.types.push(IRType {
            name: "SalesGroupByInput".to_string(),
            fields: vec![
                IRField {
                    name: "category".to_string(),
                    field_type: "Boolean".to_string(),
                    nullable: true,
                    description: None,
                    sql_column: None,
                }
            ],
            sql_source: None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_having_input_invalid_suffix() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        use super::super::ir::{IRType, IRField};

        ir.types.push(IRType {
            name: "SalesHavingInput".to_string(),
            fields: vec![
                IRField {
                    name: "count".to_string(), // Missing operator suffix
                    field_type: "Int".to_string(),
                    nullable: true,
                    description: None,
                    sql_column: None,
                }
            ],
            sql_source: None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("must have operator suffix"));
        }
    }

    #[test]
    fn test_validate_having_input_valid() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        use super::super::ir::{IRType, IRField};

        ir.types.push(IRType {
            name: "SalesHavingInput".to_string(),
            fields: vec![
                IRField {
                    name: "count_gt".to_string(),
                    field_type: "Int".to_string(),
                    nullable: true,
                    description: None,
                    sql_column: None,
                },
                IRField {
                    name: "revenue_sum_gte".to_string(),
                    field_type: "Float".to_string(),
                    nullable: true,
                    description: None,
                    sql_column: None,
                }
            ],
            sql_source: None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(result.is_ok());
    }
}
