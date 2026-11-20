//! Query and mutation operation definitions for TOML schema.

use fraiseql_core::schema::NamingConvention;
use serde::{Deserialize, Serialize};

use super::types::ArgumentDefinition;

/// What the compiler does about the order a paginated read's pages are cut in
/// (#1303).
///
/// An offset page is a slice of a *sequence*, and a read with no `ORDER BY` is not
/// a sequence — two pages of the same relation can overlap and skip rows, under a
/// `200`. The deployment-wide answer is here; a query overrides it with its own
/// `pagination_order`.
///
/// # Why all three are compile-time
///
/// #1303 framed the strict posture as a request-time `400`, because it framed the
/// ordering as a request property. It is not one: since the order is recorded per
/// query in the compiled schema, every posture resolves into what the compiler
/// writes there, and a second switch at runtime would be a second place the answer
/// lives — free to disagree with the artifact that is supposed to *be* the answer.
/// Refusing at compile time is also strictly earlier than refusing at request time.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PaginationPosture {
    /// Every paginating query gets the entity identity, and an author may declare
    /// `pagination_order = "none"` to keep a self-ordering view's own `ORDER BY`.
    ///
    /// The default, and the founder decision of 2026-09-08: the boilerplate of a
    /// must-declare rule is a permanent tax on every paginated query written from
    /// now on, while the identity default's cost is bounded, measured (1.7–7.7×
    /// on unordered paginated reads) and recoverable by one declaration in the
    /// minority of cases where it is wrong.
    #[default]
    Identity,
    /// As [`Identity`](Self::Identity), and `pagination_order = "none"` is refused.
    ///
    /// #1303's option 1, kept as a posture rather than a default: in this
    /// deployment every paginated read has a total order and there are no
    /// exceptions, including the self-ordering-view one. The refusal is a compile
    /// error naming the query, not a `400` a client discovers.
    Refuse,
    /// Derive nothing: a query is ordered only where its author declared a column.
    ///
    /// The pre-#1303 behaviour, kept because this release changes what an
    /// unordered paginated read returns and an operator may need the old answer
    /// while their clients catch up. It restores the defect with it — which is why
    /// the compile warns for every query it leaves unordered.
    Allow,
}

/// Global defaults for list-query auto-params.
///
/// Applied when a per-query `auto_params` does not specify a given flag.
/// Relay queries and single-item queries are never affected.
///
/// ```toml
/// [query_defaults]
/// where            = true
/// order_by         = true
/// limit            = false  # e.g. Relay-first project
/// offset           = false
/// pagination_order = "identity"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct QueryDefaults {
    /// Enable automatic `where` filter parameter (default: true)
    #[serde(rename = "where", default = "default_true")]
    pub where_clause:     bool,
    /// Enable automatic `order_by` parameter (default: true)
    #[serde(default = "default_true")]
    pub order_by:         bool,
    /// Enable automatic `limit` parameter (default: true)
    #[serde(default = "default_true")]
    pub limit:            bool,
    /// Enable automatic `offset` parameter (default: true)
    #[serde(default = "default_true")]
    pub offset:           bool,
    /// What the compiler does about the order paginated reads are cut in (#1303).
    ///
    /// ```toml
    /// [query_defaults]
    /// pagination_order = "identity"  # default | "refuse" | "allow"
    /// ```
    #[serde(default)]
    pub pagination_order: PaginationPosture,
}

impl Default for QueryDefaults {
    fn default() -> Self {
        Self {
            where_clause:     true,
            order_by:         true,
            limit:            true,
            offset:           true,
            pagination_order: PaginationPosture::Identity,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Query definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
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
    /// Whether the server's transaction retry policy may run this mutation's transaction
    /// again when PostgreSQL aborts it. Set `false` for a function with an effect outside
    /// its transaction, which each attempt would repeat. Defaults to `true`: the mutation
    /// follows the server.
    #[serde(default = "default_retry_on_abort")]
    pub retry_on_abort:      bool,
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
            retry_on_abort:      true,
            description:         None,
            args:                vec![],
        }
    }
}

/// Serde default for [`MutationDefinition::input_style`]: positional flatten.
fn default_input_style() -> String {
    "flatten".to_string()
}

/// Serde default for [`MutationDefinition::retry_on_abort`]: follow the server's policy.
const fn default_retry_on_abort() -> bool {
    true
}

/// `[naming]`: the GraphQL surface's casing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct NamingConfig {
    /// The GraphQL surface's naming convention. Unwritten, a schema document's own
    /// convention stands, and with none the surface is `camelCase`: `snake_case`
    /// columns and functions in the database, `camelCase` operation and field names for
    /// clients, and mutation input keys recased `camelCase` → `snake_case` before they
    /// reach the SQL functions. `"preserve"` keeps names exactly as authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub convention: Option<NamingConvention>,
    /// Acronyms whose internal digit stays attached when a GraphQL field name is mapped
    /// back to its `snake_case` JSONB key (e.g. `s3`, `ipv4`, `oauth2`), added on top of
    /// the engine's built-in defaults. Author the atomic form (`s3`, not `s_3`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub acronyms:   Vec<String>,
}

impl NamingConfig {
    /// The convention the schema compiles to: the written one, else `document`'s own,
    /// else `camelCase`.
    #[must_use]
    pub fn convention_over(&self, document: Option<NamingConvention>) -> NamingConvention {
        self.convention.or(document).unwrap_or(NamingConvention::CamelCase)
    }
}

/// `[mutation_defaults]`: project-wide mutation compilation options, beside
/// `[query_defaults]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct MutationDefaults {
    /// Opt-in: synthesize a shared `MutationError` type and a per-mutation result union
    /// (`<Mutation>Result = Entity | MutationError`) for every object-returning mutation,
    /// rewriting its return type to that union so the server's success/error
    /// discrimination has a union to resolve against. Mutations that already return a
    /// union are left untouched.
    pub auto_error_union: bool,
}
