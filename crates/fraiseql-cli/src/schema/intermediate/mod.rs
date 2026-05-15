//! Intermediate Schema Format
//!
//! Language-agnostic schema representation that all language libraries output.
//! See `docs/architecture/intermediate-schema.md` for full specification.

pub mod advanced_types;
pub mod analytics;
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
pub use fragments::{
    IntermediateAppliedDirective, IntermediateDirective, IntermediateFragment,
    IntermediateFragmentField, IntermediateFragmentFieldDef,
};
use fraiseql_core::schema::{
    DebugConfig, McpConfig, NamingConvention, RestConfig, SessionVariablesConfig,
    SubscriptionsConfig, ValidationConfig,
};
pub use operations::{
    IntermediateArgument, IntermediateAutoParams, IntermediateMutation, IntermediateQuery,
    IntermediateQueryDefaults,
};
use serde::{Deserialize, Serialize};
pub use subscriptions::{
    IntermediateFilterCondition, IntermediateObserver, IntermediateObserverAction,
    IntermediateRetryConfig, IntermediateSubscription, IntermediateSubscriptionFilter,
};
pub use types::{
    IntermediateDeprecation, IntermediateEnum, IntermediateEnumValue, IntermediateField,
    IntermediateScalar, IntermediateType,
};

/// Intermediate schema - universal format from all language libraries
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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

    /// Custom scalar type definitions
    ///
    /// Defines custom GraphQL scalar types with validation rules.
    /// Custom scalars can be defined in Python, TypeScript, Java, Go, and Rust SDKs,
    /// and are compiled into the CompiledSchema's CustomTypeRegistry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_scalars: Option<Vec<IntermediateScalar>>,

    /// Security configuration (from fraiseql.toml)
    /// Compiled from the security section of fraiseql.toml at compile time.
    /// Optional - if not provided, defaults are used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<serde_json::Value>,

    /// Observers/event system configuration (from fraiseql.toml).
    ///
    /// Contains backend connection settings (redis_url, nats_url, etc.) compiled
    /// from the `[observers]` TOML section. Embedded verbatim into the compiled schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observers_config: Option<serde_json::Value>,

    /// Federation configuration (from fraiseql.toml).
    ///
    /// Contains Apollo Federation settings and circuit breaker configuration compiled
    /// from the `[federation]` TOML section. Embedded verbatim into the compiled schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

    /// Global auto-param defaults for list queries (injected from TOML by the merger).
    ///
    /// Never present in `schema.json` — populated at compile time from `[query_defaults]`
    /// in `fraiseql.toml`. Used by the converter to resolve per-query `auto_params`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_defaults: Option<IntermediateQueryDefaults>,

    /// Naming convention for GraphQL operation names.
    ///
    /// Compiled from `fraiseql.toml` top-level `naming_convention` setting.
    #[serde(default)]
    pub naming_convention: NamingConvention,

    /// Session variable injection configuration.
    ///
    /// When populated, the executor calls `set_config()` before each query and
    /// mutation to inject per-request values (JWT claims, HTTP headers, or literals)
    /// as PostgreSQL transaction-scoped settings.
    ///
    /// Embedded verbatim from the `session_variables` key in `schema.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_variables: Option<SessionVariablesConfig>,

    /// Hierarchy definitions for ID-based ltree operators.
    ///
    /// Compiled from the `[hierarchies]` TOML section. Maps hierarchy names
    /// to table/path_column pairs for subquery generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchies_config: Option<serde_json::Value>,
}

fn default_version() -> String {
    "2.0.0".to_string()
}


#[cfg(test)]
mod tests;
