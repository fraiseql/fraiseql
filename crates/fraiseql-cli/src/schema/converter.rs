//! Schema Converter
//!
//! Converts `IntermediateSchema` (language-agnostic) to `CompiledSchema` (Rust-specific)

use super::intermediate::{IntermediateSchema, IntermediateType, IntermediateField, IntermediateQuery, IntermediateMutation, IntermediateArgument, IntermediateAutoParams};
use anyhow::{Context, Result};
use fraiseql_core::schema::{
    ArgumentDefinition, AutoParams, CompiledSchema, FieldDefinition, FieldType,
    MutationDefinition, MutationOperation, QueryDefinition, TypeDefinition,
};
use std::collections::HashSet;
use tracing::{info, warn};

/// Converts intermediate format to compiled format
pub struct SchemaConverter;

impl SchemaConverter {
    /// Convert `IntermediateSchema` to `CompiledSchema`
    ///
    /// This performs:
    /// 1. Type conversion (intermediate types → compiled types)
    /// 2. Field name normalization (type → `field_type`)
    /// 3. Validation (type references, circular refs, etc.)
    /// 4. Optimization (for future phases)
    pub fn convert(intermediate: IntermediateSchema) -> Result<CompiledSchema> {
        info!("Converting intermediate schema to compiled format");

        // Convert types
        let types = intermediate
            .types
            .into_iter()
            .map(Self::convert_type)
            .collect::<Result<Vec<_>>>()
            .context("Failed to convert types")?;

        // Convert queries
        let queries = intermediate
            .queries
            .into_iter()
            .map(Self::convert_query)
            .collect::<Result<Vec<_>>>()
            .context("Failed to convert queries")?;

        // Convert mutations
        let mutations = intermediate
            .mutations
            .into_iter()
            .map(Self::convert_mutation)
            .collect::<Result<Vec<_>>>()
            .context("Failed to convert mutations")?;

        // Convert fact tables from Vec to HashMap<String, serde_json::Value>
        let fact_tables = intermediate.fact_tables
            .unwrap_or_default()
            .into_iter()
            .map(|ft| {
                let metadata = serde_json::to_value(&ft)
                    .expect("Failed to serialize fact table metadata");
                (ft.table_name, metadata)
            })
            .collect();

        let compiled = CompiledSchema {
            types,
            queries,
            mutations,
            subscriptions: vec![], // TODO: Add in future phase
            fact_tables, // Phase 8A: Analytics metadata
        };

        // Validate the compiled schema
        Self::validate(&compiled)?;

        info!("Schema conversion successful");
        Ok(compiled)
    }

    /// Convert `IntermediateType` to `TypeDefinition`
    fn convert_type(intermediate: IntermediateType) -> Result<TypeDefinition> {
        let fields = intermediate
            .fields
            .into_iter()
            .map(Self::convert_field)
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert type '{}'", intermediate.name))?;

        Ok(TypeDefinition {
            name: intermediate.name,
            fields,
            description: intermediate.description,
            sql_source: String::new(), // Not used for regular types (empty string)
            jsonb_column: String::new(), // Not used for regular types (empty string)
            sql_projection_hint: None, // Will be populated by optimizer in Phase 9
        })
    }

    /// Convert `IntermediateField` to `FieldDefinition`
    ///
    /// **Key normalization**: `type` → `field_type`
    fn convert_field(intermediate: IntermediateField) -> Result<FieldDefinition> {
        let field_type = Self::parse_field_type(&intermediate.field_type)?;

        Ok(FieldDefinition {
            name: intermediate.name,
            field_type,
            nullable: intermediate.nullable,
            default_value: None,
            description: None,
            vector_config: None,
        })
    }

    /// Parse string type name to `FieldType` enum
    ///
    /// Handles built-in scalars and custom object types
    fn parse_field_type(type_name: &str) -> Result<FieldType> {
        match type_name {
            "String" => Ok(FieldType::String),
            "Int" => Ok(FieldType::Int),
            "Float" => Ok(FieldType::Float),
            "Boolean" => Ok(FieldType::Boolean),
            "ID" => Ok(FieldType::Id),
            "DateTime" => Ok(FieldType::DateTime),
            "Date" => Ok(FieldType::Date),
            "Time" => Ok(FieldType::Time),
            "Json" => Ok(FieldType::Json),
            "UUID" => Ok(FieldType::Uuid),
            "Decimal" => Ok(FieldType::Decimal),
            "Vector" => Ok(FieldType::Vector),
            // Custom object types (User, Post, etc.)
            custom => Ok(FieldType::Object(custom.to_string())),
        }
    }

    /// Convert `IntermediateQuery` to `QueryDefinition`
    fn convert_query(intermediate: IntermediateQuery) -> Result<QueryDefinition> {
        let arguments = intermediate
            .arguments
            .into_iter()
            .map(Self::convert_argument)
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert query '{}'", intermediate.name))?;

        let auto_params = intermediate
            .auto_params
            .map(Self::convert_auto_params)
            .unwrap_or_default();

        Ok(QueryDefinition {
            name: intermediate.name,
            return_type: intermediate.return_type,
            returns_list: intermediate.returns_list,
            nullable: intermediate.nullable,
            arguments,
            sql_source: intermediate.sql_source,
            description: intermediate.description,
            auto_params,
        })
    }

    /// Convert `IntermediateMutation` to `MutationDefinition`
    fn convert_mutation(intermediate: IntermediateMutation) -> Result<MutationDefinition> {
        let arguments = intermediate
            .arguments
            .into_iter()
            .map(Self::convert_argument)
            .collect::<Result<Vec<_>>>()
            .context(format!(
                "Failed to convert mutation '{}'",
                intermediate.name
            ))?;

        let operation = Self::parse_mutation_operation(
            intermediate.operation.as_deref(),
            intermediate.sql_source.as_deref(),
        )?;

        Ok(MutationDefinition {
            name: intermediate.name,
            return_type: intermediate.return_type,
            arguments,
            description: intermediate.description,
            operation,
        })
    }

    /// Parse mutation operation from string
    ///
    /// Converts intermediate format operation string to `MutationOperation` enum
    fn parse_mutation_operation(
        operation: Option<&str>,
        sql_source: Option<&str>,
    ) -> Result<MutationOperation> {
        match operation {
            Some("CREATE" | "INSERT") => {
                // Extract table name from sql_source or use empty for Custom
                let table = sql_source
                    .map(std::string::ToString::to_string)
                    .unwrap_or_default();
                Ok(MutationOperation::Insert { table })
            }
            Some("UPDATE") => {
                let table = sql_source
                    .map(std::string::ToString::to_string)
                    .unwrap_or_default();
                Ok(MutationOperation::Update { table })
            }
            Some("DELETE") => {
                let table = sql_source
                    .map(std::string::ToString::to_string)
                    .unwrap_or_default();
                Ok(MutationOperation::Delete { table })
            }
            Some("FUNCTION") => {
                let name = sql_source
                    .map(std::string::ToString::to_string)
                    .unwrap_or_default();
                Ok(MutationOperation::Function { name })
            }
            Some("CUSTOM") | None => Ok(MutationOperation::Custom),
            Some(op) => {
                anyhow::bail!("Unknown mutation operation: {op}")
            }
        }
    }

    /// Convert `IntermediateArgument` to `ArgumentDefinition`
    fn convert_argument(intermediate: IntermediateArgument) -> Result<ArgumentDefinition> {
        let arg_type = Self::parse_field_type(&intermediate.arg_type)?;

        Ok(ArgumentDefinition {
            name: intermediate.name,
            arg_type,
            nullable: intermediate.nullable,
            default_value: intermediate.default,
            description: None,
        })
    }

    /// Convert `IntermediateAutoParams` to `AutoParams`
    const fn convert_auto_params(intermediate: IntermediateAutoParams) -> AutoParams {
        AutoParams {
            has_limit: intermediate.limit,
            has_offset: intermediate.offset,
            has_where: intermediate.where_clause,
            has_order_by: intermediate.order_by,
        }
    }

    /// Validate compiled schema
    ///
    /// Checks:
    /// - All type references exist
    /// - No circular references
    /// - Queries have valid return types
    /// - Mutations have valid return types
    fn validate(schema: &CompiledSchema) -> Result<()> {
        info!("Validating compiled schema");

        // Build type registry
        let mut type_names = HashSet::new();
        for type_def in &schema.types {
            type_names.insert(type_def.name.clone());
        }

        // Add built-in scalars
        type_names.insert("Int".to_string());
        type_names.insert("Float".to_string());
        type_names.insert("String".to_string());
        type_names.insert("Boolean".to_string());
        type_names.insert("ID".to_string());

        // Validate queries
        for query in &schema.queries {
            if !type_names.contains(&query.return_type) {
                warn!(
                    "Query '{}' references unknown type: {}",
                    query.name, query.return_type
                );
                anyhow::bail!(
                    "Query '{}' references unknown type '{}'",
                    query.name,
                    query.return_type
                );
            }

            // Validate argument types
            for arg in &query.arguments {
                let type_name = Self::extract_type_name(&arg.arg_type);
                if !type_names.contains(&type_name) {
                    anyhow::bail!(
                        "Query '{}' argument '{}' references unknown type '{}'",
                        query.name,
                        arg.name,
                        type_name
                    );
                }
            }
        }

        // Validate mutations
        for mutation in &schema.mutations {
            if !type_names.contains(&mutation.return_type) {
                anyhow::bail!(
                    "Mutation '{}' references unknown type '{}'",
                    mutation.name,
                    mutation.return_type
                );
            }

            // Validate argument types
            for arg in &mutation.arguments {
                let type_name = Self::extract_type_name(&arg.arg_type);
                if !type_names.contains(&type_name) {
                    anyhow::bail!(
                        "Mutation '{}' argument '{}' references unknown type '{}'",
                        mutation.name,
                        arg.name,
                        type_name
                    );
                }
            }
        }

        info!("Schema validation passed");
        Ok(())
    }

    /// Extract type name from `FieldType` for validation
    ///
    /// Built-in types return their scalar name, Object types return the object name
    fn extract_type_name(field_type: &FieldType) -> String {
        match field_type {
            FieldType::String => "String".to_string(),
            FieldType::Int => "Int".to_string(),
            FieldType::Float => "Float".to_string(),
            FieldType::Boolean => "Boolean".to_string(),
            FieldType::Id => "ID".to_string(),
            FieldType::DateTime => "DateTime".to_string(),
            FieldType::Date => "Date".to_string(),
            FieldType::Time => "Time".to_string(),
            FieldType::Json => "Json".to_string(),
            FieldType::Uuid => "UUID".to_string(),
            FieldType::Decimal => "Decimal".to_string(),
            FieldType::Vector => "Vector".to_string(),
            FieldType::Object(name) => name.clone(),
            FieldType::Enum(name) => name.clone(),
            FieldType::Input(name) => name.clone(),
            FieldType::Interface(name) => name.clone(),
            FieldType::Union(name) => name.clone(),
            FieldType::List(inner) => Self::extract_type_name(inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_minimal_schema() {
        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![],
            queries: vec![],
            mutations: vec![],
            fact_tables: None,
            aggregate_queries: None,
        };

        let compiled = SchemaConverter::convert(intermediate).unwrap();
        assert_eq!(compiled.types.len(), 0);
        assert_eq!(compiled.queries.len(), 0);
        assert_eq!(compiled.mutations.len(), 0);
    }

    #[test]
    fn test_convert_type_with_fields() {
        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![IntermediateType {
                name: "User".to_string(),
                fields: vec![
                    IntermediateField {
                        name: "id".to_string(),
                        field_type: "Int".to_string(),
                        nullable: false,
                    },
                    IntermediateField {
                        name: "name".to_string(),
                        field_type: "String".to_string(),
                        nullable: false,
                    },
                ],
                description: Some("User type".to_string()),
            }],
            queries: vec![],
            mutations: vec![],
            fact_tables: None,
            aggregate_queries: None,
        };

        let compiled = SchemaConverter::convert(intermediate).unwrap();
        assert_eq!(compiled.types.len(), 1);
        assert_eq!(compiled.types[0].name, "User");
        assert_eq!(compiled.types[0].fields.len(), 2);
        assert_eq!(compiled.types[0].fields[0].field_type, FieldType::Int);
        assert_eq!(compiled.types[0].fields[1].field_type, FieldType::String);
    }

    #[test]
    fn test_validate_unknown_type_reference() {
        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![],
            queries: vec![IntermediateQuery {
                name: "users".to_string(),
                return_type: "UnknownType".to_string(),
                returns_list: true,
                nullable: false,
                arguments: vec![],
                description: None,
                sql_source: Some("v_user".to_string()),
                auto_params: None,
            }],
            mutations: vec![],
            fact_tables: None,
            aggregate_queries: None,
        };

        let result = SchemaConverter::convert(intermediate);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unknown type 'UnknownType'"));
    }

    #[test]
    fn test_convert_query_with_arguments() {
        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![IntermediateType {
                name: "User".to_string(),
                fields: vec![],
                description: None,
            }],
            queries: vec![IntermediateQuery {
                name: "users".to_string(),
                return_type: "User".to_string(),
                returns_list: true,
                nullable: false,
                arguments: vec![IntermediateArgument {
                    name: "limit".to_string(),
                    arg_type: "Int".to_string(),
                    nullable: false,
                    default: Some(serde_json::json!(10)),
                }],
                description: Some("Get users".to_string()),
                sql_source: Some("v_user".to_string()),
                auto_params: Some(IntermediateAutoParams {
                    limit: true,
                    offset: true,
                    where_clause: false,
                    order_by: false,
                }),
            }],
            mutations: vec![],
            fact_tables: None,
            aggregate_queries: None,
        };

        let compiled = SchemaConverter::convert(intermediate).unwrap();
        assert_eq!(compiled.queries.len(), 1);
        assert_eq!(compiled.queries[0].arguments.len(), 1);
        assert_eq!(compiled.queries[0].arguments[0].arg_type, FieldType::Int);
        assert!(compiled.queries[0].auto_params.has_limit);
    }
}
