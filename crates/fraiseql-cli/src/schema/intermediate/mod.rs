//! Intermediate Schema Format
//!
//! Language-agnostic schema representation that all language libraries output.
//! See `docs/architecture/intermediate-schema.md` for full specification.

pub mod advanced_types;
pub mod analytics;
pub mod drift_guard;
pub mod fragments;
pub mod operations;
pub mod subscriptions;
pub mod types;

pub use advanced_types::{
    IntermediateInputField, IntermediateInputObject, IntermediateInterface, IntermediateUnion,
};
pub use analytics::{
    IntermediateAggregateQuery, IntermediateDimensionPath, IntermediateDimensions,
    IntermediateFactTable, IntermediateFilter, IntermediateMeasure,
};
pub use drift_guard::reject_drifted_keys;
pub use fragments::{
    IntermediateAppliedDirective, IntermediateDirective, IntermediateFragment,
    IntermediateFragmentField, IntermediateFragmentFieldDef,
};
use fraiseql_core::schema::{
    DebugConfig, McpConfig, NamingConvention, RestConfig, SessionVariablesConfig,
    SubscriptionsConfig, ValidationConfig,
};
pub use operations::{
    IntermediateArgument, IntermediateAutoParams, IntermediateInjectDefaults, IntermediateMutation,
    IntermediateQuery, IntermediateQueryDefaults, IntermediateRest,
};
use serde::{Deserialize, Serialize};
pub use subscriptions::{
    IntermediateFilterCondition, IntermediateObserver, IntermediateObserverAction,
    IntermediateRetryConfig, IntermediateSubscription, IntermediateSubscriptionFilter,
};
pub use types::{
    IntermediateDeprecation, IntermediateEnum, IntermediateEnumValue, IntermediateField,
    IntermediateRelationship, IntermediateScalar, IntermediateType,
};

/// Intermediate schema - universal format from all language libraries
///
/// # `deny_unknown_fields` is the load-bearing attribute on this struct
///
/// Every field below carries `#[serde(default)]`, because an SDK legitimately omits most of
/// them. Without `deny_unknown_fields`, that combination means **any** key the compiler does
/// not read binds to an empty default and the compile reports success. That is not a
/// hypothetical: it is the mechanism behind nine separate shipped defects.
///
/// * `#847` — seven SDKs emitted `inject_defaults`; the compiler had never heard of the key. An
///   operator configuring a project-wide tenant predicate got `✓ Schema compiled successfully` and
///   not one operation with a tenant filter.
/// * `#848` — four SDKs emitted `is_input`; the type compiled as an *output* type and the served
///   schema violated GraphQL §3.10.
/// * `#756` — the merger wrote `args`/`required`, this struct read `arguments`/`nullable`. Every
///   TOML-declared operation argument disappeared.
/// * `#806`/`#807` — `inject` vs `inject_params`, and five spellings of `requires_scope`. Queries
///   compiled with no tenant predicate; gated fields compiled as public.
///
/// In each case the *author was explicit* and the compiler was silent. Denying unknown
/// fields converts every one of those into an error naming the offending key, and — the
/// point of putting it here rather than patching each instance — it does the same for the
/// tenth key, which has not been invented yet.
///
/// Consequence to accept deliberately: a key an SDK emits that this struct does not declare
/// now **fails the compile**. That is the intended breaking change. A producer emitting a
/// key no consumer reads was never doing anything; now it says so.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateSchema {
    /// Schema format version
    #[serde(default = "default_version")]
    pub version: String,

    /// GraphQL object types
    #[serde(default)]
    pub types: Vec<IntermediateType>,

    /// GraphQL enum types
    #[serde(default)]
    pub enums: Vec<IntermediateEnum>,

    /// GraphQL input object types
    #[serde(default)]
    pub input_types: Vec<IntermediateInputObject>,

    /// GraphQL interface types (per GraphQL spec §3.7)
    #[serde(default)]
    pub interfaces: Vec<IntermediateInterface>,

    /// GraphQL union types (per GraphQL spec §3.10)
    #[serde(default)]
    pub unions: Vec<IntermediateUnion>,

    /// GraphQL queries
    #[serde(default)]
    pub queries: Vec<IntermediateQuery>,

    /// GraphQL mutations
    #[serde(default)]
    pub mutations: Vec<IntermediateMutation>,

    /// GraphQL subscriptions
    #[serde(default)]
    pub subscriptions: Vec<IntermediateSubscription>,

    /// GraphQL fragments (reusable field selections)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragments: Option<Vec<IntermediateFragment>>,

    /// GraphQL directive definitions (custom directives)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directives: Option<Vec<IntermediateDirective>>,

    /// Analytics fact tables (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact_tables: Option<Vec<IntermediateFactTable>>,

    /// Analytics aggregate queries (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregate_queries: Option<Vec<IntermediateAggregateQuery>>,

    /// Observer definitions (database change event listeners)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observers: Option<Vec<IntermediateObserver>>,

    /// Scheduled ingress source definitions (#573) — the dual of `observers`.
    /// Reuses the compiled [`SourceDefinition`](fraiseql_core::schema::`SourceDefinition`)
    /// shape directly (it already carries serde defaults); the converter copies them
    /// into the compiled schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<fraiseql_core::schema::SourceDefinition>>,

    /// Custom scalar type definitions
    ///
    /// Defines custom GraphQL scalar types with validation rules.
    /// Custom scalars can be defined in Python, `TypeScript`, Java, Go, and Rust SDKs,
    /// and are compiled into the CompiledSchema's `CustomTypeRegistry`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_scalars: Option<Vec<IntermediateScalar>>,

    /// Security configuration (from fraiseql.toml)
    /// Compiled from the security section of fraiseql.toml at compile time.
    /// Optional - if not provided, defaults are used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<serde_json::Value>,

    /// PKCE OAuth-client configuration (the `[auth]` PKCE group, #621).
    ///
    /// The TOML merger emits the compiled `auth` object here (`discovery_url`,
    /// `client_id`, `client_secret_env`, `server_redirect_uri`); the converter
    /// carries it verbatim into `CompiledSchema.auth`. `None` when the group is
    /// absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<serde_json::Value>,

    /// Observers/event system configuration (from fraiseql.toml).
    ///
    /// Contains backend connection settings (redis_url, nats_url, etc.) compiled
    /// from the `[observers]` TOML section. Embedded verbatim into the compiled schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observers_config: Option<serde_json::Value>,

    /// Federation configuration.
    ///
    /// Two authoring workflows feed this field:
    /// - The TOML merger embeds the `[federation]` section under `federation_config`.
    /// - Language SDKs (e.g. the Python SDK's `export_schema(federation=...)`) emit the block
    ///   under the top-level `federation` key in `schema.json`; the `alias` binds it here so the
    ///   legacy JSON workflow does not silently drop it.
    ///
    /// Carried verbatim by the converter into `CompiledSchema.federation`.
    #[serde(default, alias = "federation", skip_serializing_if = "Option::is_none")]
    pub federation_config: Option<serde_json::Value>,

    /// WebSocket subscription configuration (hooks, limits).
    ///
    /// Compiled from the `[subscriptions]` TOML section. Embedded verbatim into
    /// the compiled schema for server-side consumption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscriptions_config: Option<SubscriptionsConfig>,

    /// Query validation config (depth/complexity limits).
    ///
    /// Compiled from `[validation]` in `fraiseql.toml`. Embedded into the compiled
    /// schema for server-side consumption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_config: Option<ValidationConfig>,

    /// Debug/development configuration.
    ///
    /// Compiled from `[debug]` in `fraiseql.toml`. Embedded into the compiled
    /// schema for server-side consumption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_config: Option<DebugConfig>,

    /// MCP (Model Context Protocol) server configuration.
    ///
    /// Compiled from `[mcp]` in `fraiseql.toml`. Embedded into the compiled
    /// schema for server-side consumption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_config: Option<McpConfig>,

    /// REST transport configuration.
    ///
    /// Compiled from `[rest]` in `fraiseql.toml`. Embedded into the compiled
    /// schema for server-side consumption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_config: Option<RestConfig>,

    /// gRPC transport configuration.
    ///
    /// Compiled from `[grpc]` in `fraiseql.toml`. Embedded verbatim into the compiled
    /// schema, whose `grpc_config` gates the transport (#780).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc_config: Option<fraiseql_core::schema::GrpcConfig>,

    /// Global auto-param defaults for list queries (injected from TOML by the merger).
    ///
    /// Never present in `schema.json` — populated at compile time from `[query_defaults]`
    /// in `fraiseql.toml`. Used by the converter to resolve per-query `auto_params`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_defaults: Option<IntermediateQueryDefaults>,

    /// Project-wide default injected parameters, from `[inject_defaults]`.
    ///
    /// Emitted as a top-level key by seven SDKs' `ConfigLoader`s and consumed by the
    /// converter. See [`IntermediateInjectDefaults`] for the precedence rules and for why
    /// the merge happens after `[fraiseql.tenancy]` validation rather than before it
    /// (#847).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inject_defaults: Option<IntermediateInjectDefaults>,

    /// Naming convention for GraphQL operation names.
    ///
    /// Compiled from `fraiseql.toml` top-level `naming_convention` setting.
    #[serde(default)]
    pub naming_convention: NamingConvention,

    /// Session variable injection configuration.
    ///
    /// When populated, the executor calls `set_config()` before each query and
    /// mutation to inject per-request values (JWT claims, HTTP headers, or literals)
    /// as `PostgreSQL` transaction-scoped settings.
    ///
    /// Embedded verbatim from the `session_variables` key in `schema.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_variables: Option<SessionVariablesConfig>,

    /// Hierarchy definitions for ID-based ltree operators.
    ///
    /// Compiled from the `[hierarchies]` TOML section. Maps hierarchy names
    /// to table/path_column pairs for subquery generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchies_config: Option<fraiseql_core::schema::HierarchiesConfig>,

    /// Changelog GraphQL-exposure configuration.
    ///
    /// Compiled from the `[changelog]` TOML section by the merger. Drives the
    /// converter's injection of the `EntityChangeLog` / `TransportCheckpoint`
    /// types and their operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changelog_config: Option<fraiseql_core::schema::ChangelogConfig>,
}

fn default_version() -> String {
    "2.0.0".to_string()
}

#[cfg(test)]
mod tests;
