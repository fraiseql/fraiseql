use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::argument::{ArgumentDefinition, AutoParams};
use crate::schema::{
    field_type::DeprecationInfo, graphql_type_defs::default_jsonb_column,
    security_config::InjectedParamSource,
};

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

    /// Custom REST path override (from `@fraiseql.query(rest_path="/custom/path")`).
    ///
    /// When set, the REST transport uses this path instead of the auto-derived one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_path: Option<String>,

    /// Custom REST HTTP method override (from `@fraiseql.query(rest_method="POST")`).
    ///
    /// When set, the REST transport uses this method instead of the auto-derived one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_method: Option<String>,
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
            rest_path:           None,
            rest_method:         None,
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;

    #[test]
    fn test_rest_path_defaults_none() {
        let q = QueryDefinition::new("users", "User");
        assert!(q.rest_path.is_none());
        assert!(q.rest_method.is_none());
    }

    #[test]
    fn test_rest_path_skipped_when_none() {
        let q = QueryDefinition::new("users", "User");
        let json = serde_json::to_string(&q).unwrap();
        assert!(!json.contains("rest_path"));
        assert!(!json.contains("rest_method"));
    }

    #[test]
    fn test_rest_path_roundtrip() {
        let mut q = QueryDefinition::new("users", "User");
        q.rest_path = Some("/custom/users".to_string());
        q.rest_method = Some("POST".to_string());
        let json = serde_json::to_string(&q).unwrap();
        let restored: QueryDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.rest_path.as_deref(), Some("/custom/users"));
        assert_eq!(restored.rest_method.as_deref(), Some("POST"));
    }

    #[test]
    fn test_deserialization_without_rest_fields() {
        let json = r#"{"name":"users","return_type":"User"}"#;
        let q: QueryDefinition = serde_json::from_str(json).unwrap();
        assert!(q.rest_path.is_none());
        assert!(q.rest_method.is_none());
    }
}
