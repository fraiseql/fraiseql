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

use serde::{Deserialize, Serialize};

use super::{directive::DirectiveDefinition, mutation::MutationDefinition, query::QueryDefinition};
use crate::{
    compiler::fact_table::FactTableMetadata,
    schema::{
        config_types::{
            ChangelogConfig, DebugConfig, FederationConfig, GrpcConfig, McpConfig,
            NamingConvention, ObserversConfig, RestConfig, SessionVariablesConfig,
            SubscriptionsConfig, ValidationConfig,
        },
        graphql_type_defs::{
            EnumDefinition, InputObjectDefinition, InterfaceDefinition, TypeDefinition,
            UnionDefinition,
        },
        hierarchy::HierarchiesConfig,
        observer_types::ObserverDefinition,
        security_config::SecurityConfig,
        source_types::SourceDefinition,
        subscription_types::SubscriptionDefinition,
    },
    validation::CustomTypeRegistry,
};

/// Current schema format version.
///
/// Increment this constant when the compiled schema JSON format changes in a
/// backward-incompatible way so that startup rejects stale compiled schemas.
pub const CURRENT_SCHEMA_FORMAT_VERSION: u32 = 1;

/// A `@subscribable` declaration in the compiled schema (#366).
///
/// Maps a GraphQL type to the physical base table(s) whose **external** writes
/// (a raw `INSERT`/`UPDATE`/`DELETE` from psql / a migration / a third-party
/// tool) should be captured onto the Change Spine by the shipped fallback trigger
/// `core.fn_entity_change_log_capture`. The compiler aggregates one of these per
/// type carrying `@subscribable(tables=[...])`; the
/// [`generate_capture_trigger_ddl`](crate::schema::generate_capture_trigger_ddl)
/// generator turns them into per-table statement-level triggers that stamp
/// `object_type = entity_type` — the GraphQL type name the reader and the
/// subscription matcher key on, never the table name — so a captured external
/// write fans out through the existing poller with no table→type lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribableEntity {
    /// The GraphQL type name (e.g. `"Post"`) stamped as `object_type` on every
    /// captured change-log row.
    pub entity_type: String,

    /// The physical base table(s) backing `entity_type` (e.g. `["tb_post"]`,
    /// optionally schema-qualified `["public.tb_post"]`). A capture trigger is
    /// installed on each.
    pub tables: Vec<String>,

    /// Whether the capture triggers on this entity's tables also record the
    /// changed entity's **pre-image** (OLD) into `object_data_before` — the
    /// out-of-band parity for the per-mutation
    /// [`changelog_pre_image`](super::MutationDefinition::changelog_pre_image).
    ///
    /// The trigger always unifies `object_data` on the after-image (NEW)
    /// regardless of this flag; `pre_image` only adds the separate before-image
    /// column for opted-in tables, so audit-sensitive entities get an inline
    /// Debezium `{before, after}` even for raw external writes. Default `false`
    /// (opt in via `@subscribable(tables=[...], pre_image=True)`); an absent value
    /// is byte-identical to before this field existed, so it does not churn the
    /// codegen schema hash.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub pre_image: bool,
}

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
/// let schema = CompiledSchema::from_json(json, false).unwrap();
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

    /// Scheduled ingress source definitions (#573) — the dual of `observers`.
    ///
    /// Each runs its `function` on a cron schedule, pulling from an external system
    /// into the database via mutations with a durable cursor. Empty (and omitted
    /// from the compiled JSON) when no source is declared, so a schema that predates
    /// this field deserializes and re-serializes byte-for-byte unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceDefinition>,

    /// `@subscribable` declarations (#366): GraphQL types whose underlying
    /// table(s) get the shipped external-write capture trigger.
    ///
    /// Aggregated by the compiler from each type's `@subscribable(tables=[...])`
    /// annotation; consumed by
    /// [`generate_capture_trigger_ddl`](crate::schema::generate_capture_trigger_ddl)
    /// to emit per-table capture triggers. Empty (and omitted from the compiled
    /// JSON) when no type is subscribable — so a schema that predates this field
    /// deserializes and re-serializes byte-for-byte unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscribable: Vec<SubscribableEntity>,

    /// Per-operation `@cost(weight: N)` overrides (#379): root query/mutation name
    /// → manual cost weight, consulted by the runtime per-tenant cost-budget check
    /// (`estimate_query_cost`) so a top-level operation counts as exactly `N`
    /// instead of its walked subtree complexity. Aggregated by the compiler from
    /// each operation's `@cost` annotation. Empty (and omitted from the compiled
    /// JSON) when no operation carries `@cost` — so a schema that predates this
    /// field deserializes and re-serializes byte-for-byte unchanged.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub operation_cost_weights: HashMap<String, usize>,

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

    /// Changelog GraphQL-exposure configuration.
    ///
    /// Compiled from the `[changelog]` TOML section. When present with
    /// `expose = true`, the compiler injects the `EntityChangeLog` /
    /// `TransportCheckpoint` types plus their cursor query, point-lookup query, and
    /// checkpoint upsert mutation. `None` when the block is absent (the default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changelog: Option<ChangelogConfig>,

    /// Session variable injection configuration.
    ///
    /// When populated, the executor calls PostgreSQL `set_config()` before each
    /// mutation, injecting per-request values (JWT claims, HTTP headers, literals)
    /// as transaction-scoped settings.  SQL functions read these via
    /// `current_setting('app.tenant_id', true)`.
    ///
    /// Compiled from the `[session_variables]` TOML section.
    #[serde(default)]
    pub session_variables: SessionVariablesConfig,

    /// Hierarchy definitions for ID-based ltree operators.
    ///
    /// Maps hierarchy names to `table`/`path_column` pairs. Compiled from the
    /// `[hierarchies]` TOML section. Used at runtime to resolve `HierarchyContext`
    /// for `descendantOfId` / `ancestorOfId` WHERE clause generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchies_config: Option<HierarchiesConfig>,

    /// Naming convention for GraphQL operation names.
    ///
    /// When set to `CamelCase`, operation names are converted from `snake_case`
    /// (e.g., `create_dns_server` → `createDnsServer`) in the introspection
    /// schema and lookup indexes. Compiled from `[fraiseql]` in `fraiseql.toml`.
    #[serde(default)]
    pub naming_convention: NamingConvention,

    /// Acronyms whose internal digit stays attached when resolving a GraphQL field
    /// name back to its `snake_case` JSONB key (e.g. `s3`, `ipv4`, `oauth2`). Added
    /// to the built-in defaults at boot via `fraiseql_db::utils::set_runtime_acronyms`.
    /// Skipped when empty so a schema with no project acronyms serializes byte-for-byte
    /// as before this field existed (no schema-hash churn; back-compat on load).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub naming_acronyms: Vec<String>,

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
            && self.sources == other.sources
            && self.subscribable == other.subscribable
            && self.federation == other.federation
            && self.security == other.security
            && self.observers_config == other.observers_config
            && self.subscriptions_config == other.subscriptions_config
            && self.validation_config == other.validation_config
            && self.debug_config == other.debug_config
            && self.mcp_config == other.mcp_config
            && self.changelog == other.changelog
            && self.naming_convention == other.naming_convention
            && self.naming_acronyms == other.naming_acronyms
            && self.schema_sdl == other.schema_sdl
    }
}

impl CompiledSchema {
    /// Create empty schema.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}
