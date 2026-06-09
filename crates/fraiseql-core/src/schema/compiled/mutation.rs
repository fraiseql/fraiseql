use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::argument::ArgumentDefinition;
use crate::schema::{field_type::DeprecationInfo, security_config::InjectedParamSource};

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
    pub deprecation: Option<DeprecationInfo>,

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

    /// Custom REST path override (e.g., `"/users/{id}"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_path: Option<String>,

    /// REST HTTP method override (e.g., `"POST"`, `"PUT"`, `"PATCH"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_method: Option<String>,

    /// PostgreSQL upsert function name for `PUT` semantics (insert-or-update).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upsert_function: Option<String>,

    /// Role required to execute this mutation and see it in introspection.
    ///
    /// When set, only users whose `SecurityContext.roles` contains this role can
    /// discover and execute the mutation. Others receive `"Unknown mutation"`
    /// (not `FORBIDDEN`) to prevent role enumeration — mirroring
    /// [`QueryDefinition::requires_role`](super::query::QueryDefinition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_role: Option<String>,

    /// Whether a successful run of this mutation writes a Change-Spine change-log
    /// row (default `true`).
    ///
    /// Composes as a logical AND with the global
    /// [`RuntimeConfig.changelog_enabled`](crate::runtime::RuntimeConfig): a row
    /// is written only when the global switch is on **and** this flag is `true`.
    /// Set `false` to opt a single mutation out of the in-transaction outbox
    /// write — e.g. a hot endpoint that need not appear in the Change Spine —
    /// while leaving the rest of the schema logging. Serde-defaults to `true`, so
    /// a compiled schema produced before this field existed keeps logging.
    #[serde(default = "default_changelog")]
    pub changelog: bool,
}

/// Serde default for [`MutationDefinition::changelog`]: log by default (opt-out).
const fn default_changelog() -> bool {
    true
}

impl MutationDefinition {
    /// Create a new mutation definition.
    #[must_use]
    pub fn new(name: impl Into<String>, return_type: impl Into<String>) -> Self {
        Self {
            name:                    name.into(),
            return_type:             return_type.into(),
            arguments:               Vec::new(),
            description:             None,
            operation:               MutationOperation::default(),
            deprecation:             None,
            sql_source:              None,
            inject_params:           IndexMap::new(),
            invalidates_fact_tables: Vec::new(),
            invalidates_views:       Vec::new(),
            rest_path:               None,
            rest_method:             None,
            upsert_function:         None,
            requires_role:           None,
            changelog:               true,
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
        self.deprecation = Some(DeprecationInfo { reason });
        self
    }

    /// Check if this mutation is deprecated.
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

/// Mutation operation types.
///
/// This enum describes what kind of database operation a mutation performs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[non_exhaustive]
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

impl MutationOperation {
    /// Return a lowercase string label for the operation kind.
    ///
    /// Used in structured audit log events to identify the DML type.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::MutationOperation;
    ///
    /// assert_eq!(MutationOperation::Insert { table: "users".into() }.kind_str(), "insert");
    /// assert_eq!(MutationOperation::Update { table: "users".into() }.kind_str(), "update");
    /// assert_eq!(MutationOperation::Delete { table: "users".into() }.kind_str(), "delete");
    /// assert_eq!(MutationOperation::Custom.kind_str(), "custom");
    /// ```
    #[must_use]
    pub const fn kind_str(&self) -> &'static str {
        match self {
            Self::Insert { .. } => "insert",
            Self::Update { .. } => "update",
            Self::Delete { .. } => "delete",
            Self::Custom => "custom",
        }
    }
}
