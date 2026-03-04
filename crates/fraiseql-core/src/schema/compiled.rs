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

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::compiler::fact_table::FactTableMetadata;

use super::config_types::{
    DebugConfig, FederationConfig, McpConfig, ObserversConfig, SubscriptionsConfig, ValidationConfig,
};
use super::field_type::FieldType;
use super::graphql_type_defs::{
    EnumDefinition, InputObjectDefinition, InterfaceDefinition, TypeDefinition,
    UnionDefinition, default_jsonb_column,
};
use super::observer_types::ObserverDefinition;
use super::security_config::{InjectedParamSource, RoleDefinition, SecurityConfig};
use super::subscription_types::SubscriptionDefinition;
use crate::validation::CustomTypeRegistry;

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
    /// Contains backend connection settings (redis_url, nats_url, etc.) and
    /// event handler definitions compiled from the `[observers]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observers_config: Option<ObserversConfig>,

    /// WebSocket subscription configuration (hooks, limits).
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

    /// Raw GraphQL schema as string (for SDL generation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_sdl: Option<String>,

    /// Custom scalar type registry.
    ///
    /// Contains definitions for custom scalar types defined in the schema.
    /// Built during code generation from IRScalar definitions.
    /// Not serialized - populated at runtime from `ir.scalars`.
    #[serde(skip)]
    pub custom_scalars: CustomTypeRegistry,
}

impl PartialEq for CompiledSchema {
    fn eq(&self, other: &Self) -> bool {
        // Compare all fields except custom_scalars (runtime state)
        self.types == other.types
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

    /// Deserialize from JSON string.
    ///
    /// This is the primary way to create a schema from any authoring language.
    /// The authoring language emits `schema.json`; `fraiseql-cli compile` produces
    /// `schema.compiled.json`; Rust deserializes and owns the result.
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
        serde_json::from_str(json)
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
    #[must_use]
    pub fn find_query(&self, name: &str) -> Option<&QueryDefinition> {
        self.queries.iter().find(|q| q.name == name)
    }

    /// Find a mutation definition by name.
    #[must_use]
    pub fn find_mutation(&self, name: &str) -> Option<&MutationDefinition> {
        self.mutations.iter().find(|m| m.name == name)
    }

    /// Find a subscription definition by name.
    #[must_use]
    pub fn find_subscription(&self, name: &str) -> Option<&SubscriptionDefinition> {
        self.subscriptions.iter().find(|s| s.name == name)
    }

    /// Find a custom directive definition by name.
    #[must_use]
    pub fn find_directive(&self, name: &str) -> Option<&DirectiveDefinition> {
        self.directives.iter().find(|d| d.name == name)
    }

    /// Get total number of operations (queries + mutations + subscriptions).
    #[must_use]
    pub fn operation_count(&self) -> usize {
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
    pub fn has_observers(&self) -> bool {
        !self.observers.is_empty()
    }

    /// Get total number of observers.
    #[must_use]
    pub fn observer_count(&self) -> usize {
        self.observers.len()
    }

    /// Get federation metadata from schema.
    ///
    /// # Returns
    ///
    /// Federation metadata if configured in schema
    #[must_use]
    pub fn federation_metadata(&self) -> Option<crate::federation::FederationMetadata> {
        self.federation.as_ref().filter(|fed| fed.enabled).map(|fed| {
            let types = fed
                .entities
                .iter()
                .map(|e| crate::federation::types::FederatedType {
                    name:             e.name.clone(),
                    keys:             vec![crate::federation::types::KeyDirective {
                        fields:     e.key_fields.clone(),
                        resolvable: true,
                    }],
                    is_extends:       false,
                    external_fields:  Vec::new(),
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

    /// Get security configuration from schema.
    ///
    /// # Returns
    ///
    /// Security configuration if present (includes role definitions)
    #[must_use]
    pub fn security_config(&self) -> Option<&SecurityConfig> {
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
        let json = self
            .to_json()
            .expect("CompiledSchema always serialises — BUG if this fails");
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
        self.security
            .as_ref()
            .map(|s| {
                !s.additional
                    .get("policies")
                    .and_then(|p: &serde_json::Value| p.as_array())
                    .is_none_or(|a| a.is_empty())
            })
            .unwrap_or(false)
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
                sdl.push_str(&format!("type {} {{\n", type_def.name));
                for field in &type_def.fields {
                    sdl.push_str(&format!("  {}: {}\n", field.name, field.field_type));
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

/// The type of column used as the keyset cursor for relay pagination.
///
/// Determines how the cursor value is encoded/decoded and how the SQL comparison
/// is emitted (`bigint` vs `uuid` cast).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CursorType {
    /// BIGINT / INTEGER column (default, backward-compatible).
    /// Cursor is `base64(decimal_string)`.
    #[default]
    Int64,
    /// UUID column.
    /// Cursor is `base64(uuid_string)`.
    Uuid,
}

fn is_default_cursor_type(ct: &CursorType) -> bool {
    *ct == CursorType::Int64
}

/// A query definition compiled from `@fraiseql.query`.
///
/// Queries are declarative bindings to database views/tables.
/// They describe *what* to fetch, not *how* to fetch it.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::QueryDefinition;
///
/// let query = QueryDefinition::new("users", "User");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryDefinition {
    /// Query name (e.g., "users").
    pub name: String,

    /// Return type name (e.g., "User").
    pub return_type: String,

    /// Does this query return a list?
    #[serde(default)]
    pub returns_list: bool,

    /// Is the return value nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Query arguments.
    #[serde(default)]
    pub arguments: Vec<ArgumentDefinition>,

    /// SQL source table/view (for direct table queries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,

    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Auto-wired parameters (where, orderBy, limit, offset).
    #[serde(default)]
    pub auto_params: AutoParams,

    /// Deprecation information (from @deprecated directive).
    /// When set, this query is marked as deprecated in the schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<super::field_type::DeprecationInfo>,

    /// JSONB column name (e.g., "data").
    /// Used to extract data from JSONB columns in query results.
    #[serde(default = "default_jsonb_column")]
    pub jsonb_column: String,

    /// Whether this query is a Relay connection query.
    ///
    /// When `true`, the compiler wraps the result in `XxxConnection` with
    /// `edges { cursor node { ... } }` and `pageInfo` fields, using keyset
    /// pagination on `pk_{snake_case(return_type)}` (BIGINT).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub relay: bool,

    /// Keyset pagination column for relay queries.
    ///
    /// Derived from the return type name: `User` → `pk_user`.
    /// This BIGINT column lives in the view (`sql_source`) and is used as the
    /// stable sort key for cursor-based keyset pagination:
    /// - Forward: `WHERE {col} > $cursor ORDER BY {col} ASC LIMIT $first`
    /// - Backward: `WHERE {col} < $cursor ORDER BY {col} DESC LIMIT $last`
    ///
    /// Only set when `relay = true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_cursor_column: Option<String>,

    /// Type of the keyset cursor column.
    ///
    /// Defaults to `Int64` for backward compatibility with schemas that use `pk_{type}`
    /// BIGINT columns. Set to `Uuid` when the cursor column has a UUID type.
    ///
    /// Only meaningful when `relay = true`.
    #[serde(default, skip_serializing_if = "is_default_cursor_type")]
    pub relay_cursor_type: CursorType,

    /// Server-side parameters injected from JWT claims at runtime.
    ///
    /// Keys are SQL column names. Values describe where to source the runtime value.
    /// These params are NOT exposed as GraphQL arguments.
    ///
    /// For queries: adds a `WHERE key = $value` condition per entry using the same
    /// `WhereClause` mechanism as `TenantEnforcer`. Works on all adapters.
    ///
    /// Clients cannot override these values.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub inject_params: IndexMap<String, InjectedParamSource>,

    /// Per-query result cache TTL in seconds.
    ///
    /// Overrides the global `CacheConfig::ttl_seconds` for this query's view.
    /// Common use-cases:
    /// - Reference data (countries, currencies): `3600` (1 h)
    /// - Live / real-time data: `0` (bypass cache entirely)
    ///
    /// `None` → use the global cache TTL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_ttl_seconds: Option<u64>,

    /// Additional database views this query reads beyond the primary `sql_source`.
    ///
    /// When this query JOINs or queries multiple views, list all secondary views here
    /// so that mutations touching those views correctly invalidate this query's cache
    /// entries.
    ///
    /// Without this list, only `sql_source` is registered for invalidation. Any mutation
    /// that modifies a secondary view will NOT invalidate this query's cache — silently
    /// serving stale data.
    ///
    /// Each entry must be a valid SQL identifier (letters, digits, `_`) validated by the
    /// CLI compiler at schema compile time.
    ///
    /// # Example
    ///
    /// ```python
    /// @fraiseql.query(
    ///     sql_source="v_user_with_posts",
    ///     additional_views=["v_post"],
    /// )
    /// def users_with_posts() -> list[UserWithPosts]: ...
    /// ```
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_views: Vec<String>,

    /// Role required to execute this query and see it in introspection.
    ///
    /// When set, only users with this role can discover and execute this query.
    /// Users without the role receive `"Unknown query"` (not `FORBIDDEN`)
    /// to prevent role enumeration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_role: Option<String>,
}

impl QueryDefinition {
    /// Create a new query definition.
    #[must_use]
    pub fn new(name: impl Into<String>, return_type: impl Into<String>) -> Self {
        Self {
            name:                name.into(),
            return_type:         return_type.into(),
            returns_list:        false,
            nullable:            false,
            arguments:           Vec::new(),
            sql_source:          None,
            description:         None,
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::Int64,
            inject_params:       IndexMap::new(),
            cache_ttl_seconds:   None,
            additional_views:    Vec::new(),
            requires_role:       None,
        }
    }

    /// Set this query to return a list.
    #[must_use]
    pub fn returning_list(mut self) -> Self {
        self.returns_list = true;
        self
    }

    /// Set the SQL source.
    #[must_use]
    pub fn with_sql_source(mut self, source: impl Into<String>) -> Self {
        self.sql_source = Some(source.into());
        self
    }

    /// Mark this query as deprecated.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::QueryDefinition;
    ///
    /// let query = QueryDefinition::new("oldUsers", "User")
    ///     .deprecated(Some("Use 'users' instead".to_string()));
    /// assert!(query.is_deprecated());
    /// ```
    #[must_use]
    pub fn deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecation = Some(super::field_type::DeprecationInfo { reason });
        self
    }

    /// Check if this query is deprecated.
    #[must_use]
    pub fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Get the deprecation reason if deprecated.
    #[must_use]
    pub fn deprecation_reason(&self) -> Option<&str> {
        self.deprecation.as_ref().and_then(|d| d.reason.as_deref())
    }
}

/// A mutation definition compiled from `@fraiseql.mutation`.
///
/// Mutations are declarative bindings to database functions.
/// They describe *which function* to call, not arbitrary logic.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{MutationDefinition, MutationOperation};
///
/// let mutation = MutationDefinition::new("createUser", "User");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationDefinition {
    /// Mutation name (e.g., "createUser").
    pub name: String,

    /// Return type name.
    pub return_type: String,

    /// Input arguments.
    #[serde(default)]
    pub arguments: Vec<ArgumentDefinition>,

    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SQL operation type.
    #[serde(default)]
    pub operation: MutationOperation,

    /// Deprecation information (from @deprecated directive).
    /// When set, this mutation is marked as deprecated in the schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<super::field_type::DeprecationInfo>,

    /// PostgreSQL function name to call for this mutation.
    ///
    /// When set, the runtime calls `SELECT * FROM {sql_source}($1, $2, ...)` with the
    /// mutation arguments in `ArgumentDefinition` order, and parses the result as an
    /// `app.mutation_response` composite row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,

    /// Server-side parameters injected from JWT claims at runtime.
    ///
    /// Keys are SQL parameter names. Values describe where to source the runtime value.
    /// These params are NOT exposed as GraphQL arguments.
    ///
    /// For mutations: injected params are appended to the positional function call args
    /// **after** client-provided arguments, in map insertion order. The SQL function
    /// signature must declare the injected parameters last.
    ///
    /// Works on PostgreSQL, SQL Server, and MySQL. SQLite has no stored-routine mechanism
    /// and will return an error if inject is configured on a mutation.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub inject_params: IndexMap<String, InjectedParamSource>,

    /// Fact tables whose version counter should be bumped after this mutation succeeds.
    ///
    /// When the mutation PostgreSQL function returns successfully, the runtime calls
    /// `SELECT bump_tf_version($1)` for each listed table, incrementing the version used
    /// in fact-table cache keys. This ensures that analytic/aggregate queries backed by
    /// `FactTableVersionStrategy::VersionTable` are automatically invalidated.
    ///
    /// Each entry must be a valid SQL identifier validated at compile time.
    ///
    /// # Example
    ///
    /// ```python
    /// @fraiseql.mutation(
    ///     sql_source="fn_create_order",
    ///     invalidates_fact_tables=["tf_sales", "tf_order_count"],
    /// )
    /// def create_order(amount: Decimal) -> Order: ...
    /// ```
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidates_fact_tables: Vec<String>,

    /// View names whose cached query results should be invalidated after this
    /// mutation succeeds.
    ///
    /// When the `CachedDatabaseAdapter` is active, the runtime calls
    /// `invalidate_views()` with these names, clearing all cache entries that
    /// read from the specified views.
    ///
    /// If empty and the mutation return type has a `sql_source`, the runtime
    /// infers the primary view from the return type.
    ///
    /// Each entry must be a valid SQL identifier validated at compile time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidates_views: Vec<String>,
}

impl MutationDefinition {
    /// Create a new mutation definition.
    #[must_use]
    pub fn new(name: impl Into<String>, return_type: impl Into<String>) -> Self {
        Self {
            name:                   name.into(),
            return_type:            return_type.into(),
            arguments:              Vec::new(),
            description:            None,
            operation:              MutationOperation::default(),
            deprecation:            None,
            sql_source:             None,
            inject_params:          IndexMap::new(),
            invalidates_fact_tables: Vec::new(),
            invalidates_views:      Vec::new(),
        }
    }

    /// Mark this mutation as deprecated.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::MutationDefinition;
    ///
    /// let mutation = MutationDefinition::new("oldCreateUser", "User")
    ///     .deprecated(Some("Use 'createUser' instead".to_string()));
    /// assert!(mutation.is_deprecated());
    /// ```
    #[must_use]
    pub fn deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecation = Some(super::field_type::DeprecationInfo { reason });
        self
    }

    /// Check if this mutation is deprecated.
    #[must_use]
    pub fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Get the deprecation reason if deprecated.
    #[must_use]
    pub fn deprecation_reason(&self) -> Option<&str> {
        self.deprecation.as_ref().and_then(|d| d.reason.as_deref())
    }
}

/// Mutation operation types.
///
/// This enum describes what kind of database operation a mutation performs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum MutationOperation {
    /// INSERT into a table.
    Insert {
        /// Target table name.
        table: String,
    },

    /// UPDATE a table.
    Update {
        /// Target table name.
        table: String,
    },

    /// DELETE from a table.
    Delete {
        /// Target table name.
        table: String,
    },

    /// Custom mutation (for complex operations).
    #[default]
    Custom,
}

/// Query/mutation/subscription argument definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArgumentDefinition {
    /// Argument name.
    pub name: String,

    /// Argument type.
    pub arg_type: FieldType,

    /// Is this argument optional?
    #[serde(default)]
    pub nullable: bool,

    /// Default value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<super::graphql_value::GraphQLValue>,

    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Deprecation information (from @deprecated directive).
    /// When set, this argument is marked as deprecated in the schema.
    /// Per GraphQL spec, deprecated arguments should still be accepted but
    /// clients are encouraged to migrate to alternatives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<super::field_type::DeprecationInfo>,
}

impl ArgumentDefinition {
    /// Create a new required argument.
    #[must_use]
    pub fn new(name: impl Into<String>, arg_type: FieldType) -> Self {
        Self {
            name: name.into(),
            arg_type,
            nullable: false,
            default_value: None,
            description: None,
            deprecation: None,
        }
    }

    /// Create a new optional argument.
    #[must_use]
    pub fn optional(name: impl Into<String>, arg_type: FieldType) -> Self {
        Self {
            name: name.into(),
            arg_type,
            nullable: true,
            default_value: None,
            description: None,
            deprecation: None,
        }
    }

    /// Mark this argument as deprecated.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::{ArgumentDefinition, FieldType};
    ///
    /// let arg = ArgumentDefinition::optional("oldLimit", FieldType::Int)
    ///     .deprecated(Some("Use 'first' instead".to_string()));
    /// assert!(arg.is_deprecated());
    /// ```
    #[must_use]
    pub fn deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecation = Some(super::field_type::DeprecationInfo { reason });
        self
    }

    /// Check if this argument is deprecated.
    #[must_use]
    pub fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Get the deprecation reason if deprecated.
    #[must_use]
    pub fn deprecation_reason(&self) -> Option<&str> {
        self.deprecation.as_ref().and_then(|d| d.reason.as_deref())
    }
}

/// Auto-wired query parameters.
///
/// These are standard parameters automatically added to list queries.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // Reason: these are intentional feature flags
pub struct AutoParams {
    /// Enable `where` filtering.
    #[serde(default)]
    pub has_where: bool,

    /// Enable `orderBy` sorting.
    #[serde(default)]
    pub has_order_by: bool,

    /// Enable `limit` pagination.
    #[serde(default)]
    pub has_limit: bool,

    /// Enable `offset` pagination.
    #[serde(default)]
    pub has_offset: bool,
}

impl AutoParams {
    /// Create with all auto-params enabled (common for list queries).
    #[must_use]
    pub fn all() -> Self {
        Self {
            has_where:    true,
            has_order_by: true,
            has_limit:    true,
            has_offset:   true,
        }
    }

    /// Create with no auto-params (common for single-item queries).
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }
}

// =============================================================================
// Custom Directive Definitions
// =============================================================================

/// A custom directive definition for schema extension.
///
/// Allows defining custom directives beyond the built-in `@skip`, `@include`,
/// and `@deprecated` directives. Custom directives are exposed via introspection
/// and can be evaluated at runtime via registered handlers.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{DirectiveDefinition, DirectiveLocationKind, ArgumentDefinition, FieldType};
///
/// let rate_limit = DirectiveDefinition {
///     name: "rateLimit".to_string(),
///     description: Some("Apply rate limiting to this field".to_string()),
///     locations: vec![DirectiveLocationKind::FieldDefinition],
///     arguments: vec![
///         ArgumentDefinition::new("limit", FieldType::Int),
///         ArgumentDefinition::optional("window", FieldType::String),
///     ],
///     is_repeatable: false,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectiveDefinition {
    /// Directive name (e.g., "rateLimit", "auth").
    pub name: String,

    /// Description of what this directive does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Valid locations where this directive can be applied.
    pub locations: Vec<DirectiveLocationKind>,

    /// Arguments this directive accepts.
    #[serde(default)]
    pub arguments: Vec<ArgumentDefinition>,

    /// Whether this directive can be applied multiple times to the same location.
    #[serde(default)]
    pub is_repeatable: bool,
}

impl DirectiveDefinition {
    /// Create a new directive definition.
    #[must_use]
    pub fn new(name: impl Into<String>, locations: Vec<DirectiveLocationKind>) -> Self {
        Self {
            name: name.into(),
            description: None,
            locations,
            arguments: Vec::new(),
            is_repeatable: false,
        }
    }

    /// Set the description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add an argument to this directive.
    #[must_use]
    pub fn with_argument(mut self, arg: ArgumentDefinition) -> Self {
        self.arguments.push(arg);
        self
    }

    /// Add multiple arguments to this directive.
    #[must_use]
    pub fn with_arguments(mut self, args: Vec<ArgumentDefinition>) -> Self {
        self.arguments = args;
        self
    }

    /// Mark this directive as repeatable.
    #[must_use]
    pub fn repeatable(mut self) -> Self {
        self.is_repeatable = true;
        self
    }

    /// Check if this directive can be applied at the given location.
    #[must_use]
    pub fn valid_at(&self, location: DirectiveLocationKind) -> bool {
        self.locations.contains(&location)
    }

    /// Find an argument by name.
    #[must_use]
    pub fn find_argument(&self, name: &str) -> Option<&ArgumentDefinition> {
        self.arguments.iter().find(|a| a.name == name)
    }
}

/// Directive location kinds for custom directive definitions.
///
/// This mirrors `DirectiveLocation` in introspection but is used for
/// compiled schema definitions. The two types can be converted between
/// each other for introspection purposes.
///
/// Per GraphQL spec §3.13, directive locations fall into two categories:
/// - Executable locations (operations, fields, fragments)
/// - Type system locations (schema definitions)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DirectiveLocationKind {
    // Executable directive locations
    /// Directive on query operation.
    Query,
    /// Directive on mutation operation.
    Mutation,
    /// Directive on subscription operation.
    Subscription,
    /// Directive on field selection.
    Field,
    /// Directive on fragment definition.
    FragmentDefinition,
    /// Directive on fragment spread.
    FragmentSpread,
    /// Directive on inline fragment.
    InlineFragment,
    /// Directive on variable definition.
    VariableDefinition,

    // Type system directive locations
    /// Directive on schema definition.
    Schema,
    /// Directive on scalar type definition.
    Scalar,
    /// Directive on object type definition.
    Object,
    /// Directive on field definition.
    FieldDefinition,
    /// Directive on argument definition.
    ArgumentDefinition,
    /// Directive on interface definition.
    Interface,
    /// Directive on union definition.
    Union,
    /// Directive on enum definition.
    Enum,
    /// Directive on enum value definition.
    EnumValue,
    /// Directive on input object definition.
    InputObject,
    /// Directive on input field definition.
    InputFieldDefinition,
}

impl DirectiveLocationKind {
    /// Check if this is an executable directive location.
    #[must_use]
    pub fn is_executable(&self) -> bool {
        matches!(
            self,
            Self::Query
                | Self::Mutation
                | Self::Subscription
                | Self::Field
                | Self::FragmentDefinition
                | Self::FragmentSpread
                | Self::InlineFragment
                | Self::VariableDefinition
        )
    }

    /// Check if this is a type system directive location.
    #[must_use]
    pub fn is_type_system(&self) -> bool {
        !self.is_executable()
    }
}

// =============================================================================
// SQL safety validation
// =============================================================================

/// Validates that a string is a safe SQL identifier.
///
/// Accepts only ASCII alphanumerics and underscores (no spaces, semicolons,
/// hyphens, or other characters). Used to guard against SQL injection when
/// schema-derived names such as view names or entity type names are
/// interpolated into raw SQL strings.
///
/// # Rules
/// - Non-empty
/// - Maximum 128 characters
/// - All characters are `[A-Za-z0-9_]`
///
/// # Examples
///
/// ```
/// use fraiseql_core::schema::is_safe_sql_identifier;
///
/// assert!(is_safe_sql_identifier("v_users"));
/// assert!(is_safe_sql_identifier("Order123"));
/// assert!(!is_safe_sql_identifier("users; DROP TABLE users"));
/// assert!(!is_safe_sql_identifier(""));
/// ```
pub fn is_safe_sql_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::observer_types::RetryConfig;

    #[test]
    fn test_compiled_schema_with_observers() {
        let json = r#"{
            "types": [],
            "enums": [],
            "input_types": [],
            "interfaces": [],
            "unions": [],
            "queries": [],
            "mutations": [],
            "subscriptions": [],
            "observers": [
                {
                    "name": "onHighValueOrder",
                    "entity": "Order",
                    "event": "INSERT",
                    "condition": "total > 1000",
                    "actions": [
                        {
                            "type": "webhook",
                            "url": "https://api.example.com/webhook"
                        }
                    ],
                    "retry": {
                        "max_attempts": 3,
                        "backoff_strategy": "exponential",
                        "initial_delay_ms": 1000,
                        "max_delay_ms": 60000
                    }
                }
            ]
        }"#;

        let schema = CompiledSchema::from_json(json).unwrap();

        assert!(schema.has_observers());
        assert_eq!(schema.observer_count(), 1);

        let observer = schema.find_observer("onHighValueOrder").unwrap();
        assert_eq!(observer.entity, "Order");
        assert_eq!(observer.event, "INSERT");
        assert_eq!(observer.condition, Some("total > 1000".to_string()));
        assert_eq!(observer.actions.len(), 1);
        assert_eq!(observer.retry.max_attempts, 3);
        assert!(observer.retry.is_exponential());
    }

    #[test]
    fn test_compiled_schema_backward_compatible() {
        // Schema without observers field should still load
        let json = r#"{
            "types": [],
            "enums": [],
            "input_types": [],
            "interfaces": [],
            "unions": [],
            "queries": [],
            "mutations": [],
            "subscriptions": []
        }"#;

        let schema = CompiledSchema::from_json(json).unwrap();
        assert!(!schema.has_observers());
        assert_eq!(schema.observer_count(), 0);
    }

    #[test]
    fn test_find_observers_for_entity() {
        let schema = CompiledSchema {
            observers: vec![
                ObserverDefinition::new("onOrderInsert", "Order", "INSERT"),
                ObserverDefinition::new("onOrderUpdate", "Order", "UPDATE"),
                ObserverDefinition::new("onUserInsert", "User", "INSERT"),
            ],
            ..Default::default()
        };

        let order_observers = schema.find_observers_for_entity("Order");
        assert_eq!(order_observers.len(), 2);

        let user_observers = schema.find_observers_for_entity("User");
        assert_eq!(user_observers.len(), 1);
    }

    #[test]
    fn test_find_observers_for_event() {
        let schema = CompiledSchema {
            observers: vec![
                ObserverDefinition::new("onOrderInsert", "Order", "INSERT"),
                ObserverDefinition::new("onOrderUpdate", "Order", "UPDATE"),
                ObserverDefinition::new("onUserInsert", "User", "INSERT"),
            ],
            ..Default::default()
        };

        let insert_observers = schema.find_observers_for_event("INSERT");
        assert_eq!(insert_observers.len(), 2);

        let update_observers = schema.find_observers_for_event("UPDATE");
        assert_eq!(update_observers.len(), 1);
    }

    #[test]
    fn test_observer_definition_builder() {
        let observer = ObserverDefinition::new("test", "Order", "INSERT")
            .with_condition("total > 1000")
            .with_action(serde_json::json!({"type": "webhook", "url": "https://example.com"}))
            .with_retry(RetryConfig::exponential(5, 1000, 60000));

        assert_eq!(observer.name, "test");
        assert_eq!(observer.entity, "Order");
        assert_eq!(observer.event, "INSERT");
        assert!(observer.has_condition());
        assert_eq!(observer.action_count(), 1);
        assert_eq!(observer.retry.max_attempts, 5);
    }

    #[test]
    fn test_retry_config_types() {
        let exponential = RetryConfig::exponential(3, 1000, 60000);
        assert!(exponential.is_exponential());
        assert!(!exponential.is_linear());
        assert!(!exponential.is_fixed());

        let linear = RetryConfig::linear(3, 1000, 60000);
        assert!(!linear.is_exponential());
        assert!(linear.is_linear());
        assert!(!linear.is_fixed());

        let fixed = RetryConfig::fixed(3, 5000);
        assert!(!fixed.is_exponential());
        assert!(!fixed.is_linear());
        assert!(fixed.is_fixed());
        assert_eq!(fixed.initial_delay_ms, 5000);
        assert_eq!(fixed.max_delay_ms, 5000);
    }

    // =========================================================================
    // content_hash tests
    // =========================================================================

    #[test]
    fn test_content_hash_stable() {
        let schema = CompiledSchema::default();
        assert_eq!(schema.content_hash(), schema.content_hash(), "Same schema must produce same hash");
    }

    #[test]
    fn test_content_hash_length() {
        let hash = CompiledSchema::default().content_hash();
        assert_eq!(hash.len(), 32, "Hash must be 32 hex chars (16 bytes)");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "Hash must be valid hex");
    }

    #[test]
    fn test_content_hash_changes_on_field_rename() {
        let mut schema_a = CompiledSchema::default();
        schema_a.queries.push(QueryDefinition::new("users", "User").with_sql_source("v_user"));

        let mut schema_b = CompiledSchema::default();
        schema_b.queries.push(QueryDefinition::new("users", "User").with_sql_source("v_account")); // different view

        assert_ne!(
            schema_a.content_hash(),
            schema_b.content_hash(),
            "Schemas with different view names must produce different hashes"
        );
    }

    // =========================================================================
    // has_rls_configured tests
    // =========================================================================

    #[test]
    fn test_has_rls_configured_no_security() {
        let schema = CompiledSchema::default();
        assert!(!schema.has_rls_configured(), "Schema with no security section must return false");
    }

    #[test]
    fn test_has_rls_configured_with_empty_policies() {
        let mut sec = SecurityConfig::default();
        sec.additional.insert("policies".to_string(), serde_json::json!([]));
        let schema = CompiledSchema { security: Some(sec), ..CompiledSchema::default() };
        assert!(!schema.has_rls_configured(), "Empty policies array must return false");
    }

    #[test]
    fn test_has_rls_configured_with_policies() {
        let mut sec = SecurityConfig::default();
        sec.additional.insert(
            "policies".to_string(),
            serde_json::json!([{"name": "tenant_isolation", "condition": "tenant_id = $1"}]),
        );
        let schema = CompiledSchema { security: Some(sec), ..CompiledSchema::default() };
        assert!(schema.has_rls_configured(), "Non-empty policies array must return true");
    }

    #[test]
    fn test_has_rls_configured_no_policies_key() {
        let mut sec = SecurityConfig::default();
        sec.additional.insert("rate_limiting".to_string(), serde_json::json!({"enabled": true}));
        let schema = CompiledSchema { security: Some(sec), ..CompiledSchema::default() };
        assert!(!schema.has_rls_configured(), "Security without policies key must return false");
    }
}
