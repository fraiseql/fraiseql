//! Domain-specific methods for [`CompiledSchema`].
//!
//! Fact table management, observers, federation metadata, security configuration,
//! RLS, role scopes, tenancy, SDL generation, and schema validation.

#[cfg(feature = "federation")]
use std::collections::HashMap;
use std::fmt::Write as _;

use super::schema::{CURRENT_SCHEMA_FORMAT_VERSION, CompiledSchema};
use crate::{
    compiler::fact_table::FactTableMetadata,
    schema::{
        observer_types::ObserverDefinition,
        security_config::{RoleDefinition, SecurityConfig},
    },
};

impl CompiledSchema {
    /// Verify that the compiled schema was produced by a compatible compiler version.
    ///
    /// Schemas without a `schema_format_version` field (produced before v2.1) are
    /// accepted with a warning. Schemas with a mismatched version are rejected to
    /// prevent silent data corruption from structural changes.
    ///
    /// # Errors
    ///
    /// Returns an error string if the version is present and incompatible.
    pub fn validate_format_version(&self) -> Result<(), String> {
        match self.schema_format_version {
            None => {
                // Pre-versioning schema — accept but callers may want to warn.
                Ok(())
            },
            Some(v) if v == CURRENT_SCHEMA_FORMAT_VERSION => Ok(()),
            Some(v) => Err(format!(
                "Schema format version mismatch: compiled schema has version {v}, \
                 but this runtime expects version {CURRENT_SCHEMA_FORMAT_VERSION}. \
                 Please recompile your schema with the matching fraiseql-cli version."
            )),
        }
    }

    /// Register fact table metadata.
    ///
    /// # Arguments
    ///
    /// * `table_name` - Fact table name (e.g., `tf_sales`)
    /// * `metadata` - Typed `FactTableMetadata`
    pub fn add_fact_table(&mut self, table_name: String, metadata: FactTableMetadata) {
        self.fact_tables.insert(table_name, metadata);
    }

    /// Get fact table metadata by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Fact table name
    ///
    /// # Returns
    ///
    /// Fact table metadata if found
    #[must_use]
    pub fn get_fact_table(&self, name: &str) -> Option<&FactTableMetadata> {
        self.fact_tables.get(name)
    }

    /// List all fact table names.
    ///
    /// # Returns
    ///
    /// Vector of fact table names
    #[must_use]
    pub fn list_fact_tables(&self) -> Vec<&str> {
        self.fact_tables.keys().map(String::as_str).collect()
    }

    /// Check if schema contains any fact tables.
    #[must_use]
    pub fn has_fact_tables(&self) -> bool {
        !self.fact_tables.is_empty()
    }

    /// Find an observer definition by name.
    #[must_use]
    pub fn find_observer(&self, name: &str) -> Option<&ObserverDefinition> {
        self.observers.iter().find(|o| o.name == name)
    }

    /// Get all observers for a specific entity type.
    #[must_use]
    pub fn find_observers_for_entity(&self, entity: &str) -> Vec<&ObserverDefinition> {
        self.observers.iter().filter(|o| o.entity == entity).collect()
    }

    /// Get all observers for a specific event type (INSERT, UPDATE, DELETE).
    #[must_use]
    pub fn find_observers_for_event(&self, event: &str) -> Vec<&ObserverDefinition> {
        self.observers.iter().filter(|o| o.event == event).collect()
    }

    /// Check if schema contains any observers.
    #[must_use]
    pub const fn has_observers(&self) -> bool {
        !self.observers.is_empty()
    }

    /// Get total number of observers.
    #[must_use]
    pub const fn observer_count(&self) -> usize {
        self.observers.len()
    }

    /// Get federation metadata from schema.
    ///
    /// # Returns
    ///
    /// Federation metadata if configured in schema
    #[cfg(feature = "federation")]
    #[must_use]
    pub fn federation_metadata(&self) -> Option<crate::federation::FederationMetadata> {
        self.federation.as_ref().filter(|fed| fed.enabled).map(|fed| {
            use crate::federation::types::{
                FederatedType, FieldFederationDirectives, KeyDirective,
            };

            // Entities carry an `@key` (and, for an extended entity, `extend type` +
            // `@external` on the borrowed key/fields). Per-field directives are
            // rebuilt from the entity's `external_fields` / `shareable_fields` so the
            // SDL renderer can append `@external` / `@shareable` to each field line.
            let mut types: Vec<FederatedType> = fed
                .entities
                .iter()
                .map(|e| {
                    let mut field_directives: HashMap<String, FieldFederationDirectives> =
                        HashMap::new();
                    for f in &e.external_fields {
                        field_directives.entry(f.clone()).or_default().external = true;
                    }
                    for f in &e.shareable_fields {
                        field_directives.entry(f.clone()).or_default().shareable = true;
                    }
                    FederatedType {
                        name: e.name.clone(),
                        keys: vec![KeyDirective {
                            fields:     e.key_fields.clone(),
                            resolvable: true,
                        }],
                        is_extends: e.extends,
                        external_fields: e.external_fields.clone(),
                        shareable_fields: e.shareable_fields.clone(),
                        inaccessible_fields: Vec::new(),
                        field_directives,
                        type_shareable: false,
                    }
                })
                .collect();

            // Non-entity `@shareable` value types (e.g. a shared `MutationError`):
            // no `@key`, never a member of the `_Entity` union — they only receive a
            // type-level `@shareable` so both subgraphs can define the identical type
            // without an `INVALID_FIELD_SHARING` composition error.
            for name in &fed.shareable_types {
                types.push(FederatedType {
                    name:                name.clone(),
                    keys:                Vec::new(),
                    is_extends:          false,
                    external_fields:     Vec::new(),
                    shareable_fields:    Vec::new(),
                    inaccessible_fields: Vec::new(),
                    field_directives:    HashMap::new(),
                    type_shareable:      true,
                });
            }

            crate::federation::FederationMetadata {
                enabled: fed.enabled,
                version: fed.version.clone().unwrap_or_else(|| "v2".to_string()),
                types,
                remote_subscription_fields: HashMap::new(),
            }
        })
    }

    /// Build the per-entity-type backing source map (`typename` →
    /// [`EntitySource`](crate::federation::EntitySource)) the federation
    /// `_entities` resolver reads from instead of guessing `lower(typename)`
    /// (#504/#507).
    ///
    /// Two sources, query-wins:
    ///
    /// 1. **Query-sourced** (owned entities): the backing relation rides on the root query that
    ///    returns the type, keyed by `return_type`, first-wins — the same query→type binding the
    ///    Relay `node` path uses. The query's `jsonb_column` (which the compiler defaults to
    ///    `"data"`) drives jsonb projection.
    /// 2. **Type-sourced fallback** (#507): an owner-split `extend type … @key` entity resolved in
    ///    a subgraph that does not own it exposes no root query, so there is nothing in (1) to
    ///    source its relation from. Its relation instead rides on the type-level `sql_source` the
    ///    compiler carries from the authoring SDK. This only fills gaps — a query-sourced entry
    ///    always wins.
    ///
    /// Both sources read the entity's `jsonb_column` the same way: a non-empty column selects
    /// jsonb-projection mode (`<col>->'<field>'`), an empty one selects flat-column mode (bare
    /// columns). The compiler defaults both a query's and an extends type's `jsonb_column` to the
    /// standard `"data"` view shape, so a flat-column entity must be authored with an explicit
    /// empty `jsonb_column`.
    #[cfg(feature = "federation")]
    #[must_use]
    pub fn entity_sources(&self) -> HashMap<String, crate::federation::EntitySource> {
        use crate::federation::EntitySource;

        let mut sources: HashMap<String, EntitySource> = HashMap::new();

        // (1) Query-sourced — owned entities. First-wins per return_type.
        for q in &self.queries {
            if let Some(relation) = &q.sql_source {
                sources.entry(q.return_type.clone()).or_insert_with(|| EntitySource {
                    relation:     relation.clone(),
                    jsonb_column: (!q.jsonb_column.is_empty()).then(|| q.jsonb_column.clone()),
                });
            }
        }

        // (2) Type-sourced fallback — owner-split `extend type` entities (#507).
        // Skipped for owned types (their type-level sql_source is empty) and never
        // overrides a query-sourced entry. The empty-jsonb-column → flat-mode rule
        // mirrors the query path above, so flat-column extends entities resolve too.
        for t in &self.types {
            if t.sql_source.as_str().is_empty() {
                continue;
            }
            sources.entry(t.name.to_string()).or_insert_with(|| EntitySource {
                relation:     t.sql_source.to_string(),
                jsonb_column: (!t.jsonb_column.is_empty()).then(|| t.jsonb_column.clone()),
            });
        }

        sources
    }

    /// Stub federation metadata when federation feature is disabled.
    #[cfg(not(feature = "federation"))]
    #[must_use]
    pub const fn federation_metadata(&self) -> Option<()> {
        None
    }

    /// Get security configuration from schema.
    ///
    /// # Returns
    ///
    /// Security configuration if present (includes role definitions)
    #[must_use]
    pub const fn security_config(&self) -> Option<&SecurityConfig> {
        self.security.as_ref()
    }

    /// Returns `true` if this schema declares a multi-tenant deployment.
    ///
    /// Multi-tenant schemas require Row-Level Security (RLS) to be active whenever
    /// query result caching is enabled. Without RLS, all tenants sharing the same
    /// query parameters would receive the same cached response.
    ///
    /// Detection is based on `security.multi_tenant` in the compiled schema JSON.
    #[must_use]
    pub fn is_multi_tenant(&self) -> bool {
        self.security.as_ref().is_some_and(|s| s.multi_tenant)
    }

    /// Returns the tenancy isolation mode configured for this schema.
    ///
    /// Defaults to `TenancyMode::None` when no security or tenancy configuration
    /// is present, meaning single-tenant operation with no isolation machinery.
    #[must_use]
    pub fn tenancy_mode(&self) -> crate::schema::TenancyMode {
        self.security
            .as_ref()
            .map_or(crate::schema::TenancyMode::None, |s| s.tenancy.mode)
    }

    /// Returns the tenancy configuration, if present.
    ///
    /// Returns `None` when no security configuration exists. Returns the
    /// default `TenancyConfig` (mode=none) when security exists but tenancy
    /// is not explicitly configured.
    #[must_use]
    pub fn tenancy_config(&self) -> Option<&crate::schema::TenancyConfig> {
        self.security.as_ref().map(|s| &s.tenancy)
    }

    /// Find a role definition by name.
    ///
    /// # Arguments
    ///
    /// * `role_name` - Name of the role to find
    ///
    /// # Returns
    ///
    /// Role definition if found
    #[must_use]
    pub fn find_role(&self, role_name: &str) -> Option<RoleDefinition> {
        self.security.as_ref().and_then(|config| config.find_role(role_name).cloned())
    }

    /// Get scopes for a role.
    ///
    /// # Arguments
    ///
    /// * `role_name` - Name of the role
    ///
    /// # Returns
    ///
    /// Vector of scopes granted to the role
    #[must_use]
    pub fn get_role_scopes(&self, role_name: &str) -> Vec<String> {
        self.security
            .as_ref()
            .map(|config| config.get_role_scopes(role_name))
            .unwrap_or_default()
    }

    /// Check if a role has a specific scope.
    ///
    /// # Arguments
    ///
    /// * `role_name` - Name of the role
    /// * `scope` - Scope to check for
    ///
    /// # Returns
    ///
    /// true if role has the scope, false otherwise
    #[must_use]
    pub fn role_has_scope(&self, role_name: &str, scope: &str) -> bool {
        self.security
            .as_ref()
            .is_some_and(|config| config.role_has_scope(role_name, scope))
    }

    /// Returns `true` if Row-Level Security policies are declared in this schema.
    ///
    /// Used at server startup to validate that caching is safe for multi-tenant
    /// deployments. When caching is enabled and no RLS policies are configured,
    /// the server emits a startup warning about potential data leakage.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::CompiledSchema;
    ///
    /// let schema = CompiledSchema::default();
    /// assert!(!schema.has_rls_configured());
    /// ```
    #[must_use]
    pub fn has_rls_configured(&self) -> bool {
        self.security.as_ref().is_some_and(|s| {
            !s.additional
                .get("policies")
                .and_then(|p: &serde_json::Value| p.as_array())
                .is_none_or(|a| a.is_empty())
        })
    }

    /// Get raw GraphQL schema SDL.
    ///
    /// # Returns
    ///
    /// Raw schema string if available, otherwise generates from type definitions,
    /// including the root `Query`/`Mutation` types.
    ///
    /// Root operations are stored in [`self.queries`](Self::queries) /
    /// [`self.mutations`](Self::mutations) rather than as `Query`/`Mutation` object
    /// types in [`self.types`](Self::types), so they are rendered here explicitly.
    /// Omitting them produces an SDL that advertises no root fields — which makes the
    /// federation `_service` SDL (built from this output) fail gateway composition
    /// with `NO_QUERIES`.
    #[must_use]
    pub fn raw_schema(&self) -> String {
        self.schema_sdl.clone().unwrap_or_else(|| {
            // Generate basic SDL from type definitions if not provided
            let mut sdl = String::new();

            // Non-built-in scalar declarations. The rendered operations and fields
            // reference custom and standard-but-non-built-in scalars (`DateTime`,
            // `JSON`, `Decimal`, rich scalars, …); a gateway composing the subgraph
            // reports `Unknown type` for any it isn't declared.
            for name in self.referenced_scalars() {
                let _ = writeln!(sdl, "scalar {name}");
            }
            if !self.enums.is_empty()
                || !self.interfaces.is_empty()
                || !self.input_types.is_empty()
                || !self.unions.is_empty()
                || !self.types.is_empty()
            {
                sdl.push('\n');
            }

            // Enum types
            for enum_def in &self.enums {
                let _ = writeln!(sdl, "enum {} {{", enum_def.name);
                for value in &enum_def.values {
                    let _ = writeln!(sdl, "  {}", value.name);
                }
                sdl.push_str("}\n\n");
            }

            // Interface types
            for iface in &self.interfaces {
                let _ = writeln!(sdl, "interface {} {{", iface.name);
                for field in &iface.fields {
                    let _ = writeln!(sdl, "  {}: {}", field.name, field.field_type);
                }
                sdl.push_str("}\n\n");
            }

            // Input object types (`field_type` is a pre-rendered GraphQL string;
            // normalise the trailing non-null marker against the `nullable` flag).
            for input in &self.input_types {
                let _ = writeln!(sdl, "input {} {{", input.name);
                for field in &input.fields {
                    let base = field.field_type.trim_end_matches('!');
                    let non_null = if field.nullable { "" } else { "!" };
                    let _ = writeln!(sdl, "  {}: {base}{non_null}", field.name);
                }
                sdl.push_str("}\n\n");
            }

            // Union types (covers synthesized mutation result unions)
            for union_def in &self.unions {
                let _ = writeln!(
                    sdl,
                    "union {} = {}",
                    union_def.name,
                    union_def.member_types.join(" | ")
                );
            }
            if !self.unions.is_empty() {
                sdl.push('\n');
            }

            // Add output/object types
            for type_def in &self.types {
                let _ = writeln!(sdl, "type {} {{", type_def.name);
                for field in &type_def.fields {
                    let _ = writeln!(sdl, "  {}: {}", field.name, field.field_type);
                }
                sdl.push_str("}\n\n");
            }

            // Root Query type (rendered from `self.queries`, never present in `types`)
            if !self.queries.is_empty() {
                sdl.push_str("type Query {\n");
                for q in &self.queries {
                    let _ = writeln!(
                        sdl,
                        "  {}",
                        render_operation_field(
                            &q.name,
                            &q.arguments,
                            &q.return_type,
                            q.returns_list,
                            q.nullable,
                        )
                    );
                }
                sdl.push_str("}\n\n");
            }

            // Root Mutation type (rendered from `self.mutations`). Mutation payloads
            // are single, non-null values, so they render as `Name(args): Return!`.
            if !self.mutations.is_empty() {
                sdl.push_str("type Mutation {\n");
                for m in &self.mutations {
                    let _ = writeln!(
                        sdl,
                        "  {}",
                        render_operation_field(&m.name, &m.arguments, &m.return_type, false, false)
                    );
                }
                sdl.push_str("}\n\n");
            }

            sdl
        })
    }

    /// Collect the non-built-in scalar type names the schema references, so
    /// [`raw_schema`](Self::raw_schema) can declare each one (`scalar Name`) and the
    /// SDL is type-complete.
    ///
    /// A referenced type is treated as a scalar to declare when it is neither a
    /// built-in GraphQL scalar nor a type the schema defines as an object, enum,
    /// input, interface, or union. Names are collected **exactly as the fields render
    /// them** (the verbatim leaf of each field/argument type and each operation return
    /// type) so the declaration and the reference always agree — declaring a canonical
    /// alias (`DateTime`) while a field renders `datetime` would leave the reference
    /// dangling (`Unknown type datetime`). The custom-scalar registry is also included.
    /// The federation `_Any`/`_Entity`/`_Service`/`_FieldSet` built-ins (supplied by the
    /// federation layer) are excluded.
    fn referenced_scalars(&self) -> Vec<String> {
        use std::collections::{BTreeSet, HashSet};

        const BUILTINS: [&str; 5] = ["String", "Int", "Float", "Boolean", "ID"];
        const FED_BUILTINS: [&str; 4] = ["_Any", "_Entity", "_Service", "_FieldSet"];

        // Names the schema defines as composite types — never re-declared as scalars.
        let mut defined: HashSet<&str> = HashSet::new();
        for t in &self.types {
            defined.insert(t.name.as_str());
        }
        for e in &self.enums {
            defined.insert(e.name.as_str());
        }
        for i in &self.input_types {
            defined.insert(i.name.as_str());
        }
        for i in &self.interfaces {
            defined.insert(i.name.as_str());
        }
        for u in &self.unions {
            defined.insert(u.name.as_str());
        }

        // Every type reference, collected as the verbatim leaf name fields render.
        let mut referenced: BTreeSet<String> = BTreeSet::new();
        let add = |rendered: &str, set: &mut BTreeSet<String>| {
            let leaf = leaf_type_name(rendered);
            if !leaf.is_empty() {
                set.insert(leaf);
            }
        };
        for type_def in &self.types {
            for field in &type_def.fields {
                add(&field.field_type.to_string(), &mut referenced);
            }
        }
        for iface in &self.interfaces {
            for field in &iface.fields {
                add(&field.field_type.to_string(), &mut referenced);
            }
        }
        for query in &self.queries {
            for arg in &query.arguments {
                add(&arg.arg_type.to_string(), &mut referenced);
            }
            add(&query.return_type, &mut referenced);
        }
        for mutation in &self.mutations {
            for arg in &mutation.arguments {
                add(&arg.arg_type.to_string(), &mut referenced);
            }
            add(&mutation.return_type, &mut referenced);
        }
        for input in &self.input_types {
            for field in &input.fields {
                add(&field.field_type, &mut referenced);
            }
        }
        for (name, _) in self.custom_scalars.list_all() {
            referenced.insert(name);
        }

        referenced
            .into_iter()
            .filter(|name| {
                !defined.contains(name.as_str())
                    && !BUILTINS.contains(&name.as_str())
                    && !FED_BUILTINS.contains(&name.as_str())
            })
            .collect()
    }

    /// Validate the schema for internal consistency.
    ///
    /// Checks:
    /// - All type references resolve to defined types
    /// - No duplicate type/operation names
    /// - Required fields have valid types
    ///
    /// # Errors
    ///
    /// Returns list of validation errors if schema is invalid.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Check for duplicate type names
        let mut type_names: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for type_def in &self.types {
            if !type_names.insert(type_def.name.as_str()) {
                errors.push(format!("Duplicate type name: {}", type_def.name));
            }
        }

        // Check for duplicate query names
        let mut query_names: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for query in &self.queries {
            if !query_names.insert(&query.name) {
                errors.push(format!("Duplicate query name: {}", query.name));
            }
        }

        // Check for duplicate mutation names
        let mut mutation_names: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for mutation in &self.mutations {
            if !mutation_names.insert(&mutation.name) {
                errors.push(format!("Duplicate mutation name: {}", mutation.name));
            }
        }

        // Check type references in queries
        for query in &self.queries {
            if !type_names.contains(query.return_type.as_str())
                && !is_builtin_type(&query.return_type)
            {
                errors.push(format!(
                    "Query '{}' references undefined type '{}'",
                    query.name, query.return_type
                ));
            }
        }

        // Check type references in mutations
        for mutation in &self.mutations {
            if !type_names.contains(mutation.return_type.as_str())
                && !is_builtin_type(&mutation.return_type)
            {
                errors.push(format!(
                    "Mutation '{}' references undefined type '{}'",
                    mutation.name, mutation.return_type
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Render a root operation as a GraphQL SDL field: `name(arg: T!, …): Return`.
///
/// `return_type` is a bare type name; list-ness and nullability are applied here so
/// the rendered signature matches GraphQL conventions (`[User!]!`, `User`, `User!`).
fn render_operation_field(
    name: &str,
    arguments: &[crate::schema::ArgumentDefinition],
    return_type: &str,
    returns_list: bool,
    nullable: bool,
) -> String {
    let non_null = if nullable { "" } else { "!" };
    let ret = if returns_list {
        format!("[{return_type}!]{non_null}")
    } else {
        format!("{return_type}{non_null}")
    };
    if arguments.is_empty() {
        return format!("{name}: {ret}");
    }
    let args = arguments
        .iter()
        .map(|a| format!("{}: {}{}", a.name, a.arg_type, if a.nullable { "" } else { "!" }))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{name}({args}): {ret}")
}

/// Strip GraphQL list and non-null markers from a rendered type string, leaving the
/// bare leaf type name: `[User!]!` → `User`, `datetime` → `datetime`.
fn leaf_type_name(rendered: &str) -> String {
    rendered
        .chars()
        .filter(|c| !matches!(c, '[' | ']' | '!'))
        .collect::<String>()
        .trim()
        .to_string()
}

/// Check if a type name is a built-in scalar type.
fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "String"
            | "Int"
            | "Float"
            | "Boolean"
            | "ID"
            | "DateTime"
            | "Date"
            | "Time"
            | "JSON"
            | "UUID"
            | "Decimal"
    )
}
