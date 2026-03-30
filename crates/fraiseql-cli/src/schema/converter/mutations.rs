use std::collections::HashSet;

use anyhow::{Context, Result};
use fraiseql_core::schema::{MutationDefinition, MutationOperation};

use super::SchemaConverter;
use crate::schema::intermediate::IntermediateMutation;

impl SchemaConverter {
    /// Convert `IntermediateMutation` to `MutationDefinition`
    pub(super) fn convert_mutation(
        intermediate: IntermediateMutation,
    ) -> Result<MutationDefinition> {
        let arguments = intermediate
            .arguments
            .into_iter()
            .map(Self::convert_argument)
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert mutation '{}'", intermediate.name))?;

        let arg_names: HashSet<&str> = arguments.iter().map(|a| a.name.as_str()).collect();
        let inject_params =
            Self::convert_inject_params(&intermediate.name, &arg_names, intermediate.inject)
                .context(format!(
                    "Failed to convert inject params for mutation '{}'",
                    intermediate.name
                ))?;

        let operation = Self::parse_mutation_operation(
            intermediate.operation.as_deref(),
            intermediate.sql_source.as_deref(),
        )?;

        let deprecation = intermediate
            .deprecated
            .map(|d| fraiseql_core::schema::DeprecationInfo { reason: d.reason });

        // Validate invalidates_fact_tables entries as safe SQL identifiers.
        for table in &intermediate.invalidates_fact_tables {
            if !Self::is_safe_sql_identifier(table) {
                anyhow::bail!(
                    "Mutation '{}': invalidates_fact_tables entry {:?} is not a valid SQL \
                     identifier. Use only letters, digits, and underscores (must start with \
                     a letter or underscore).",
                    intermediate.name,
                    table
                );
            }
        }

        // Validate invalidates_views entries as safe SQL identifiers.
        for view in &intermediate.invalidates_views {
            if !Self::is_safe_sql_identifier(view) {
                anyhow::bail!(
                    "Mutation '{}': invalidates_views entry {:?} is not a valid SQL \
                     identifier. Use only letters, digits, and underscores (must start with \
                     a letter or underscore).",
                    intermediate.name,
                    view
                );
            }
        }

        Ok(MutationDefinition {
            name: intermediate.name,
            return_type: intermediate.return_type,
            arguments,
            description: intermediate.description,
            operation,
            deprecation,
            sql_source: intermediate.sql_source,
            inject_params,
            invalidates_fact_tables: intermediate.invalidates_fact_tables,
            invalidates_views: intermediate.invalidates_views,
            rest_path: None,
            rest_method: None,
            upsert_function: None,
        })
    }

    /// Parse mutation operation from string
    ///
    /// Converts intermediate format operation string to `MutationOperation` enum
    pub(super) fn parse_mutation_operation(
        operation: Option<&str>,
        sql_source: Option<&str>,
    ) -> Result<MutationOperation> {
        match operation {
            Some("CREATE" | "INSERT") => {
                // Extract table name from sql_source or use empty for Custom
                let table = sql_source.map(std::string::ToString::to_string).unwrap_or_default();
                Ok(MutationOperation::Insert { table })
            },
            Some("UPDATE") => {
                let table = sql_source.map(std::string::ToString::to_string).unwrap_or_default();
                Ok(MutationOperation::Update { table })
            },
            Some("DELETE") => {
                let table = sql_source.map(std::string::ToString::to_string).unwrap_or_default();
                Ok(MutationOperation::Delete { table })
            },
            Some("CUSTOM") | None => Ok(MutationOperation::Custom),
            Some(op) => {
                anyhow::bail!("Unknown mutation operation: {op}")
            },
        }
    }
}
