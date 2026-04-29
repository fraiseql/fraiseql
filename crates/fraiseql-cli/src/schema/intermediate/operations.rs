//! Query/mutation structs: `IntermediateQuery`, `IntermediateMutation`,
//! `IntermediateArgument`, `IntermediateAutoParams`, `IntermediateQueryDefaults`.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::types::IntermediateDeprecation;

/// SQL source dispatch configuration in intermediate format.
///
/// Specifies that the query's SQL source should be resolved dynamically
/// based on an enum argument value, either via an explicit mapping or a
/// template string expanded at compile time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateSqlSourceDispatch {
    /// The argument name used for dispatch (e.g., "timeInterval").
    pub argument: String,

    /// Explicit enum-value-to-table mapping.
    /// Empty when `template` is used (compiler expands the template).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub mapping: IndexMap<String, String>,

    /// Template string with a `{placeholder}` for the enum value.
    /// E.g., `"tf_orders_{time_interval}"`.
    /// Mutually exclusive with a non-empty `mapping` (the compiler rejects both).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
}

/// Argument definition in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateArgument {
    /// Argument name
    pub name: String,

    /// Argument type name
    ///
    /// **Language-agnostic**: Uses "type", not "`arg_type`"
    #[serde(rename = "type")]
    pub arg_type: String,

    /// Is argument optional?
    pub nullable: bool,

    /// Default value (JSON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    /// Deprecation info (from @deprecated directive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,
}

/// Query definition in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct IntermediateQuery {
    /// Query name (e.g., "users")
    pub name: String,

    /// Return type name (e.g., "User")
    pub return_type: String,

    /// Returns a list?
    #[serde(default)]
    pub returns_list: bool,

    /// Result is nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Query arguments
    #[serde(default)]
    pub arguments: Vec<IntermediateArgument>,

    /// Query description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SQL source (table/view name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,

    /// Dynamic SQL source dispatch. Mutually exclusive with `sql_source`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_source_dispatch: Option<IntermediateSqlSourceDispatch>,

    /// Auto-generated parameters config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_params: Option<IntermediateAutoParams>,

    /// Deprecation info (from @deprecated directive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,

    /// JSONB column name for extracting data (e.g., "data")
    /// Used for tv_* (denormalized JSONB tables) pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsonb_column: Option<String>,

    /// Whether this is a Relay connection query.
    /// When true, the compiler wraps results in `{ edges: [{ node, cursor }], pageInfo }`
    /// and generates `first`/`after`/`last`/`before` arguments instead of `limit`/`offset`.
    /// Requires `returns_list = true` and `sql_source` to be set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub relay: bool,

    /// Server-injected parameters: SQL column name → source expression (e.g. `"jwt:org_id"`).
    /// Not exposed as GraphQL arguments.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub inject: IndexMap<String, String>,

    /// Per-query result cache TTL in seconds. Overrides the global cache TTL for this query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_ttl_seconds: Option<u64>,

    /// Additional database views this query reads beyond the primary `sql_source`.
    ///
    /// Used for correct cache invalidation when a query JOINs or reads multiple views.
    /// Each entry is validated as a safe SQL identifier at schema compile time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_views: Vec<String>,

    /// Role required to execute this query and see it in introspection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_role: Option<String>,

    /// Relay cursor column type: `"uuid"` for UUID PKs, `"int64"` (or absent) for bigint PKs.
    /// Only meaningful when `relay = true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_cursor_type: Option<String>,
}

/// Mutation definition in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct IntermediateMutation {
    /// Mutation name (e.g., "createUser")
    pub name: String,

    /// Return type name (e.g., "User")
    pub return_type: String,

    /// Returns a list?
    #[serde(default)]
    pub returns_list: bool,

    /// Result is nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Mutation arguments
    #[serde(default)]
    pub arguments: Vec<IntermediateArgument>,

    /// Mutation description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SQL source (function name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,

    /// Operation type (CREATE, UPDATE, DELETE, CUSTOM)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,

    /// Deprecation info (from @deprecated directive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,

    /// Server-injected parameters: SQL parameter name → source expression (e.g. `"jwt:org_id"`).
    /// Not exposed as GraphQL arguments.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub inject: IndexMap<String, String>,

    /// Fact tables whose version counter should be bumped after this mutation succeeds.
    ///
    /// Used for correct invalidation of analytic/aggregate cache entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidates_fact_tables: Vec<String>,

    /// View names whose cached query results should be invalidated after this
    /// mutation succeeds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidates_views: Vec<String>,
}

/// Auto-params configuration in intermediate format.
///
/// Each field is `Option<bool>`: `None` means "not specified — inherit from
/// `[query_defaults]`"; `Some(v)` means explicitly set by the authoring-language decorator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateAutoParams {
    /// Enable automatic limit parameter (None = inherit from query_defaults)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit:        Option<bool>,
    /// Enable automatic offset parameter (None = inherit from query_defaults)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset:       Option<bool>,
    /// Enable automatic where clause parameter (None = inherit from query_defaults)
    #[serde(rename = "where", default, skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<bool>,
    /// Enable automatic order_by parameter (None = inherit from query_defaults)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_by:     Option<bool>,
}

/// Global auto-param defaults for list queries (injected from TOML by the merger).
///
/// Never present in `schema.json` — set only at compile time via `[query_defaults]`
/// in `fraiseql.toml`.
///
/// The `Default` implementation returns all-`true`, matching the historical behaviour
/// when no `[query_defaults]` section is present in TOML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateQueryDefaults {
    /// Default for `where` parameter
    pub where_clause: bool,
    /// Default for `order_by` parameter
    pub order_by:     bool,
    /// Default for `limit` parameter
    pub limit:        bool,
    /// Default for `offset` parameter
    pub offset:       bool,
}

impl Default for IntermediateQueryDefaults {
    fn default() -> Self {
        Self {
            where_clause: true,
            order_by:     true,
            limit:        true,
            offset:       true,
        }
    }
}
