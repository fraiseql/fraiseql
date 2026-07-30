//! Schema Converter
//!
//! Converts `IntermediateSchema` (language-agnostic) to `CompiledSchema` (Rust-specific)

mod cascade_types;
mod directives;
mod identity;
mod interface_conformance;
mod mutation_error_union;
mod mutations;
mod queries;
mod relay;
mod subscriptions;
pub(crate) mod tenancy;
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
    intermediate::{IntermediateFactTable, IntermediateInjectDefaults, IntermediateSchema},
    rich_filters::{RichFilterConfig, compile_rich_filters},
};

/// Converts intermediate format to compiled format
pub struct SchemaConverter;

/// Optional, default-off behaviours for [`SchemaConverter::convert_with_options`].
///
/// Threaded here rather than on [`IntermediateSchema`] so adding an option never
/// breaks the many full-literal `IntermediateSchema { .. }` constructions, and
/// the plain [`SchemaConverter::convert`] keeps its historical signature.
#[derive(Debug, Clone, Default)]
pub struct ConvertOptions {
    /// Auto-synthesize a shared `MutationError` type + per-mutation result unions
    /// and rewrite object-returning mutations to those unions (`[fraiseql.mutations]
    /// auto_error_union`).
    pub auto_error_union: bool,
}

impl SchemaConverter {
    /// Convert `IntermediateSchema` to `CompiledSchema` with default options.
    ///
    /// # Errors
    ///
    /// See [`SchemaConverter::convert_with_options`].
    pub fn convert(intermediate: IntermediateSchema) -> Result<CompiledSchema> {
        Self::convert_with_options(intermediate, &ConvertOptions::default())
    }

    /// Convert `IntermediateSchema` to `CompiledSchema`.
    ///
    /// This performs:
    /// 1. Type conversion (intermediate types → compiled types)
    /// 2. Field name normalization (type → `field_type`)
    /// 3. Optional mutation-error-union synthesis (`options.auto_error_union`)
    /// 4. Validation (type references, circular refs, etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if any type, query, mutation, interface, subscription,
    /// or directive conversion fails, if federation/security/observer config JSON
    /// cannot be deserialized, or if compiled schema validation detects unknown
    /// type references.
    pub fn convert_with_options(
        intermediate: IntermediateSchema,
        options: &ConvertOptions,
    ) -> Result<CompiledSchema> {
        info!("Converting intermediate schema to compiled format");

        // Aggregate the per-type `@subscribable(tables=[...])` annotations (#366)
        // into the top-level `CompiledSchema.subscribable` list BEFORE the types
        // are consumed below. Empty `tables` is treated as "not subscribable".
        let subscribable: Vec<fraiseql_core::schema::SubscribableEntity> = intermediate
            .types
            .iter()
            .filter_map(|t| {
                t.subscribable_tables
                    .as_ref()
                    .filter(|tables| !tables.is_empty())
                    .map(|tables| fraiseql_core::schema::SubscribableEntity {
                        entity_type: t.name.clone(),
                        tables:      tables.clone(),
                        pre_image:   t.subscribable_pre_image,
                    })
            })
            .collect();

        // Refuse a schema that declares observers, rather than validating them and throwing
        // them away (#779).
        //
        // The `observers` block is emitted by the Python SDK's `@fraiseql.observer` (its
        // shipped example declares five), it binds to `IntermediateSchema.observers`, and
        // `SchemaValidator` spends ~220 lines checking names, entities, events, actions and
        // retry policy — a typo in any of them fails the compile, which tells the author
        // emphatically that the block is honoured. It was not: this function set
        // `observers: Vec::new()` under a comment claiming they were "populated from
        // IntermediateSchema", nothing else ever wrote the field, and no webhook, Slack
        // message or email ever fired for any declared event.
        //
        // The runtime loads observers exclusively from `tb_observer` / the admin API and
        // reads nothing from the compiled schema. So *carrying* them here would only move
        // the silent drop one layer down and produce a compiled artifact whose `observers`
        // array is decoration. Until a runtime consumer exists, the honest outcome is to
        // fail and name the mechanism that does work.
        if let Some(observers) = intermediate.observers.as_ref().filter(|o| !o.is_empty()) {
            let names: Vec<&str> = observers.iter().map(|o| o.name.as_str()).collect();
            anyhow::bail!(
                "This schema declares {} observer(s) ({}), but declared observers are not \
                 loaded by the runtime — it reads them from the `tb_observer` table and the \
                 admin API only, so compiling them would produce a schema whose observers \
                 never fire.\n\nDefine them in `tb_observer`, or register them at runtime \
                 with `POST /api/observers`, and remove the `observers` block from the \
                 authored schema.",
                observers.len(),
                names.join(", ")
            );
        }

        // Split the authored `types` array on `is_input` before converting anything (#848).
        //
        // A type marked `is_input` is an input object that four SDKs happen to declare in
        // the `types` array — Elixir's exporter emits no `input_types` key at all, so this
        // is its only route. Routing it here rather than converting it as an object type is
        // what keeps the compiled schema GraphQL-legal: an argument may only be typed with
        // an input type (§3.10).
        let (input_marked_types, object_types): (Vec<_>, Vec<_>) =
            intermediate.types.into_iter().partition(|t| t.is_input);

        let types = object_types
            .into_iter()
            .map(Self::convert_type)
            .collect::<Result<Vec<_>>>()
            .context("Failed to convert types")?;

        // Extract query_defaults before consuming intermediate.queries.
        // unwrap_or_default() → all-true, matching historical behaviour when no
        // [query_defaults] section is present in fraiseql.toml.
        let defaults = intermediate.query_defaults.unwrap_or_default();

        // Apply `[inject_defaults]` (#847) to any operation that has not named the
        // parameter itself.
        //
        // This runs here, in the converter, rather than in `commands::compile` — the
        // converter is the single path every caller of the public API goes through, so a
        // default cannot be lost by reaching the compiled schema some other way.
        //
        // It is also deliberately *after* `[fraiseql.tenancy]` validation (which
        // `commands::compile` performs on the intermediate schema before calling this).
        // Tenancy auto-injects when `inject_params` is empty and errors when it is
        // non-empty but lacks the annotated field, so applying defaults first would make a
        // single unrelated default break every tenancy-annotated operation.
        let inject_defaults = intermediate.inject_defaults.unwrap_or_default();
        let query_defaults_inject = inject_defaults.for_queries();
        let mutation_defaults_inject = inject_defaults.for_mutations();

        // Convert queries
        let queries = intermediate
            .queries
            .into_iter()
            .map(|mut q| {
                IntermediateInjectDefaults::apply_to(&query_defaults_inject, &mut q.inject);
                Self::convert_query(q, &defaults)
            })
            .collect::<Result<Vec<_>>>()
            .context("Failed to convert queries")?;

        // Convert mutations
        let mutations = intermediate
            .mutations
            .into_iter()
            .map(|mut m| {
                IntermediateInjectDefaults::apply_to(&mutation_defaults_inject, &mut m.inject);
                Self::convert_mutation(m)
            })
            .collect::<Result<Vec<_>>>()
            .context("Failed to convert mutations")?;

        // Convert enums
        let enums = intermediate.enums.into_iter().map(Self::convert_enum).collect::<Vec<_>>();

        // Convert input types — the `input_types` array plus the `is_input`-marked entries
        // lifted out of `types` above. Both routes land in one registry, so a duplicate
        // between them is an authoring conflict and is refused rather than shadowed.
        let mut input_objects = intermediate.input_types;
        for marked in input_marked_types {
            let lifted = Self::input_object_from_marked_type(marked)?;
            if input_objects.iter().any(|existing| existing.name == lifted.name) {
                anyhow::bail!(
                    "'{}' is declared both in `input_types` and as a type marked \
                     `is_input: true`. Keep exactly one declaration — two definitions of one \
                     input object cannot be reconciled, and whichever won would depend on \
                     compile order.",
                    lifted.name
                );
            }
            input_objects.push(lifted);
        }
        let input_types =
            input_objects.into_iter().map(Self::convert_input_object).collect::<Vec<_>>();

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
            // Refused above when non-empty (#779); an empty vector is the only reachable
            // value, so this is not a drop.
            observers: Vec::new(),
            sources: intermediate.sources.clone().unwrap_or_default(), /* #573 scheduled ingress
                                                                        * sources */
            subscribable, // @subscribable capture-trigger declarations (#366)
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
            grpc_config: intermediate.grpc_config,             // gRPC config from TOML (#780)
            naming_convention: intermediate.naming_convention, // Naming convention from TOML
            session_variables: intermediate.session_variables.unwrap_or_default(),
            hierarchies_config: intermediate.hierarchies_config,
            changelog: intermediate.changelog_config, // Changelog exposure config from TOML
            schema_sdl: None,                         // Raw GraphQL SDL
            custom_scalars: CustomTypeRegistry::default(), // Custom scalar registry
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

        // Changelog exposure requires the observer system: the views it exposes read
        // from tables (`tb_entity_change_log`, `tb_transport_checkpoint`) that the
        // observer install convention supplies. Emitting GraphQL types over absent
        // tables would only fail later, at runtime — reject it here instead.
        if let Some(ref cl) = compiled.changelog {
            if cl.expose && !compiled.observers_config.as_ref().is_some_and(|o| o.enabled) {
                anyhow::bail!(
                    "[changelog] expose = true requires [observers] to be enabled: the tables \
                     it exposes (tb_entity_change_log, tb_transport_checkpoint) are installed by \
                     the observer system."
                );
            }
            // #497: the change-log surface is a per-database stream, not a federated
            // value type — the injected `EntityChangeLog` type and `entity_change_logs`
            // root query are not `@shareable`. Two subgraphs in one supergraph that both
            // `expose` it inject the identical type/root field, which `rover supergraph
            // compose` rejects with INVALID_FIELD_SHARING. We can't detect the collision
            // here (each subgraph compiles alone), so warn on the federated+exposed combo
            // and point at the single-owner pattern. This is a reminder, not an error —
            // the owning subgraph legitimately sets both.
            if cl.expose && compiled.federation.as_ref().is_some_and(|f| f.enabled) {
                warn!(
                    "[changelog] expose = true on a federation subgraph: EntityChangeLog / \
                     entity_change_logs are not @shareable, so expose the change-log in exactly \
                     one subgraph per supergraph (others keep capturing via [changelog] \
                     write_enabled). Two exposing subgraphs fail `rover supergraph compose` with \
                     INVALID_FIELD_SHARING. See the changelog-graphql guide for the single-owner \
                     pattern."
                );
            }
        }

        // Canonicalize the Trinity external identity (`id: UUID` → `id: ID`, ADR-0017)
        // before the interface-forcing passes, so Relay `Node` / `CascadeNode` see a
        // conformant `id: ID` on every entity. Wire-transparent (a UUID is an `ID`).
        identity::normalize_entity_identity(&mut compiled);

        // Inject synthetic Relay types (PageInfo, Node interface, XxxConnection, XxxEdge).
        relay::inject_relay_types(&mut compiled)?;

        // Compile rich filter types (EmailAddress, VIN, IBAN, etc.)
        let rich_filter_config = RichFilterConfig::default();
        compile_rich_filters(&mut compiled, &rich_filter_config)
            .context("Failed to compile rich filter types")?;

        // Inject the changelog GraphQL surface (EntityChangeLog / TransportCheckpoint
        // types + cursor query + point lookup + upsert mutation) when opted in.
        fraiseql_core::schema::inject_changelog(&mut compiled);

        // Synthesize the typed cascade surface (CascadeNode interface + envelope
        // types + per-mutation `<Name>Payload` wrappers) for mutations that opt in
        // via `cascade = true`. Runs BEFORE error-union synthesis so a cascade
        // payload becomes the success member of the result union
        // (`<Name>Result = <Name>Payload | MutationError`). Inert with no cascade
        // mutations.
        cascade_types::synthesize_cascade_types(&mut compiled)?;

        // Auto-synthesize a shared MutationError type + per-mutation result unions
        // when opted in (`[fraiseql.mutations] auto_error_union`), so the runtime's
        // success/error discrimination has a union to resolve against.
        if options.auto_error_union {
            mutation_error_union::synthesize_mutation_error_unions(&mut compiled);
        }

        // IR honesty (#456): `parse_field_type` has no `Input` variant, so an
        // argument referencing a declared input type was resolved to
        // `FieldType::Object`. Rewrite those argument types to `FieldType::Input`
        // now that every input type is registered (after relay / rich-filter /
        // changelog / error-union injection) so the IR honestly represents the
        // GraphQL input position — which also makes introspection report the correct
        // `INPUT_OBJECT` kind for those args. The runtime already accepts both
        // shapes, so this is purely representational.
        Self::promote_input_type_args(&mut compiled);

        // Validate the compiled schema
        Self::validate(&compiled)?;

        info!("Schema conversion successful");
        Ok(compiled)
    }

    /// Rewrite mutation/query argument types of the form `Object(name)` (and lists
    /// thereof) to `Input(name)` when `name` is a registered input type, so the IR
    /// honestly represents input-position references (#456). Input type names are
    /// disjoint from output type names per the GraphQL spec, so the lookup is exact
    /// — an output-object argument is never misclassified.
    fn promote_input_type_args(schema: &mut CompiledSchema) {
        fn promote(ft: &mut FieldType, input_names: &HashSet<String>) {
            match ft {
                FieldType::Object(name) if input_names.contains(name) => {
                    *ft = FieldType::Input(name.clone());
                },
                FieldType::List(inner) => promote(inner, input_names),
                _ => {},
            }
        }

        let input_names: HashSet<String> =
            schema.input_types.iter().map(|i| i.name.clone()).collect();
        if input_names.is_empty() {
            return;
        }

        for mutation in &mut schema.mutations {
            for arg in &mut mutation.arguments {
                promote(&mut arg.arg_type, &input_names);
            }
        }
        for query in &mut schema.queries {
            for arg in &mut query.arguments {
                promote(&mut arg.arg_type, &input_names);
            }
        }
    }

    #[allow(clippy::cognitive_complexity)] // Reason: comprehensive schema validation with many field-level checks
    fn validate(schema: &CompiledSchema) -> Result<()> {
        info!("Validating compiled schema");

        // Build type registry
        let mut type_names: HashSet<String> = HashSet::new();
        for type_def in &schema.types {
            type_names.insert(type_def.name.to_string());
        }

        // Build interface registry. Interfaces are *also* valid query/mutation
        // return types (a field may return an interface, narrowed via inline
        // fragments), so register them in `type_names` too — previously omitted,
        // which made a query/mutation returning an interface silently fail the
        // reference check below.
        let mut interface_names = HashSet::new();
        for interface_def in &schema.interfaces {
            interface_names.insert(interface_def.name.clone());
            type_names.insert(interface_def.name.clone());
        }

        // Add input types — valid as mutation argument types (fraiseql/fraiseql#190)
        for input_type in &schema.input_types {
            type_names.insert(input_type.name.clone());
        }

        // Add union type names — valid as mutation/query return types
        for union_def in &schema.unions {
            type_names.insert(union_def.name.clone());
        }

        // Add enum type names — valid as field and argument types
        for enum_def in &schema.enums {
            type_names.insert(enum_def.name.clone());
        }

        // Add built-in scalars
        for scalar in crate::schema::BUILTIN_SCALAR_NAMES {
            type_names.insert((*scalar).to_string());
        }

        // Add **declared** custom scalars, from the registry the compiler actually carries.
        //
        // This used to register every type name appearing in any object field, which made a
        // field-type typo legalize itself as a return type too (#724 item 2) — the same
        // blindness as `SchemaValidator`, duplicated here.
        for (name, _) in schema.custom_scalars.list_all() {
            type_names.insert(name);
        }

        // Collect every problem, then report them together.
        //
        // This tier used to `bail!` on the first error, with no suggestion, after redundantly
        // `warn!`ing the same text (#724 item 4). Errors that only surface after synthesis —
        // relay, cascade, changelog-injected types — are validated *here*, so the same class
        // of mistake got a materially worse experience depending on which tier caught it, and
        // a user with three typos needed three compile cycles for information the compiler
        // had in hand on the first.
        let mut problems: Vec<String> = Vec::new();
        let mut known: Vec<&str> = type_names.iter().map(String::as_str).collect();
        known.sort_unstable();

        let describe = |name: &str| -> String {
            let similar = fraiseql_core::runtime::suggest_similar(name, &known);
            if similar.is_empty() {
                String::new()
            } else {
                format!(" (did you mean: {}?)", similar.join(", "))
            }
        };

        // Validate queries
        for query in &schema.queries {
            if !type_names.contains(&query.return_type) {
                problems.push(format!(
                    "Query '{}' references unknown type '{}'{}",
                    query.name,
                    query.return_type,
                    describe(&query.return_type)
                ));
            }

            for arg in &query.arguments {
                let type_name = Self::extract_type_name(&arg.arg_type);
                if !type_names.contains(&type_name) {
                    problems.push(format!(
                        "Query '{}' argument '{}' references unknown type '{}'{}",
                        query.name,
                        arg.name,
                        type_name,
                        describe(&type_name)
                    ));
                }
            }
        }

        // Validate mutations
        for mutation in &schema.mutations {
            if !type_names.contains(&mutation.return_type) {
                problems.push(format!(
                    "Mutation '{}' references unknown type '{}'{}",
                    mutation.name,
                    mutation.return_type,
                    describe(&mutation.return_type)
                ));
            }

            for arg in &mutation.arguments {
                let type_name = Self::extract_type_name(&arg.arg_type);
                if !type_names.contains(&type_name) {
                    problems.push(format!(
                        "Mutation '{}' argument '{}' references unknown type '{}'{}",
                        mutation.name,
                        arg.name,
                        type_name,
                        describe(&type_name)
                    ));
                }
            }
        }

        // Validate interface implementations
        for type_def in &schema.types {
            for interface_name in &type_def.implements {
                if !interface_names.contains(interface_name) {
                    let mut candidates: Vec<&str> =
                        interface_names.iter().map(String::as_str).collect();
                    candidates.sort_unstable();
                    let similar =
                        fraiseql_core::runtime::suggest_similar(interface_name, &candidates);
                    let hint = if similar.is_empty() {
                        String::new()
                    } else {
                        format!(" (did you mean: {}?)", similar.join(", "))
                    };
                    problems.push(format!(
                        "Type '{}' implements unknown interface '{interface_name}'{hint}",
                        type_def.name
                    ));
                    continue;
                }

                // Validate that the type has all fields required by the interface
                if let Some(interface) = schema.find_interface(interface_name) {
                    for interface_field in &interface.fields {
                        let type_has_field = type_def.fields.iter().any(|f| {
                            f.name == interface_field.name
                                && f.field_type == interface_field.field_type
                        });
                        if !type_has_field {
                            problems.push(format!(
                                "Type '{}' implements interface '{interface_name}' but is \
                                 missing field '{}'",
                                type_def.name, interface_field.name
                            ));
                        }
                    }
                }
            }
        }

        if !problems.is_empty() {
            let count = problems.len();
            let noun = if count == 1 { "problem" } else { "problems" };
            anyhow::bail!(
                "Schema validation found {count} {noun}:\n{}",
                problems.iter().map(|p| format!("  - {p}")).collect::<Vec<_>>().join("\n")
            );
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
            // Reason: non_exhaustive requires catch-all for cross-crate matches
            _ => "Unknown".to_string(),
        }
    }

    /// Convert `IntermediateFactTable` to `FactTableMetadata`.
    fn convert_fact_table(ft: IntermediateFactTable) -> FactTableMetadata {
        FactTableMetadata {
            table_name:               ft.table_name,
            measures:                 ft
                .measures
                .into_iter()
                .map(|m| MeasureColumn {
                    name:     m.name,
                    sql_type: Self::parse_sql_type(&m.sql_type),
                    nullable: m.nullable,
                })
                .collect(),
            dimensions:               DimensionColumn {
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
            denormalized_filters:     ft
                .denormalized_filters
                .into_iter()
                .map(|f| FilterColumn {
                    name:     f.name,
                    sql_type: Self::parse_sql_type(&f.sql_type),
                    indexed:  f.indexed,
                })
                .collect(),
            calendar_dimensions:      vec![],
            partial_period:           None,
            native_measures:          ft.native_measures,
            native_dimension_mapping: ft.native_dimension_mapping,
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
