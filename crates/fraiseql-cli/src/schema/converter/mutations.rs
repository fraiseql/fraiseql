use std::collections::HashSet;

use anyhow::{Context, Result};
use fraiseql_core::schema::{MutationDefinition, MutationOperation};

use super::{DeclaredTypeNames, SchemaConverter};

/// The fields a cascade payload has of its own (`cascade_types::cascade_payload_type`).
const PAYLOAD_OWN_FIELDS: [&str; 3] = ["entity", "cascade", "updatedFields"];
use crate::schema::intermediate::IntermediateMutation;

impl SchemaConverter {
    /// Convert `IntermediateMutation` to `MutationDefinition`
    pub(super) fn convert_mutation(
        intermediate: IntermediateMutation,
        declared: &DeclaredTypeNames,
    ) -> Result<MutationDefinition> {
        let arguments = intermediate
            .arguments
            .into_iter()
            .map(|a| Self::convert_argument(a, declared))
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert mutation '{}'", intermediate.name))?;

        let arg_names: HashSet<&str> = arguments.iter().map(|a| a.name.as_str()).collect();
        let inject_params =
            Self::convert_inject_params(&intermediate.name, &arg_names, intermediate.inject, false)
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

        // #846: see the query converter — one shared helper, validating loudly.
        let (rest_path, rest_method) =
            Self::convert_rest_annotation("Mutation", &intermediate.name, intermediate.rest)?;

        let requires_actor = super::parse_requires_actor(
            "Mutation",
            &intermediate.name,
            &intermediate.requires_actor,
        )?;

        // #1391: pg_tviews' affected set is merged into a cascade; without one there is
        // nothing to merge it into.
        if !intermediate.cascade_source.is_function() && !intermediate.cascade {
            anyhow::bail!(
                "Mutation `{}`: cascade_source = \"pg_tviews\" merges pg_tviews' affected set \
                 into the mutation's cascade, and this mutation has none; declare it `cascade = \
                 true`, or drop cascade_source",
                intermediate.name
            );
        }

        let success_fields = Self::convert_success_fields(
            &intermediate.name,
            intermediate.cascade,
            intermediate.success_fields,
            declared,
        )?;

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
            rest_path,
            rest_method,
            upsert_function: None,
            requires_role: intermediate.requires_role,
            requires_actor,
            changelog: intermediate.changelog,
            input_style: intermediate.input_style,
            changelog_pre_image: intermediate.changelog_pre_image,
            cascade: intermediate.cascade,
            success_fields,
            cascade_source: intermediate.cascade_source,
        })
    }

    /// Convert a mutation's `success_fields` (#1397), refusing any the cascade payload
    /// could not carry: on a mutation with no payload (not `cascade`), under a name the
    /// payload already has or that is declared twice, or of a type that is not a leaf.
    fn convert_success_fields(
        mutation: &str,
        cascade: bool,
        fields: Vec<crate::schema::intermediate::IntermediateField>,
        declared: &DeclaredTypeNames,
    ) -> Result<Vec<fraiseql_core::schema::FieldDefinition>> {
        if fields.is_empty() {
            return Ok(Vec::new());
        }
        if !cascade {
            anyhow::bail!(
                "Mutation `{mutation}`: success_fields are served on the cascade payload, and \
                 this mutation has none; declare it `cascade = true`, or drop success_fields"
            );
        }
        let mut seen: HashSet<String> = HashSet::new();
        let mut converted = Vec::with_capacity(fields.len());
        for field in fields {
            let name = field.name.clone();
            if PAYLOAD_OWN_FIELDS.contains(&name.as_str()) || name.starts_with("__") {
                anyhow::bail!(
                    "Mutation `{mutation}`: success_fields `{name}` is a name the payload \
                     already has (`entity`, `cascade`, `updatedFields`, `__typename`)"
                );
            }
            if !seen.insert(name.clone()) {
                anyhow::bail!("Mutation `{mutation}`: success_fields declares `{name}` twice");
            }
            let definition = Self::convert_field(field, declared)
                .context(format!("Mutation `{mutation}`: success_fields `{name}`"))?;
            let leaf = definition.field_type.is_scalar()
                || matches!(definition.field_type, fraiseql_core::schema::FieldType::Enum(_));
            if !leaf {
                anyhow::bail!(
                    "Mutation `{mutation}`: success_fields `{name}` is a {}, not a scalar or \
                     an enum: a success field is one value the function computes",
                    definition.field_type
                );
            }
            converted.push(definition);
        }
        Ok(converted)
    }

    /// Parse mutation operation from string
    ///
    /// Converts intermediate format operation string to `MutationOperation` enum.
    ///
    /// # The verb is matched case-insensitively
    ///
    /// Every authoring surface in this repository spells it lowercase — `docs/authoring.md`,
    /// `docs/architecture/intermediate-schema.md`, the Python SDK's
    /// `@fraiseql.mutation(operation="insert")`, the PHP `MutationBuilder`'s own class
    /// docblock, the Java `OperationBuilder` — while this function accepted only uppercase.
    /// A developer following the project's own documentation got
    /// `Error: Unknown mutation operation: insert` and no indication that the *case* was
    /// the problem. Uppercasing eleven SDKs and every doc would have been the larger
    /// breaking change and would have invalidated schemas already written.
    ///
    /// The verb set stays closed: an unrecognized word is still a hard error, because a
    /// mutation whose DML verb the compiler could not read must not silently become
    /// `Custom` and skip the write path's `INSERT`/`UPDATE`/`DELETE` handling.
    pub(super) fn parse_mutation_operation(
        operation: Option<&str>,
        sql_source: Option<&str>,
    ) -> Result<MutationOperation> {
        // Compared uppercase so the arms below read as the canonical spelling.
        let normalized = operation.map(str::to_uppercase);
        match normalized.as_deref() {
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
            // Echo what the author wrote, not the uppercased form — reporting `UPSERT`
            // for an authored `upsert` reads as if the compiler mangled the input, and
            // sends the reader looking for a casing bug that is not there.
            Some(_) => {
                let authored = operation.unwrap_or_default();
                anyhow::bail!(
                    "Unknown mutation operation: {authored}. Expected one of CREATE, INSERT, \
                     UPDATE, DELETE or CUSTOM (any case)."
                )
            },
        }
    }
}
