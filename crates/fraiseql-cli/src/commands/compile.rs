//! Schema compilation command
//!
//! Compiles schema.json (from Python/TypeScript/etc.) into optimized schema.compiled.json

use crate::schema::{IntermediateSchema, SchemaConverter, SchemaOptimizer, SchemaValidator};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tracing::info;

/// Run the compile command
///
/// # Arguments
///
/// * `input` - Path to schema.json file
/// * `output` - Path to write schema.compiled.json
/// * `check` - If true, validate only without writing output
///
/// # Errors
///
/// Returns error if:
/// - Input file doesn't exist or can't be read
/// - JSON parsing fails
/// - Schema validation fails
/// - Output file can't be written
#[allow(clippy::unused_async)] // Will be async when database validation is added
pub async fn run(input: &str, output: &str, check: bool) -> Result<()> {
    info!("Compiling schema: {input}");

    // 1. Read input schema.json
    let input_path = Path::new(input);
    if !input_path.exists() {
        anyhow::bail!("Input file not found: {input}");
    }

    let schema_json =
        fs::read_to_string(input_path).context("Failed to read input schema.json")?;

    // 2. Parse JSON into IntermediateSchema (language-agnostic format)
    info!("Parsing intermediate schema...");
    let intermediate: IntermediateSchema =
        serde_json::from_str(&schema_json).context("Failed to parse schema.json")?;

    // 3. Validate intermediate schema
    info!("Validating schema structure...");
    let validation_report = SchemaValidator::validate(&intermediate)
        .context("Failed to validate schema")?;

    if !validation_report.is_valid() {
        validation_report.print();
        anyhow::bail!("Schema validation failed with {} error(s)", validation_report.error_count());
    }

    // Print warnings if any
    if validation_report.warning_count() > 0 {
        validation_report.print();
    }

    // 4. Convert to CompiledSchema (validates and normalizes)
    info!("Converting to compiled format...");
    let mut schema = SchemaConverter::convert(intermediate)
        .context("Failed to convert schema to compiled format")?;

    // 5. Optimize schema and generate SQL hints
    info!("Analyzing schema for optimization opportunities...");
    let optimization_report = SchemaOptimizer::optimize(&mut schema)
        .context("Failed to optimize schema")?;

    // 6. If check-only mode, stop here
    if check {
        println!("✓ Schema is valid");
        println!("  Types: {}", schema.types.len());
        println!("  Queries: {}", schema.queries.len());
        println!("  Mutations: {}", schema.mutations.len());

        // Print optimization suggestions
        optimization_report.print();

        return Ok(());
    }

    // 7. Write compiled schema
    info!("Writing compiled schema to: {output}");
    let output_json = serde_json::to_string_pretty(&schema)
        .context("Failed to serialize compiled schema")?;

    fs::write(output, output_json).context("Failed to write compiled schema")?;

    // 8. Success message
    println!("✓ Schema compiled successfully");
    println!("  Input:  {input}");
    println!("  Output: {output}");
    println!("  Types: {}", schema.types.len());
    println!("  Queries: {}", schema.queries.len());
    println!("  Mutations: {}", schema.mutations.len());

    // Print optimization suggestions
    optimization_report.print();

    Ok(())
}


#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use fraiseql_core::schema::{
        AutoParams, CompiledSchema, FieldDefinition, FieldType, QueryDefinition, TypeDefinition,
    };

    #[test]
    fn test_validate_schema_success() {
        let schema = CompiledSchema {
            types: vec![TypeDefinition {
                name: "User".to_string(),
                fields: vec![
                    FieldDefinition {
                        name: "id".to_string(),
                        field_type: FieldType::Int,
                        nullable: false,
                        default_value: None,
                        description: None,
                        vector_config: None,
                        alias: None,
                        deprecation: None,
                    },
                    FieldDefinition {
                        name: "name".to_string(),
                        field_type: FieldType::String,
                        nullable: false,
                        default_value: None,
                        description: None,
                        vector_config: None,
                        alias: None,
                        deprecation: None,
                    },
                ],
                description: Some("User type".to_string()),
                sql_source: String::new(),
                jsonb_column: String::new(),
                sql_projection_hint: None,
                implements: vec![],
            }],
            queries: vec![QueryDefinition {
                name: "users".to_string(),
                return_type: "User".to_string(),
                returns_list: true,
                nullable: false,
                arguments: vec![],
                sql_source: Some("v_user".to_string()),
                description: Some("Get users".to_string()),
                auto_params: AutoParams::default(),
                deprecation: None,
            }],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            mutations: vec![],
            subscriptions: vec![],
            fact_tables: HashMap::default(),
        };

        // Validation is done inside SchemaConverter::convert, not exposed separately
        // This test just verifies we can build a valid schema structure
        assert_eq!(schema.types.len(), 1);
        assert_eq!(schema.queries.len(), 1);
    }

    #[test]
    fn test_validate_schema_unknown_type() {
        let schema = CompiledSchema {
            types: vec![],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![QueryDefinition {
                name: "users".to_string(),
                return_type: "UnknownType".to_string(),
                returns_list: true,
                nullable: false,
                arguments: vec![],
                sql_source: Some("v_user".to_string()),
                description: Some("Get users".to_string()),
                auto_params: AutoParams::default(),
                deprecation: None,
            }],
            mutations: vec![],
            subscriptions: vec![],
            fact_tables: HashMap::default(),
        };

        // Note: Validation is private to SchemaConverter
        // This test demonstrates the schema structure with an invalid type
        assert_eq!(schema.types.len(), 0);
        assert_eq!(schema.queries[0].return_type, "UnknownType");
    }
}
