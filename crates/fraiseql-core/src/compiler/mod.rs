//! Compiler sub-modules for FraiseQL v2.
//!
//! # Overview
//!
//! This module contains:
//!
//! - **Parser / IR / Validator** — parse authoring-time JSON into an intermediate
//!   representation (`AuthoringIR`) and validate it. Used by the CLI `validate-facts`
//!   command and by `SchemaConverter` (in `fraiseql-cli`).
//!
//! - **Runtime sub-modules** — aggregate types, aggregation planning, fact tables,
//!   and window functions, consumed by the runtime executor.

pub mod aggregate_types;
pub mod aggregation;
pub mod enum_validator;
pub mod fact_table;
pub mod ir;
pub mod parser;
pub mod validator;
pub mod window_allowlist;
pub mod window_functions;

pub use aggregate_types::{AggregateType, AggregateTypeGenerator, GroupByInput, HavingInput};
pub use aggregation::{AggregationPlan, AggregationPlanner, AggregationRequest};
pub use enum_validator::EnumValidator;
pub use ir::{
    AuthoringIR, AutoParams, IRArgument, IRField, IRMutation, IRQuery, IRSubscription, IRType,
    MutationOperation,
};
pub use parser::SchemaParser;
pub use validator::{SchemaValidationError, SchemaValidator};
pub use window_functions::{WindowExecutionPlan, WindowFunction, WindowFunctionPlanner};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use crate::compiler::fact_table::{DimensionColumn, FactTableMetadata};
    use crate::schema::CompiledSchema;

    #[test]
    fn test_compiled_schema_fact_table_operations() {
        let mut schema = CompiledSchema::new();

        let metadata = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![],
            dimensions:           DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![],
        };

        schema.add_fact_table("tf_sales".to_string(), metadata.clone());

        assert!(schema.has_fact_tables());

        let tables = schema.list_fact_tables();
        assert_eq!(tables.len(), 1);
        assert!(tables.contains(&"tf_sales"));

        let retrieved = schema.get_fact_table("tf_sales");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &metadata);

        assert!(schema.get_fact_table("tf_nonexistent").is_none());
    }
}
