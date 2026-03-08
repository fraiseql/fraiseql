//! Fluent builders for constructing test schemas.
//!
//! [`TestSchemaBuilder`] lets tests describe what they need (queries, types,
//! security config) rather than how `CompiledSchema` is laid out internally.
//! When `CompiledSchema` gains new fields, tests using these builders require
//! **zero changes**.  When `QueryDefinition` or `TypeDefinition` gain new fields,
//! only the builders' `build()` methods need updating — not every test site.
//!
//! # Example
//!
//! ```rust
//! use fraiseql_test_utils::schema_builder::{TestSchemaBuilder, TestQueryBuilder, TestTypeBuilder, TestFieldBuilder};
//! use fraiseql_core::schema::FieldType;
//!
//! let schema = TestSchemaBuilder::new()
//!     .with_simple_query("users", "User", true)
//!     .with_simple_query("user", "User", false)
//!     .with_type(
//!         TestTypeBuilder::new("User", "v_user")
//!             .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
//!             .with_field(TestFieldBuilder::nullable("email", FieldType::String).build())
//!             .build()
//!     )
//!     .build();
//!
//! assert_eq!(schema.queries.len(), 2);
//! assert_eq!(schema.types.len(), 1);
//! ```

use fraiseql_core::schema::{
    CompiledSchema, CursorType, DeprecationInfo, FieldDefinition, FieldDenyPolicy, FieldType,
    MutationDefinition, QueryDefinition, SecurityConfig, TypeDefinition,
};

// ============================================================================
// TestSchemaBuilder
// ============================================================================

/// Fluent builder for `CompiledSchema` in tests.
///
/// Abstracts over the internal struct layout so that tests don't break when
/// `CompiledSchema` gains new fields.  Always calls `build_indexes()` before
/// returning, so tests never need to call it manually.
///
/// # Example
///
/// ```rust
/// use fraiseql_test_utils::schema_builder::TestSchemaBuilder;
///
/// let schema = TestSchemaBuilder::new()
///     .with_simple_query("authors", "Author", true)
///     .build();
///
/// assert_eq!(schema.queries.len(), 1);
/// ```
#[derive(Default)]
pub struct TestSchemaBuilder {
    queries:    Vec<QueryDefinition>,
    mutations:  Vec<MutationDefinition>,
    types:      Vec<TypeDefinition>,
    security:   Option<SecurityConfig>,
    federation: Option<serde_json::Value>,
}

impl TestSchemaBuilder {
    /// Create an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a pre-built query definition.
    #[must_use]
    pub fn with_query(mut self, query: QueryDefinition) -> Self {
        self.queries.push(query);
        self
    }

    /// Add a simple query by name and return type.
    ///
    /// Defaults: `sql_source = "v_{name}"`, no arguments, `jsonb_column` = "data".
    #[must_use]
    pub fn with_simple_query(self, name: &str, return_type: &str, returns_list: bool) -> Self {
        self.with_query(
            TestQueryBuilder::new(name, return_type)
                .returns_list(returns_list)
                .build(),
        )
    }

    /// Add a query that requires a specific role to execute.
    #[must_use]
    pub fn with_role_guarded_query(
        self,
        name: &str,
        return_type: &str,
        returns_list: bool,
        role: &str,
    ) -> Self {
        self.with_query(
            TestQueryBuilder::new(name, return_type)
                .returns_list(returns_list)
                .requires_role(role)
                .build(),
        )
    }

    /// Add a pre-built mutation definition.
    #[must_use]
    pub fn with_mutation(mut self, mutation: MutationDefinition) -> Self {
        self.mutations.push(mutation);
        self
    }

    /// Add a simple mutation by name.
    ///
    /// Defaults: `sql_source = "fn_{name}"`, operation = `Custom`.
    #[must_use]
    pub fn with_simple_mutation(self, name: &str, return_type: &str) -> Self {
        self.with_mutation(TestMutationBuilder::new(name, return_type).build())
    }

    /// Add a pre-built type definition.
    #[must_use]
    pub fn with_type(mut self, type_def: TypeDefinition) -> Self {
        self.types.push(type_def);
        self
    }

    /// Add a type by name and SQL source with no fields.
    ///
    /// Use [`TestTypeBuilder`] when you need to specify fields.
    #[must_use]
    pub fn with_empty_type(self, name: &str, sql_source: &str) -> Self {
        self.with_type(TestTypeBuilder::new(name, sql_source).build())
    }

    /// Set the security configuration.
    #[must_use]
    pub fn with_security(mut self, security: SecurityConfig) -> Self {
        self.security = Some(security);
        self
    }

    /// Set the federation configuration as raw JSON.
    #[must_use]
    pub fn with_federation(mut self, config: serde_json::Value) -> Self {
        self.federation = Some(config);
        self
    }

    /// Build the schema and populate all lookup indexes.
    ///
    /// Equivalent to calling `CompiledSchema::from_json()` — always indexes.
    #[must_use]
    pub fn build(self) -> CompiledSchema {
        let mut schema = CompiledSchema {
            queries:    self.queries,
            mutations:  self.mutations,
            types:      self.types,
            security:   self.security,
            federation: self
                .federation
                .map(|v| serde_json::from_value(v).unwrap_or_default()),
            ..CompiledSchema::default()
        };
        schema.build_indexes();
        schema
    }
}

// ============================================================================
// TestQueryBuilder
// ============================================================================

/// Fluent builder for `QueryDefinition` in tests.
///
/// Uses `QueryDefinition::new()` internally, so new fields added to
/// `QueryDefinition` will be picked up automatically with their defaults.
///
/// # Example
///
/// ```rust
/// use fraiseql_test_utils::schema_builder::TestQueryBuilder;
///
/// let query = TestQueryBuilder::new("users", "User")
///     .returns_list(true)
///     .requires_role("admin")
///     .with_sql_source("v_user")
///     .build();
///
/// assert_eq!(query.name, "users");
/// assert!(query.returns_list);
/// ```
pub struct TestQueryBuilder {
    name:                String,
    return_type:         String,
    returns_list:        bool,
    sql_source:          Option<String>,
    requires_role:       Option<String>,
    cache_ttl:           Option<u64>,
    description:         Option<String>,
    deprecated:          Option<String>,
    additional_views:    Vec<String>,
    no_source:           bool,
    relay:               bool,
    relay_cursor_column: Option<String>,
    relay_cursor_type:   CursorType,
}

impl TestQueryBuilder {
    /// Create a query builder with the given name and return type.
    ///
    /// Defaults: `sql_source = "v_{name}"`, single result, no role requirement.
    #[must_use]
    pub fn new(name: &str, return_type: &str) -> Self {
        Self {
            name:                name.to_string(),
            return_type:         return_type.to_string(),
            returns_list:        false,
            sql_source:          None,
            requires_role:       None,
            cache_ttl:           None,
            description:         None,
            deprecated:          None,
            additional_views:    Vec::new(),
            no_source:           false,
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
        }
    }

    /// Set whether the query returns a list.
    #[must_use]
    pub const fn returns_list(mut self, flag: bool) -> Self {
        self.returns_list = flag;
        self
    }

    /// Override the default SQL source view name.
    #[must_use]
    pub fn with_sql_source(mut self, source: &str) -> Self {
        self.sql_source = Some(source.to_string());
        self
    }

    /// Require this role to execute the query.
    #[must_use]
    pub fn requires_role(mut self, role: &str) -> Self {
        self.requires_role = Some(role.to_string());
        self
    }

    /// Set a per-query cache TTL (in seconds).
    #[must_use]
    pub const fn with_cache_ttl(mut self, secs: u64) -> Self {
        self.cache_ttl = Some(secs);
        self
    }

    /// Set a human-readable description.
    #[must_use]
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Mark the query as deprecated with the given reason.
    #[must_use]
    pub fn deprecated(mut self, reason: &str) -> Self {
        self.deprecated = Some(reason.to_string());
        self
    }

    /// Do not set a SQL source (custom resolver, no database view).
    ///
    /// Overrides the default `"v_{name}"` source with `None`.
    #[must_use]
    pub const fn no_sql_source(mut self) -> Self {
        self.no_source = true;
        self
    }

    /// Enable Relay connection pagination for this query.
    #[must_use]
    pub const fn relay(mut self, flag: bool) -> Self {
        self.relay = flag;
        self
    }

    /// Set the keyset cursor column for Relay pagination (e.g., `"pk_user"`).
    ///
    /// Implies `relay(true)`.
    #[must_use]
    pub fn relay_cursor_column(mut self, col: &str) -> Self {
        self.relay = true;
        self.relay_cursor_column = Some(col.to_string());
        self
    }

    /// Set the cursor column type for Relay pagination.
    ///
    /// Defaults to `CursorType::Int64` (bigint). Use `CursorType::Uuid` when
    /// the cursor column holds a UUID.
    #[must_use]
    pub const fn relay_cursor_type(mut self, cursor_type: CursorType) -> Self {
        self.relay_cursor_type = cursor_type;
        self
    }

    /// Add secondary views for cache invalidation.
    ///
    /// These are views that the query reads from in addition to the primary SQL source.
    #[must_use]
    pub fn with_additional_views(mut self, views: Vec<String>) -> Self {
        self.additional_views = views;
        self
    }

    /// Build the `QueryDefinition`.
    ///
    /// Uses `QueryDefinition::new()` so new fields are picked up automatically.
    #[must_use]
    pub fn build(self) -> QueryDefinition {
        let mut q = QueryDefinition::new(&self.name, &self.return_type);

        if !self.no_source {
            let src = self.sql_source.unwrap_or_else(|| format!("v_{}", self.name));
            q = q.with_sql_source(src);
        }

        if self.returns_list {
            q = q.returning_list();
        }

        if let Some(role) = self.requires_role {
            q.requires_role = Some(role);
        }

        if let Some(ttl) = self.cache_ttl {
            q.cache_ttl_seconds = Some(ttl);
        }

        if let Some(desc) = self.description {
            q.description = Some(desc);
        }

        if let Some(reason) = self.deprecated {
            q = q.deprecated(Some(reason));
        }

        if !self.additional_views.is_empty() {
            q.additional_views = self.additional_views;
        }

        if self.relay {
            q.relay = true;
        }

        if let Some(col) = self.relay_cursor_column {
            q.relay_cursor_column = Some(col);
        }

        q.relay_cursor_type = self.relay_cursor_type;

        q
    }
}

// ============================================================================
// TestMutationBuilder
// ============================================================================

/// Fluent builder for `MutationDefinition` in tests.
///
/// # Example
///
/// ```rust
/// use fraiseql_test_utils::schema_builder::TestMutationBuilder;
///
/// let mutation = TestMutationBuilder::new("createUser", "User")
///     .with_sql_source("fn_create_user")
///     .build();
///
/// assert_eq!(mutation.name, "createUser");
/// ```
pub struct TestMutationBuilder {
    name:        String,
    return_type: String,
    sql_source:  Option<String>,
    description: Option<String>,
    deprecated:  Option<String>,
}

impl TestMutationBuilder {
    /// Create a mutation builder with the given name and return type.
    ///
    /// Defaults: `sql_source = "fn_{name}"`, operation = `Custom`.
    #[must_use]
    pub fn new(name: &str, return_type: &str) -> Self {
        Self {
            name:        name.to_string(),
            return_type: return_type.to_string(),
            sql_source:  None,
            description: None,
            deprecated:  None,
        }
    }

    /// Override the default SQL function name.
    #[must_use]
    pub fn with_sql_source(mut self, source: &str) -> Self {
        self.sql_source = Some(source.to_string());
        self
    }

    /// Set a human-readable description.
    #[must_use]
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Mark the mutation as deprecated with the given reason.
    #[must_use]
    pub fn deprecated(mut self, reason: &str) -> Self {
        self.deprecated = Some(reason.to_string());
        self
    }

    /// Build the `MutationDefinition`.
    ///
    /// Uses `MutationDefinition::new()` so new fields are picked up automatically.
    #[must_use]
    pub fn build(self) -> MutationDefinition {
        let sql_source = self
            .sql_source
            .unwrap_or_else(|| format!("fn_{}", self.name));

        let mut m = MutationDefinition::new(&self.name, &self.return_type);
        m.sql_source = Some(sql_source);

        if let Some(desc) = self.description {
            m.description = Some(desc);
        }

        if let Some(reason) = self.deprecated {
            m = m.deprecated(Some(reason));
        }

        m
    }
}

// ============================================================================
// TestTypeBuilder
// ============================================================================

/// Fluent builder for `TypeDefinition` in tests.
///
/// # Example
///
/// ```rust
/// use fraiseql_test_utils::schema_builder::{TestTypeBuilder, TestFieldBuilder};
/// use fraiseql_core::schema::FieldType;
///
/// let type_def = TestTypeBuilder::new("User", "v_user")
///     .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
///     .with_field(TestFieldBuilder::nullable("email", FieldType::String).build())
///     .build();
///
/// assert_eq!(type_def.fields.len(), 2);
/// ```
pub struct TestTypeBuilder {
    name:          String,
    sql_source:    String,
    fields:        Vec<FieldDefinition>,
    requires_role: Option<String>,
    description:   Option<String>,
    relay:         bool,
    implements:    Vec<String>,
}

impl TestTypeBuilder {
    /// Create a type builder with the given name and SQL source.
    #[must_use]
    pub fn new(name: &str, sql_source: &str) -> Self {
        Self {
            name:          name.to_string(),
            sql_source:    sql_source.to_string(),
            fields:        Vec::new(),
            requires_role: None,
            description:   None,
            relay:         false,
            implements:    Vec::new(),
        }
    }

    /// Add a field to the type.
    #[must_use]
    pub fn with_field(mut self, field: FieldDefinition) -> Self {
        self.fields.push(field);
        self
    }

    /// Add a simple non-nullable field by name and type.
    ///
    /// Shorthand for `.with_field(TestFieldBuilder::new(name, ty).build())`.
    #[must_use]
    pub fn with_simple_field(self, name: &str, ty: FieldType) -> Self {
        self.with_field(TestFieldBuilder::new(name, ty).build())
    }

    /// Add a nullable field by name and type.
    #[must_use]
    pub fn with_nullable_field(self, name: &str, ty: FieldType) -> Self {
        self.with_field(TestFieldBuilder::nullable(name, ty).build())
    }

    /// Add a scope-guarded field by name and type.
    #[must_use]
    pub fn with_scoped_field(self, name: &str, ty: FieldType, scope: &str) -> Self {
        self.with_field(
            TestFieldBuilder::new(name, ty)
                .requires_scope(scope)
                .build(),
        )
    }

    /// Require this role to see the type in introspection.
    #[must_use]
    pub fn requires_role(mut self, role: &str) -> Self {
        self.requires_role = Some(role.to_string());
        self
    }

    /// Set a human-readable description.
    #[must_use]
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Mark the type as a Relay node (implements `Node` interface).
    ///
    /// Sets `relay = true` on the underlying `TypeDefinition`.
    #[must_use]
    pub const fn relay_node(mut self) -> Self {
        self.relay = true;
        self
    }

    /// Declare that this type implements the given interfaces.
    ///
    /// Pass the interface names (e.g., `&["Node"]`).
    #[must_use]
    pub fn with_implements(mut self, interfaces: &[&str]) -> Self {
        self.implements = interfaces.iter().map(|s| (*s).to_string()).collect();
        self
    }

    /// Build the `TypeDefinition`.
    ///
    /// Uses `TypeDefinition::new()` so new fields are picked up automatically.
    #[must_use]
    pub fn build(self) -> TypeDefinition {
        let mut t = TypeDefinition::new(&self.name, &self.sql_source);
        t.fields = self.fields;
        t.requires_role = self.requires_role;
        t.description = self.description;
        t.relay = self.relay;
        t.implements = self.implements;
        t
    }
}

// ============================================================================
// TestFieldBuilder
// ============================================================================

/// Fluent builder for `FieldDefinition` in tests.
///
/// # Example
///
/// ```rust
/// use fraiseql_test_utils::schema_builder::TestFieldBuilder;
/// use fraiseql_core::schema::{FieldType, FieldDenyPolicy};
///
/// // Non-nullable public field
/// let field = TestFieldBuilder::new("email", FieldType::String).build();
///
/// // Nullable field using the nullable constructor
/// let bio = TestFieldBuilder::nullable("bio", FieldType::String).build();
///
/// // Scope-guarded field that returns null on deny
/// let private = TestFieldBuilder::new("salary", FieldType::Int)
///     .requires_scope("read:Employee.salary")
///     .on_deny(FieldDenyPolicy::Nullify)
///     .build();
/// ```
pub struct TestFieldBuilder {
    inner:          FieldDefinition,
}

impl TestFieldBuilder {
    /// Create a non-nullable, public field.
    #[must_use]
    pub fn new(name: &str, field_type: FieldType) -> Self {
        Self {
            inner: FieldDefinition::new(name, field_type),
        }
    }

    /// Create a nullable field.
    ///
    /// Equivalent to `TestFieldBuilder::new(name, ty).nullable_flag()`.
    #[must_use]
    pub fn nullable(name: &str, field_type: FieldType) -> Self {
        // FieldDefinition::nullable is a static constructor (not a method)
        #[allow(clippy::cast_sign_loss)] // Reason: using the static constructor, not a cast
        Self {
            inner: FieldDefinition::nullable(name, field_type),
        }
    }

    /// Require `scope` to access this field.
    #[must_use]
    pub fn requires_scope(mut self, scope: &str) -> Self {
        self.inner.requires_scope = Some(scope.to_string());
        self
    }

    /// Set the deny policy (what happens when the scope is missing).
    #[must_use]
    pub const fn on_deny(mut self, policy: FieldDenyPolicy) -> Self {
        self.inner.on_deny = policy;
        self
    }

    /// Set a human-readable description.
    #[must_use]
    pub fn with_description(mut self, desc: &str) -> Self {
        self.inner.description = Some(desc.to_string());
        self
    }

    /// Mark the field as deprecated with the given reason.
    #[must_use]
    pub fn deprecated(mut self, reason: &str) -> Self {
        self.inner.deprecation = Some(DeprecationInfo { reason: Some(reason.to_string()) });
        self
    }

    /// Build the `FieldDefinition`.
    #[must_use]
    pub fn build(self) -> FieldDefinition {
        self.inner
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test assertions, panics are acceptable
    use fraiseql_core::schema::FieldType;

    use super::*;

    #[test]
    fn test_schema_builder_empty() {
        let schema = TestSchemaBuilder::new().build();
        assert!(schema.queries.is_empty());
        assert!(schema.mutations.is_empty());
        assert!(schema.types.is_empty());
    }

    #[test]
    fn test_schema_builder_with_simple_query() {
        let schema = TestSchemaBuilder::new()
            .with_simple_query("users", "User", true)
            .build();

        assert_eq!(schema.queries.len(), 1);
        assert_eq!(schema.queries[0].name, "users");
        assert_eq!(schema.queries[0].return_type, "User");
        assert!(schema.queries[0].returns_list);
        // Default sql_source
        assert_eq!(schema.queries[0].sql_source.as_deref(), Some("v_users"));
    }

    #[test]
    fn test_schema_builder_indexes_populated() {
        let schema = TestSchemaBuilder::new()
            .with_simple_query("users", "User", true)
            .with_simple_query("user", "User", false)
            .build();

        // Indexes must be populated for O(1) lookup to work
        assert!(schema.find_query("users").is_some());
        assert!(schema.find_query("user").is_some());
        assert!(schema.find_query("missing").is_none());
    }

    #[test]
    fn test_schema_builder_with_role_guarded_query() {
        let schema = TestSchemaBuilder::new()
            .with_role_guarded_query("adminStats", "Stats", false, "admin")
            .build();

        let query = schema.find_query("adminStats").unwrap();
        assert_eq!(query.requires_role.as_deref(), Some("admin"));
    }

    #[test]
    fn test_schema_builder_with_mutation() {
        let schema = TestSchemaBuilder::new()
            .with_simple_mutation("createUser", "User")
            .build();

        assert_eq!(schema.mutations.len(), 1);
        assert_eq!(schema.mutations[0].name, "createUser");
        assert_eq!(schema.mutations[0].sql_source.as_deref(), Some("fn_createUser"));
    }

    #[test]
    fn test_type_builder_with_fields() {
        let type_def = TestTypeBuilder::new("User", "v_user")
            .with_simple_field("id", FieldType::Int)
            .with_nullable_field("bio", FieldType::String)
            .build();

        assert_eq!(type_def.fields.len(), 2);
        assert!(!type_def.fields[0].nullable);
        assert!(type_def.fields[1].nullable);
    }

    #[test]
    fn test_type_builder_scoped_field() {
        let type_def = TestTypeBuilder::new("Employee", "v_employee")
            .with_scoped_field("salary", FieldType::Int, "read:Employee.salary")
            .build();

        assert_eq!(
            type_def.fields[0].requires_scope.as_deref(),
            Some("read:Employee.salary")
        );
    }

    #[test]
    fn test_query_builder_deprecated() {
        let query = TestQueryBuilder::new("oldQuery", "Result")
            .deprecated("Use newQuery instead")
            .build();

        assert!(query.deprecation.is_some());
        assert_eq!(
            query.deprecation.unwrap().reason.as_deref(),
            Some("Use newQuery instead")
        );
    }

    #[test]
    fn test_field_builder_on_deny_policy() {
        let field = TestFieldBuilder::new("secret", FieldType::String)
            .requires_scope("admin:read")
            .on_deny(FieldDenyPolicy::Reject)
            .build();

        assert_eq!(field.on_deny, FieldDenyPolicy::Reject);
    }

    #[test]
    fn test_mutation_builder_with_sql_source() {
        let mutation = TestMutationBuilder::new("archivePost", "Post")
            .with_sql_source("fn_archive_post")
            .build();

        assert_eq!(mutation.sql_source.as_deref(), Some("fn_archive_post"));
    }

    #[test]
    fn test_schema_with_type_and_query() {
        let schema = TestSchemaBuilder::new()
            .with_simple_query("users", "User", true)
            .with_type(
                TestTypeBuilder::new("User", "v_user")
                    .with_simple_field("id", FieldType::Int)
                    .with_simple_field("name", FieldType::String)
                    .build(),
            )
            .build();

        assert_eq!(schema.queries.len(), 1);
        assert_eq!(schema.types.len(), 1);
        assert_eq!(schema.types[0].fields.len(), 2);
    }

    #[test]
    fn test_query_builder_cache_ttl() {
        let query = TestQueryBuilder::new("hot", "Item")
            .with_cache_ttl(300)
            .build();

        assert_eq!(query.cache_ttl_seconds, Some(300));
    }
}
