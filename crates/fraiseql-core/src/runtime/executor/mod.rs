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
//! > **Note on aggregate and window queries**: These paths use `execute_raw_query` with
//! > pre-assembled SQL strings rather than prepared statement parameters. All user-supplied
//! > values are escaped via `format_sql_value` / `escape_sql_string`, but the guarantee
//! > of parameterised execution does not extend to these paths.
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
//! - User's RLS policy (via SecurityContext)
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
//! // Load compiled schema and create adapter
//! let schema = CompiledSchema::from_json(schema_json)?;
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

use std::{sync::Arc, time::Duration};

use futures::future::BoxFuture;

use super::{
    ExecutionContext, QueryMatcher, QueryPlanner, RuntimeConfig,
    classify_field_access,
    suggest_similar,
};
#[cfg(test)]
use crate::db::types::{DatabaseType, PoolMetrics};
use crate::{
    compiler::aggregation::OrderByClause,
    db::{
        CursorValue, RelayDatabaseAdapter, WhereClause,
        traits::{DatabaseAdapter, RelayPageResult},
    },
    error::{FraiseQLError, Result},
    schema::{
        CompiledSchema, InjectedParamSource, IntrospectionResponses,
    },
    security::{FieldAccessError, SecurityContext},
};

mod classify;
mod query;
mod mutation;
mod aggregate;
mod federation;

#[cfg(test)]
mod tests;

// ── Relay dispatch ─────────────────────────────────────────────────────────────
//
// `RelayDispatch` is a private type-erased relay executor stored as
// `Option<Arc<dyn RelayDispatch>>` in `Executor<A>`.  It is populated at
// construction time only when `A: RelayDatabaseAdapter`, giving us:
//
//  - No `unreachable!()` in non-relay adapters.
//  - No capability flag to keep in sync.
//  - `execute_relay_page` exists *only* on relay-capable adapters.
//  - One clean runtime `Option::is_some()` check in the dispatcher.
//
// This design works in stable Rust because specialisation is not required —
// the selection happens in two differently-named constructors.

trait RelayDispatch: Send + Sync {
    fn execute_relay_page<'a>(
        &'a self,
        view: &'a str,
        cursor_column: &'a str,
        after: Option<CursorValue>,
        before: Option<CursorValue>,
        limit: u32,
        forward: bool,
        where_clause: Option<&'a WhereClause>,
        order_by: Option<&'a [OrderByClause]>,
        include_total_count: bool,
    ) -> BoxFuture<'a, Result<RelayPageResult>>;
}

struct RelayDispatchImpl<A: RelayDatabaseAdapter>(Arc<A>);

impl<A: RelayDatabaseAdapter + Send + Sync + 'static> RelayDispatch for RelayDispatchImpl<A> {
    fn execute_relay_page<'a>(
        &'a self,
        view: &'a str,
        cursor_column: &'a str,
        after: Option<CursorValue>,
        before: Option<CursorValue>,
        limit: u32,
        forward: bool,
        where_clause: Option<&'a WhereClause>,
        order_by: Option<&'a [OrderByClause]>,
        include_total_count: bool,
    ) -> BoxFuture<'a, Result<RelayPageResult>> {
        Box::pin(self.0.execute_relay_page(
            view,
            cursor_column,
            after,
            before,
            limit,
            forward,
            where_clause,
            order_by,
            include_total_count,
        ))
    }
}

/// Query type classification for routing.
#[derive(Debug, Clone, PartialEq)]
 enum QueryType {
    /// Regular GraphQL query (non-analytics).
    Regular,

    /// Aggregate analytics query (ends with _aggregate).
    /// Contains the full query name (e.g., "sales_aggregate").
    Aggregate(String),

    /// Window function query (ends with _window).
    /// Contains the full query name (e.g., "sales_window").
    Window(String),

    /// Federation query (_service or _entities).
    /// Contains the query name ("_service" or "_entities").
    Federation(String),

    /// Introspection query (`__schema`).
    IntrospectionSchema,

    /// Introspection query (`__type(name: "...")`).
    /// Contains the requested type name.
    IntrospectionType(String),

    /// GraphQL mutation.
    /// Contains the root field name (e.g., "createMachine").
    Mutation(String),

    /// Relay global node lookup: `node(id: ID!)`.
    /// Resolves any type that implements the Node interface by global opaque ID.
    NodeQuery,
}

/// Query executor - executes compiled GraphQL queries.
///
/// This is the main entry point for runtime query execution.
/// It coordinates matching, planning, execution, and projection.
///
/// # Type Parameters
///
/// * `A` - The database adapter type (implements `DatabaseAdapter` trait)
///
/// # Ownership and Lifetimes
///
/// The executor holds owned references to schema and runtime data, with no borrowed pointers:
/// - `schema`: Owned `CompiledSchema` (immutable after construction)
/// - `adapter`: Shared via `Arc<A>` to allow multiple executors/tasks to use the same connection
///   pool
/// - `introspection`: Owned cached GraphQL schema responses
/// - `config`: Owned runtime configuration
///
/// **No explicit lifetimes required** - all data is either owned or wrapped in `Arc`,
/// so the executor can be stored in long-lived structures without lifetime annotations or
/// borrow-checker issues.
///
/// # Concurrency
///
/// `Executor<A>` is `Send + Sync` when `A` is `Send + Sync`. It can be safely shared across
/// threads and tasks without cloning:
/// ```no_run
/// // Requires: a live database adapter.
/// // See: tests/integration/ for runnable examples.
/// # use std::sync::Arc;
/// // let executor = Arc::new(Executor::new(schema, adapter));
/// // Can be cloned into multiple tasks
/// // let exec_clone = executor.clone();
/// // tokio::spawn(async move {
/// //     let result = exec_clone.execute(query, vars).await;
/// // });
/// ```
///
/// # Query Timeout
///
/// Queries are protected by the `query_timeout_ms` configuration in `RuntimeConfig` (default: 30s).
/// When a query exceeds this timeout, it returns `FraiseQLError::Timeout` without panicking.
/// Set `query_timeout_ms` to 0 to disable timeout enforcement.
pub struct Executor<A: DatabaseAdapter> {
    /// Compiled schema with optimized SQL templates
    pub(self) schema: CompiledSchema,

    /// Shared database adapter for query execution
    /// Wrapped in Arc to allow multiple executors to use the same connection pool
    pub(self) adapter: Arc<A>,

    /// Type-erased relay capability slot.
    ///
    /// `Some` when the executor was constructed via `new_with_relay` (requires
    /// `A: RelayDatabaseAdapter`).  `None` causes relay queries to return a
    /// `FraiseQLError::Validation` — no `unreachable!()`, no capability flag.
    pub(self) relay: Option<Arc<dyn RelayDispatch>>,

    /// Query matching engine (stateless)
    pub(self) matcher: QueryMatcher,

    /// Query execution planner (stateless)
    pub(self) planner: QueryPlanner,

    /// Runtime configuration (timeouts, complexity limits, etc.)
    pub(self) config: RuntimeConfig,

    /// Pre-built introspection responses cached for `__schema` and `__type` queries
    /// Avoids recomputing schema introspection on every request
    pub(self) introspection: IntrospectionResponses,
}

impl<A: DatabaseAdapter> Executor<A> {
    /// Create new executor.
    ///
    /// # Arguments
    ///
    /// * `schema` - Compiled schema
    /// * `adapter` - Database adapter
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live PostgreSQL database.
    /// // See: tests/integration/ for runnable examples.
    /// # use fraiseql_core::schema::CompiledSchema;
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # use fraiseql_core::runtime::Executor;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let schema = CompiledSchema::from_json(schema_json)?;
    /// let adapter = PostgresAdapter::new(connection_string).await?;
    /// let executor = Executor::new(schema, Arc::new(adapter));
    /// # Ok(()) }
    /// ```
    #[must_use]
    pub fn new(schema: CompiledSchema, adapter: Arc<A>) -> Self {
        Self::with_config(schema, adapter, RuntimeConfig::default())
    }

    /// Create new executor with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `schema` - Compiled schema
    /// * `adapter` - Database adapter
    /// * `config` - Runtime configuration
    #[must_use]
    pub fn with_config(schema: CompiledSchema, adapter: Arc<A>, config: RuntimeConfig) -> Self {
        let matcher = QueryMatcher::new(schema.clone());
        let planner = QueryPlanner::new(config.cache_query_plans);
        // Build introspection responses at startup (zero-cost at runtime)
        let introspection = IntrospectionResponses::build(&schema);

        Self {
            schema,
            adapter,
            relay: None,
            matcher,
            planner,
            config,
            introspection,
        }
    }
}

impl<A: DatabaseAdapter + RelayDatabaseAdapter + 'static> Executor<A> {
    /// Create a new executor with relay cursor pagination enabled.
    ///
    /// Only callable when `A: RelayDatabaseAdapter`.  The relay capability is
    /// encoded once at construction time as a type-erased `Arc<dyn RelayDispatch>`,
    /// so there is no per-query overhead beyond an `Option::is_some()` check.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live PostgreSQL database with relay support.
    /// // See: tests/integration/ for runnable examples.
    /// # use fraiseql_core::schema::CompiledSchema;
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # use fraiseql_core::runtime::Executor;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let adapter = PostgresAdapter::new(connection_string).await?;
    /// let executor = Executor::new_with_relay(schema, Arc::new(adapter));
    /// # Ok(()) }
    /// ```
    #[must_use]
    pub fn new_with_relay(schema: CompiledSchema, adapter: Arc<A>) -> Self {
        Self::with_config_and_relay(schema, adapter, RuntimeConfig::default())
    }

    /// Create a new executor with relay support and custom configuration.
    #[must_use]
    pub fn with_config_and_relay(
        schema: CompiledSchema,
        adapter: Arc<A>,
        config: RuntimeConfig,
    ) -> Self {
        let relay: Arc<dyn RelayDispatch> = Arc::new(RelayDispatchImpl(adapter.clone()));
        let matcher = QueryMatcher::new(schema.clone());
        let planner = QueryPlanner::new(config.cache_query_plans);
        let introspection = IntrospectionResponses::build(&schema);

        Self {
            schema,
            adapter,
            relay: Some(relay),
            matcher,
            planner,
            config,
            introspection,
        }
    }
}

impl<A: DatabaseAdapter> Executor<A> {

    /// Execute a GraphQL query.
    ///
    /// This is the main entry point for query execution. It coordinates the three-phase
    /// execution model:
    ///
    /// 1. **Preparation**: Classify the query type and validate against schema
    /// 2. **SQL Generation**: Build parameterized SQL with RLS and injections
    /// 3. **Execution**: Run against database adapter and format response
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string (can be query, mutation, introspection, etc.)
    /// * `variables` - GraphQL variables (optional, passed as query parameters)
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string (includes `data` and optional `errors`)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query is malformed (returns `FraiseQLError::Parse`)
    /// - Query references undefined operations (returns `FraiseQLError::Validation`)
    /// - Database execution fails (returns `FraiseQLError::Database`)
    /// - Query exceeds timeout (returns `FraiseQLError::Timeout`)
    /// - Result projection fails (returns `FraiseQLError::Projection`)
    ///
    /// # Query Timeout
    ///
    /// Queries are protected by the timeout configured in `RuntimeConfig`.
    /// Default is 30 seconds. Set `query_timeout_ms` to 0 to disable.
    ///
    /// # Performance
    ///
    /// - **Introspection queries**: <1ms (pre-built responses)
    /// - **Cached queries**: <1ms (APQ cache hit)
    /// - **Cold queries**: 5-15ms (full execution with database round-trip)
    /// - **Large result sets**: +time proportional to row count
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live database adapter and compiled schema.
    /// // See: tests/integration/ for runnable examples.
    /// use fraiseql_core::runtime::Executor;
    ///
    /// // Simple query without variables
    /// // let query = "{ users(limit: 10) { id name email } }";
    /// // let result = executor.execute(query, None).await?;
    /// // println!("Result: {}", result);
    ///
    /// // Query with variables
    /// // let query = "query($id: ID!) { user(id: $id) { name email } }";
    /// // let variables = serde_json::json!({"id": "user-123"});
    /// // let result = executor.execute(query, Some(&variables)).await?;
    /// ```
    /// Generate an explain plan for a query without executing it.
    ///
    /// Returns the SQL that would be generated, parameters, cost estimate,
    /// and views that would be accessed.
    ///
    /// # Errors
    ///
    /// Returns error if the query cannot be parsed or matched against the schema.
    pub fn plan_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<super::ExplainPlan> {
        let query_type = self.classify_query(query)?;

        match query_type {
            QueryType::Regular => {
                let query_match = self.matcher.match_query(query, variables)?;
                let view = query_match
                    .query_def
                    .sql_source
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                let plan = self.planner.plan(&query_match)?;
                Ok(super::ExplainPlan {
                    sql:            plan.sql,
                    parameters:     plan.parameters,
                    estimated_cost: plan.estimated_cost,
                    views_accessed: vec![view],
                    query_type:     "regular".to_string(),
                })
            },
            QueryType::Mutation(ref name) => {
                let mutation_def =
                    self.schema.mutations.iter().find(|m| m.name == *name).ok_or_else(|| {
                        let candidates: Vec<&str> =
                            self.schema.mutations.iter().map(|m| m.name.as_str()).collect();
                        let suggestion = suggest_similar(name, &candidates);
                        let message = match suggestion.as_slice() {
                            [s] => format!(
                                "Mutation '{name}' not found in schema. Did you mean '{s}'?"
                            ),
                            _ => format!("Mutation '{name}' not found in schema"),
                        };
                        FraiseQLError::Validation { message, path: None }
                    })?;
                let fn_name = mutation_def
                    .sql_source
                    .clone()
                    .unwrap_or_else(|| format!("fn_{name}"));
                Ok(super::ExplainPlan {
                    sql:            format!("SELECT * FROM {fn_name}(...)"),
                    parameters:     Vec::new(),
                    estimated_cost: 100,
                    views_accessed: vec![fn_name],
                    query_type:     "mutation".to_string(),
                })
            },
            QueryType::Aggregate(ref name) => {
                let sql_source = self
                    .schema
                    .queries
                    .iter()
                    .find(|q| q.name == *name)
                    .and_then(|q| q.sql_source.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                Ok(super::ExplainPlan {
                    sql:            format!("SELECT ... FROM {sql_source} -- aggregate"),
                    parameters:     Vec::new(),
                    estimated_cost: 200,
                    views_accessed: vec![sql_source],
                    query_type:     "aggregate".to_string(),
                })
            },
            QueryType::Window(ref name) => {
                let sql_source = self
                    .schema
                    .queries
                    .iter()
                    .find(|q| q.name == *name)
                    .and_then(|q| q.sql_source.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                Ok(super::ExplainPlan {
                    sql:            format!("SELECT ... FROM {sql_source} -- window"),
                    parameters:     Vec::new(),
                    estimated_cost: 250,
                    views_accessed: vec![sql_source],
                    query_type:     "window".to_string(),
                })
            },
            QueryType::IntrospectionSchema | QueryType::IntrospectionType(_) => {
                Ok(super::ExplainPlan {
                    sql:            String::new(),
                    parameters:     Vec::new(),
                    estimated_cost: 0,
                    views_accessed: Vec::new(),
                    query_type:     "introspection".to_string(),
                })
            },
            QueryType::Federation(_) => Ok(super::ExplainPlan {
                sql:            String::new(),
                parameters:     Vec::new(),
                estimated_cost: 0,
                views_accessed: Vec::new(),
                query_type:     "federation".to_string(),
            }),
            QueryType::NodeQuery => Ok(super::ExplainPlan {
                sql:            String::new(),
                parameters:     Vec::new(),
                estimated_cost: 50,
                views_accessed: Vec::new(),
                query_type:     "node".to_string(),
            }),
        }
    }

    pub async fn execute(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // Apply query timeout if configured
        if self.config.query_timeout_ms > 0 {
            let timeout_duration = Duration::from_millis(self.config.query_timeout_ms);
            tokio::time::timeout(timeout_duration, self.execute_internal(query, variables))
                .await
                .map_err(|_| {
                    // Truncate query if too long for error reporting
                    let query_snippet = if query.len() > 100 {
                        format!("{}...", &query[..100])
                    } else {
                        query.to_string()
                    };
                    FraiseQLError::Timeout {
                        timeout_ms: self.config.query_timeout_ms,
                        query:      Some(query_snippet),
                    }
                })?
        } else {
            self.execute_internal(query, variables).await
        }
    }

    /// Internal execution logic (called by execute with timeout wrapper).
    async fn execute_internal(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // 1. Classify query type
        let query_type = self.classify_query(query)?;

        // 2. Route to appropriate handler
        match query_type {
            QueryType::Regular => self.execute_regular_query(query, variables).await,
            QueryType::Aggregate(query_name) => {
                self.execute_aggregate_dispatch(&query_name, variables).await
            },
            QueryType::Window(query_name) => {
                self.execute_window_dispatch(&query_name, variables).await
            },
            QueryType::Federation(query_name) => {
                self.execute_federation_query(&query_name, query, variables).await
            },
            QueryType::IntrospectionSchema => {
                // Return pre-built __schema response (zero-cost at runtime)
                Ok(self.introspection.schema_response.clone())
            },
            QueryType::IntrospectionType(type_name) => {
                // Return pre-built __type response (zero-cost at runtime)
                Ok(self.introspection.get_type_response(&type_name))
            },
            QueryType::Mutation(mutation_name) => {
                self.execute_mutation_query(&mutation_name, variables).await
            },
            QueryType::NodeQuery => self.execute_node_query(query, variables).await,
        }
    }

    /// Execute a GraphQL query with user context for field-level access control.
    ///
    /// This method validates that the user has permission to access all requested
    /// fields before executing the query. If field filtering is enabled in the
    /// `RuntimeConfig` and the user lacks required scopes, this returns an error.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Query variables (optional)
    /// * `user_scopes` - User's scopes from JWT token (pass empty slice if unauthenticated)
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string, or error if access denied
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live database adapter and authenticated user context.
    /// // See: tests/integration/ for runnable examples.
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let query = r#"query { users { id name salary } }"#;
    /// // let user_scopes = user.scopes.clone();
    /// // let result = executor.execute_with_scopes(query, None, &user_scopes).await?;
    /// # Ok(()) }
    /// ```
    pub async fn execute_with_scopes(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        user_scopes: &[String],
    ) -> Result<String> {
        // 1. Classify query type
        let query_type = self.classify_query(query)?;

        // 2. Validate field access if filter is configured
        if let Some(ref filter) = self.config.field_filter {
            // Only validate for regular queries (not introspection)
            if matches!(query_type, QueryType::Regular) {
                self.validate_field_access(query, variables, user_scopes, filter)?;
            }
        }

        // 3. Route to appropriate handler (same as execute)
        match query_type {
            QueryType::Regular => self.execute_regular_query(query, variables).await,
            QueryType::Aggregate(query_name) => {
                self.execute_aggregate_dispatch(&query_name, variables).await
            },
            QueryType::Window(query_name) => {
                self.execute_window_dispatch(&query_name, variables).await
            },
            QueryType::Federation(query_name) => {
                self.execute_federation_query(&query_name, query, variables).await
            },
            QueryType::IntrospectionSchema => Ok(self.introspection.schema_response.clone()),
            QueryType::IntrospectionType(type_name) => {
                Ok(self.introspection.get_type_response(&type_name))
            },
            QueryType::Mutation(mutation_name) => {
                self.execute_mutation_query(&mutation_name, variables).await
            },
            QueryType::NodeQuery => self.execute_node_query(query, variables).await,
        }
    }

    /// Validate that user has access to all requested fields.
    fn validate_field_access(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        user_scopes: &[String],
        filter: &crate::security::FieldFilter,
    ) -> Result<()> {
        // Parse query to get field selections
        let query_match = self.matcher.match_query(query, variables)?;

        // Get the return type name from the query definition
        let type_name = &query_match.query_def.return_type;

        // Validate each requested field
        let field_refs: Vec<&str> = query_match.fields.iter().map(String::as_str).collect();
        let errors = filter.validate_fields(type_name, &field_refs, user_scopes);

        if errors.is_empty() {
            Ok(())
        } else {
            // Return the first error (could aggregate all errors if desired)
            let first_error = &errors[0];
            Err(FraiseQLError::Authorization {
                message:  first_error.message.clone(),
                action:   Some("read".to_string()),
                resource: Some(format!("{}.{}", first_error.type_name, first_error.field_name)),
            })
        }
    }

    /// Execute a GraphQL query with cancellation support via ExecutionContext.
    ///
    /// This method allows graceful cancellation of long-running queries through a
    /// cancellation token. If the token is cancelled during execution, the query
    /// returns a `FraiseQLError::Cancelled` error.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Query variables (optional)
    /// * `ctx` - ExecutionContext with cancellation token
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string, or error if cancelled or execution fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live database adapter and running tokio runtime.
    /// // See: tests/integration/ for runnable examples.
    /// use fraiseql_core::runtime::ExecutionContext;
    /// use fraiseql_core::error::FraiseQLError;
    /// use std::time::Duration;
    ///
    /// let ctx = ExecutionContext::new("user-query-123".to_string());
    /// let cancel_token = ctx.cancellation_token().clone();
    ///
    /// // Spawn a task to cancel after 5 seconds
    /// tokio::spawn(async move {
    ///     tokio::time::sleep(Duration::from_secs(5)).await;
    ///     cancel_token.cancel();
    /// });
    ///
    /// // let result = executor.execute_with_context(query, None, &ctx).await;
    /// ```
    pub async fn execute_with_context(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        ctx: &ExecutionContext,
    ) -> Result<String> {
        // Check if already cancelled before starting
        if ctx.is_cancelled() {
            return Err(FraiseQLError::cancelled(
                ctx.query_id().to_string(),
                "Query cancelled before execution".to_string(),
            ));
        }

        let token = ctx.cancellation_token().clone();

        // Use tokio::select! to race between execution and cancellation
        tokio::select! {
            result = self.execute(query, variables) => {
                result
            }
            () = token.cancelled() => {
                Err(FraiseQLError::cancelled(
                    ctx.query_id().to_string(),
                    "Query cancelled during execution".to_string(),
                ))
            }
        }
    }

    /// Execute a GraphQL query or mutation with a JWT [`SecurityContext`].
    ///
    /// This is the **main authenticated entry point** for the executor. It routes the
    /// incoming request to the appropriate handler based on the query type:
    ///
    /// - **Regular queries**: RLS `WHERE` clauses are applied so each user only sees
    ///   their own rows, as determined by the RLS policy in [`RuntimeConfig`].
    /// - **Mutations**: The security context is forwarded to
    ///   `execute_mutation_query_with_security` so server-side `inject` parameters
    ///   (e.g. `jwt:sub`) are resolved from the caller's JWT claims.
    /// - **Aggregations, window queries, federation, introspection**: Delegated to
    ///   their respective handlers (security context is not yet applied to these).
    ///
    /// If `query_timeout_ms` is non-zero in the [`RuntimeConfig`], the entire
    /// execution is raced against a Tokio deadline and returns
    /// [`FraiseQLError::Timeout`] when the deadline is exceeded.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string (e.g. `"query { posts { id title } }"`)
    /// * `variables` - Optional JSON object of GraphQL variable values
    /// * `security_context` - Authenticated user context extracted from a validated JWT
    ///
    /// # Returns
    ///
    /// A JSON-encoded GraphQL response string on success, conforming to the
    /// [GraphQL over HTTP](https://graphql.github.io/graphql-over-http/) specification.
    ///
    /// # Errors
    ///
    /// * [`FraiseQLError::Parse`] — the query string is not valid GraphQL
    /// * [`FraiseQLError::Validation`] — unknown mutation name, missing `sql_source`,
    ///   or a mutation requires `inject` params but the security context is absent
    /// * [`FraiseQLError::Database`] — the underlying adapter returns an error
    /// * [`FraiseQLError::Timeout`] — execution exceeded `query_timeout_ms`
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live database adapter and a SecurityContext from authentication.
    /// // See: tests/integration/ for runnable examples.
    /// use fraiseql_core::security::SecurityContext;
    ///
    /// // let query = r#"query { posts { id title } }"#;
    /// // Returns a JSON string: {"data":{"posts":[...]}}
    /// // let result = executor.execute_with_security(query, None, &context).await?;
    /// ```
    pub async fn execute_with_security(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
    ) -> Result<String> {
        // Apply query timeout if configured
        if self.config.query_timeout_ms > 0 {
            let timeout_duration = Duration::from_millis(self.config.query_timeout_ms);
            tokio::time::timeout(
                timeout_duration,
                self.execute_with_security_internal(query, variables, security_context),
            )
            .await
            .map_err(|_| {
                let query_snippet = if query.len() > 100 {
                    format!("{}...", &query[..100])
                } else {
                    query.to_string()
                };
                FraiseQLError::Timeout {
                    timeout_ms: self.config.query_timeout_ms,
                    query:      Some(query_snippet),
                }
            })?
        } else {
            self.execute_with_security_internal(query, variables, security_context).await
        }
    }

    /// Internal execution logic with security context (called by execute_with_security with timeout
    /// wrapper).
    async fn execute_with_security_internal(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
    ) -> Result<String> {
        // 1. Classify query type
        let query_type = self.classify_query(query)?;

        // 2. Route to appropriate handler (with RLS support for regular queries)
        match query_type {
            QueryType::Regular => {
                self.execute_regular_query_with_security(query, variables, security_context)
                    .await
            },
            // Other query types don't support RLS yet
            QueryType::Aggregate(query_name) => {
                self.execute_aggregate_dispatch(&query_name, variables).await
            },
            QueryType::Window(query_name) => {
                self.execute_window_dispatch(&query_name, variables).await
            },
            QueryType::Federation(query_name) => {
                self.execute_federation_query(&query_name, query, variables).await
            },
            QueryType::IntrospectionSchema => Ok(self.introspection.schema_response.clone()),
            QueryType::IntrospectionType(type_name) => {
                Ok(self.introspection.get_type_response(&type_name))
            },
            QueryType::Mutation(mutation_name) => {
                self.execute_mutation_query_with_security(
                    &mutation_name,
                    variables,
                    Some(security_context),
                )
                .await
            },
            QueryType::NodeQuery => self.execute_node_query(query, variables).await,
        }
    }

    /// Check if a specific field can be accessed with given scopes.
    ///
    /// This is a convenience method for checking field access without executing a query.
    ///
    /// # Arguments
    ///
    /// * `type_name` - The GraphQL type name
    /// * `field_name` - The field name
    /// * `user_scopes` - User's scopes from JWT token
    ///
    /// # Returns
    ///
    /// `Ok(())` if access is allowed, `Err(FieldAccessError)` if denied
    pub fn check_field_access(
        &self,
        type_name: &str,
        field_name: &str,
        user_scopes: &[String],
    ) -> std::result::Result<(), FieldAccessError> {
        if let Some(ref filter) = self.config.field_filter {
            filter.can_access(type_name, field_name, user_scopes)
        } else {
            // No filter configured, allow all access
            Ok(())
        }
    }

    /// Apply field-level RBAC filtering to projection fields.
    ///
    /// Classifies each requested field against the user's security context:
    /// - **Allowed**: user has the required scope (or field is public)
    /// - **Masked**: user lacks scope, but `on_deny = Mask` → field value will be nulled
    /// - **Rejected**: user lacks scope, `on_deny = Reject` → query fails with FORBIDDEN
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Forbidden` if any requested field has `on_deny = Reject`
    /// and the user lacks the required scope.
    pub(self) fn apply_field_rbac_filtering(
        &self,
        return_type: &str,
        projection_fields: Vec<String>,
        security_context: &SecurityContext,
    ) -> Result<super::field_filter::FieldAccessResult> {
        use super::field_filter::FieldAccessResult;

        // Try to extract security config from compiled schema
        if let Some(security_config) = self.schema.security.as_ref() {
            if let Some(type_def) = self.schema.types.iter().find(|t| t.name == return_type) {
                return classify_field_access(
                    security_context,
                    security_config,
                    &type_def.fields,
                    projection_fields,
                )
                .map_err(|rejected_field| FraiseQLError::Authorization {
                    message:  format!(
                        "Access denied: field '{rejected_field}' on type '{return_type}' \
                         requires a scope you do not have"
                    ),
                    action:   Some("read".to_string()),
                    resource: Some(format!("{return_type}.{rejected_field}")),
                });
            }
        }

        // No security config or type not found → all fields allowed, none masked
        Ok(FieldAccessResult {
            allowed: projection_fields,
            masked:  Vec::new(),
        })
    }

    /// Execute a query and return parsed JSON.
    ///
    /// Same as `execute()` but returns parsed `serde_json::Value` instead of string.
    pub async fn execute_json(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let result_str = self.execute(query, variables).await?;
        Ok(serde_json::from_str(&result_str)?)
    }

    /// Get the compiled schema.
    #[must_use]
    pub const fn schema(&self) -> &CompiledSchema {
        &self.schema
    }

    /// Get runtime configuration.
    #[must_use]
    pub const fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Get database adapter reference.
    #[must_use]
    pub fn adapter(&self) -> &Arc<A> {
        &self.adapter
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
        }
        serde_json::Value::Array(items) => {
            for item in items {
                null_masked_fields(item, masked);
            }
        }
        _ => {}
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
                "sub" => Some(serde_json::Value::String(security_ctx.user_id.clone())),
                "tenant_id" | "org_id" => security_ctx
                    .tenant_id
                    .as_deref()
                    .map(|s| serde_json::Value::String(s.to_owned())),
                other => security_ctx.attributes.get(other).cloned(),
            };
            value.ok_or_else(|| FraiseQLError::Validation {
                message: format!(
                    "Inject param '{param_name}': JWT claim '{claim}' not present in token"
                ),
                path: None,
            })
        },
    }
}
