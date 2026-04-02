//! Compiled schema types - pure Rust, no authoring-language references.
//!
//! These types represent GraphQL schemas after compilation from authoring languages.
//! All data is owned by Rust - no foreign object references.
//!
//! # Schema Freeze Invariant
//!
//! After `CompiledSchema::from_json()`, the schema is frozen:
//! - All data is Rust-owned
//! - No authoring-language callbacks or object references
//! - Safe to use from any Tokio worker thread
//!
//! This enables the Axum server to handle requests without any
//! interaction with the authoring-language runtime.

use std::{collections::HashMap, fmt::Write as _};

use serde::{Deserialize, Serialize};

use super::{directive::DirectiveDefinition, mutation::MutationDefinition, query::QueryDefinition};
use crate::{
    compiler::fact_table::FactTableMetadata,
    schema::{
        config_types::{
            DebugConfig, FederationConfig, GrpcConfig, McpConfig, ObserversConfig, RestConfig,
            SubscriptionsConfig, ValidationConfig,
        },
        graphql_type_defs::{
            EnumDefinition, InputObjectDefinition, InterfaceDefinition, TypeDefinition,
            UnionDefinition,
        },
        observer_types::ObserverDefinition,
        security_config::{RoleDefinition, SecurityConfig},
        subscription_types::SubscriptionDefinition,
    },
    validation::CustomTypeRegistry,
};

/// Current schema format version.
///
/// Increment this constant when the compiled schema JSON format changes in a
/// backward-incompatible way so that startup rejects stale compiled schemas.
pub const CURRENT_SCHEMA_FORMAT_VERSION: u32 = 1;

/// Complete compiled schema - all type information for serving.
///
/// This is the central type that holds the entire GraphQL schema
/// after compilation from any supported authoring language.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::CompiledSchema;
///
/// let json = r#"{
///     "types": [],
///     "queries": [],
///     "mutations": [],
///     "subscriptions": []
/// }"#;
///
/// let schema = CompiledSchema::from_json(json).unwrap();
/// assert_eq!(schema.types.len(), 0);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompiledSchema {
    /// GraphQL object type definitions.
    #[serde(default)]
    pub types: Vec<TypeDefinition>,

    /// GraphQL enum type definitions.
    #[serde(default)]
    pub enums: Vec<EnumDefinition>,

    /// GraphQL input object type definitions.
    #[serde(default)]
    pub input_types: Vec<InputObjectDefinition>,

    /// GraphQL interface type definitions.
    #[serde(default)]
    pub interfaces: Vec<InterfaceDefinition>,

    /// GraphQL union type definitions.
    #[serde(default)]
    pub unions: Vec<UnionDefinition>,

    /// GraphQL query definitions.
    #[serde(default)]
    pub queries: Vec<QueryDefinition>,

    /// GraphQL mutation definitions.
    #[serde(default)]
    pub mutations: Vec<MutationDefinition>,

    /// GraphQL subscription definitions.
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionDefinition>,

    /// Custom directive definitions.
    /// These are user-defined directives beyond the built-in @skip, @include, @deprecated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directives: Vec<DirectiveDefinition>,

    /// Fact table metadata (for analytics queries).
    /// Key: table name (e.g., `tf_sales`)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fact_tables: HashMap<String, FactTableMetadata>,

    /// Observer definitions (database change event listeners).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observers: Vec<ObserverDefinition>,

    /// Federation metadata for Apollo Federation v2 support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub federation: Option<FederationConfig>,

    /// Security configuration (from fraiseql.toml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<SecurityConfig>,

    /// Observers/event system configuration (from fraiseql.toml).
    ///
    /// Contains backend connection settings (`redis_url`, `nats_url`, etc.) and
    /// event handler definitions compiled from the `[observers]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observers_config: Option<ObserversConfig>,

    /// `WebSocket` subscription configuration (hooks, limits).
    /// Compiled from the `[subscriptions]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscriptions_config: Option<SubscriptionsConfig>,

    /// Query validation config (depth/complexity limits).
    /// Compiled from the `[validation]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_config: Option<ValidationConfig>,

    /// Debug/development configuration.
    /// Compiled from the `[debug]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_config: Option<DebugConfig>,

    /// MCP (Model Context Protocol) server configuration.
    /// Compiled from the `[mcp]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_config: Option<McpConfig>,

    /// REST transport configuration.
    /// Compiled from the `[rest]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_config: Option<RestConfig>,

    /// gRPC transport configuration.
    /// Compiled from the `[grpc]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc_config: Option<GrpcConfig>,

    /// Schema format version emitted by the compiler.
    ///
    /// Used to detect runtime/compiler skew. If present and ≠ `CURRENT_SCHEMA_FORMAT_VERSION`,
    /// `validate_format_version()` returns an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_format_version: Option<u32>,

    /// Raw GraphQL schema as string (for SDL generation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_sdl: Option<String>,

    /// Custom scalar type registry.
    ///
    /// Contains definitions for custom scalar types defined in the schema.
    /// Built during code generation from `IRScalar` definitions.
    /// Not serialized - populated at runtime from `ir.scalars`.
    #[serde(skip)]
    pub custom_scalars: CustomTypeRegistry,

    /// O(1) lookup index: query name → index into `self.queries`.
    /// Built at construction time by `build_indexes()`; not serialized.
    /// Populated automatically by `from_json()`; call `build_indexes()` after
    /// direct mutation of `self.queries`.
    #[serde(skip)]
    pub query_index: HashMap<String, usize>,

    /// O(1) lookup index: mutation name → index into `self.mutations`.
    /// Built at construction time by `build_indexes()`; not serialized.
    /// Populated automatically by `from_json()`; call `build_indexes()` after
    /// direct mutation of `self.mutations`.
    #[serde(skip)]
    pub mutation_index: HashMap<String, usize>,

    /// O(1) lookup index: subscription name → index into `self.subscriptions`.
    /// Built at construction time by `build_indexes()`; not serialized.
    /// Populated automatically by `from_json()`; call `build_indexes()` after
    /// direct mutation of `self.subscriptions`.
    #[serde(skip)]
    pub subscription_index: HashMap<String, usize>,
}

impl PartialEq for CompiledSchema {
    fn eq(&self, other: &Self) -> bool {
        // Compare all fields except custom_scalars (runtime state)
        self.schema_format_version == other.schema_format_version
            && self.types == other.types
            && self.enums == other.enums
            && self.input_types == other.input_types
            && self.interfaces == other.interfaces
            && self.unions == other.unions
            && self.queries == other.queries
            && self.mutations == other.mutations
            && self.subscriptions == other.subscriptions
            && self.directives == other.directives
            && self.fact_tables == other.fact_tables
            && self.observers == other.observers
            && self.federation == other.federation
            && self.security == other.security
            && self.observers_config == other.observers_config
            && self.subscriptions_config == other.subscriptions_config
            && self.validation_config == other.validation_config
            && self.debug_config == other.debug_config
            && self.mcp_config == other.mcp_config
            && self.schema_sdl == other.schema_sdl
    }
}

impl CompiledSchema {
    /// Create empty schema.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

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

    /// Build O(1) lookup indexes for queries, mutations, and subscriptions.
    ///
    /// Called automatically by `from_json()`. Must be called manually after any
    /// direct mutation of `self.queries`, `self.mutations`, or `self.subscriptions`.
    pub fn build_indexes(&mut self) {
        self.query_index =
            self.queries.iter().enumerate().map(|(i, q)| (q.name.clone(), i)).collect();
        self.mutation_index =
            self.mutations.iter().enumerate().map(|(i, m)| (m.name.clone(), i)).collect();
        self.subscription_index = self
            .subscriptions
            .iter()
            .enumerate()
            .map(|(i, s)| (s.name.clone(), i))
            .collect();
    }

    /// Deserialize from JSON string.
    ///
    /// This is the primary way to create a schema from any authoring language.
    /// The authoring language emits `schema.json`; `fraiseql-cli compile` produces
    /// `schema.compiled.json`; Rust deserializes and owns the result.
    ///
    /// # Integrity Checking
    ///
    /// When `fraiseql-cli compile` embeds a `_content_hash` field in the compiled JSON,
    /// the runtime should verify it against `content_hash()` before accepting the schema.
    /// This guards against accidental corruption or tampering between compilation and
    /// deployment. The check is not performed here because `_content_hash` is not yet
    /// written by the CLI; once it is, add a post-deserialization step:
    ///
    /// ```rust,ignore
    /// let schema = CompiledSchema::from_json(json)?;
    /// if let Some(expected) = &schema._content_hash {
    ///     let actual = schema.content_hash();
    ///     if *expected != actual {
    ///         return Err(IntegrityError::HashMismatch { expected, actual });
    ///     }
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns error if JSON is malformed or doesn't match schema structure.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::CompiledSchema;
    ///
    /// let json = r#"{"types": [], "queries": [], "mutations": [], "subscriptions": []}"#;
    /// let schema = CompiledSchema::from_json(json).unwrap();
    /// ```
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let mut schema: Self = serde_json::from_str(json)?;
        schema.build_indexes();
        Ok(schema)
    }

    /// Serialize to JSON string.
    ///
    /// # Errors
    ///
    /// Returns error if serialization fails (should not happen for valid schema).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize to pretty JSON string (for debugging/config files).
    ///
    /// # Errors
    ///
    /// Returns error if serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Find a type definition by name.
    #[must_use]
    pub fn find_type(&self, name: &str) -> Option<&TypeDefinition> {
        self.types.iter().find(|t| t.name == name)
    }

    /// Find an enum definition by name.
    #[must_use]
    pub fn find_enum(&self, name: &str) -> Option<&EnumDefinition> {
        self.enums.iter().find(|e| e.name == name)
    }

    /// Find an input object definition by name.
    #[must_use]
    pub fn find_input_type(&self, name: &str) -> Option<&InputObjectDefinition> {
        self.input_types.iter().find(|i| i.name == name)
    }

    /// Find an interface definition by name.
    #[must_use]
    pub fn find_interface(&self, name: &str) -> Option<&InterfaceDefinition> {
        self.interfaces.iter().find(|i| i.name == name)
    }

    /// Find all types that implement a given interface.
    #[must_use]
    pub fn find_implementors(&self, interface_name: &str) -> Vec<&TypeDefinition> {
        self.types
            .iter()
            .filter(|t| t.implements.contains(&interface_name.to_string()))
            .collect()
    }

    /// Find a union definition by name.
    #[must_use]
    pub fn find_union(&self, name: &str) -> Option<&UnionDefinition> {
        self.unions.iter().find(|u| u.name == name)
    }

    /// Find a query definition by name.
    ///
    /// Uses the O(1) pre-built index when available; falls back to O(n) linear
    /// scan for schemas built directly in tests without calling `build_indexes()`.
    #[must_use]
    pub fn find_query(&self, name: &str) -> Option<&QueryDefinition> {
        if self.query_index.is_empty() && !self.queries.is_empty() {
            self.queries.iter().find(|q| q.name == name)
        } else {
            self.query_index.get(name).map(|&i| &self.queries[i])
        }
    }

    /// Find a mutation definition by name.
    ///
    /// Uses the O(1) pre-built index when available; falls back to O(n) linear
    /// scan for schemas built directly in tests without calling `build_indexes()`.
    #[must_use]
    pub fn find_mutation(&self, name: &str) -> Option<&MutationDefinition> {
        if self.mutation_index.is_empty() && !self.mutations.is_empty() {
            self.mutations.iter().find(|m| m.name == name)
        } else {
            self.mutation_index.get(name).map(|&i| &self.mutations[i])
        }
    }

    /// Find a subscription definition by name.
    ///
    /// Uses the O(1) pre-built index when available; falls back to O(n) linear
    /// scan for schemas built directly in tests without calling `build_indexes()`.
    #[must_use]
    pub fn find_subscription(&self, name: &str) -> Option<&SubscriptionDefinition> {
        if self.subscription_index.is_empty() && !self.subscriptions.is_empty() {
            self.subscriptions.iter().find(|s| s.name == name)
        } else {
            self.subscription_index.get(name).map(|&i| &self.subscriptions[i])
        }
    }

    /// Find a custom directive definition by name.
    #[must_use]
    pub fn find_directive(&self, name: &str) -> Option<&DirectiveDefinition> {
        self.directives.iter().find(|d| d.name == name)
    }

    /// Get total number of operations (queries + mutations + subscriptions).
    #[must_use]
    pub const fn operation_count(&self) -> usize {
        self.queries.len() + self.mutations.len() + self.subscriptions.len()
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
                    name: e.name.clone(),
                    keys: vec![crate::federation::types::KeyDirective {
                        fields: e.key_fields.clone(),
                        resolvable: true,
                    }],
                    is_extends: false,
                    external_fields: Vec::new(),
                    shareable_fields: Vec::new(),
                    field_directives: std::collections::HashMap::new(),
                })
                .collect();

            crate::federation::FederationMetadata {
                enabled: fed.enabled,
                version: fed.version.clone().unwrap_or_else(|| "v2".to_string()),
                types,
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

    /// Returns a 32-character hex SHA-256 content hash of this schema's canonical JSON.
    ///
    /// Use as `schema_version` when constructing `CachedDatabaseAdapter` to guarantee
    /// cache invalidation on any schema change, regardless of whether the package
    /// version was bumped.
    ///
    /// Two schemas that differ by even one field will produce different hashes.
    /// The same schema serialised twice always produces the same hash (stable).
    ///
    /// # Panics
    ///
    /// Does not panic — `CompiledSchema` always serialises to valid JSON.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::CompiledSchema;
    ///
    /// let schema = CompiledSchema::default();
    /// let hash = schema.content_hash();
    /// assert_eq!(hash.len(), 32); // 16 bytes → 32 hex chars
    /// ```
    #[must_use]
    pub fn content_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let json = self.to_json().expect("CompiledSchema always serialises — BUG if this fails");
        let digest = Sha256::digest(json.as_bytes());
        hex::encode(&digest[..16]) // 32 hex chars — sufficient collision resistance
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
    /// Raw schema string if available, otherwise generates from type definitions
    #[must_use]
    pub fn raw_schema(&self) -> String {
        self.schema_sdl.clone().unwrap_or_else(|| {
            // Generate basic SDL from type definitions if not provided
            let mut sdl = String::new();

            // Add types
            for type_def in &self.types {
                let _ = writeln!(sdl, "type {} {{", type_def.name);
                for field in &type_def.fields {
                    let _ = writeln!(sdl, "  {}: {}", field.name, field.field_type);
                }
                sdl.push_str("}\n\n");
            }

            sdl
        })
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions
    use super::*;
    use crate::schema::{
        config_types::{FederationConfig, FederationEntity},
        graphql_type_defs::TypeDefinition,
        observer_types::ObserverDefinition,
        security_config::{RoleDefinition, SecurityConfig},
    };

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    fn make_type_def(name: &str) -> TypeDefinition {
        TypeDefinition {
            name: name.into(),
            sql_source: format!("v_{}", name.to_lowercase()).as_str().into(),
            jsonb_column: "data".to_string(),
            fields: vec![],
            description: None,
            sql_projection_hint: None,
            implements: vec![],
            requires_role: None,
            is_error: false,
            relay: false,
            relationships: vec![],
        }
    }

    fn make_query(name: &str, return_type: &str) -> QueryDefinition {
        QueryDefinition::new(name, return_type)
    }

    fn make_mutation(name: &str, return_type: &str) -> MutationDefinition {
        MutationDefinition::new(name, return_type)
    }

    // -------------------------------------------------------------------------
    // Constructor behaviour
    // -------------------------------------------------------------------------

    #[test]
    fn new_returns_empty_schema() {
        let schema = CompiledSchema::new();
        assert!(schema.types.is_empty());
        assert!(schema.queries.is_empty());
        assert!(schema.mutations.is_empty());
        assert!(schema.subscriptions.is_empty());
        assert!(schema.enums.is_empty());
        assert!(schema.interfaces.is_empty());
        assert!(schema.unions.is_empty());
    }

    #[test]
    fn from_json_empty_array_fields() {
        let json = r#"{"types":[],"queries":[],"mutations":[],"subscriptions":[]}"#;
        let schema = CompiledSchema::from_json(json).unwrap();
        assert_eq!(schema.types.len(), 0);
        assert_eq!(schema.queries.len(), 0);
        assert_eq!(schema.mutations.len(), 0);
        assert_eq!(schema.subscriptions.len(), 0);
    }

    #[test]
    fn from_json_minimal_empty_object() {
        // All fields have #[serde(default)] — an empty JSON object is valid
        let schema = CompiledSchema::from_json("{}").unwrap();
        assert!(schema.types.is_empty());
        assert!(schema.queries.is_empty());
    }

    #[test]
    fn from_json_invalid_returns_error() {
        let result = CompiledSchema::from_json("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn from_json_builds_query_index() {
        let json = r#"{
            "types": [{"name":"User","sql_source":"v_user","fields":[]}],
            "queries": [{"name":"users","return_type":"User"}],
            "mutations": [],
            "subscriptions": []
        }"#;
        let schema = CompiledSchema::from_json(json).unwrap();
        assert!(schema.query_index.contains_key("users"));
        assert_eq!(schema.query_index["users"], 0);
    }

    #[test]
    fn from_json_builds_mutation_index() {
        let json = r#"{
            "types": [{"name":"User","sql_source":"v_user","fields":[]}],
            "mutations": [{"name":"createUser","return_type":"User"}],
            "queries": [],
            "subscriptions": []
        }"#;
        let schema = CompiledSchema::from_json(json).unwrap();
        assert!(schema.mutation_index.contains_key("createUser"));
    }

    // -------------------------------------------------------------------------
    // Serialization round-trip
    // -------------------------------------------------------------------------

    #[test]
    fn to_json_and_back_is_identity() {
        let mut schema = CompiledSchema::new();
        schema.schema_format_version = Some(1);
        let json = schema.to_json().unwrap();
        let schema2 = CompiledSchema::from_json(&json).unwrap();
        assert_eq!(schema, schema2);
    }

    #[test]
    fn to_json_pretty_is_valid_json() {
        let schema = CompiledSchema::new();
        let pretty = schema.to_json_pretty().unwrap();
        // Should re-parse without error
        let _: serde_json::Value = serde_json::from_str(&pretty).unwrap();
    }

    // -------------------------------------------------------------------------
    // Format version
    // -------------------------------------------------------------------------

    #[test]
    fn validate_format_version_none_is_ok() {
        let schema = CompiledSchema::new(); // schema_format_version = None
        assert!(schema.validate_format_version().is_ok());
    }

    #[test]
    fn validate_format_version_current_is_ok() {
        let mut schema = CompiledSchema::new();
        schema.schema_format_version = Some(CURRENT_SCHEMA_FORMAT_VERSION);
        assert!(schema.validate_format_version().is_ok());
    }

    #[test]
    fn validate_format_version_mismatch_is_err() {
        let mut schema = CompiledSchema::new();
        schema.schema_format_version = Some(CURRENT_SCHEMA_FORMAT_VERSION + 1);
        let result = schema.validate_format_version();
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("mismatch"));
    }

    // -------------------------------------------------------------------------
    // Index building
    // -------------------------------------------------------------------------

    #[test]
    fn build_indexes_populates_all_three_maps() {
        let mut schema = CompiledSchema::new();
        schema.queries.push(make_query("getUser", "User"));
        schema.mutations.push(make_mutation("createUser", "User"));
        schema.build_indexes();
        assert!(schema.query_index.contains_key("getUser"));
        assert!(schema.mutation_index.contains_key("createUser"));
    }

    #[test]
    fn build_indexes_multiple_queries() {
        let mut schema = CompiledSchema::new();
        schema.queries.push(make_query("alpha", "A"));
        schema.queries.push(make_query("beta", "B"));
        schema.queries.push(make_query("gamma", "C"));
        schema.build_indexes();
        assert_eq!(schema.query_index["alpha"], 0);
        assert_eq!(schema.query_index["beta"], 1);
        assert_eq!(schema.query_index["gamma"], 2);
    }

    // -------------------------------------------------------------------------
    // Finder methods
    // -------------------------------------------------------------------------

    #[test]
    fn find_type_returns_none_for_missing() {
        let schema = CompiledSchema::new();
        assert!(schema.find_type("Ghost").is_none());
    }

    #[test]
    fn find_type_returns_existing() {
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type_def("User"));
        assert!(schema.find_type("User").is_some());
        assert_eq!(schema.find_type("User").unwrap().name, "User");
    }

    #[test]
    fn find_query_uses_index_when_populated() {
        let json = r#"{
            "types": [{"name":"User","sql_source":"v_user","fields":[]}],
            "queries": [{"name":"users","return_type":"User"}],
            "mutations": [],
            "subscriptions": []
        }"#;
        let schema = CompiledSchema::from_json(json).unwrap();
        let q = schema.find_query("users");
        assert!(q.is_some());
        assert_eq!(q.unwrap().name, "users");
    }

    #[test]
    fn find_query_falls_back_to_linear_scan_without_index() {
        // Build schema directly without calling build_indexes
        let mut schema = CompiledSchema::new();
        schema.queries.push(make_query("direct", "String"));
        // query_index is empty but queries is not — should fall back to linear scan
        let q = schema.find_query("direct");
        assert!(q.is_some());
    }

    #[test]
    fn find_query_returns_none_for_missing() {
        let schema = CompiledSchema::from_json("{}").unwrap();
        assert!(schema.find_query("nope").is_none());
    }

    #[test]
    fn find_mutation_returns_correct_entry() {
        let json = r#"{
            "types": [{"name":"User","sql_source":"v_user","fields":[]}],
            "mutations": [{"name":"createUser","return_type":"User"}],
            "queries": [],
            "subscriptions": []
        }"#;
        let schema = CompiledSchema::from_json(json).unwrap();
        assert!(schema.find_mutation("createUser").is_some());
        assert!(schema.find_mutation("nope").is_none());
    }

    #[test]
    fn find_interface_returns_none_when_absent() {
        let schema = CompiledSchema::new();
        assert!(schema.find_interface("Node").is_none());
    }

    #[test]
    fn find_implementors_filters_by_interface() {
        let mut schema = CompiledSchema::new();
        let mut user = make_type_def("User");
        user.implements = vec!["Node".to_string()];
        schema.types.push(user);
        schema.types.push(make_type_def("Product")); // does not implement Node

        let implementors = schema.find_implementors("Node");
        assert_eq!(implementors.len(), 1);
        assert_eq!(implementors[0].name, "User");
    }

    // -------------------------------------------------------------------------
    // operation_count
    // -------------------------------------------------------------------------

    #[test]
    fn operation_count_sums_all_three() {
        let mut schema = CompiledSchema::new();
        schema.queries.push(make_query("q1", "String"));
        schema.queries.push(make_query("q2", "String"));
        schema.mutations.push(make_mutation("m1", "String"));
        assert_eq!(schema.operation_count(), 3);
    }

    #[test]
    fn operation_count_zero_for_empty_schema() {
        assert_eq!(CompiledSchema::new().operation_count(), 0);
    }

    // -------------------------------------------------------------------------
    // Fact tables
    // -------------------------------------------------------------------------

    #[test]
    fn fact_table_add_and_get() {
        use crate::compiler::fact_table::{DimensionColumn, FactTableMetadata};

        let mut schema = CompiledSchema::new();
        assert!(!schema.has_fact_tables());

        let meta = FactTableMetadata {
            table_name: "tf_sales".to_string(),
            measures: vec![],
            dimensions: DimensionColumn {
                name: "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions: vec![],
        };
        schema.add_fact_table("tf_sales".to_string(), meta);

        assert!(schema.has_fact_tables());
        assert!(schema.get_fact_table("tf_sales").is_some());
        assert!(schema.get_fact_table("tf_missing").is_none());
    }

    #[test]
    fn list_fact_tables_returns_all_names() {
        use crate::compiler::fact_table::{DimensionColumn, FactTableMetadata};

        let make_meta = |name: &str| FactTableMetadata {
            table_name: name.to_string(),
            measures: vec![],
            dimensions: DimensionColumn {
                name: "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions: vec![],
        };

        let mut schema = CompiledSchema::new();
        schema.add_fact_table("tf_a".to_string(), make_meta("tf_a"));
        schema.add_fact_table("tf_b".to_string(), make_meta("tf_b"));

        let names = schema.list_fact_tables();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"tf_a"));
        assert!(names.contains(&"tf_b"));
    }

    // -------------------------------------------------------------------------
    // Observers
    // -------------------------------------------------------------------------

    #[test]
    fn has_observers_false_for_empty_schema() {
        assert!(!CompiledSchema::new().has_observers());
    }

    #[test]
    fn find_observer_returns_by_name() {
        let mut schema = CompiledSchema::new();
        schema.observers.push(ObserverDefinition::new("onInsert", "Order", "INSERT"));
        assert!(schema.find_observer("onInsert").is_some());
        assert!(schema.find_observer("missing").is_none());
    }

    #[test]
    fn find_observers_for_entity_filters_correctly() {
        let mut schema = CompiledSchema::new();
        schema.observers.push(ObserverDefinition::new("obs1", "Order", "INSERT"));
        schema.observers.push(ObserverDefinition::new("obs2", "Order", "UPDATE"));
        schema.observers.push(ObserverDefinition::new("obs3", "User", "INSERT"));

        let order_obs = schema.find_observers_for_entity("Order");
        assert_eq!(order_obs.len(), 2);
        let user_obs = schema.find_observers_for_entity("User");
        assert_eq!(user_obs.len(), 1);
    }

    #[test]
    fn find_observers_for_event_filters_correctly() {
        let mut schema = CompiledSchema::new();
        schema.observers.push(ObserverDefinition::new("obs1", "Order", "INSERT"));
        schema.observers.push(ObserverDefinition::new("obs2", "User", "INSERT"));
        schema.observers.push(ObserverDefinition::new("obs3", "Order", "DELETE"));

        let inserts = schema.find_observers_for_event("INSERT");
        assert_eq!(inserts.len(), 2);
    }

    #[test]
    fn observer_count_matches_vec_length() {
        let mut schema = CompiledSchema::new();
        assert_eq!(schema.observer_count(), 0);
        schema.observers.push(ObserverDefinition::new("o1", "A", "INSERT"));
        assert_eq!(schema.observer_count(), 1);
    }

    // -------------------------------------------------------------------------
    // Security helpers
    // -------------------------------------------------------------------------

    #[test]
    fn is_multi_tenant_false_by_default() {
        assert!(!CompiledSchema::new().is_multi_tenant());
    }

    #[test]
    fn is_multi_tenant_true_when_configured() {
        let mut schema = CompiledSchema::new();
        let mut sec = SecurityConfig::new();
        sec.multi_tenant = true;
        schema.security = Some(sec);
        assert!(schema.is_multi_tenant());
    }

    #[test]
    fn find_role_returns_none_without_security_config() {
        assert!(CompiledSchema::new().find_role("admin").is_none());
    }

    #[test]
    fn find_role_returns_defined_role() {
        let mut schema = CompiledSchema::new();
        let mut sec = SecurityConfig::new();
        sec.add_role(RoleDefinition::new("editor", vec!["read:*".to_string()]));
        schema.security = Some(sec);
        assert!(schema.find_role("editor").is_some());
    }

    #[test]
    fn role_has_scope_false_without_security() {
        assert!(!CompiledSchema::new().role_has_scope("admin", "read:*"));
    }

    #[test]
    fn role_has_scope_true_when_granted() {
        let mut schema = CompiledSchema::new();
        let mut sec = SecurityConfig::new();
        sec.add_role(RoleDefinition::new("admin", vec!["read:*".to_string()]));
        schema.security = Some(sec);
        assert!(schema.role_has_scope("admin", "read:anything"));
        assert!(!schema.role_has_scope("admin", "write:anything"));
    }

    #[test]
    fn get_role_scopes_empty_for_missing_role() {
        let schema = CompiledSchema::new();
        assert!(schema.get_role_scopes("ghost").is_empty());
    }

    // -------------------------------------------------------------------------
    // Federation metadata
    // -------------------------------------------------------------------------

    #[test]
    fn federation_metadata_none_when_no_federation() {
        assert!(CompiledSchema::new().federation_metadata().is_none());
    }

    #[test]
    fn federation_metadata_none_when_disabled() {
        let mut schema = CompiledSchema::new();
        schema.federation = Some(FederationConfig {
            enabled: false,
            ..Default::default()
        });
        assert!(schema.federation_metadata().is_none());
    }

    #[test]
    #[cfg(feature = "federation")]
    fn federation_metadata_some_when_enabled() {
        let mut schema = CompiledSchema::new();
        schema.federation = Some(FederationConfig {
            enabled: true,
            version: Some("v2".to_string()),
            entities: vec![FederationEntity {
                name: "User".to_string(),
                key_fields: vec!["id".to_string()],
            }],
            ..Default::default()
        });
        let meta = schema.federation_metadata();
        assert!(meta.is_some());
        let meta = meta.unwrap();
        assert!(meta.enabled);
        assert_eq!(meta.types.len(), 1);
        assert_eq!(meta.types[0].name, "User");
    }

    // -------------------------------------------------------------------------
    // content_hash
    // -------------------------------------------------------------------------

    #[test]
    fn content_hash_is_32_hex_chars() {
        let hash = CompiledSchema::new().content_hash();
        assert_eq!(hash.len(), 32);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn content_hash_is_stable() {
        let schema = CompiledSchema::new();
        assert_eq!(schema.content_hash(), schema.content_hash());
    }

    #[test]
    fn content_hash_differs_for_different_schemas() {
        let s1 = CompiledSchema::new();
        let mut s2 = CompiledSchema::new();
        s2.schema_format_version = Some(1);
        assert_ne!(s1.content_hash(), s2.content_hash());
    }

    // -------------------------------------------------------------------------
    // has_rls_configured
    // -------------------------------------------------------------------------

    #[test]
    fn has_rls_configured_false_without_security() {
        assert!(!CompiledSchema::new().has_rls_configured());
    }

    #[test]
    fn has_rls_configured_false_when_policies_empty() {
        let mut schema = CompiledSchema::new();
        let mut sec = SecurityConfig::new();
        sec.additional.insert("policies".to_string(), serde_json::json!([]));
        schema.security = Some(sec);
        assert!(!schema.has_rls_configured());
    }

    #[test]
    fn has_rls_configured_true_when_policies_present() {
        let mut schema = CompiledSchema::new();
        let mut sec = SecurityConfig::new();
        sec.additional.insert(
            "policies".to_string(),
            serde_json::json!([{"table": "orders", "using": "tenant_id = current_setting('app.tenant_id')"}]),
        );
        schema.security = Some(sec);
        assert!(schema.has_rls_configured());
    }

    // -------------------------------------------------------------------------
    // validate()
    // -------------------------------------------------------------------------

    #[test]
    fn validate_empty_schema_is_ok() {
        assert!(CompiledSchema::new().validate().is_ok());
    }

    #[test]
    fn validate_detects_duplicate_type_names() {
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type_def("User"));
        schema.types.push(make_type_def("User")); // duplicate
        let result = schema.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("Duplicate type name")));
    }

    #[test]
    fn validate_detects_duplicate_query_names() {
        let mut schema = CompiledSchema::new();
        schema.queries.push(make_query("getUser", "String"));
        schema.queries.push(make_query("getUser", "String")); // duplicate
        let result = schema.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("Duplicate query name")));
    }

    #[test]
    fn validate_detects_duplicate_mutation_names() {
        let mut schema = CompiledSchema::new();
        schema.mutations.push(make_mutation("createUser", "String"));
        schema.mutations.push(make_mutation("createUser", "String")); // duplicate
        let result = schema.validate();
        assert!(result.is_err());
    }

    #[test]
    fn validate_undefined_return_type_in_query_is_error() {
        let mut schema = CompiledSchema::new();
        // No "Widget" type defined
        schema.queries.push(make_query("getWidget", "Widget"));
        let result = schema.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("Widget")));
    }

    #[test]
    fn validate_builtin_scalar_return_type_is_ok() {
        let mut schema = CompiledSchema::new();
        schema.queries.push(make_query("ping", "String"));
        schema.queries.push(make_query("count", "Int"));
        assert!(schema.validate().is_ok());
    }

    #[test]
    fn validate_defined_type_as_return_type_is_ok() {
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type_def("User"));
        schema.queries.push(make_query("getUser", "User"));
        assert!(schema.validate().is_ok());
    }

    // -------------------------------------------------------------------------
    // raw_schema
    // -------------------------------------------------------------------------

    #[test]
    fn raw_schema_returns_sdl_when_set() {
        let mut schema = CompiledSchema::new();
        schema.schema_sdl = Some("type Query { ping: String }".to_string());
        assert_eq!(schema.raw_schema(), "type Query { ping: String }");
    }

    #[test]
    fn raw_schema_generates_from_types_when_sdl_absent() {
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type_def("User"));
        let sdl = schema.raw_schema();
        assert!(sdl.contains("User"));
    }

    // -------------------------------------------------------------------------
    // is_builtin_type (private fn — tested via validate())
    // -------------------------------------------------------------------------

    #[test]
    fn builtin_scalar_types_pass_validation() {
        let scalars = [
            "String", "Int", "Float", "Boolean", "ID", "DateTime", "Date", "Time", "JSON", "UUID",
            "Decimal",
        ];
        for scalar in scalars {
            let mut schema = CompiledSchema::new();
            schema.queries.push(make_query("q", scalar));
            assert!(schema.validate().is_ok(), "{scalar} should be a recognised built-in");
        }
    }

    #[test]
    fn unknown_scalar_fails_validation() {
        let mut schema = CompiledSchema::new();
        schema.queries.push(make_query("q", "Blob"));
        assert!(schema.validate().is_err());
    }
}
