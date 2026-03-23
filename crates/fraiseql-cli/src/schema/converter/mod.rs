//! Schema Converter
//!
//! Converts `IntermediateSchema` (language-agnostic) to `CompiledSchema` (Rust-specific)

mod directives;
mod mutations;
mod queries;
mod relay;
mod subscriptions;
mod types;

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use anyhow::{Context, Result};
use fraiseql_core::{
    compiler::fact_table::{
        DimensionColumn, DimensionPath, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
    },
    schema::{CompiledSchema, FieldType},
    validation::CustomTypeRegistry,
};
use tracing::{info, warn};

use super::{
    intermediate::{IntermediateFactTable, IntermediateSchema},
    rich_filters::{RichFilterConfig, compile_rich_filters},
};

/// Converts intermediate format to compiled format
pub struct SchemaConverter;

impl SchemaConverter {
    /// Convert `IntermediateSchema` to `CompiledSchema`
    ///
    /// This performs:
    /// 1. Type conversion (intermediate types → compiled types)
    /// 2. Field name normalization (type → `field_type`)
    /// 3. Validation (type references, circular refs, etc.)
    /// 4. Optimization
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    pub fn convert(intermediate: IntermediateSchema) -> Result<CompiledSchema> {
        info!("Converting intermediate schema to compiled format");

        // Convert types
        let types = intermediate
            .types
            .into_iter()
            .map(Self::convert_type)
            .collect::<Result<Vec<_>>>()
            .context("Failed to convert types")?;

        // Extract query_defaults before consuming intermediate.queries.
        // unwrap_or_default() → all-true, matching historical behaviour when no
        // [query_defaults] section is present in fraiseql.toml.
        let defaults = intermediate.query_defaults.unwrap_or_default();

        // Convert queries
        let queries = intermediate
            .queries
            .into_iter()
            .map(|q| Self::convert_query(q, &defaults))
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
        let enums = intermediate.enums.into_iter().map(Self::convert_enum).collect::<Vec<_>>();

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
            .map(Self::convert_interface)
            .collect::<Result<Vec<_>>>()
            .context("Failed to convert interfaces")?;

        // Convert unions
        let unions = intermediate.unions.into_iter().map(Self::convert_union).collect::<Vec<_>>();

        // Convert subscriptions
        let subscriptions = intermediate
            .subscriptions
            .into_iter()
            .map(Self::convert_subscription)
            .collect::<Result<Vec<_>>>()
            .context("Failed to convert subscriptions")?;

        // Convert custom directives
        let directives = intermediate
            .directives
            .unwrap_or_default()
            .into_iter()
            .map(Self::convert_directive)
            .collect::<Result<Vec<_>>>()
            .context("Failed to convert directives")?;

        // Convert fact tables from Vec<IntermediateFactTable> to HashMap<String, FactTableMetadata>
        let fact_tables = intermediate
            .fact_tables
            .unwrap_or_default()
            .into_iter()
            .map(|ft| {
                let name = ft.table_name.clone();
                let metadata = Self::convert_fact_table(ft);
                (name, metadata)
            })
            .collect();

        let mut compiled = CompiledSchema {
            types,
            enums,
            input_types,
            interfaces,
            unions,
            queries,
            mutations,
            subscriptions,
            directives,
            fact_tables, // Analytics metadata
            observers: Vec::new(), /* Observer definitions (populated from
                          * IntermediateSchema) */
            federation: intermediate
                .federation_config
                .map(serde_json::from_value)
                .transpose()
                .context("federation_config: invalid JSON structure")?,
            security: intermediate
                .security
                .map(serde_json::from_value)
                .transpose()
                .context("security: invalid JSON structure")?,
            observers_config: intermediate
                .observers_config
                .map(serde_json::from_value)
                .transpose()
                .context("observers_config: invalid JSON structure")?,
            subscriptions_config: intermediate.subscriptions_config, /* Subscriptions config from
                                                                      * TOML */
            validation_config: intermediate.validation_config, // Validation limits from TOML
            debug_config: intermediate.debug_config,           // Debug config from TOML
            mcp_config: intermediate.mcp_config,               // MCP config from TOML
            rest_config: intermediate.rest_config,             // REST config from TOML
            grpc_config: intermediate.grpc_config,             // gRPC config from TOML
            dev_config: intermediate.dev_config,               // Dev mode config from TOML
            schema_sdl: None,                                  // Raw GraphQL SDL
            custom_scalars: CustomTypeRegistry::default(),     // Custom scalar registry
            schema_format_version: Some(fraiseql_core::schema::CURRENT_SCHEMA_FORMAT_VERSION),
            ..Default::default()
        };

        // Populate custom scalars from intermediate schema
        if let Some(custom_scalars_vec) = intermediate.custom_scalars {
            for scalar_def in custom_scalars_vec {
                let custom_type = Self::convert_custom_scalar(scalar_def)?;
                compiled
                    .custom_scalars
                    .register(custom_type.name.clone(), custom_type)
                    .context("Failed to register custom scalar")?;
            }
        }

        // Inject synthetic Relay types (PageInfo, Node interface, XxxConnection, XxxEdge).
        relay::inject_relay_types(&mut compiled);

        // Inject synthetic Cascade types when any mutation has cascade enabled.
        fraiseql_core::schema::inject_cascade_types(&mut compiled);

        // Compile rich filter types (EmailAddress, VIN, IBAN, etc.)
        let rich_filter_config = RichFilterConfig::default();
        compile_rich_filters(&mut compiled, &rich_filter_config)
            .context("Failed to compile rich filter types")?;

        // Validate the compiled schema
        Self::validate(&compiled)?;

        // Warn when dev mode is compiled into the schema
        if compiled.dev_config.as_ref().is_some_and(|d| d.enabled) {
            warn!(
                "Dev mode is enabled — default claims will be injected when no JWT \
                 is present. Do NOT use in production."
            );
        }

        info!("Schema conversion successful");
        Ok(compiled)
    }

    fn validate(schema: &CompiledSchema) -> Result<()> {
        info!("Validating compiled schema");

        // Build type registry
        let mut type_names: HashSet<String> = HashSet::new();
        for type_def in &schema.types {
            type_names.insert(type_def.name.to_string());
        }

        // Register input types
        for input_type in &schema.input_types {
            type_names.insert(input_type.name.clone());
        }

        // Register enum types
        for enum_type in &schema.enums {
            type_names.insert(enum_type.name.clone());
        }

        // Build interface registry and add to type_names
        let mut interface_names = HashSet::new();
        for interface_def in &schema.interfaces {
            interface_names.insert(interface_def.name.clone());
            type_names.insert(interface_def.name.clone());
        }

        // Register union types
        for union_type in &schema.unions {
            type_names.insert(union_type.name.clone());
        }

        // Add built-in scalars (GraphQL standard + common PostgreSQL types)
        for scalar in &[
            "Int", "Float", "String", "Boolean", "ID",
            "Date", "DateTime", "Time", "Json", "JSON",
            "UUID", "Decimal", "Vector", "BigInt",
            "date", "timestamp", "timestamptz", "jsonb", "json",
            "bigint", "numeric", "uuid", "text", "integer",
            "boolean", "real", "smallint", "bytea", "inet", "interval",
        ] {
            type_names.insert((*scalar).to_string());
        }

        // Add built-in FraiseQL scalars
        for name in ["DateTime", "Date", "Time", "Json", "UUID", "Decimal", "Vector"] {
            type_names.insert((*name).to_string());
        }

        // Add custom scalars
        for (name, _) in schema.custom_scalars.list_all() {
            type_names.insert(name);
        }

        // Add enum names
        for enum_def in &schema.enums {
            type_names.insert(enum_def.name.clone());
        }

        // Add input type names
        for input in &schema.input_types {
            type_names.insert(input.name.clone());
        }

        // Validate queries
        for query in &schema.queries {
            if !type_names.contains(&query.return_type) {
                warn!("Query '{}' references unknown type: {}", query.name, query.return_type);
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
            FieldType::Scalar(name) => name.clone(),
            FieldType::Object(name) => name.clone(),
            FieldType::Enum(name) => name.clone(),
            FieldType::Input(name) => name.clone(),
            FieldType::Interface(name) => name.clone(),
            FieldType::Union(name) => name.clone(),
            FieldType::List(inner) => Self::extract_type_name(inner),
        }
    }

    /// Convert `IntermediateFactTable` to `FactTableMetadata`.
    fn convert_fact_table(ft: IntermediateFactTable) -> FactTableMetadata {
        FactTableMetadata {
            table_name:           ft.table_name,
            measures:             ft
                .measures
                .into_iter()
                .map(|m| MeasureColumn {
                    name:     m.name,
                    sql_type: Self::parse_sql_type(&m.sql_type),
                    nullable: m.nullable,
                })
                .collect(),
            dimensions:           DimensionColumn {
                name:  ft.dimensions.name,
                paths: ft
                    .dimensions
                    .paths
                    .into_iter()
                    .map(|p| DimensionPath {
                        name:      p.name,
                        json_path: p.json_path,
                        data_type: p.data_type,
                    })
                    .collect(),
            },
            denormalized_filters: ft
                .denormalized_filters
                .into_iter()
                .map(|f| FilterColumn {
                    name:     f.name,
                    sql_type: Self::parse_sql_type(&f.sql_type),
                    indexed:  f.indexed,
                })
                .collect(),
            calendar_dimensions:  vec![],
        }
    }

    /// Parse a SQL type string into a `SqlType` enum variant.
    fn parse_sql_type(s: &str) -> SqlType {
        match s.to_uppercase().as_str() {
            "INT" | "INTEGER" | "SMALLINT" | "INT4" | "INT2" => SqlType::Int,
            "BIGINT" | "INT8" => SqlType::BigInt,
            "DECIMAL" | "NUMERIC" | "MONEY" => SqlType::Decimal,
            "REAL" | "FLOAT" | "DOUBLE" | "FLOAT8" | "FLOAT4" | "DOUBLE PRECISION" => {
                SqlType::Float
            },
            "JSONB" => SqlType::Jsonb,
            "JSON" => SqlType::Json,
            "TEXT" | "VARCHAR" | "STRING" | "CHAR" | "CHARACTER VARYING" => SqlType::Text,
            "UUID" => SqlType::Uuid,
            "TIMESTAMP" | "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" | "DATETIME" => {
                SqlType::Timestamp
            },
            "DATE" => SqlType::Date,
            "BOOLEAN" | "BOOL" => SqlType::Boolean,
            _ => SqlType::Other(s.to_string()),
        }
    }
}
