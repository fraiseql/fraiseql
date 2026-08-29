//! GraphQL query execution engine.
//!
//! This module transforms a parsed GraphQL query into parameterized SQL,
//! applies row-level security (RLS) policies, injects server-side context parameters,
//! and executes the resulting query against a database adapter.
//!
//! # Architecture Overview
//!
//! Execution follows a three-phase model:
//!
//! ## 1. Preparation Phase — Classify and Validate
//! The `classify_query()` method determines the operation type:
//! - **Regular queries**: Standard field selections (e.g., `{ users { id name } }`)
//! - **Mutations**: Write operations (e.g., `mutation { createUser(...) { id } }`)
//! - **Aggregate queries**: Analytics (e.g., `sales_aggregate { total revenue }`)
//! - **Window queries**: Time-series (e.g., `sales_window { hourly average }`)
//! - **Federation queries**: GraphQL federation support (`_service`, `_entities`)
//! - **Introspection**: Schema introspection (`__schema`, `__type`)
//! - **Relay node**: Global ID lookup (`.node(id: "...")`)
//!
//! For each query type, validation occurs:
//! - Check schema has the requested field
//! - Validate field types and arguments
//! - Resolve `@inject` parameters from JWT claims (if present)
//! - Check field-level access control (if enabled)
//!
//! ## 2. SQL Generation Phase — Build Parameterized SQL
//! The `QueryPlanner` builds parameterized SQL:
//! - Generate `WHERE` clauses from GraphQL filter arguments
//! - Apply row-level security (RLS) WHERE clauses (always AND-ed with application WHERE)
//! - Generate `ORDER BY` and `LIMIT`/`OFFSET` clauses
//! - For mutations: dispatch to stored procedure or table mutation function
//! - Inject server-side context as query parameters
//! - Generate SQL field projections for optimization (40-55% network reduction)
//!
//! All user input (variables, WHERE operators) is sent as prepared statement parameters.
//! **Zero SQL string concatenation for regular queries and mutations** — complete
//! protection against SQL injection on the standard execution paths.
//!
//! > **Note on window queries**: Window queries use `execute_parameterized_aggregate`
//! > with `$N`/`?`/`@P1` bind parameters, matching the aggregate query path.
//! > Column names in `PARTITION BY` / `ORDER BY` are schema-derived and validated
//! > against `WindowAllowlist` — they are not user-controlled at runtime.
//!
//! ## 3. Execution Phase — Run and Process Results
//! The `DatabaseAdapter` executes the parameterized SQL:
//! - Execute parameterized SQL against the database
//! - For queries: parse rows into GraphQL response format
//! - For mutations: parse mutation result, populate error fields, compute cascade effects
//! - Return typed result as JSON or error
//!
//! # Security Properties
//!
//! ## Row-Level Security (RLS)
//! User's RLS WHERE clause is **always AND-ed** (never OR-ed) with other WHERE conditions.
//! RLS always wins — no user input can bypass it.
//!
//! Example:
//! - Application WHERE: `email LIKE '%example.com%'`
//! - User's RLS: `tenant_id = 'tenant-123'`
//! - Effective WHERE: `email LIKE '%example.com%' AND tenant_id = 'tenant-123'`
//!
//! ## Injection Guards
//! `@inject` parameters require a `SecurityContext` with decoded JWT claims.
//! If a query has inject params but no auth context, the query fails immediately
//! with `FraiseQLError::Validation`.
//!
//! Example:
//! ```python
//! @fraiseql.query(inject={"userId": "jwt:sub"})
//! def current_user(userId: str) -> User:
//!     pass
//! ```
//! → If no JWT provided: **Validation error** (no unauthenticated execution possible)
//!
//! ## Parameterization
//! All user input is sent as query parameters to the database driver:
//! - GraphQL variables → prepared statement parameters
//! - WHERE operators (`eq`, `like`, `in`) → parameterized operators
//! - Inject values → bound parameters
//!
//! **No string concatenation for regular queries and mutations** — SQL injection is
//! prevented at the driver level. Aggregate and window queries escape values in-process
//! before embedding them in the SQL string; see the note in the SQL Generation section.
//!
//! ## APQ Cache Isolation
//! Automatic Persisted Query (APQ) cache keys include:
//! - Query operation (not just query string)
//! - All GraphQL variables
//! - Schema version
//! - User's RLS policy (via `SecurityContext`)
//!
//! Different users with different RLS policies generate different cache entries.
//! Cache isolation is **automatic and correct by design**.
//!
//! # Performance Characteristics
//!
//! ## Latency
//! - **Cold read** (cache miss): ~5-15ms (PostgreSQL local)
//! - **Cache hit**: <1ms (in-memory lookup + serialization)
//! - **Mutation**: ~10-50ms (depends on cascade complexity)
//! - **Relay pagination**: ~15-30ms (keyset cursor on PostgreSQL)
//!
//! ## Throughput
//! - Cached queries: 10,000+ QPS per executor instance
//! - Non-cached queries: 250+ Kelem/s (elements per second)
//! - Connection pooling: Default 20 connections per database
//!
//! ## Memory
//! - APQ cache: Configurable, default 100MB LRU
//! - Query plans: Cached and reused, minimal overhead
//! - Executor: ~5-10MB overhead per instance
//!
//! # Query Timeout and Cancellation
//!
//! Queries are protected from long-running operations through the `query_timeout_ms`
//! configuration in `RuntimeConfig`. When a query exceeds this timeout, the operation
//! is cancelled via `tokio::time::timeout()`, which aborts the future.
//!
//! - **Default timeout**: 30 seconds
//! - **No timeout**: Set `query_timeout_ms` to 0
//! - **Custom timeout**: Set `query_timeout_ms` to desired milliseconds
//!
//! For graceful shutdown of long-running tasks, callers can wrap `execute()` calls
//! with their own `tokio::time::timeout()` or use `tokio_util::task::AbortOnDrop`
//! for task lifecycle management.
//!
//! # Example Usage
//!
//! ```no_run
//! // Requires: a live PostgreSQL database with fraiseql schema.
//! // See: tests/integration/ for runnable examples.
//! use fraiseql_core::runtime::Executor;
//! use fraiseql_core::schema::CompiledSchema;
//! use fraiseql_core::db::postgres::PostgresAdapter;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let schema_json = r#"{"types":[],"queries":[]}"#;
//! // Load compiled schema and create adapter
//! let schema = CompiledSchema::from_json(schema_json, false)?;
//! let adapter = PostgresAdapter::new("postgresql://localhost/mydb").await?;
//!
//! // Create executor
//! let executor = Executor::new(schema, std::sync::Arc::new(adapter));
//!
//! // Execute a query
//! let query = r#"{ users(limit: 10) { id name email } }"#;
//! let result = executor.execute(query, None).await?;
//! println!("Result: {}", result);
//! # Ok(())
//! # }
//! ```
//!
//! # See Also
//!
//! - `Executor` — Main entry point for query execution
//! - `QueryPlanner` — Converts GraphQL to parameterized SQL
//! - `DatabaseAdapter` — Trait for database-specific implementations
//! - `FraiseQLError` — Error types

mod context;

#[cfg(test)]
use std::{sync::Arc, time::Duration};

#[cfg(test)]
use crate::runtime::ExecutionContext;
use crate::{
    error::{FraiseQLError, Result},
    schema::InjectedParamSource,
    security::SecurityContext,
};

mod core;
pub use core::Executor;

/// Projected response rows delivered as the database produces them (#958).
///
/// What [`Executor::stream_query_direct`] returns: the same rows a buffered read
/// of the same query would put inside its `data` envelope, one at a time and
/// without the envelope, for a representation that has no end to wrap.
pub type JsonRowStream =
    std::pin::Pin<Box<dyn futures::Stream<Item = crate::error::Result<serde_json::Value>> + Send>>;

mod execution;
mod mutation;
mod runners;

/// One definition of "is this argument a row count", re-exported so every
/// transport that reads `limit`/`offset`/`first`/`last` fails closed the same
/// way (#1197). A second, lenient reading of the same argument is how the
/// bound got dropped in the first place.
pub use runners::query_params::coerce_pagination_arg;
/// One definition of "what cast does a comparison against this field need",
/// re-exported to the schema layer so `where_keys_of` types a nested key with the
/// same function that types a top-level one (#1157). Two mappings would drift, and
/// the drift would be silent: a filter that returns the wrong rows.
pub use runners::query_projection::field_type_to_where_type;
pub mod security;
mod support;

// Re-export pipeline at this level so `runtime/mod.rs` can use `executor::pipeline::*`.
pub use support::pipeline;

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
mod tests;

/// Query type classification for routing.
#[derive(Debug, Clone, PartialEq)]
enum QueryType {
    /// Regular GraphQL query (non-analytics).
    Regular,

    /// Aggregate analytics query (ends with _aggregate).
    /// Contains the full query name (e.g., "`sales_aggregate`").
    Aggregate(String),

    /// Window function query (ends with _window).
    /// Contains the full query name (e.g., "`sales_window`").
    Window(String),

    /// Federation query (_service or _entities).
    /// Contains the query name ("_service" or "_entities").
    Federation(String),

    /// Introspection query (`__schema`).
    IntrospectionSchema,

    /// Introspection query (`__type(name: "...")`).
    /// Contains the requested type name.
    IntrospectionType(String),

    /// GraphQL mutation — **every** root field of the operation, in document
    /// order.
    ///
    /// The GraphQL spec requires a mutation operation's root fields to execute
    /// serially, all of them. This used to be a single root built from
    /// `parsed.selections.first()`, so `mutation { createUser(…) { id }
    /// createAuditLog(…) { id } }` ran the first write, silently discarded the
    /// second, and answered with a success envelope that named only the first
    /// (#759).
    Mutation { roots: Vec<MutationRoot> },

    /// Relay global node lookup: `node(id: ID!)`.
    /// Resolves any type that implements the Node interface by global opaque ID.
    /// Contains the field selections from the inline fragment for projection.
    NodeQuery {
        selections: Vec<crate::graphql::FieldSelection>,
    },

    /// Root `__typename` meta-field — a single-root selection consisting solely
    /// of `__typename` (optionally aliased). Resolves to the operation's root
    /// type name (`Query` / `Mutation` / `Subscription`) without a DB round-trip.
    ///
    /// Carries the root [`FieldSelection`](crate::graphql::FieldSelection) so the
    /// executor can honour the response key (alias) and evaluate `@skip` /
    /// `@include` directives against the request variables.
    ///
    /// Scope: mixed roots (`{ __typename users { id } }`) are not classified
    /// here — they fall through to `Regular` and are resolved by the multi-root
    /// pipeline. Fragment-wrapped root `__typename` (`{ ...F }` or
    /// `{ ... on Query { __typename } }`) is likewise not special-cased.
    TypeName {
        /// The root `__typename` selection (alias + `@skip`/`@include` directives).
        selection:      Box<crate::graphql::FieldSelection>,
        /// Operation type string from the parsed query (`query` / `mutation` /
        /// `subscription`), used to resolve the root type name.
        operation_type: String,
    },
}

/// One root field of a mutation operation.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::runtime::executor) struct MutationRoot {
    /// Key this root's result appears under in `data` — the alias when the
    /// document supplies one, otherwise the field name. Two roots calling the
    /// same mutation are distinguishable only by this.
    pub response_key: String,
    /// The compiled mutation to dispatch.
    pub name:         String,
    /// The result selection set, inline fragments intact, so projection mirrors
    /// the query path's selection-faithful, depth-aware behaviour. Named spreads
    /// are already expanded; `@skip`/`@include` are evaluated in the runner,
    /// where the request variables are available.
    pub selections:   Vec<crate::graphql::FieldSelection>,
    /// The root field's inline arguments (e.g. `createMachine(input: { … })`) so
    /// the runner can resolve inline-literal inputs — including nested `$var`
    /// references — that do not appear in the request `variables` map.
    pub arguments:    Vec<crate::graphql::GraphQLArgument>,
}

/// Resolve a GraphQL operation type string to its root type name.
///
/// Matches the hardcoded root type names emitted by the introspection schema
/// builder (`Query` / `Mutation` / `Subscription`), so a root `__typename`
/// resolves to the same name as `__schema { queryType { name } }`. Defaults to
/// `Query` for unknown operation strings.
pub(in crate::runtime::executor) fn root_type_name(operation_type: &str) -> &'static str {
    match operation_type {
        "mutation" => "Mutation",
        "subscription" => "Subscription",
        _ => "Query",
    }
}

/// Null out masked fields in a projected JSON result.
///
/// Walks the result (which may be a single object or an array of objects)
/// and sets each masked field's value to `null`.
fn null_masked_fields(value: &mut serde_json::Value, masked: &[String]) {
    match value {
        serde_json::Value::Object(map) => {
            for field_name in masked {
                if map.contains_key(field_name) {
                    map.insert(field_name.clone(), serde_json::Value::Null);
                }
            }
        },
        serde_json::Value::Array(items) => {
            for item in items {
                null_masked_fields(item, masked);
            }
        },
        _ => {},
    }
}

/// Resolve a server-side `@inject` parameter from JWT claims.
///
/// This function extracts values from the security context (decoded JWT token)
/// and provides them to GraphQL queries/mutations without exposing them to the client.
///
/// # Security Properties
///
/// - **Non-bypassable**: Injected parameters come ONLY from JWT, not from GraphQL args
/// - **Mandatory auth**: Query fails if inject params required but no JWT provided
/// - **No confusion**: Same parameter cannot be both GraphQL arg and injected
///
/// # Mapping Rules
///
/// The `@fraiseql.query(inject={"param": "jwt:claim"})` decorator maps JWT claims:
///
/// | Claim | Source | Example |
/// |-------|--------|---------|
/// | `"sub"` | User ID from JWT | `"user-123"` |
/// | `"tenant_id"` | Tenant from JWT | `"tenant-456"` |
/// | `"org_id"` | Org from JWT | `"org-789"` |
/// | Other claim names | Custom JWT attributes | Any value |
///
/// # Error Handling
///
/// Returns `FraiseQLError::Validation` if the JWT claim is missing.
/// For example, if query injects `{"userId": "jwt:sub"}` but JWT has no `sub` claim.
///
/// # Example
///
/// ```python
/// # Python decorator
/// @fraiseql.query(
///     inject={"userId": "jwt:sub", "tenantId": "jwt:tenant_id"}
/// )
/// def current_user(userId: str, tenantId: str) -> User:
///     '''Get current user - userId and tenantId are injected from JWT'''
///     pass
/// ```
///
/// When executed:
/// 1. JWT is decoded: `{"sub": "user-123", "tenant_id": "tenant-456", ...}`
/// 2. `resolve_inject_value("userId", "jwt:sub", context)` → `"user-123"`
/// 3. `resolve_inject_value("tenantId", "jwt:tenant_id", context)` → `"tenant-456"`
/// 4. SQL is generated with these as parameters (not from GraphQL args)
/// 5. User cannot override these values in the query
///
/// # Multi-Tenant Example
///
/// ```graphql
/// # Client sends this (no userId or tenantId in args)
/// query { currentUser { id name email } }
/// ```
///
/// ```no_run
/// // Requires: a SecurityContext from authenticated request metadata.
/// // See: tests/integration/ for runnable examples.
/// // Executor does this:
/// // let user_id = resolve_inject_value("userId", "jwt:sub", &security_ctx)?;
/// // let tenant_id = resolve_inject_value("tenantId", "jwt:tenant_id", &security_ctx)?;
/// // Builds SQL: SELECT * FROM fn_current_user($1, $2) with params [user_id, tenant_id]
/// // User cannot bypass this by passing different values
/// ```
fn resolve_inject_value(
    param_name: &str,
    source: &InjectedParamSource,
    security_ctx: &SecurityContext,
) -> Result<serde_json::Value> {
    match source {
        InjectedParamSource::Jwt(claim) => {
            let value = match claim.as_str() {
                "sub" => Some(serde_json::Value::String(security_ctx.user_id.0.clone())),
                "tenant_id" | "org_id" => {
                    security_ctx.tenant_id.as_ref().map(|t| serde_json::Value::String(t.0.clone()))
                },
                other => security_ctx.attributes.get(other).cloned(),
            };
            value.ok_or_else(|| FraiseQLError::Validation {
                message: format!(
                    "Inject param '{param_name}': JWT claim '{claim}' not present in token"
                ),
                path:    None,
            })
        },
        InjectedParamSource::Enrichment(field) => {
            // Read ONLY the reserved namespace — no fallback to a raw claim or a
            // well-known field (#539). A raw claim of the same name must never
            // impersonate a DB-derived identity parameter.
            let key = format!("{}{field}", crate::security::ENRICHED_NAMESPACE_PREFIX);
            security_ctx
                .attributes
                .get(&key)
                .cloned()
                .ok_or_else(|| FraiseQLError::Validation {
                    message: format!(
                        "Inject param '{param_name}': enriched field '{field}' absent from the \
                         resolved identity (enrichment did not run, or the query's `map` does \
                         not produce it)"
                    ),
                    path:    None,
                })
        },
    }
}
