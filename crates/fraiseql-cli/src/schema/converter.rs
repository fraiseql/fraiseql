//! Schema Converter
//!
//! Converts `IntermediateSchema` (language-agnostic) to `CompiledSchema` (Rust-specific)

use super::intermediate::{
    IntermediateSchema, IntermediateType, IntermediateField, IntermediateQuery,
    IntermediateMutation, IntermediateArgument, IntermediateAutoParams,
    IntermediateEnum, IntermediateEnumValue, IntermediateInputObject, IntermediateInputField,
    IntermediateInterface, IntermediateUnion,
};
use anyhow::{Context, Result};
use fraiseql_core::schema::{
    ArgumentDefinition, AutoParams, CompiledSchema, EnumDefinition, EnumValueDefinition,
    FieldDefinition, FieldType, InputFieldDefinition, InputObjectDefinition,
    InterfaceDefinition, MutationDefinition, MutationOperation, QueryDefinition, TypeDefinition,
    UnionDefinition,
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

        // Convert enums
        let enums = intermediate
            .enums
            .into_iter()
            .map(Self::convert_enum)
            .collect::<Vec<_>>();

        // Convert input types
        let input_types = intermediate
            .input_types
            .into_iter()
            .map(Self::convert_input_object)
            .collect::<Vec<_>>();

        // Convert interfaces
        let interfaces = intermediate
            .interfaces
            .into_iter()
            .map(|i| Self::convert_interface(i))
            .collect::<Result<Vec<_>>>()
            .context("Failed to convert interfaces")?;

        // Convert unions
        let unions = intermediate
            .unions
            .into_iter()
            .map(Self::convert_union)
            .collect::<Vec<_>>();

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
            enums,
            input_types,
            interfaces,
            unions,
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
            implements: intermediate.implements,
        })
    }

    /// Convert `IntermediateEnum` to `EnumDefinition`
    fn convert_enum(intermediate: IntermediateEnum) -> EnumDefinition {
        let values = intermediate
            .values
            .into_iter()
            .map(Self::convert_enum_value)
            .collect();

        EnumDefinition {
            name: intermediate.name,
            values,
            description: intermediate.description,
        }
    }

    /// Convert `IntermediateEnumValue` to `EnumValueDefinition`
    fn convert_enum_value(intermediate: IntermediateEnumValue) -> EnumValueDefinition {
        let deprecation = intermediate.deprecated.map(|d| {
            fraiseql_core::schema::DeprecationInfo { reason: d.reason }
        });

        EnumValueDefinition {
            name: intermediate.name,
            description: intermediate.description,
            deprecation,
        }
    }

    /// Convert `IntermediateInputObject` to `InputObjectDefinition`
    fn convert_input_object(intermediate: IntermediateInputObject) -> InputObjectDefinition {
        let fields = intermediate
            .fields
            .into_iter()
            .map(Self::convert_input_field)
            .collect();

        InputObjectDefinition {
            name: intermediate.name,
            fields,
            description: intermediate.description,
        }
    }

    /// Convert `IntermediateInputField` to `InputFieldDefinition`
    fn convert_input_field(intermediate: IntermediateInputField) -> InputFieldDefinition {
        let deprecation = intermediate.deprecated.map(|d| {
            fraiseql_core::schema::DeprecationInfo { reason: d.reason }
        });

        // Convert default value to JSON string if present
        let default_value = intermediate.default.map(|v| v.to_string());

        InputFieldDefinition {
            name: intermediate.name,
            field_type: intermediate.field_type,
            description: intermediate.description,
            default_value,
            deprecation,
        }
    }

    /// Convert `IntermediateInterface` to `InterfaceDefinition`
    fn convert_interface(intermediate: IntermediateInterface) -> Result<InterfaceDefinition> {
        let fields = intermediate
            .fields
            .into_iter()
            .map(Self::convert_field)
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert interface '{}'", intermediate.name))?;

        Ok(InterfaceDefinition {
            name: intermediate.name,
            fields,
            description: intermediate.description,
        })
    }

    /// Convert `IntermediateUnion` to `UnionDefinition`
    fn convert_union(intermediate: IntermediateUnion) -> UnionDefinition {
        let mut union_def = UnionDefinition::new(&intermediate.name)
            .with_members(intermediate.member_types);
        if let Some(desc) = intermediate.description {
            union_def = union_def.with_description(&desc);
        }
        union_def
    }

    /// Convert `IntermediateField` to `FieldDefinition`
    ///
    /// **Key normalization**: `type` → `field_type`
    fn convert_field(intermediate: IntermediateField) -> Result<FieldDefinition> {
        let field_type = Self::parse_field_type(&intermediate.field_type)?;

        // Extract deprecation info from @deprecated directive if present
        let deprecation = intermediate.directives.as_ref().and_then(|directives| {
            directives.iter().find(|d| d.name == "deprecated").map(|d| {
                let reason = d.arguments.as_ref().and_then(|args| {
                    args.get("reason").and_then(|v| v.as_str()).map(String::from)
                });
                fraiseql_core::schema::DeprecationInfo { reason }
            })
        });

        Ok(FieldDefinition {
            name: intermediate.name,
            field_type,
            nullable: intermediate.nullable,
            default_value: None,
            description: intermediate.description,
            vector_config: None,
            alias: None,
            deprecation,
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
    /// - Interface implementations are valid
    fn validate(schema: &CompiledSchema) -> Result<()> {
        info!("Validating compiled schema");

        // Build type registry
        let mut type_names = HashSet::new();
        for type_def in &schema.types {
            type_names.insert(type_def.name.clone());
        }

        // Build interface registry
        let mut interface_names = HashSet::new();
        for interface_def in &schema.interfaces {
            interface_names.insert(interface_def.name.clone());
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

        // Validate interface implementations
        for type_def in &schema.types {
            for interface_name in &type_def.implements {
                if !interface_names.contains(interface_name) {
                    anyhow::bail!(
                        "Type '{}' implements unknown interface '{}'",
                        type_def.name,
                        interface_name
                    );
                }

                // Validate that the type has all fields required by the interface
                if let Some(interface) = schema.find_interface(interface_name) {
                    for interface_field in &interface.fields {
                        let type_has_field = type_def.fields.iter().any(|f| {
                            f.name == interface_field.name
                                && f.field_type == interface_field.field_type
                        });
                        if !type_has_field {
                            anyhow::bail!(
                                "Type '{}' implements interface '{}' but is missing field '{}'",
                                type_def.name,
                                interface_name,
                                interface_field.name
                            );
                        }
                    }
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
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            fragments: None,
            directives: None,
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
                        description: None,
                        directives: None,
                    },
                    IntermediateField {
                        name: "name".to_string(),
                        field_type: "String".to_string(),
                        nullable: false,
                        description: None,
                        directives: None,
                    },
                ],
                description: Some("User type".to_string()),
                implements: vec![],
            }],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            fragments: None,
            directives: None,
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
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
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
            fragments: None,
            directives: None,
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
                implements: vec![],
            }],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
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
            fragments: None,
            directives: None,
            fact_tables: None,
            aggregate_queries: None,
        };

        let compiled = SchemaConverter::convert(intermediate).unwrap();
        assert_eq!(compiled.queries.len(), 1);
        assert_eq!(compiled.queries[0].arguments.len(), 1);
        assert_eq!(compiled.queries[0].arguments[0].arg_type, FieldType::Int);
        assert!(compiled.queries[0].auto_params.has_limit);
    }

    #[test]
    fn test_convert_field_with_deprecated_directive() {
        use crate::schema::intermediate::IntermediateAppliedDirective;

        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![IntermediateType {
                name: "User".to_string(),
                fields: vec![
                    IntermediateField {
                        name: "oldId".to_string(),
                        field_type: "Int".to_string(),
                        nullable: false,
                        description: None,
                        directives: Some(vec![IntermediateAppliedDirective {
                            name: "deprecated".to_string(),
                            arguments: Some(serde_json::json!({"reason": "Use 'id' instead"})),
                        }]),
                    },
                    IntermediateField {
                        name: "id".to_string(),
                        field_type: "Int".to_string(),
                        nullable: false,
                        description: None,
                        directives: None,
                    },
                ],
                description: None,
                implements: vec![],
            }],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            fragments: None,
            directives: None,
            fact_tables: None,
            aggregate_queries: None,
        };

        let compiled = SchemaConverter::convert(intermediate).unwrap();
        assert_eq!(compiled.types.len(), 1);
        assert_eq!(compiled.types[0].fields.len(), 2);

        // Check deprecated field
        let old_id_field = &compiled.types[0].fields[0];
        assert_eq!(old_id_field.name, "oldId");
        assert!(old_id_field.is_deprecated());
        assert_eq!(
            old_id_field.deprecation_reason(),
            Some("Use 'id' instead")
        );

        // Check non-deprecated field
        let id_field = &compiled.types[0].fields[1];
        assert_eq!(id_field.name, "id");
        assert!(!id_field.is_deprecated());
        assert_eq!(id_field.deprecation_reason(), None);
    }

    #[test]
    fn test_convert_enum() {
        use crate::schema::intermediate::{IntermediateEnum, IntermediateEnumValue, IntermediateDeprecation};

        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![],
            enums: vec![IntermediateEnum {
                name: "OrderStatus".to_string(),
                values: vec![
                    IntermediateEnumValue {
                        name: "PENDING".to_string(),
                        description: None,
                        deprecated: None,
                    },
                    IntermediateEnumValue {
                        name: "PROCESSING".to_string(),
                        description: Some("Currently being processed".to_string()),
                        deprecated: None,
                    },
                    IntermediateEnumValue {
                        name: "CANCELLED".to_string(),
                        description: None,
                        deprecated: Some(IntermediateDeprecation {
                            reason: Some("Use VOIDED instead".to_string()),
                        }),
                    },
                ],
                description: Some("Order status enum".to_string()),
            }],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            fragments: None,
            directives: None,
            fact_tables: None,
            aggregate_queries: None,
        };

        let compiled = SchemaConverter::convert(intermediate).unwrap();
        assert_eq!(compiled.enums.len(), 1);

        let status_enum = &compiled.enums[0];
        assert_eq!(status_enum.name, "OrderStatus");
        assert_eq!(status_enum.description, Some("Order status enum".to_string()));
        assert_eq!(status_enum.values.len(), 3);

        // Check PENDING value
        assert_eq!(status_enum.values[0].name, "PENDING");
        assert!(!status_enum.values[0].is_deprecated());

        // Check PROCESSING value with description
        assert_eq!(status_enum.values[1].name, "PROCESSING");
        assert_eq!(status_enum.values[1].description, Some("Currently being processed".to_string()));

        // Check CANCELLED deprecated value
        assert_eq!(status_enum.values[2].name, "CANCELLED");
        assert!(status_enum.values[2].is_deprecated());
    }

    #[test]
    fn test_convert_input_object() {
        use crate::schema::intermediate::{IntermediateInputObject, IntermediateInputField, IntermediateDeprecation};

        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![],
            enums: vec![],
            input_types: vec![IntermediateInputObject {
                name: "UserFilter".to_string(),
                fields: vec![
                    IntermediateInputField {
                        name: "name".to_string(),
                        field_type: "String".to_string(),
                        nullable: true,
                        description: None,
                        default: None,
                        deprecated: None,
                    },
                    IntermediateInputField {
                        name: "active".to_string(),
                        field_type: "Boolean".to_string(),
                        nullable: true,
                        description: Some("Filter by active status".to_string()),
                        default: Some(serde_json::json!(true)),
                        deprecated: None,
                    },
                    IntermediateInputField {
                        name: "oldField".to_string(),
                        field_type: "String".to_string(),
                        nullable: true,
                        description: None,
                        default: None,
                        deprecated: Some(IntermediateDeprecation {
                            reason: Some("Use newField instead".to_string()),
                        }),
                    },
                ],
                description: Some("User filter input".to_string()),
            }],
            interfaces: vec![],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            fragments: None,
            directives: None,
            fact_tables: None,
            aggregate_queries: None,
        };

        let compiled = SchemaConverter::convert(intermediate).unwrap();
        assert_eq!(compiled.input_types.len(), 1);

        let filter = &compiled.input_types[0];
        assert_eq!(filter.name, "UserFilter");
        assert_eq!(filter.description, Some("User filter input".to_string()));
        assert_eq!(filter.fields.len(), 3);

        // Check name field
        let name_field = filter.find_field("name").unwrap();
        assert_eq!(name_field.field_type, "String");
        assert!(!name_field.is_deprecated());

        // Check active field with default value
        let active_field = filter.find_field("active").unwrap();
        assert_eq!(active_field.field_type, "Boolean");
        assert_eq!(active_field.default_value, Some("true".to_string()));
        assert_eq!(active_field.description, Some("Filter by active status".to_string()));

        // Check deprecated field
        let old_field = filter.find_field("oldField").unwrap();
        assert!(old_field.is_deprecated());
    }

    #[test]
    fn test_convert_interface() {
        use crate::schema::intermediate::{IntermediateInterface, IntermediateField};

        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![IntermediateInterface {
                name: "Node".to_string(),
                fields: vec![IntermediateField {
                    name: "id".to_string(),
                    field_type: "ID".to_string(),
                    nullable: false,
                    description: None,
                    directives: None,
                }],
                description: Some("An object with a globally unique ID".to_string()),
            }],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            fragments: None,
            directives: None,
            fact_tables: None,
            aggregate_queries: None,
        };

        let compiled = SchemaConverter::convert(intermediate).unwrap();
        assert_eq!(compiled.interfaces.len(), 1);

        let interface = &compiled.interfaces[0];
        assert_eq!(interface.name, "Node");
        assert_eq!(interface.description, Some("An object with a globally unique ID".to_string()));
        assert_eq!(interface.fields.len(), 1);
        assert_eq!(interface.fields[0].name, "id");
        assert_eq!(interface.fields[0].field_type, FieldType::Id);
    }

    #[test]
    fn test_convert_type_implements_interface() {
        use crate::schema::intermediate::{IntermediateInterface, IntermediateType, IntermediateField};

        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![IntermediateType {
                name: "User".to_string(),
                fields: vec![
                    IntermediateField {
                        name: "id".to_string(),
                        field_type: "ID".to_string(),
                        nullable: false,
                        description: None,
                        directives: None,
                    },
                    IntermediateField {
                        name: "name".to_string(),
                        field_type: "String".to_string(),
                        nullable: false,
                        description: None,
                        directives: None,
                    },
                ],
                description: None,
                implements: vec!["Node".to_string()],
            }],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![IntermediateInterface {
                name: "Node".to_string(),
                fields: vec![IntermediateField {
                    name: "id".to_string(),
                    field_type: "ID".to_string(),
                    nullable: false,
                    description: None,
                    directives: None,
                }],
                description: None,
            }],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            fragments: None,
            directives: None,
            fact_tables: None,
            aggregate_queries: None,
        };

        let compiled = SchemaConverter::convert(intermediate).unwrap();

        // Check type implements interface
        assert_eq!(compiled.types.len(), 1);
        assert_eq!(compiled.types[0].implements, vec!["Node"]);

        // Check interface exists
        assert_eq!(compiled.interfaces.len(), 1);
        assert_eq!(compiled.interfaces[0].name, "Node");
    }

    #[test]
    fn test_validate_unknown_interface() {
        use crate::schema::intermediate::{IntermediateType, IntermediateField};

        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![IntermediateType {
                name: "User".to_string(),
                fields: vec![IntermediateField {
                    name: "id".to_string(),
                    field_type: "ID".to_string(),
                    nullable: false,
                    description: None,
                    directives: None,
                }],
                description: None,
                implements: vec!["UnknownInterface".to_string()],
            }],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![], // No interface defined!
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            fragments: None,
            directives: None,
            fact_tables: None,
            aggregate_queries: None,
        };

        let result = SchemaConverter::convert(intermediate);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown interface"));
    }

    #[test]
    fn test_validate_missing_interface_field() {
        use crate::schema::intermediate::{IntermediateInterface, IntermediateType, IntermediateField};

        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![IntermediateType {
                name: "User".to_string(),
                fields: vec![
                    // Missing the required 'id' field from Node interface!
                    IntermediateField {
                        name: "name".to_string(),
                        field_type: "String".to_string(),
                        nullable: false,
                        description: None,
                        directives: None,
                    },
                ],
                description: None,
                implements: vec!["Node".to_string()],
            }],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![IntermediateInterface {
                name: "Node".to_string(),
                fields: vec![IntermediateField {
                    name: "id".to_string(),
                    field_type: "ID".to_string(),
                    nullable: false,
                    description: None,
                    directives: None,
                }],
                description: None,
            }],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            fragments: None,
            directives: None,
            fact_tables: None,
            aggregate_queries: None,
        };

        let result = SchemaConverter::convert(intermediate);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing field 'id'"));
    }

    #[test]
    fn test_convert_union() {
        use crate::schema::intermediate::{IntermediateUnion, IntermediateType, IntermediateField};

        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![
                IntermediateType {
                    name: "User".to_string(),
                    fields: vec![IntermediateField {
                        name: "id".to_string(),
                        field_type: "ID".to_string(),
                        nullable: false,
                        description: None,
                        directives: None,
                    }],
                    description: None,
                    implements: vec![],
                },
                IntermediateType {
                    name: "Post".to_string(),
                    fields: vec![IntermediateField {
                        name: "id".to_string(),
                        field_type: "ID".to_string(),
                        nullable: false,
                        description: None,
                        directives: None,
                    }],
                    description: None,
                    implements: vec![],
                },
            ],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![IntermediateUnion {
                name: "SearchResult".to_string(),
                member_types: vec!["User".to_string(), "Post".to_string()],
                description: Some("Result from a search query".to_string()),
            }],
            queries: vec![],
            mutations: vec![],
            fragments: None,
            directives: None,
            fact_tables: None,
            aggregate_queries: None,
        };

        let compiled = SchemaConverter::convert(intermediate).unwrap();

        // Check union exists
        assert_eq!(compiled.unions.len(), 1);
        let union_def = &compiled.unions[0];
        assert_eq!(union_def.name, "SearchResult");
        assert_eq!(union_def.member_types, vec!["User", "Post"]);
        assert_eq!(union_def.description, Some("Result from a search query".to_string()));
    }
}
