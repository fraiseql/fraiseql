//! Query and mutation operation definitions for TOML schema.

use serde::{Deserialize, Serialize};

use super::types::ArgumentDefinition;

/// Global defaults for list-query auto-params.
///
/// Applied when a per-query `auto_params` does not specify a given flag.
/// Relay queries and single-item queries are never affected.
///
/// ```toml
/// [query_defaults]
/// where    = true
/// order_by = true
/// limit    = false  # e.g. Relay-first project
/// offset   = false
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct QueryDefaults {
    /// Enable automatic `where` filter parameter (default: true)
    #[serde(rename = "where", default = "default_true")]
    pub where_clause: bool,
    /// Enable automatic `order_by` parameter (default: true)
    #[serde(default = "default_true")]
    pub order_by:     bool,
    /// Enable automatic `limit` parameter (default: true)
    #[serde(default = "default_true")]
    pub limit:        bool,
    /// Enable automatic `offset` parameter (default: true)
    #[serde(default = "default_true")]
    pub offset:       bool,
}

impl Default for QueryDefaults {
    fn default() -> Self {
        Self {
            where_clause: true,
            order_by:     true,
            limit:        true,
            offset:       true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Schema metadata
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SchemaMetadata {
    /// Schema name
    pub name:            String,
    /// Schema version
    pub version:         String,
    /// Optional schema description
    pub description:     Option<String>,
    /// Target database (postgresql, mysql, sqlite, sqlserver)
    pub database_target: String,
}

impl Default for SchemaMetadata {
    fn default() -> Self {
        Self {
            name:            "myapp".to_string(),
            version:         "1.0.0".to_string(),
            description:     None,
            database_target: "postgresql".to_string(),
        }
    }
}

/// Query definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct QueryDefinition {
    /// Return type name
    pub return_type:  String,
    /// Whether query returns an array
    #[serde(default)]
    pub return_array: bool,
    /// SQL source for the query
    pub sql_source:   String,
    /// Query description
    pub description:  Option<String>,
    /// Query arguments
    pub args:         Vec<ArgumentDefinition>,
}

impl Default for QueryDefinition {
    fn default() -> Self {
        Self {
            return_type:  "String".to_string(),
            return_array: false,
            sql_source:   "v_entity".to_string(),
            description:  None,
            args:         vec![],
        }
    }
}

/// Mutation definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MutationDefinition {
    /// Return type name
    pub return_type:         String,
    /// SQL function or procedure source.
    ///
    /// When absent, the compiler resolves the function name from the `[crud]`
    /// naming config using the `operation` and the entity name derived from
    /// `return_type`. A compile error is emitted if both are missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_source:          Option<String>,
    /// Operation type (CREATE, UPDATE, DELETE)
    pub operation:           String,
    /// How the GraphQL `input` argument is passed to the SQL function:
    /// `"flatten"` (positional columns, the default) or `"jsonb"` (the whole
    /// input as one `jsonb` arg).
    ///
    /// Orthogonal to `operation`: set `"jsonb"` so a backend using the
    /// single-`jsonb`-wrapper convention can register the real DML verb and
    /// still receive the whole input as one argument — letting the Change Spine
    /// record the true `modification_type`. Defaults to `"flatten"`.
    #[serde(default = "default_input_style")]
    pub input_style:         String,
    /// Whether a successful, state-changing run of this mutation also records the
    /// changed entity's pre-image (before-state) into the Change-Spine
    /// `object_data_before` column, sourced from an optional `entity_before` on the
    /// mutation's `app.mutation_response`.
    ///
    /// Set `true` to opt an audit-sensitive mutation into an inline Debezium-style
    /// `{before, after}` on the single change event. Defaults to `false`,
    /// byte-identical to a schema authored before this field existed.
    #[serde(default)]
    pub changelog_pre_image: bool,
    /// Whether this mutation exposes the typed graphql-cascade `cascade` field
    /// on its success payload.
    ///
    /// Set `true` to expose a typed, selection-gated `cascade` (mutation
    /// responses carrying all affected entities per the graphql-cascade spec),
    /// projected to camelCase and field-authorized per entity. Defaults to
    /// `false`, byte-identical to a schema authored before this field existed.
    #[serde(default)]
    pub cascade:             bool,
    /// Mutation description
    pub description:         Option<String>,
    /// Mutation arguments
    pub args:                Vec<ArgumentDefinition>,
}

impl Default for MutationDefinition {
    fn default() -> Self {
        Self {
            return_type:         "String".to_string(),
            sql_source:          None,
            operation:           "CREATE".to_string(),
            input_style:         default_input_style(),
            changelog_pre_image: false,
            cascade:             false,
            description:         None,
            args:                vec![],
        }
    }
}

/// Serde default for [`MutationDefinition::input_style`]: positional flatten.
fn default_input_style() -> String {
    "flatten".to_string()
}
