use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::argument::{ArgumentDefinition, AutoParams};
use crate::{
    db::types::ReadRouting,
    schema::{
        field_type::{DeprecationInfo, FieldType},
        graphql_type_defs::default_jsonb_column,
        security_config::InjectedParamSource,
    },
};

/// The type of column used as the keyset cursor for relay pagination.
///
/// Determines how the cursor value is encoded/decoded and how the SQL comparison
/// is emitted (`bigint` vs `uuid` cast).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CursorType {
    /// BIGINT / INTEGER column (default, backward-compatible).
    /// Cursor is `base64(decimal_string)`.
    #[default]
    Int64,
    /// UUID column.
    /// Cursor is `base64(uuid_string)`.
    Uuid,
}

pub(super) fn is_default_cursor_type(ct: &CursorType) -> bool {
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

    /// Does this query return `COUNT(*)` over [`sql_source`](Self::sql_source)
    /// instead of rows?
    ///
    /// Compiled as the sibling of a list query that opted in (`count = true`),
    /// named `<query>Count` and exposed as `Int!`. It exists because a bare
    /// `[T]` list has nowhere to hang a total, so an offset-paginated client had
    /// no way to learn how many rows match its filter — `totalCount` was
    /// reachable only through a Relay connection, and a Relay connection is
    /// keyset-only, so obtaining the count cost random access (#938).
    ///
    /// [`return_type`](Self::return_type) stays the **entity** type rather than
    /// `Int`: the filter machinery is keyed on it (`where` input types, native
    /// column casts, RLS policy lookup), and re-pointing it at a scalar would
    /// silently detach the count from the filters whose rows it is counting.
    /// Only the *rendered* GraphQL return type differs.
    ///
    /// The count reflects the full filtered set, independent of `limit`/`offset`
    /// — which is the entire point — so the sibling carries `where` alone and
    /// no pagination arguments.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub returns_count: bool,

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
    pub deprecation: Option<DeprecationInfo>,

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

    /// Where this query's reads may be served from (#957).
    ///
    /// Read-replica routing is otherwise a whole-server decision: the pin window
    /// and the staleness budget apply to every query alike, so an operator sizing
    /// them for the strictest query gives up the offload on all the others, and
    /// sizing them for the common case silently serves the strict one stale.
    ///
    /// FraiseQL defines and enforces this shape; an authoring language emits it —
    /// `SpecQL`'s `@reads_from(...)` (evoludigit/specql#13) is one spelling of it.
    /// Replica **topology** deliberately stays out of the compiled artifact: URLs
    /// are server configuration and secrets.
    ///
    /// See [`ReadRouting`] for what each answer guarantees, including why
    /// `primary` also bypasses the result cache.
    #[serde(default, skip_serializing_if = "ReadRouting::is_default")]
    pub read_routing: ReadRouting,

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

    /// Custom REST path override (e.g., `"/users/{id}/posts"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_path: Option<String>,

    /// REST HTTP method override (e.g., `"GET"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_method: Option<String>,

    /// Native columns detected at compile time for direct query arguments.
    ///
    /// Maps argument name → PostgreSQL cast suffix (e.g., `"uuid"`, `"int4"`, `""`).
    /// An empty string means the column exists but needs no type cast (e.g. `text`).
    ///
    /// At runtime, arguments present in this map generate `WHERE col = $N` (native column
    /// lookup) instead of `WHERE data->>'col' = $N` (JSONB extraction), enabling B-tree
    /// index usage for single-entity lookups.
    ///
    /// Only populated when `fraiseql compile --database <url>` is used. Schemas compiled
    /// without a database URL omit this field and fall back to JSONB extraction.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub native_columns: HashMap<String, String>,
}

impl QueryDefinition {
    /// Create a new query definition.
    #[must_use]
    pub fn new(name: impl Into<String>, return_type: impl Into<String>) -> Self {
        Self {
            name:                name.into(),
            return_type:         return_type.into(),
            returns_list:        false,
            returns_count:       false,
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
            read_routing:        ReadRouting::default(),
            cache_ttl_seconds:   None,
            additional_views:    Vec::new(),
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        }
    }

    /// Set this query to return a list.
    #[must_use]
    pub const fn returning_list(mut self) -> Self {
        self.returns_list = true;
        self
    }

    /// Set the SQL source.
    #[must_use]
    pub fn with_sql_source(mut self, source: impl Into<String>) -> Self {
        self.sql_source = Some(source.into());
        self
    }

    /// Derive the `<name>Count` sibling of this list query (#938).
    ///
    /// This is the **only** way a count query is built. A sibling query that
    /// answers "how many rows match?" against the same view is a second door
    /// onto the same rows, and a second door that forgets one of the first's
    /// restrictions is an oracle: a count that ignores `inject_params` reports
    /// other tenants' row totals, and one that ignores `requires_role` answers
    /// callers who cannot see the list at all. Neither leaks a row, which is
    /// exactly why it would survive review — so the inheritance is centralised
    /// here rather than spelled out at each construction site (the `_entities`
    /// lesson, #1030).
    ///
    /// Inherited, deliberately: the entity `return_type` and `sql_source` (same
    /// rows), `inject_params` (same tenant scoping), `requires_role` (same
    /// visibility), explicitly declared `arguments` (they lower into the same
    /// `WHERE`), `native_columns` (same casts, so the count uses the same
    /// indexes), `additional_views` (same cache invalidation) and `deprecation`.
    ///
    /// Dropped, deliberately: `limit`/`offset`/`orderBy` — a total that moved
    /// with the page would answer a question nobody asked; `relay`, which has
    /// its own `totalCount`; and the REST overrides, since the REST surface
    /// already counts through `Prefer: count=exact`.
    #[must_use]
    pub fn count_sibling(&self) -> Self {
        Self {
            name: format!("{}Count", self.name),
            returns_list: false,
            returns_count: true,
            nullable: false,
            auto_params: AutoParams {
                has_where:    self.auto_params.has_where,
                has_order_by: false,
                has_limit:    false,
                has_offset:   false,
            },
            relay: false,
            relay_cursor_column: None,
            relay_cursor_type: CursorType::Int64,
            rest_path: None,
            rest_method: None,
            description: Some(format!(
                "Total number of `{}` rows matching `where`, independent of any \
                 limit/offset applied to `{}`.",
                self.return_type, self.name
            )),
            ..self.clone()
        }
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
        self.deprecation = Some(DeprecationInfo { reason });
        self
    }

    /// Check if this query is deprecated.
    #[must_use]
    pub const fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Get the deprecation reason if deprecated.
    #[must_use]
    pub fn deprecation_reason(&self) -> Option<&str> {
        self.deprecation.as_ref().and_then(|d| d.reason.as_deref())
    }

    /// The full set of GraphQL arguments this query accepts, for rendering into
    /// the federation `_service` SDL, generated clients, and introspection.
    ///
    /// The auto-wired `where`/`orderBy`/`limit`/`offset` arguments are gated by
    /// [`auto_params`](Self::auto_params) and read directly from the argument map
    /// at runtime, so they are deliberately *not* stored in
    /// [`arguments`](Self::arguments) (where the runtime would otherwise mistake a
    /// synthesized `limit`/`offset` for an explicit column filter). This method
    /// materialises them so every consumer that renders from the argument list can
    /// surface — and a generated client can actually pass — them.
    ///
    /// `where`/`orderBy` carry dynamic, per-field shapes, so they are typed as the
    /// `JSON` scalar; the runtime parses the raw value via
    /// `WhereClause::from_graphql_json` / `OrderByClause::from_graphql_json`.
    ///
    /// An explicit argument always wins: if the query already declares an argument
    /// of the same name it is left untouched and no duplicate is synthesized.
    ///
    /// Relay connection queries are returned unchanged — their pagination surface
    /// (`first`/`after`/`last`/`before`) is owned by each renderer's dedicated
    /// relay path, not by `auto_params`.
    #[must_use]
    pub fn graphql_arguments(&self) -> Vec<ArgumentDefinition> {
        let mut args = self.arguments.clone();
        if self.relay {
            return args;
        }

        let declared = |name: &str| self.arguments.iter().any(|a| a.name == name);
        let ap = &self.auto_params;

        if ap.has_where && !declared("where") {
            args.push(ArgumentDefinition::optional("where", FieldType::Json).with_description(
                "Filter predicate: a nested object of `{ field: { operator: value } }`, \
                 combined with `_and`/`_or`/`_not`.",
            ));
        }
        if ap.has_order_by && !declared("orderBy") {
            args.push(ArgumentDefinition::optional("orderBy", FieldType::Json).with_description(
                "Sort order: `{ field: \"ASC\" | \"DESC\" }` or \
                     `[{ field, direction }]`.",
            ));
        }
        if ap.has_limit && !declared("limit") {
            args.push(
                ArgumentDefinition::optional("limit", FieldType::Int)
                    .with_description("Maximum number of items to return."),
            );
        }
        if ap.has_offset && !declared("offset") {
            args.push(
                ArgumentDefinition::optional("offset", FieldType::Int)
                    .with_description("Number of items to skip before returning results."),
            );
        }

        args
    }
}

impl Default for QueryDefinition {
    fn default() -> Self {
        Self::new("", "")
    }
}
