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
            let types = fed
                .entities
                .iter()
                .map(|e| crate::federation::types::FederatedType {
                    name:                e.name.clone(),
                    keys:                vec![crate::federation::types::KeyDirective {
                        fields:     e.key_fields.clone(),
                        resolvable: true,
                    }],
                    is_extends:          false,
                    external_fields:     Vec::new(),
                    shareable_fields:    Vec::new(),
                    inaccessible_fields: Vec::new(),
                    field_directives:    std::collections::HashMap::new(),
                    type_shareable:      false,
                })
                .collect();

            crate::federation::FederationMetadata {
                enabled: fed.enabled,
                version: fed.version.clone().unwrap_or_else(|| "v2".to_string()),
                types,
                remote_subscription_fields: HashMap::new(),
            }
        })
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
    /// SDL is type-complete. Sources: the custom-scalar registry, every field /
    /// argument [`FieldType`](crate::schema::FieldType), and the string-typed input
    /// fields and operation return types. Built-in GraphQL scalars and the federation
    /// `_Any` (supplied separately) are excluded.
    fn referenced_scalars(&self) -> Vec<String> {
        use std::collections::BTreeSet;

        use crate::schema::FieldType;

        let mut scalars: BTreeSet<String> = BTreeSet::new();

        for (name, _) in self.custom_scalars.list_all() {
            scalars.insert(name);
        }
        let collect = |ft: &FieldType, set: &mut BTreeSet<String>| {
            if let Some(name) = scalar_leaf_name(ft) {
                set.insert(name);
            }
        };
        for type_def in &self.types {
            for field in &type_def.fields {
                collect(&field.field_type, &mut scalars);
            }
        }
        for iface in &self.interfaces {
            for field in &iface.fields {
                collect(&field.field_type, &mut scalars);
            }
        }
        for query in &self.queries {
            for arg in &query.arguments {
                collect(&arg.arg_type, &mut scalars);
            }
            collect(&FieldType::parse(&query.return_type), &mut scalars);
        }
        for mutation in &self.mutations {
            for arg in &mutation.arguments {
                collect(&arg.arg_type, &mut scalars);
            }
            collect(&FieldType::parse(&mutation.return_type), &mut scalars);
        }
        for input in &self.input_types {
            for field in &input.fields {
                collect(&FieldType::parse(&field.field_type), &mut scalars);
            }
        }

        scalars.remove("_Any"); // federation built-in, declared by the federation layer
        scalars.into_iter().collect()
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

/// Extract the leaf scalar type name from a [`FieldType`](crate::schema::FieldType),
/// or `None` for built-in GraphQL scalars, vectors, and composite types.
///
/// Lists are unwrapped to their element type. Non-built-in standard scalars render
/// under their GraphQL name (`DateTime`, `JSON`, `UUID`, …); rich/custom scalars under
/// their registered name.
fn scalar_leaf_name(field_type: &crate::schema::FieldType) -> Option<String> {
    use crate::schema::FieldType as F;
    match field_type {
        F::List(inner) => scalar_leaf_name(inner),
        F::DateTime => Some("DateTime".to_string()),
        F::Date => Some("Date".to_string()),
        F::Time => Some("Time".to_string()),
        F::Json => Some("JSON".to_string()),
        F::Uuid => Some("UUID".to_string()),
        F::Decimal => Some("Decimal".to_string()),
        F::Scalar(name) => Some(name.clone()),
        // Built-in GraphQL scalars, vectors (`[Float!]!`), and composite types need
        // no `scalar` declaration.
        F::String
        | F::Int
        | F::Float
        | F::Boolean
        | F::Id
        | F::Vector
        | F::Object(_)
        | F::Enum(_)
        | F::Input(_)
        | F::Interface(_)
        | F::Union(_) => None,
    }
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
