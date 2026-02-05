# FraiseQL v2: Rust Core Architecture Design

**Version:** 1.0
**Date:** 2026-01-11
**Author:** Senior Rust Architect
**Status:** Design Complete - Ready for Implementation

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Module Structure](#module-structure)
3. [Core Trait Definitions](#core-trait-definitions)
4. [Type Designs](#type-designs)
5. [WHERE Clause Generation](#where-clause-generation)
6. [JSONB Projection Architecture](#jsonb-projection-architecture)
7. [Authorization Strategy](#authorization-strategy)
8. [Connection Pooling](#connection-pooling)
9. [Caching Architecture](#caching-architecture)
10. [Error Handling](#error-handling)
11. [Testing Strategy](#testing-strategy)
12. [Performance Optimizations](#performance-optimizations)
13. [Trade-off Analysis](#trade-off-analysis)
14. [Migration Plan](#migration-plan)

---

## Executive Summary

### The FraiseQL v2 Execution Model (CRITICAL)

**FraiseQL does NOT generate complex SQL with JOINs and field lists.** Instead:

1. **Compile-time**: Create database views that return denormalized JSONB
2. **Runtime**: Execute simple `SELECT data FROM v_X WHERE ...` queries
3. **Rust**: Project JSONB to requested fields + apply auth masking

**Example Flow:**

```sql
-- Compile-time: Create view
CREATE VIEW v_user AS
SELECT id, jsonb_build_object(
  'id', id,
  'email', email,
  'posts', (SELECT jsonb_agg(...) FROM posts WHERE user_id = users.id),
  'password_hash', password_hash
) AS data
FROM users;

-- Runtime: Simple query
SELECT data FROM v_user WHERE data->>'email' ILIKE '%example.com%';

-- Returns complete JSONB:
{
  "id": "user-123",
  "email": "alice@example.com",
  "posts": [...],
  "password_hash": "$2a$10$..."
}

-- Rust projects to requested fields:
{
  "id": "user-123",
  "email": "alice@example.com",
  "posts": [...] // filtered to requested nested fields
  // password_hash removed (field-level auth)
}
```

**This fundamentally simplifies the architecture:**

- ✅ No complex JOIN generation
- ✅ No field list generation
- ✅ Just WHERE clause + JSONB projection
- ✅ Database does aggregation, Rust does filtering

---

## Module Structure

### Proposed Directory Layout

```
crates/fraiseql-core/src/
├── lib.rs
├── error.rs                ✅ Complete
├── config/                 ✅ Complete
├── schema/                 ✅ Complete
│   ├── compiled.rs
│   ├── field_type.rs
│   ├── mod.rs
│   └── tests.rs
├── apq/                    ✅ Complete
│   ├── hasher.rs
│   ├── metrics.rs
│   ├── mod.rs
│   └── storage.rs
│
├── db/                     🔧 Phase 2 (THIS DESIGN)
│   ├── mod.rs              // Database abstraction + exports
│   ├── traits.rs           // DatabaseAdapter trait
│   ├── pool.rs             // Connection pooling
│   ├── where_builder.rs    // WHERE clause AST
│   ├── where_gen.rs        // WHERE clause SQL generation
│   ├── postgres/
│   │   ├── mod.rs
│   │   ├── adapter.rs      // PostgresAdapter impl
│   │   ├── where_gen.rs    // PostgreSQL-specific WHERE syntax
│   │   └── jsonb.rs        // JSONB path helpers
│   ├── mysql/
│   │   ├── mod.rs
│   │   ├── adapter.rs
│   │   └── where_gen.rs
│   ├── sqlite/
│   │   └── ...
│   └── sqlserver/
│       └── ...
│
├── runtime/                🔧 Phase 5 (THIS DESIGN)
│   ├── mod.rs
│   ├── executor.rs         // Query execution pipeline
│   ├── projector.rs        // JSONB → GraphQL projection
│   ├── selection.rs        // SelectionSet representation
│   └── auth_mask.rs        // Field-level auth masking
│
├── cache/                  🔧 Phase 2 (THIS DESIGN)
│   ├── mod.rs
│   ├── backend.rs          // CacheBackend trait
│   ├── memory.rs           // In-memory cache
│   ├── redis.rs            // Redis cache (optional)
│   ├── key_gen.rs          // Cache key generation
│   └── invalidation.rs     // Invalidation cascades
│
├── security/               🔧 Phase 3 (THIS DESIGN)
│   ├── mod.rs
│   ├── auth_context.rs     // User roles, permissions
│   ├── field_auth.rs       // Field-level auth rules
│   └── query_auth.rs       // Query-level auth rules
│
└── utils/                  ⏳ Phase 7
    ├── casing.rs
    ├── operators.rs
    └── vector.rs
```

### Module Dependencies

```
┌─────────────┐
│ runtime/    │ ← High-level execution
└──────┬──────┘
       ↓
┌─────────────┐
│ db/         │ ← Database operations
└──────┬──────┘
       ↓
┌─────────────┐
│ cache/      │ ← Caching layer
└──────┬──────┘
       ↓
┌─────────────┐
│ security/   │ ← Authorization
└──────┬──────┘
       ↓
┌─────────────┐
│ schema/     │ ← Type definitions
└──────┬──────┘
       ↓
┌─────────────┐
│ error       │ ← Error types
└─────────────┘
```

**Design Principle:** Dependencies flow downward only. No circular dependencies.

---

## Core Trait Definitions

### 1. DatabaseAdapter Trait

**Purpose:** Abstract over different database backends (PostgreSQL, MySQL, SQLite, SQL Server).

```rust
/// Database adapter for executing WHERE queries against views.
#[async_trait::async_trait]
pub trait DatabaseAdapter: Send + Sync {
    /// Execute a WHERE query against a view and return JSONB rows.
    ///
    /// # Arguments
    ///
    /// * `view` - View name (e.g., "v_user")
    /// * `where_clause` - WHERE clause AST
    /// * `limit` - Optional row limit
    /// * `offset` - Optional row offset
    ///
    /// # Returns
    ///
    /// Vec of JSONB values from the `data` column.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on query execution failure.
    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>>;

    /// Get database type (for logging/metrics).
    fn database_type(&self) -> DatabaseType;

    /// Health check - verify database connectivity.
    async fn health_check(&self) -> Result<()>;

    /// Get connection pool metrics.
    fn pool_metrics(&self) -> PoolMetrics;
}

/// Database types supported by FraiseQL.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    SQLite,
    SQLServer,
}

/// JSONB value from database (wraps serde_json::Value).
#[derive(Debug, Clone)]
pub struct JsonbValue {
    pub data: serde_json::Value,
}

/// Connection pool metrics.
#[derive(Debug, Clone)]
pub struct PoolMetrics {
    pub total_connections: u32,
    pub idle_connections: u32,
    pub active_connections: u32,
    pub waiting_requests: u32,
}
```

**Design Decisions:**

1. **Async trait**: All DB operations are async (tokio runtime)
2. **Owned return types**: `Vec<JsonbValue>` not streams (simpler, views limit rows anyway)
3. **No raw SQL exposure**: Only `WhereClause` AST (type-safe)
4. **Trait bounds**: `Send + Sync` for multi-threaded Axum server

---

### 2. WhereClauseGenerator Trait

**Purpose:** Generate database-specific WHERE clause SQL from AST.

```rust
/// Generate WHERE clause SQL for a specific database.
pub trait WhereClauseGenerator {
    /// Generate WHERE clause SQL and parameter bindings.
    ///
    /// # Arguments
    ///
    /// * `where_clause` - WHERE clause AST
    /// * `bindings` - Type bindings from CompiledSchema
    ///
    /// # Returns
    ///
    /// Tuple of (SQL string, parameter values).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if WHERE clause is invalid.
    fn generate(
        &self,
        where_clause: &WhereClause,
        bindings: &TypeBindings,
    ) -> Result<(String, Vec<QueryParameter>)>;

    /// Get database type for this generator.
    fn database_type(&self) -> DatabaseType;
}

/// Query parameter for SQL binding.
#[derive(Debug, Clone)]
pub enum QueryParameter {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    Json(serde_json::Value),
}
```

**Design Decisions:**

1. **Returns tuple**: SQL + parameters (not PreparedStatement - adapter handles that)
2. **Type-safe parameters**: Enum instead of `Box<dyn Any>`
3. **Requires TypeBindings**: Needs schema metadata to generate JSONB paths

---

### 3. JsonbProjector Trait

**Purpose:** Project JSONB response to requested GraphQL fields with auth masking.

```rust
/// Project JSONB to GraphQL response with field selection + authorization.
pub trait JsonbProjector {
    /// Project JSONB value to GraphQL response.
    ///
    /// # Arguments
    ///
    /// * `jsonb` - Complete JSONB from database
    /// * `selection_set` - Requested fields from GraphQL query
    /// * `auth_mask` - Field-level authorization mask
    ///
    /// # Returns
    ///
    /// Projected JSON value with only requested + authorized fields.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` on projection failure.
    fn project(
        &self,
        jsonb: &serde_json::Value,
        selection_set: &SelectionSet,
        auth_mask: &AuthMask,
    ) -> Result<serde_json::Value>;

    /// Project array of JSONB values (batch operation).
    fn project_many(
        &self,
        jsonb_list: &[serde_json::Value],
        selection_set: &SelectionSet,
        auth_mask: &AuthMask,
    ) -> Result<Vec<serde_json::Value>> {
        jsonb_list
            .iter()
            .map(|jsonb| self.project(jsonb, selection_set, auth_mask))
            .collect()
    }
}
```

**Design Decisions:**

1. **Takes references**: No ownership transfer (avoid clones)
2. **Batch support**: `project_many` for multiple rows
3. **Synchronous**: No async needed (pure in-memory operation)

---

### 4. CacheBackend Trait

**Purpose:** Abstract over cache implementations (in-memory, Redis, etc.).

```rust
/// Cache backend for query results.
#[async_trait::async_trait]
pub trait CacheBackend: Send + Sync {
    /// Get cached value by key.
    async fn get(&self, key: &CacheKey) -> Result<Option<CachedValue>>;

    /// Set cached value with optional TTL.
    async fn set(&self, key: &CacheKey, value: &CachedValue, ttl: Option<Duration>) -> Result<()>;

    /// Delete cached value by key.
    async fn delete(&self, key: &CacheKey) -> Result<()>;

    /// Delete all cached values matching a pattern (for invalidation).
    async fn delete_pattern(&self, pattern: &str) -> Result<u64>;

    /// Get cache statistics.
    async fn stats(&self) -> Result<CacheStats>;
}

/// Cache key (hash of query + variables + tenant).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey(pub String);

/// Cached query result value.
#[derive(Debug, Clone)]
pub struct CachedValue {
    pub data: serde_json::Value,
    pub cached_at: std::time::Instant,
}

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: u64,
    pub memory_bytes: u64,
}
```

**Design Decisions:**

1. **Async trait**: Cache backends may be remote (Redis)
2. **Pattern deletion**: Supports invalidation cascades
3. **TTL support**: Configurable expiration
4. **Stats**: For monitoring/metrics

---

## Type Designs

### 1. WhereClause AST

**Purpose:** Type-safe representation of WHERE conditions.

```rust
/// WHERE clause abstract syntax tree.
#[derive(Debug, Clone, PartialEq)]
pub enum WhereClause {
    /// Single field condition.
    Field {
        /// JSONB path (e.g., ["email"] or ["posts", "title"])
        path: Vec<String>,
        /// Operator (e.g., "eq", "icontains", "gte")
        operator: WhereOperator,
        /// Value to compare against
        value: serde_json::Value,
    },

    /// Logical AND of multiple conditions.
    And(Vec<WhereClause>),

    /// Logical OR of multiple conditions.
    Or(Vec<WhereClause>),

    /// Logical NOT of a condition.
    Not(Box<WhereClause>),
}

/// WHERE operators (FraiseQL v1 compatibility).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WhereOperator {
    // Comparison
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,

    // Containment
    In,
    Nin,

    // String operators
    Contains,
    Icontains,
    Startswith,
    Istartswith,
    Endswith,
    Iendswith,
    Like,
    Ilike,

    // Null checks
    IsNull,

    // Array operators
    ArrayContains,
    ArrayContainedBy,
    ArrayOverlaps,
    LenEq,
    LenGt,
    LenLt,
    LenGte,
    LenLte,
    LenNeq,

    // Vector operators (pgvector)
    CosineDistance,
    L2Distance,
    L1Distance,
    HammingDistance,

    // Full-text search
    Matches,
    PlainQuery,
    PhraseQuery,
    WebsearchQuery,

    // Network operators
    IsIPv4,
    IsIPv6,
    IsPrivate,
    IsPublic,
    InSubnet,
    Overlaps,

    // JSONB operators
    StrictlyContains,

    // LTree (hierarchical)
    AncestorOf,
    DescendantOf,
    MatchesLquery,
}

impl WhereOperator {
    /// Parse operator from string (GraphQL input).
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "eq" => Ok(Self::Eq),
            "neq" => Ok(Self::Neq),
            "icontains" => Ok(Self::Icontains),
            // ... (complete mapping)
            _ => Err(FraiseQLError::validation(format!("Unknown operator: {s}"))),
        }
    }

    /// Check if operator requires array value.
    pub const fn expects_array(&self) -> bool {
        matches!(self, Self::In | Self::Nin)
    }

    /// Check if operator is case-insensitive.
    pub const fn is_case_insensitive(&self) -> bool {
        matches!(
            self,
            Self::Icontains | Self::Istartswith | Self::Iendswith | Self::Ilike
        )
    }
}
```

**Design Decisions:**

1. **Recursive enum**: `And`/`Or`/`Not` nest naturally
2. **Typed operators**: Enum prevents typos
3. **Path as Vec**: Supports nested JSONB paths
4. **Helper methods**: `expects_array()`, `is_case_insensitive()`

**Example Usage:**

```rust
// GraphQL: { email: { icontains: "example.com" } }
let where_clause = WhereClause::Field {
    path: vec!["email".to_string()],
    operator: WhereOperator::Icontains,
    value: json!("example.com"),
};

// GraphQL: { _and: [{ published: { eq: true } }, { views: { gte: 100 } }] }
let where_clause = WhereClause::And(vec![
    WhereClause::Field {
        path: vec!["published".to_string()],
        operator: WhereOperator::Eq,
        value: json!(true),
    },
    WhereClause::Field {
        path: vec!["views".to_string()],
        operator: WhereOperator::Gte,
        value: json!(100),
    },
]);
```

---

### 2. SelectionSet

**Purpose:** Represent requested GraphQL fields.

```rust
/// Selection set - which fields are requested in GraphQL query.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionSet {
    /// Fields in this selection set.
    pub fields: Vec<FieldSelection>,
}

/// A single field selection.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldSelection {
    /// Field name (e.g., "id", "email", "posts").
    pub name: String,

    /// Alias (if field was aliased in query).
    pub alias: Option<String>,

    /// Nested selection (for object/array fields).
    pub selection: FieldSelectionType,
}

/// Type of field selection.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldSelectionType {
    /// Leaf field (scalar) - no nested selection.
    Leaf,

    /// Object field with nested selection.
    Object(Box<SelectionSet>),

    /// Array field with nested selection.
    Array(Box<SelectionSet>),
}

impl SelectionSet {
    /// Check if field is selected.
    pub fn has_field(&self, name: &str) -> bool {
        self.fields.iter().any(|f| f.name == name)
    }

    /// Get nested selection for a field.
    pub fn get_nested_selection(&self, name: &str) -> Option<&SelectionSet> {
        self.fields.iter().find(|f| f.name == name).and_then(|f| match &f.selection {
            FieldSelectionType::Object(sel) | FieldSelectionType::Array(sel) => Some(sel.as_ref()),
            FieldSelectionType::Leaf => None,
        })
    }

    /// Get all selected field names (non-recursive).
    pub fn field_names(&self) -> Vec<&str> {
        self.fields.iter().map(|f| f.name.as_str()).collect()
    }
}
```

**Design Decisions:**

1. **Explicit leaf/object/array**: Type-safe distinction
2. **Alias support**: For GraphQL field aliasing
3. **Helper methods**: `has_field()`, `get_nested_selection()`
4. **No HashMap**: Linear search is fast for typical GraphQL queries (< 20 fields)

**Example Usage:**

```rust
// GraphQL: { id, email, posts { title } }
let selection = SelectionSet {
    fields: vec![
        FieldSelection {
            name: "id".to_string(),
            alias: None,
            selection: FieldSelectionType::Leaf,
        },
        FieldSelection {
            name: "email".to_string(),
            alias: None,
            selection: FieldSelectionType::Leaf,
        },
        FieldSelection {
            name: "posts".to_string(),
            alias: None,
            selection: FieldSelectionType::Array(Box::new(SelectionSet {
                fields: vec![FieldSelection {
                    name: "title".to_string(),
                    alias: None,
                    selection: FieldSelectionType::Leaf,
                }],
            })),
        },
    ],
};
```

---

### 3. AuthMask

**Purpose:** Represent field-level authorization rules.

```rust
/// Field-level authorization mask.
#[derive(Debug, Clone)]
pub struct AuthMask {
    /// Map of type name → field name → authorization rule.
    rules: HashMap<String, HashMap<String, FieldAuthRule>>,
}

/// Authorization rule for a single field.
#[derive(Debug, Clone)]
pub struct FieldAuthRule {
    /// Required roles to access this field.
    pub required_roles: Option<Vec<String>>,

    /// Required permissions to access this field.
    pub required_permissions: Option<Vec<String>>,

    /// Custom predicate (future: dynamic rules).
    pub custom_predicate: Option<String>,
}

impl AuthMask {
    /// Create empty auth mask (allow all).
    pub fn allow_all() -> Self {
        Self {
            rules: HashMap::new(),
        }
    }

    /// Create auth mask from CompiledSchema authorization metadata.
    pub fn from_schema(schema: &CompiledSchema, user_context: &UserContext) -> Self {
        // Build mask based on user's roles/permissions
        todo!("Implement from schema authorization metadata")
    }

    /// Check if field is authorized for user.
    pub fn is_field_authorized(&self, type_name: &str, field_name: &str, user: &UserContext) -> bool {
        // Look up rule for type.field
        let Some(type_rules) = self.rules.get(type_name) else {
            return true; // No rules = allow
        };

        let Some(field_rule) = type_rules.get(field_name) else {
            return true; // No rule for field = allow
        };

        // Check required roles
        if let Some(required_roles) = &field_rule.required_roles {
            if !required_roles.iter().any(|role| user.has_role(role)) {
                return false;
            }
        }

        // Check required permissions
        if let Some(required_perms) = &field_rule.required_permissions {
            if !required_perms.iter().any(|perm| user.has_permission(perm)) {
                return false;
            }
        }

        true
    }
}

/// User context (roles, permissions, tenant).
#[derive(Debug, Clone)]
pub struct UserContext {
    pub user_id: Option<String>,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub tenant_id: Option<String>,
}

impl UserContext {
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }
}
```

**Design Decisions:**

1. **HashMap for rules**: Fast O(1) lookup per type/field
2. **Role + Permission support**: Flexible RBAC
3. **Allow-by-default**: Missing rules allow access (fail-open for non-sensitive fields)
4. **Lazy evaluation**: Only check rules for requested fields

---

## WHERE Clause Generation

### Algorithm: AST → SQL

**Goal:** Convert `WhereClause` AST to database-specific SQL + parameters.

**Example:**

```rust
// Input AST:
WhereClause::And(vec![
    WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Icontains,
        value: json!("example.com"),
    },
    WhereClause::Field {
        path: vec!["posts", "title"],
        operator: WhereOperator::Contains,
        value: json!("GraphQL"),
    },
])

// PostgreSQL output:
(
    "data->>'email' ILIKE $1 AND EXISTS (SELECT 1 FROM jsonb_array_elements(data->'posts') AS p WHERE p->>'title' LIKE $2)",
    vec![
        QueryParameter::String("%example.com%"),
        QueryParameter::String("%GraphQL%"),
    ]
)
```

### PostgreSQL WHERE Generator

```rust
pub struct PostgresWhereGenerator;

impl WhereClauseGenerator for PostgresWhereGenerator {
    fn generate(
        &self,
        where_clause: &WhereClause,
        bindings: &TypeBindings,
    ) -> Result<(String, Vec<QueryParameter>)> {
        let mut sql = String::with_capacity(256);
        let mut params = Vec::new();
        let mut param_counter = 1;

        self.generate_recursive(where_clause, bindings, &mut sql, &mut params, &mut param_counter)?;

        Ok((sql, params))
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }
}

impl PostgresWhereGenerator {
    fn generate_recursive(
        &self,
        clause: &WhereClause,
        bindings: &TypeBindings,
        sql: &mut String,
        params: &mut Vec<QueryParameter>,
        param_counter: &mut usize,
    ) -> Result<()> {
        match clause {
            WhereClause::Field { path, operator, value } => {
                self.generate_field_condition(path, operator, value, sql, params, param_counter)?;
            }

            WhereClause::And(clauses) => {
                sql.push('(');
                for (i, clause) in clauses.iter().enumerate() {
                    if i > 0 {
                        sql.push_str(" AND ");
                    }
                    self.generate_recursive(clause, bindings, sql, params, param_counter)?;
                }
                sql.push(')');
            }

            WhereClause::Or(clauses) => {
                sql.push('(');
                for (i, clause) in clauses.iter().enumerate() {
                    if i > 0 {
                        sql.push_str(" OR ");
                    }
                    self.generate_recursive(clause, bindings, sql, params, param_counter)?;
                }
                sql.push(')');
            }

            WhereClause::Not(clause) => {
                sql.push_str("NOT (");
                self.generate_recursive(clause, bindings, sql, params, param_counter)?;
                sql.push(')');
            }
        }

        Ok(())
    }

    fn generate_field_condition(
        &self,
        path: &[String],
        operator: &WhereOperator,
        value: &serde_json::Value,
        sql: &mut String,
        params: &mut Vec<QueryParameter>,
        param_counter: &mut usize,
    ) -> Result<()> {
        // Handle nested paths (e.g., ["posts", "title"])
        if path.len() > 1 {
            // Nested field - use EXISTS with jsonb_array_elements
            self.generate_nested_condition(path, operator, value, sql, params, param_counter)?;
            return Ok(());
        }

        // Simple field - direct JSONB path
        let field = &path[0];
        let jsonb_path = format!("data->>'{field}'");

        match operator {
            WhereOperator::Eq => {
                sql.push_str(&format!("{jsonb_path} = ${param_counter}"));
                params.push(QueryParameter::from_json(value)?);
                *param_counter += 1;
            }

            WhereOperator::Icontains => {
                sql.push_str(&format!("{jsonb_path} ILIKE ${param_counter}"));
                let pattern = format!("%{}%", value.as_str().unwrap_or(""));
                params.push(QueryParameter::String(pattern));
                *param_counter += 1;
            }

            WhereOperator::Gte => {
                sql.push_str(&format!("({jsonb_path})::numeric >= ${param_counter}"));
                params.push(QueryParameter::from_json(value)?);
                *param_counter += 1;
            }

            // ... (implement all operators)

            _ => {
                return Err(FraiseQLError::validation(format!(
                    "Unsupported operator: {operator:?}"
                )));
            }
        }

        Ok(())
    }

    fn generate_nested_condition(
        &self,
        path: &[String],
        operator: &WhereOperator,
        value: &serde_json::Value,
        sql: &mut String,
        params: &mut Vec<QueryParameter>,
        param_counter: &mut usize,
    ) -> Result<()> {
        // Example: path = ["posts", "title"], operator = Contains
        // Generate: EXISTS (SELECT 1 FROM jsonb_array_elements(data->'posts') AS p WHERE p->>'title' LIKE '%value%')

        let parent_field = &path[0];
        let nested_field = &path[1];

        sql.push_str(&format!(
            "EXISTS (SELECT 1 FROM jsonb_array_elements(data->'{}') AS nested WHERE nested->>'{}' ",
            parent_field, nested_field
        ));

        // Add operator
        match operator {
            WhereOperator::Contains => {
                sql.push_str(&format!("LIKE ${param_counter}"));
                let pattern = format!("%{}%", value.as_str().unwrap_or(""));
                params.push(QueryParameter::String(pattern));
                *param_counter += 1;
            }

            WhereOperator::Eq => {
                sql.push_str(&format!("= ${param_counter}"));
                params.push(QueryParameter::from_json(value)?);
                *param_counter += 1;
            }

            // ... (other operators)

            _ => {
                return Err(FraiseQLError::validation(format!(
                    "Unsupported nested operator: {operator:?}"
                )));
            }
        }

        sql.push(')');

        Ok(())
    }
}

impl QueryParameter {
    fn from_json(value: &serde_json::Value) -> Result<Self> {
        match value {
            serde_json::Value::String(s) => Ok(Self::String(s.clone())),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Self::Int(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Self::Float(f))
                } else {
                    Err(FraiseQLError::validation("Invalid number"))
                }
            }
            serde_json::Value::Bool(b) => Ok(Self::Bool(*b)),
            serde_json::Value::Null => Ok(Self::Null),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                Ok(Self::Json(value.clone()))
            }
        }
    }
}
```

**Design Decisions:**

1. **Recursive generation**: Handles arbitrary nesting depth
2. **Parameter binding**: All values are parameterized (SQL injection safe)
3. **Nested EXISTS**: For array field filters
4. **Type casting**: `::numeric`, `::boolean` for PostgreSQL

---

## JSONB Projection Architecture

### Algorithm: JSONB → GraphQL Response

**Goal:** Extract only requested fields from complete JSONB, applying auth masking.

**Example:**

```rust
// Input JSONB (from database):
{
  "id": "user-123",
  "email": "alice@example.com",
  "name": "Alice",
  "posts": [
    {"id": "p1", "title": "Post 1", "body": "...", "author": {...}},
    {"id": "p2", "title": "Post 2", "body": "...", "author": {...}}
  ],
  "password_hash": "$2a$10$...",
  "internal_notes": "VIP customer"
}

// SelectionSet:
{ id, email, posts { title } }

// AuthMask:
password_hash: requires_roles = ["admin"]
internal_notes: requires_roles = ["admin", "support"]

// User:
roles = ["viewer"]

// Output:
{
  "id": "user-123",
  "email": "alice@example.com",
  "posts": [
    {"title": "Post 1"},
    {"title": "Post 2"}
  ]
}
```

### DefaultJsonbProjector Implementation

```rust
pub struct DefaultJsonbProjector {
    type_name: String,  // For auth lookups
}

impl JsonbProjector for DefaultJsonbProjector {
    fn project(
        &self,
        jsonb: &serde_json::Value,
        selection_set: &SelectionSet,
        auth_mask: &AuthMask,
    ) -> Result<serde_json::Value> {
        let mut result = serde_json::Map::new();

        for field_selection in &selection_set.fields {
            // Check authorization
            if !auth_mask.is_field_authorized(&self.type_name, &field_selection.name, &UserContext::default()) {
                // Field not authorized - skip (silent omission)
                continue;
            }

            // Extract field value from JSONB
            let field_value = jsonb.get(&field_selection.name);

            if field_value.is_none() {
                // Field not in JSONB - skip or return null?
                result.insert(field_selection.name.clone(), serde_json::Value::Null);
                continue;
            }

            let field_value = field_value.unwrap();

            // Handle nested selection
            let projected_value = match &field_selection.selection {
                FieldSelectionType::Leaf => {
                    // Scalar field - return as-is
                    field_value.clone()
                }

                FieldSelectionType::Object(nested_selection) => {
                    // Object field - recurse
                    if !field_value.is_object() {
                        return Err(FraiseQLError::internal(format!(
                            "Expected object for field '{}', got {:?}",
                            field_selection.name, field_value
                        )));
                    }

                    self.project(field_value, nested_selection, auth_mask)?
                }

                FieldSelectionType::Array(nested_selection) => {
                    // Array field - project each element
                    if !field_value.is_array() {
                        return Err(FraiseQLError::internal(format!(
                            "Expected array for field '{}', got {:?}",
                            field_selection.name, field_value
                        )));
                    }

                    let array = field_value.as_array().unwrap();
                    let projected_array: Result<Vec<_>> = array
                        .iter()
                        .map(|item| self.project(item, nested_selection, auth_mask))
                        .collect();

                    serde_json::Value::Array(projected_array?)
                }
            };

            // Use alias if present
            let output_name = field_selection.alias.as_ref().unwrap_or(&field_selection.name);
            result.insert(output_name.clone(), projected_value);
        }

        Ok(serde_json::Value::Object(result))
    }
}
```

**Design Decisions:**

1. **Recursive projection**: Handles nested objects/arrays
2. **Silent auth failures**: Unauthorized fields are omitted (not errors)
3. **Clone values**: Simplicity over zero-copy (profile later)
4. **Alias support**: Output field name respects GraphQL aliases

**Performance Optimization (Future):**

```rust
// Zero-copy projection using serde_json's Value borrowing
// Requires careful lifetime management
pub fn project_borrowed<'a>(
    jsonb: &'a serde_json::Value,
    selection_set: &SelectionSet,
    auth_mask: &AuthMask,
) -> Result<Cow<'a, serde_json::Value>> {
    // Return Cow::Borrowed when no filtering needed
    // Return Cow::Owned when projection required
    todo!("Implement zero-copy projection")
}
```

---

## Authorization Strategy

### Two-Level Authorization

**1. Query-Level Authorization:**

- Enforced BEFORE database query execution
- Checks if user can execute query at all
- Returns 403 error if unauthorized

**2. Field-Level Authorization:**

- Enforced AFTER database query execution
- Filters fields from JSONB response
- Silently omits unauthorized fields

### Field-Level Auth Rules (from CompiledSchema)

```json
// In CompiledSchema JSON:
{
  "authorization": {
    "User": {
      "password_hash": {
        "requires_roles": ["admin"]
      },
      "internal_notes": {
        "requires_roles": ["admin", "support"]
      },
      "ssn": {
        "requires_permissions": ["pii:read"]
      }
    }
  }
}
```

### AuthMask Generation

```rust
impl AuthMask {
    pub fn from_schema(schema: &CompiledSchema, user: &UserContext) -> Self {
        let mut rules = HashMap::new();

        // Iterate through schema authorization rules
        for (type_name, type_auth) in &schema.authorization {
            let mut type_rules = HashMap::new();

            for (field_name, field_auth) in type_auth {
                // Check if user has required roles/permissions
                let authorized = Self::check_field_auth(field_auth, user);

                if !authorized {
                    // User NOT authorized for this field - add to mask
                    type_rules.insert(field_name.clone(), field_auth.clone());
                }
            }

            if !type_rules.is_empty() {
                rules.insert(type_name.clone(), type_rules);
            }
        }

        Self { rules }
    }

    fn check_field_auth(field_auth: &FieldAuthRule, user: &UserContext) -> bool {
        // Check required roles
        if let Some(required_roles) = &field_auth.required_roles {
            if !required_roles.iter().any(|role| user.has_role(role)) {
                return false;
            }
        }

        // Check required permissions
        if let Some(required_perms) = &field_auth.required_permissions {
            if !required_perms.iter().any(|perm| user.has_permission(perm)) {
                return false;
            }
        }

        true
    }
}
```

**Design Decisions:**

1. **Pre-computed mask**: Build once per request, not per field
2. **Fail-closed**: Explicit allow required (missing auth = deny)
3. **Role OR semantics**: Any matching role grants access
4. **Silent omission**: Unauthorized fields don't error, just vanish

---

## Connection Pooling

### Deadpool-based Implementation

**Choice:** Use `deadpool` (battle-tested, good ergonomics).

```rust
use deadpool::managed::{Manager, Pool, PoolError};
use tokio_postgres::{Client, Config, NoTls};

pub struct PostgresManager {
    config: Config,
}

#[async_trait::async_trait]
impl Manager for PostgresManager {
    type Type = Client;
    type Error = tokio_postgres::Error;

    async fn create(&self) -> Result<Client, Self::Error> {
        let (client, connection) = self.config.connect(NoTls).await?;

        // Spawn connection handler
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Connection error: {e}");
            }
        });

        Ok(client)
    }

    async fn recycle(&self, client: &mut Client, _: &Metrics) -> RecycleResult<Self::Error> {
        // Health check query
        client.query_one("SELECT 1", &[]).await?;
        Ok(())
    }
}

pub type PostgresPool = Pool<PostgresManager>;

pub fn create_postgres_pool(database_url: &str, max_size: usize) -> Result<PostgresPool> {
    let config = database_url.parse::<Config>()?;
    let manager = PostgresManager { config };

    Pool::builder(manager)
        .max_size(max_size)
        .build()
        .map_err(|e| FraiseQLError::database(format!("Failed to create pool: {e}")))
}
```

**Design Decisions:**

1. **Deadpool**: Production-ready, supports metrics
2. **Health check**: Recycle validates connection before reuse
3. **Spawn connection handler**: Required for tokio-postgres
4. **Configurable max_size**: Tunable for deployment

---

## Caching Architecture

### In-Memory Cache (Default)

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use lru::LruCache;

pub struct MemoryCache {
    cache: Arc<RwLock<LruCache<CacheKey, CachedValue>>>,
    stats: Arc<RwLock<CacheStats>>,
}

#[async_trait::async_trait]
impl CacheBackend for MemoryCache {
    async fn get(&self, key: &CacheKey) -> Result<Option<CachedValue>> {
        let mut cache = self.cache.write().await;
        let value = cache.get(key).cloned();

        // Update stats
        let mut stats = self.stats.write().await;
        if value.is_some() {
            stats.hits += 1;
        } else {
            stats.misses += 1;
        }

        Ok(value)
    }

    async fn set(&self, key: &CacheKey, value: &CachedValue, _ttl: Option<Duration>) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.put(key.clone(), value.clone());

        // Update stats
        let mut stats = self.stats.write().await;
        stats.entries = cache.len() as u64;

        Ok(())
    }

    async fn delete(&self, key: &CacheKey) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.pop(key);
        Ok(())
    }

    async fn delete_pattern(&self, pattern: &str) -> Result<u64> {
        let mut cache = self.cache.write().await;
        let mut deleted = 0;

        // Collect keys matching pattern
        let keys_to_delete: Vec<_> = cache
            .iter()
            .filter(|(k, _)| k.0.contains(pattern))
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_delete {
            cache.pop(&key);
            deleted += 1;
        }

        Ok(deleted)
    }

    async fn stats(&self) -> Result<CacheStats> {
        Ok(self.stats.read().await.clone())
    }
}
```

**Design Decisions:**

1. **LRU eviction**: Automatic size management
2. **RwLock**: Concurrent reads, exclusive writes
3. **Stats tracking**: For monitoring
4. **Pattern deletion**: Simple string contains (improve later)

### Cache Key Generation

```rust
pub fn generate_cache_key(
    query: &str,
    variables: &serde_json::Value,
    tenant_id: Option<&str>,
) -> CacheKey {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(query.as_bytes());
    hasher.update(variables.to_string().as_bytes());
    if let Some(tenant) = tenant_id {
        hasher.update(tenant.as_bytes());
    }

    let hash = hasher.finalize();
    CacheKey(format!("query:{:x}", hash))
}
```

---

## Error Handling

**Already complete** (from Phase 1). See `error.rs`.

**Additional database error conversions:**

```rust
impl From<tokio_postgres::Error> for FraiseQLError {
    fn from(e: tokio_postgres::Error) -> Self {
        let sql_state = e.code().map(|c| c.code().to_string());
        Self::Database {
            message: e.to_string(),
            sql_state,
        }
    }
}

impl From<deadpool::managed::PoolError<tokio_postgres::Error>> for FraiseQLError {
    fn from(e: deadpool::managed::PoolError<tokio_postgres::Error>) -> Self {
        Self::ConnectionPool {
            message: e.to_string(),
        }
    }
}
```

---

## Testing Strategy

### Unit Tests

**1. WHERE Clause Generation Tests:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_eq_where() {
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value: json!("alice@example.com"),
        };

        let gen = PostgresWhereGenerator;
        let (sql, params) = gen.generate(&clause, &TypeBindings::default()).unwrap();

        assert_eq!(sql, "data->>'email' = $1");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_nested_where() {
        let clause = WhereClause::Field {
            path: vec!["posts".to_string(), "title".to_string()],
            operator: WhereOperator::Contains,
            value: json!("GraphQL"),
        };

        let gen = PostgresWhereGenerator;
        let (sql, params) = gen.generate(&clause, &TypeBindings::default()).unwrap();

        assert!(sql.contains("EXISTS"));
        assert!(sql.contains("jsonb_array_elements"));
    }
}
```

**2. JSONB Projection Tests:**

```rust
#[test]
fn test_simple_projection() {
    let jsonb = json!({
        "id": "123",
        "email": "alice@example.com",
        "password_hash": "secret"
    });

    let selection = SelectionSet {
        fields: vec![
            FieldSelection {
                name: "id".to_string(),
                alias: None,
                selection: FieldSelectionType::Leaf,
            },
            FieldSelection {
                name: "email".to_string(),
                alias: None,
                selection: FieldSelectionType::Leaf,
            },
        ],
    };

    let auth_mask = AuthMask::allow_all();
    let projector = DefaultJsonbProjector { type_name: "User".to_string() };

    let result = projector.project(&jsonb, &selection, &auth_mask).unwrap();

    assert_eq!(result["id"], "123");
    assert_eq!(result["email"], "alice@example.com");
    assert!(result.get("password_hash").is_none()); // Not requested
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_end_to_end_query() {
    // Setup test database
    let pool = create_test_postgres_pool().await;

    // Insert test data
    setup_test_data(&pool).await;

    // Create adapter
    let adapter = PostgresAdapter::new(pool);

    // Build WHERE clause
    let where_clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Icontains,
        value: json!("example.com"),
    };

    // Execute query
    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None)
        .await
        .unwrap();

    assert_eq!(results.len(), 3);
}
```

---

## Performance Optimizations

### 1. Connection Pooling

- Pre-warmed pool on startup
- Configurable pool size
- Health checks on recycle

### 2. JSONB Projection

- **Current**: Clone-based (simple, correct)
- **Future**: Cow-based borrowing for zero-copy
- **Benchmark**: Profile before optimizing

### 3. WHERE Clause Generation

- Pre-allocate String capacity (256 bytes)
- Reuse parameter Vec
- Avoid string concatenation (use `write!` macro)

### 4. Caching

- LRU cache prevents unbounded growth
- TTL support for time-based invalidation
- Pattern-based invalidation for mutations

---

## Trade-off Analysis

### 1. JSONB Parsing: serde_json vs Custom

**Decision: Use serde_json::Value**

**Pros:**

- ✅ Battle-tested, full JSON spec compliance
- ✅ Rich API (as_array, as_object, get, etc.)
- ✅ Interop with entire Rust JSON ecosystem

**Cons:**

- ❌ Allocates for every field access
- ❌ Not optimized for projection use case

**Justification:**

Start with `serde_json`. Profile. If projection is a bottleneck (unlikely given database I/O dominates), consider zero-copy alternatives:

- `simd-json` for faster parsing
- Custom JSONB reader for PostgreSQL wire format
- Cow-based borrowing

**Metrics to watch:**

- Projection time > 10% of total request time → optimize
- Memory allocations > 1MB per request → optimize

---

### 2. WHERE Builder: AST vs String

**Decision: Use AST (WhereClause enum)**

**Pros:**

- ✅ Type-safe (no SQL injection)
- ✅ Composable (can analyze/optimize before generation)
- ✅ Database-agnostic (same AST for all databases)

**Cons:**

- ❌ More code than string concatenation
- ❌ Extra allocation for AST nodes

**Justification:**

Safety > convenience. WHERE clauses are complex (nested AND/OR/NOT), and an AST makes correctness provable. The performance cost is negligible compared to database I/O.

**Metrics to watch:**

- WHERE generation time > 1% of total request time → acceptable

---

### 3. Connection Pool: deadpool vs Custom

**Decision: Use deadpool**

**Pros:**

- ✅ Production-ready (used by thousands)
- ✅ Good metrics/monitoring
- ✅ Generic (works with any database driver)

**Cons:**

- ❌ Generic abstraction may have minor overhead
- ❌ Less control over internals

**Justification:**

Reinventing connection pooling is a waste of time. `deadpool` handles edge cases (connection drops, health checks, backpressure) better than we would in a first implementation.

**When to reconsider:**

- If we need database-specific pooling features (PostgreSQL prepared statements)
- If metrics show pool is a bottleneck (unlikely)

---

### 4. Auth Mask: HashMap vs BitSet

**Decision: Use HashMap<String, HashMap<String, FieldAuthRule>>**

**Pros:**

- ✅ Simple, readable
- ✅ O(1) lookups
- ✅ Flexible (supports complex rules)

**Cons:**

- ❌ Higher memory usage than BitSet
- ❌ String comparisons (slower than bit operations)

**Justification:**

GraphQL schemas typically have < 100 types with < 50 fields each. HashMap overhead is negligible. BitSet would require:

1. Assigning integer IDs to all type/field combinations
2. Maintaining bidirectional mapping (ID ↔ name)
3. More complex code

**When to reconsider:**

- Schemas with > 1000 types
- Auth checks > 10% of request time (profile first)

---

## Migration Plan

### Database Layer + WHERE Generation (6 days)

**Day 1-2: Database Abstraction**

- [ ] Implement `DatabaseAdapter` trait
- [ ] Implement `PostgresAdapter` with deadpool
- [ ] Write unit tests for adapter

**Day 3-4: WHERE Clause Generation**

- [ ] Implement `WhereClause` AST types
- [ ] Implement `PostgresWhereGenerator`
- [ ] Support all v1 operators (eq, icontains, etc.)
- [ ] Handle nested JSONB paths
- [ ] Write comprehensive WHERE generation tests

**Day 5-6: Integration & Testing**

- [ ] End-to-end integration tests
- [ ] Benchmark WHERE generation
- [ ] Add MySQL/SQLite adapters (basic)

**Acceptance Criteria:**

- ✅ Execute `SELECT data FROM v_user WHERE ...` queries
- ✅ All v1 WHERE operators supported
- ✅ 90%+ test coverage for WHERE generation
- ✅ Performance: < 1ms WHERE generation overhead

---

### Security Layer (2 days)

**Day 1: Field-Level Auth**

- [ ] Implement `AuthMask` type
- [ ] Implement `from_schema()` to build mask from CompiledSchema
- [ ] Write unit tests for auth mask generation

**Day 2: Integration**

- [ ] Integrate auth mask into JSONB projection
- [ ] Add auth tests (admin vs viewer scenarios)
- [ ] Document auth rule format

**Acceptance Criteria:**

- ✅ Unauthorized fields silently omitted
- ✅ Role + permission support
- ✅ Auth tests cover all scenarios

---

### Compiler Infrastructure (10-12 days)

**Out of scope for this architecture document.** See separate compiler design.

---

### Runtime Executor + JSONB Projection (12-15 days)

**Day 1-3: JSONB Projection**

- [ ] Implement `SelectionSet` types
- [ ] Implement `DefaultJsonbProjector`
- [ ] Handle nested objects/arrays
- [ ] Support field aliasing
- [ ] Write projection unit tests

**Day 4-6: Runtime Executor**

- [ ] Implement `Executor` type
- [ ] Query execution pipeline:
  1. Parse GraphQL query
  2. Extract WHERE clause
  3. Execute database query (via adapter)
  4. Project JSONB (via projector)
  5. Apply auth mask
  6. Return response
- [ ] Write executor integration tests

**Day 7-9: Caching Integration**

- [ ] Implement `MemoryCache`
- [ ] Implement cache key generation
- [ ] Integrate cache into executor pipeline
- [ ] Add cache invalidation hooks
- [ ] Write cache tests

**Day 10-12: Optimization & Benchmarks**

- [ ] Profile projection performance
- [ ] Optimize hot paths
- [ ] Add performance benchmarks
- [ ] Compare against v1 (if available)

**Acceptance Criteria:**

- ✅ End-to-end query execution working
- ✅ JSONB projection accurate
- ✅ Field-level auth enforced
- ✅ Caching working (95%+ hit rate on repeated queries)
- ✅ Performance: p99 < 10ms for simple queries

---

## Summary

This architecture design provides:

1. ✅ **Simplified execution model** (views do JOINs, Rust does projection)
2. ✅ **Type-safe WHERE generation** (AST-based, SQL injection proof)
3. ✅ **Efficient JSONB projection** (recursive, auth-aware)
4. ✅ **Battle-tested connection pooling** (deadpool)
5. ✅ **Flexible caching** (in-memory default, extensible)
6. ✅ **Clear module structure** (db → runtime → cache → security)
7. ✅ **Comprehensive testing strategy** (unit + integration + benchmarks)

**Ready for implementation.**

**Next Steps:**

1. Review this architecture document
2. Get approval/feedback
3. Begin Phase 2 implementation
4. Iterate based on real-world testing

---

**End of Architecture Document**
