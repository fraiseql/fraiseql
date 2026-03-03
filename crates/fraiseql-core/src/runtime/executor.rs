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
//! ```rust,ignore
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
    ExecutionContext, JsonbStrategy, QueryMatcher, QueryPlanner, ResultProjector, RuntimeConfig,
    classify_field_access,
    mutation_result::{MutationOutcome, parse_mutation_row, populate_error_fields},
    suggest_similar,
};
#[cfg(test)]
use crate::db::types::{DatabaseType, PoolMetrics};
use crate::{
    compiler::aggregation::OrderByClause,
    db::{
        CursorValue, RelayDatabaseAdapter, WhereClause, WhereOperator,
        projection_generator::PostgresProjectionGenerator,
        traits::{DatabaseAdapter, RelayPageResult},
    },
    error::{FraiseQLError, Result},
    graphql::parse_query,
    schema::{
        CompiledSchema, InjectedParamSource, IntrospectionResponses, SecurityConfig,
        SqlProjectionHint,
    },
    security::{FieldAccessError, SecurityContext},
};

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
/// ```ignore
/// let executor = Arc::new(Executor::new(schema, adapter, config));
/// // Can be cloned into multiple tasks
/// let exec_clone = executor.clone();
/// tokio::spawn(async move {
///     let result = exec_clone.execute(query, vars).await;
/// });
/// ```
///
/// # Query Timeout
///
/// Queries are protected by the `query_timeout_ms` configuration in `RuntimeConfig` (default: 30s).
/// When a query exceeds this timeout, it returns `FraiseQLError::Timeout` without panicking.
/// Set `query_timeout_ms` to 0 to disable timeout enforcement.
pub struct Executor<A: DatabaseAdapter> {
    /// Compiled schema with optimized SQL templates
    schema: CompiledSchema,

    /// Shared database adapter for query execution
    /// Wrapped in Arc to allow multiple executors to use the same connection pool
    adapter: Arc<A>,

    /// Type-erased relay capability slot.
    ///
    /// `Some` when the executor was constructed via `new_with_relay` (requires
    /// `A: RelayDatabaseAdapter`).  `None` causes relay queries to return a
    /// `FraiseQLError::Validation` — no `unreachable!()`, no capability flag.
    relay: Option<Arc<dyn RelayDispatch>>,

    /// Query matching engine (stateless)
    matcher: QueryMatcher,

    /// Query execution planner (stateless)
    planner: QueryPlanner,

    /// Runtime configuration (timeouts, complexity limits, etc.)
    config: RuntimeConfig,

    /// Pre-built introspection responses cached for `__schema` and `__type` queries
    /// Avoids recomputing schema introspection on every request
    introspection: IntrospectionResponses,
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
    /// ```rust,ignore
    /// let schema = CompiledSchema::from_json(schema_json)?;
    /// let adapter = PostgresAdapter::new(connection_string).await?;
    /// let executor = Executor::new(schema, Arc::new(adapter));
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
    /// ```rust,ignore
    /// let adapter = PostgresAdapter::new(connection_string).await?;
    /// let executor = Executor::new_with_relay(schema, Arc::new(adapter));
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
    /// ```rust,ignore
    /// use fraiseql_core::runtime::Executor;
    ///
    /// // Simple query without variables
    /// let query = "{ users(limit: 10) { id name email } }";
    /// let result = executor.execute(query, None).await?;
    /// println!("Result: {}", result);
    ///
    /// // Query with variables
    /// let query = "query($id: ID!) { user(id: $id) { name email } }";
    /// let variables = serde_json::json!({"id": "user-123"});
    /// let result = executor.execute(query, Some(&variables)).await?;
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
    /// ```rust,ignore
    /// let query = r#"query { users { id name salary } }"#;
    /// let user_scopes = user.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    /// let result = executor.execute_with_scopes(query, None, &user_scopes).await?;
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
    /// ```rust,ignore
    /// let ctx = ExecutionContext::new("user-query-123".to_string());
    /// let cancel_token = ctx.cancellation_token().clone();
    ///
    /// // Spawn a task to cancel after 5 seconds
    /// tokio::spawn(async move {
    ///     tokio::time::sleep(Duration::from_secs(5)).await;
    ///     cancel_token.cancel();
    /// });
    ///
    /// let result = executor.execute_with_context(query, None, &ctx).await;
    /// match result {
    ///     Err(FraiseQLError::Cancelled { reason, .. }) => {
    ///         eprintln!("Query cancelled: {}", reason);
    ///     }
    ///     Ok(response) => println!("{}", response),
    ///     Err(e) => eprintln!("Error: {}", e),
    /// }
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
    /// ```rust,ignore
    /// let query = r#"query { posts { id title } }"#;
    /// let context = SecurityContext {
    ///     user_id: "user1".to_string(),
    ///     roles: vec!["user".to_string()],
    ///     tenant_id: None,
    ///     scopes: vec![],
    ///     attributes: HashMap::new(),
    ///     request_id: "req-1".to_string(),
    ///     ip_address: None,
    ///     authenticated_at: Utc::now(),
    ///     expires_at: Utc::now() + Duration::hours(1),
    ///     issuer: None,
    ///     audience: None,
    /// };
    /// // Returns a JSON string: {"data":{"posts":[...]}}
    /// let result = executor.execute_with_security(query, None, &context).await?;
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
    fn apply_field_rbac_filtering(
        &self,
        return_type: &str,
        projection_fields: Vec<String>,
        security_context: &SecurityContext,
    ) -> Result<super::field_filter::FieldAccessResult> {
        use super::field_filter::FieldAccessResult;

        // Try to extract security config from compiled schema
        if let Some(ref security_json) = self.schema.security {
            let security_config: SecurityConfig = serde_json::from_value(security_json.clone())
                .map_err(|_| FraiseQLError::Validation {
                    message: "Invalid security configuration in compiled schema".to_string(),
                    path:    Some("schema.security".to_string()),
                })?;

            if let Some(type_def) = self.schema.types.iter().find(|t| t.name == return_type) {
                return classify_field_access(
                    security_context,
                    &security_config,
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

    /// Execute a regular query with row-level security (RLS) filtering.
    ///
    /// This method:
    /// 1. Validates the user's security context (token expiration, etc.)
    /// 2. Evaluates RLS policies to determine what rows the user can access
    /// 3. Composes RLS filters with user-provided WHERE clauses
    /// 4. Passes the composed filter to the database adapter for SQL-level filtering
    ///
    /// RLS filtering happens at the database level, not in Rust, ensuring:
    /// - High performance (database can optimize filters)
    /// - Correct handling of pagination (LIMIT applied after RLS filtering)
    /// - Type-safe composition via WhereClause enum
    async fn execute_regular_query_with_security(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
    ) -> Result<String> {
        // 1. Validate security context (check expiration, etc.)
        if security_context.is_expired() {
            return Err(FraiseQLError::Validation {
                message: "Security token has expired".to_string(),
                path:    Some("request.authorization".to_string()),
            });
        }

        // 2. Match query to compiled template
        let query_match = self.matcher.match_query(query, variables)?;

        // 2b. Enforce requires_role — return "not found" (not "forbidden") to prevent enumeration
        if let Some(ref required_role) = query_match.query_def.requires_role {
            if !security_context.roles.iter().any(|r| r == required_role) {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Query '{}' not found in schema",
                        query_match.query_def.name
                    ),
                    path: None,
                });
            }
        }

        // 3. Create execution plan
        let plan = self.planner.plan(&query_match)?;

        // 4. Evaluate RLS policy and build WHERE clause filter
        let rls_where_clause: Option<WhereClause> =
            if let Some(ref rls_policy) = self.config.rls_policy {
                // Evaluate RLS policy with user's security context
                rls_policy.evaluate(security_context, &query_match.query_def.name)?
            } else {
                // No RLS policy configured, allow all access
                None
            };

        // 5. Get SQL source from query definition
        let sql_source =
            query_match
                .query_def
                .sql_source
                .as_ref()
                .ok_or_else(|| FraiseQLError::Validation {
                    message: "Query has no SQL source".to_string(),
                    path:    None,
                })?;

        // 6. Generate SQL projection hint for requested fields (optimization)
        // Strategy selection: Project (extract fields) vs Stream (return full JSONB)
        let projection_hint = if !plan.projection_fields.is_empty()
            && plan.jsonb_strategy == JsonbStrategy::Project
        {
            let generator = PostgresProjectionGenerator::new();
            let projection_sql = generator
                .generate_projection_sql(&plan.projection_fields)
                .unwrap_or_else(|_| "data".to_string());

            Some(SqlProjectionHint {
                database:                    "postgresql".to_string(),
                projection_template:         projection_sql,
                estimated_reduction_percent: 50,
            })
        } else {
            // Stream strategy: return full JSONB, no projection hint
            None
        };

        // 7. AND inject conditions onto the RLS WHERE clause.
        //    Inject conditions always come after RLS so they cannot bypass it.
        let combined_where: Option<WhereClause> =
            if query_match.query_def.inject_params.is_empty() {
                rls_where_clause // common path: no-op
            } else {
                let mut conditions: Vec<WhereClause> = query_match
                    .query_def
                    .inject_params
                    .iter()
                    .map(|(col, source)| {
                        let value = resolve_inject_value(col, source, security_context)?;
                        Ok(WhereClause::Field {
                            path:     vec![col.clone()],
                            operator: WhereOperator::Eq,
                            value,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                if let Some(rls) = rls_where_clause {
                    conditions.insert(0, rls);
                }
                match conditions.len() {
                    0 => None,
                    1 => Some(conditions.remove(0)),
                    _ => Some(WhereClause::And(conditions)),
                }
            };

        // 8. Execute query with combined WHERE clause filter
        let results = self
            .adapter
            .execute_with_projection(
                sql_source,
                projection_hint.as_ref(),
                combined_where.as_ref(),
                None,
            )
            .await?;

        // 9. Apply field-level RBAC filtering (reject / mask / allow)
        let access = self.apply_field_rbac_filtering(
            &query_match.query_def.return_type,
            plan.projection_fields,
            security_context,
        )?;

        // 10. Project results — include both allowed and masked fields in projection
        let mut all_projection_fields = access.allowed;
        all_projection_fields.extend(access.masked.iter().cloned());
        let projector = ResultProjector::new(all_projection_fields);
        let mut projected =
            projector.project_results(&results, query_match.query_def.returns_list)?;

        // 11. Null out masked fields in the projected result
        if !access.masked.is_empty() {
            null_masked_fields(&mut projected, &access.masked);
        }

        // 12. Wrap in GraphQL data envelope
        let response =
            ResultProjector::wrap_in_data_envelope(projected, &query_match.query_def.name);

        // 13. Serialize to JSON string
        Ok(serde_json::to_string(&response)?)
    }

    async fn execute_regular_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // 1. Match query to compiled template
        let query_match = self.matcher.match_query(query, variables)?;

        // Guard: role-restricted queries are invisible to unauthenticated users
        if query_match.query_def.requires_role.is_some() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Query '{}' not found in schema",
                    query_match.query_def.name
                ),
                path: None,
            });
        }

        // Guard: queries with inject params require a security context.
        if !query_match.query_def.inject_params.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Query '{}' has inject params but was called without a security context",
                    query_match.query_def.name
                ),
                path: None,
            });
        }

        // Route relay queries to dedicated handler.
        if query_match.query_def.relay {
            return self.execute_relay_query(&query_match, variables).await;
        }

        // 2. Create execution plan
        let plan = self.planner.plan(&query_match)?;

        // 3. Execute SQL query
        let sql_source = query_match.query_def.sql_source.as_ref().ok_or_else(|| {
            crate::error::FraiseQLError::Validation {
                message: "Query has no SQL source".to_string(),
                path:    None,
            }
        })?;

        // 3a. Generate SQL projection hint for requested fields (optimization)
        // Strategy selection: Project (extract fields) vs Stream (return full JSONB)
        // This reduces payload by 40-55% by projecting only requested fields at the database level
        let projection_hint = if !plan.projection_fields.is_empty()
            && plan.jsonb_strategy == JsonbStrategy::Project
        {
            let generator = PostgresProjectionGenerator::new();
            let projection_sql = generator
                .generate_projection_sql(&plan.projection_fields)
                .unwrap_or_else(|_| "data".to_string());

            Some(SqlProjectionHint {
                database:                    "postgresql".to_string(),
                projection_template:         projection_sql,
                estimated_reduction_percent: 50,
            })
        } else {
            // Stream strategy: return full JSONB, no projection hint
            None
        };

        let results = self
            .adapter
            .execute_with_projection(sql_source, projection_hint.as_ref(), None, None)
            .await?;

        // 4. Project results
        let projector = ResultProjector::new(plan.projection_fields);
        let projected = projector.project_results(&results, query_match.query_def.returns_list)?;

        // 5. Wrap in GraphQL data envelope
        let response =
            ResultProjector::wrap_in_data_envelope(projected, &query_match.query_def.name);

        // 6. Serialize to JSON string
        Ok(serde_json::to_string(&response)?)
    }

    /// Execute a Relay connection query with cursor-based (keyset) pagination.
    ///
    /// Reads `first`, `after`, `last`, `before` from `variables`, fetches a page
    /// of rows using `pk_{type}` keyset ordering, and wraps the result in the
    /// Relay `XxxConnection` format:
    /// ```json
    /// {
    ///   "data": {
    ///     "users": {
    ///       "edges": [{ "cursor": "NDI=", "node": { "id": "...", ... } }],
    ///       "pageInfo": {
    ///         "hasNextPage": true, "hasPreviousPage": false,
    ///         "startCursor": "NDI=", "endCursor": "Mw=="
    ///       }
    ///     }
    ///   }
    /// }
    /// ```
    async fn execute_relay_query(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        use crate::{
            compiler::aggregation::OrderByClause,
            runtime::relay::{decode_edge_cursor, decode_uuid_cursor, encode_edge_cursor},
            schema::CursorType,
        };

        let query_def = &query_match.query_def;

        let sql_source = query_def.sql_source.as_deref().ok_or_else(|| {
            FraiseQLError::Validation {
                message: format!(
                    "Relay query '{}' has no sql_source configured",
                    query_def.name
                ),
                path: None,
            }
        })?;

        let cursor_column = query_def.relay_cursor_column.as_deref().ok_or_else(|| {
            FraiseQLError::Validation {
                message: format!(
                    "Relay query '{}' has no relay_cursor_column derived",
                    query_def.name
                ),
                path: None,
            }
        })?;

        // Guard: relay pagination requires the executor to have been constructed
        // via `Executor::new_with_relay` with a `RelayDatabaseAdapter`.
        let relay = self.relay.as_ref().ok_or_else(|| FraiseQLError::Validation {
            message: format!(
                "Relay pagination is not supported by the {} adapter. \
                 Use a relay-capable adapter (e.g. PostgreSQL) and construct \
                 the executor with `Executor::new_with_relay`.",
                self.adapter.database_type()
            ),
            path: None,
        })?;

        // Extract relay pagination arguments from variables.
        let vars = variables.and_then(|v| v.as_object());
        let first: Option<u32> = vars
            .and_then(|v| v.get("first"))
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        let last: Option<u32> = vars
            .and_then(|v| v.get("last"))
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        let after_cursor: Option<&str> =
            vars.and_then(|v| v.get("after")).and_then(|v| v.as_str());
        let before_cursor: Option<&str> =
            vars.and_then(|v| v.get("before")).and_then(|v| v.as_str());

        // Decode base64 cursors — type depends on relay_cursor_type.
        let (after_pk, before_pk) = match query_def.relay_cursor_type {
            CursorType::Int64 => (
                after_cursor.and_then(decode_edge_cursor).map(CursorValue::Int64),
                before_cursor.and_then(decode_edge_cursor).map(CursorValue::Int64),
            ),
            CursorType::Uuid => (
                after_cursor.and_then(decode_uuid_cursor).map(CursorValue::Uuid),
                before_cursor.and_then(decode_uuid_cursor).map(CursorValue::Uuid),
            ),
        };

        // Determine direction and limit.
        // Forward pagination takes priority; fallback to 20 if neither first/last given.
        let (forward, page_size) = if last.is_some() && first.is_none() {
            (false, last.unwrap_or(20))
        } else {
            (true, first.unwrap_or(20))
        };

        // Fetch page_size + 1 rows to detect hasNextPage/hasPreviousPage.
        let fetch_limit = page_size + 1;

        // Parse optional `where` filter from variables.
        let where_clause = if query_def.auto_params.has_where {
            vars.and_then(|v| v.get("where"))
                .map(WhereClause::from_graphql_json)
                .transpose()?
        } else {
            None
        };

        // Parse optional `orderBy` from variables.
        let order_by = if query_def.auto_params.has_order_by {
            vars.and_then(|v| v.get("orderBy"))
                .map(OrderByClause::from_graphql_json)
                .transpose()?
        } else {
            None
        };

        // Detect whether the client selected `totalCount` inside the connection.
        // `query_match.selections` contains the root-level fields of the query (e.g.
        // `users`). `totalCount` is a field *inside* the connection, so we look in the
        // nested_fields of the matched root field.
        //
        // Fragment spreads (e.g. `... on UserConnection { totalCount }`) are NOT
        // resolved here; clients using fragment spreads for totalCount will receive null.
        // Relay compiler and Apollo relay mode always emit totalCount as an inline field.
        // TODO(relay): flatten fragments before this check for full spec compliance.
        let include_total_count = query_match
            .selections
            .iter()
            .find(|sel| sel.name == query_def.name)
            .map(|connection_field| {
                connection_field
                    .nested_fields
                    .iter()
                    .any(|sel| sel.name == "totalCount")
            })
            .unwrap_or(false);

        // Capture before the move into execute_relay_page.
        let had_after = after_pk.is_some();
        let had_before = before_pk.is_some();

        let result = relay
            .execute_relay_page(
                sql_source,
                cursor_column,
                after_pk,
                before_pk,
                fetch_limit,
                forward,
                where_clause.as_ref(),
                order_by.as_deref(),
                include_total_count,
            )
            .await?;

        // Detect whether there are more pages.
        let has_extra = result.rows.len() > page_size as usize;
        let rows: Vec<_> = result.rows.into_iter().take(page_size as usize).collect();

        let (has_next_page, has_previous_page) = if forward {
            (has_extra, had_after)
        } else {
            (had_before, has_extra)
        };

        // Build edges: each edge has { cursor, node }.
        let mut edges = Vec::with_capacity(rows.len());
        let mut start_cursor_str: Option<String> = None;
        let mut end_cursor_str: Option<String> = None;

        for (i, row) in rows.iter().enumerate() {
            let data = &row.data;

            let col_val = data.as_object().and_then(|obj| obj.get(cursor_column));

            let cursor_str = match query_def.relay_cursor_type {
                CursorType::Int64 => col_val
                    .and_then(|v| v.as_i64())
                    .map(encode_edge_cursor)
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: format!(
                            "Relay query '{}': cursor column '{}' not found or not an integer in \
                             result JSONB. Ensure the view exposes this column inside the `data` object.",
                            query_def.name, cursor_column
                        ),
                        path: None,
                    })?,
                CursorType::Uuid => col_val
                    .and_then(|v| v.as_str())
                    .map(crate::runtime::relay::encode_uuid_cursor)
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: format!(
                            "Relay query '{}': cursor column '{}' not found or not a string in \
                             result JSONB. Ensure the view exposes this column inside the `data` object.",
                            query_def.name, cursor_column
                        ),
                        path: None,
                    })?,
            };

            if i == 0 {
                start_cursor_str = Some(cursor_str.clone());
            }
            end_cursor_str = Some(cursor_str.clone());

            edges.push(serde_json::json!({
                "cursor": cursor_str,
                "node": data,
            }));
        }

        let page_info = serde_json::json!({
            "hasNextPage": has_next_page,
            "hasPreviousPage": has_previous_page,
            "startCursor": start_cursor_str,
            "endCursor": end_cursor_str,
        });

        let mut connection = serde_json::json!({
            "edges": edges,
            "pageInfo": page_info,
        });

        // Include totalCount when the client requested it and the adapter provided it.
        if include_total_count {
            if let Some(count) = result.total_count {
                connection["totalCount"] = serde_json::json!(count);
            } else {
                connection["totalCount"] = serde_json::Value::Null;
            }
        }

        let response = ResultProjector::wrap_in_data_envelope(connection, &query_def.name);
        Ok(serde_json::to_string(&response)?)
    }

    /// Execute a GraphQL mutation by calling the configured database function.
    ///
    /// Looks up the `MutationDefinition` in the compiled schema, calls
    /// `execute_function_call` on the database adapter, parses the returned
    /// `mutation_response` row, and builds a GraphQL response containing either the
    /// success entity or a populated error-type object (when the function returns a
    /// `"failed:*"` / `"conflict:*"` / `"error"` status).
    ///
    /// This is the **unauthenticated** variant. It delegates to
    /// `execute_mutation_query_with_security` with `security_ctx = None`, which means
    /// any `inject` params on the mutation definition will cause a
    /// [`FraiseQLError::Validation`] error at runtime (inject requires a security
    /// context).
    ///
    /// # Arguments
    ///
    /// * `mutation_name` - The GraphQL mutation field name (e.g. `"createUser"`)
    /// * `variables` - Optional JSON object of GraphQL variable values
    ///
    /// # Returns
    ///
    /// A JSON-encoded GraphQL response string on success.
    ///
    /// # Errors
    ///
    /// * [`FraiseQLError::Validation`] — mutation name not found in the compiled schema
    /// * [`FraiseQLError::Validation`] — mutation definition has no `sql_source` configured
    /// * [`FraiseQLError::Validation`] — mutation requires `inject` params (needs security ctx)
    /// * [`FraiseQLError::Validation`] — the database function returned no rows
    /// * [`FraiseQLError::Database`] — the adapter's `execute_function_call` returned an error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let vars = serde_json::json!({ "name": "Alice", "email": "alice@example.com" });
    /// // Returns {"data":{"createUser":{"id":"...", "name":"Alice"}}}
    /// // or      {"data":{"createUser":{"__typename":"UserAlreadyExistsError", "email":"..."}}}
    /// let result = executor.execute_mutation_query("createUser", Some(&vars)).await?;
    /// ```
    async fn execute_mutation_query(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        self.execute_mutation_query_with_security(mutation_name, variables, None).await
    }

    /// Internal implementation shared by `execute_mutation_query` and the
    /// security-aware path in `execute_with_security_internal`.
    ///
    /// Callers provide an optional [`SecurityContext`]:
    /// - `None` — unauthenticated path; mutations with `inject` params will fail.
    /// - `Some(ctx)` — authenticated path; `inject` param values are resolved from
    ///   `ctx`'s JWT claims and appended to the positional argument list after the
    ///   client-supplied variables.
    ///
    /// # Arguments
    ///
    /// * `mutation_name` - The GraphQL mutation field name (e.g. `"deletePost"`)
    /// * `variables` - Optional JSON object of client-supplied variable values
    /// * `security_ctx` - Optional authenticated user context; required when the
    ///   mutation definition has one or more `inject` params
    async fn execute_mutation_query_with_security(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
        security_ctx: Option<&SecurityContext>,
    ) -> Result<String> {
        // 1. Locate the mutation definition
        let mutation_def =
            self.schema.find_mutation(mutation_name).ok_or_else(|| {
                let candidates: Vec<&str> =
                    self.schema.mutations.iter().map(|m| m.name.as_str()).collect();
                let suggestion = suggest_similar(mutation_name, &candidates);
                let message = match suggestion.as_slice() {
                    [s] => format!(
                        "Mutation '{mutation_name}' not found in schema. Did you mean '{s}'?"
                    ),
                    [a, b] => format!(
                        "Mutation '{mutation_name}' not found in schema. Did you mean '{a}' or \
                         '{b}'?"
                    ),
                    [a, b, c, ..] => format!(
                        "Mutation '{mutation_name}' not found in schema. Did you mean '{a}', \
                         '{b}', or '{c}'?"
                    ),
                    _ => format!("Mutation '{mutation_name}' not found in schema"),
                };
                FraiseQLError::Validation { message, path: None }
            })?;

        // 2. Require a sql_source (PostgreSQL function name).
        //
        // Fall back to the operation's table field when sql_source is absent.
        // The CLI compiler stores the SQL function name in both places
        // (sql_source and operation.{Insert|Update|Delete}.table), but older or
        // alternate compilation paths (e.g. fraiseql-core's own codegen) may only
        // populate operation.table and leave sql_source as None.
        let sql_source_owned: String;
        let sql_source: &str = if let Some(src) = mutation_def.sql_source.as_deref() {
            src
        } else {
            use crate::schema::MutationOperation;
            match &mutation_def.operation {
                MutationOperation::Insert { table }
                | MutationOperation::Update { table }
                | MutationOperation::Delete { table }
                    if !table.is_empty() =>
                {
                    sql_source_owned = table.clone();
                    &sql_source_owned
                },
                _ => {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Mutation '{mutation_name}' has no sql_source configured"
                        ),
                        path: None,
                    });
                },
            }
        };

        // 3. Build positional args Vec from variables in ArgumentDefinition order.
        //    Validate that every required (non-nullable, no default) argument is present.
        let vars_obj = variables.and_then(|v| v.as_object());

        let mut missing_required: Vec<&str> = Vec::new();
        let mut args: Vec<serde_json::Value> = mutation_def
            .arguments
            .iter()
            .map(|arg| {
                let value = vars_obj.and_then(|obj| obj.get(&arg.name)).cloned();
                match value {
                    Some(v) => v,
                    None => {
                        if !arg.nullable && arg.default_value.is_none() {
                            missing_required.push(&arg.name);
                        }
                        arg.default_value.clone().unwrap_or(serde_json::Value::Null)
                    },
                }
            })
            .collect();

        if !missing_required.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Mutation '{mutation_name}' is missing required argument(s): {}",
                    missing_required.join(", ")
                ),
                path: None,
            });
        }

        // 3a. Append server-injected parameters (after client args, in injection order).
        //
        // CONTRACT: inject params are always the *last* positional parameters of the SQL
        // function, in the order they appear in `inject_params` (insertion-ordered IndexMap).
        // The SQL function signature in the database MUST declare injected parameters after
        // all client-supplied parameters. Violating this order silently passes inject values
        // to the wrong SQL parameters. The CLI compiler (`fraiseql-cli compile`) validates
        // inject key names and source syntax when producing `schema.compiled.json`, but
        // cannot verify SQL function arity — that remains a developer responsibility.
        if !mutation_def.inject_params.is_empty() {
            let ctx = security_ctx.ok_or_else(|| FraiseQLError::Validation {
                message: format!(
                    "Mutation '{}' requires inject params but no security context is available \
                     (unauthenticated request)",
                    mutation_name
                ),
                path: None,
            })?;
            for (param_name, source) in &mutation_def.inject_params {
                args.push(resolve_inject_value(param_name, source, ctx)?);
            }
        }

        // 4. Call the database function
        let rows = self.adapter.execute_function_call(sql_source, &args).await?;

        // 5. Expect at least one row
        let row = rows.into_iter().next().ok_or_else(|| FraiseQLError::Validation {
            message: format!(
                "Mutation '{mutation_name}': function returned no rows"
            ),
            path: None,
        })?;

        // 6. Parse the mutation_response row
        let outcome = parse_mutation_row(&row)?;

        // 6a. Bump fact table versions after a successful mutation.
        //
        // This invalidates cached aggregation results for any fact tables listed
        // in `MutationDefinition.invalidates_fact_tables`.  We bump versions on
        // Success only — an Error outcome means no data was written, so caches
        // remain valid.  Non-cached adapters return Ok(()) from the default trait
        // implementation (no-op); only `CachedDatabaseAdapter` performs actual work.
        if matches!(outcome, MutationOutcome::Success { .. })
            && !mutation_def.invalidates_fact_tables.is_empty()
        {
            self.adapter
                .bump_fact_table_versions(&mutation_def.invalidates_fact_tables)
                .await?;
        }

        // Invalidate query result cache for views/entities touched by this mutation.
        //
        // Strategy:
        // - UPDATE/DELETE with entity_id: entity-aware eviction only (precise, no false positives).
        //   Evicts only the cache entries that actually contain the mutated entity UUID.
        // - CREATE or explicit invalidates_views: view-level flush.
        //   For CREATE the new entity isn't in any existing cache entry, so entity-aware is a
        //   no-op. View-level ensures list queries return the new row.
        // - No entity_id and no views declared: infer view from return type (backward-compat).
        if let MutationOutcome::Success { entity_type, entity_id, .. } = &outcome {
            // Entity-aware path: precise eviction for UPDATE/DELETE.
            if let (Some(etype), Some(eid)) = (entity_type.as_deref(), entity_id.as_deref()) {
                self.adapter.invalidate_by_entity(etype, eid).await?;
            }

            // View-level path: needed when entity_id is absent (CREATE) or when the developer
            // explicitly declared invalidates_views to also refresh list queries.
            if entity_id.is_none() || !mutation_def.invalidates_views.is_empty() {
                let views_to_invalidate = if mutation_def.invalidates_views.is_empty() {
                    self.schema
                        .types
                        .iter()
                        .find(|t| t.name == mutation_def.return_type)
                        .filter(|t| !t.sql_source.is_empty())
                        .map(|t| t.sql_source.clone())
                        .into_iter()
                        .collect::<Vec<_>>()
                } else {
                    mutation_def.invalidates_views.clone()
                };
                if !views_to_invalidate.is_empty() {
                    self.adapter.invalidate_views(&views_to_invalidate).await?;
                }
            }
        }

        // Clone name and return_type to avoid borrow issues after schema lookups
        let mutation_return_type = mutation_def.return_type.clone();
        let mutation_name_owned = mutation_name.to_string();

        let result_json = match outcome {
            MutationOutcome::Success { entity, entity_type, .. } => {
                // Determine the GraphQL __typename
                let typename = entity_type
                    .or_else(|| {
                        // Fall back to first non-error union member
                        self.schema
                            .find_union(&mutation_return_type)
                            .and_then(|u| {
                                u.member_types.iter().find(|t| {
                                    self.schema
                                        .find_type(t)
                                        .map(|td| !td.is_error)
                                        .unwrap_or(true)
                                })
                            })
                            .cloned()
                    })
                    .unwrap_or_else(|| mutation_return_type.clone());

                let mut obj = entity
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                obj.insert(
                    "__typename".to_string(),
                    serde_json::Value::String(typename),
                );
                serde_json::Value::Object(obj)
            },
            MutationOutcome::Error { status, metadata, .. } => {
                // Find the matching error type from the return union
                let error_type = self
                    .schema
                    .find_union(&mutation_return_type)
                    .and_then(|u| {
                        u.member_types.iter().find_map(|t| {
                            let td = self.schema.find_type(t)?;
                            if td.is_error { Some(td) } else { None }
                        })
                    });

                match error_type {
                    Some(td) => {
                        let mut fields =
                            populate_error_fields(&td.fields, &metadata);
                        fields.insert(
                            "__typename".to_string(),
                            serde_json::Value::String(td.name.clone()),
                        );
                        // Include status so the client can act on it
                        fields.insert(
                            "status".to_string(),
                            serde_json::Value::String(status),
                        );
                        serde_json::Value::Object(fields)
                    },
                    None => {
                        // No error type defined: surface the status as a plain object
                        serde_json::json!({ "__typename": mutation_return_type, "status": status })
                    },
                }
            },
        };

        let response = ResultProjector::wrap_in_data_envelope(result_json, &mutation_name_owned);
        Ok(serde_json::to_string(&response)?)
    }


    /// Execute a Relay global `node(id: ID!)` query.
    ///
    /// Decodes the opaque node ID (`base64("TypeName:uuid")`), locates the
    /// appropriate SQL view by searching the compiled schema for a query that
    /// returns that type, and fetches the matching row.
    ///
    /// Returns `{ "data": { "node": <object> } }` on success, or
    /// `{ "data": { "node": null } }` when the object is not found.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` when:
    /// - The `id` argument is missing or malformed
    /// - No SQL view is registered for the requested type
    async fn execute_node_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        use crate::{
            db::{WhereClause, where_clause::WhereOperator},
            runtime::relay::decode_node_id,
        };

        // 1. Extract the raw opaque ID.
        //    Priority: $variables.id > inline literal in query text.
        let raw_id: String = if let Some(id_val) = variables
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("id"))
            .and_then(|v| v.as_str())
        {
            id_val.to_string()
        } else {
            // Fall back to extracting inline literal, e.g. node(id: "NDI=")
            Self::extract_inline_node_id(query).ok_or_else(|| FraiseQLError::Validation {
                message: "node query: missing or unresolvable 'id' argument".to_string(),
                path:    Some("node.id".to_string()),
            })?
        };

        // 2. Decode base64("TypeName:uuid") → (type_name, uuid).
        let (type_name, uuid) = decode_node_id(&raw_id).ok_or_else(|| {
            FraiseQLError::Validation {
                message: format!("node query: invalid node ID '{raw_id}'"),
                path:    Some("node.id".to_string()),
            }
        })?;

        // 3. Find the SQL view for this type.
        //    Convention: look for the first query whose return_type matches.
        let sql_source = self
            .schema
            .queries
            .iter()
            .find(|q| q.return_type == type_name && q.sql_source.is_some())
            .and_then(|q| q.sql_source.as_deref())
            .ok_or_else(|| FraiseQLError::Validation {
                message: format!(
                    "node query: no registered SQL view for type '{type_name}'"
                ),
                path: Some("node.id".to_string()),
            })?
            .to_string();

        // 4. Build WHERE clause: data->>'id' = uuid
        let where_clause = WhereClause::Field {
            path:     vec!["id".to_string()],
            operator: WhereOperator::Eq,
            value:    serde_json::Value::String(uuid),
        };

        // 5. Execute the query (limit 1).
        let rows = self
            .adapter
            .execute_where_query(&sql_source, Some(&where_clause), Some(1), None)
            .await?;

        // 6. Return the first matching row (or null).
        let node_value = rows
            .into_iter()
            .next()
            .map(|row| row.data)
            .unwrap_or(serde_json::Value::Null);

        let response = ResultProjector::wrap_in_data_envelope(node_value, "node");
        Ok(serde_json::to_string(&response)?)
    }

    /// Extract an inline node ID literal from a `node(id: "...")` query string.
    ///
    /// Used as a fallback when the ID is not provided via variables.
    /// Returns `None` if no inline string literal can be found.
    fn extract_inline_node_id(query: &str) -> Option<String> {
        // Look for  node(  ...  id:  "value"  or  id: 'value'
        let after_node = query.find("node(")?;
        let args_region = &query[after_node..];
        // Find `id:` within the argument region.
        let after_id = args_region.find("id:")?;
        let after_colon = args_region[after_id + 3..].trim_start();
        // Expect a quoted string.
        let quote_char = after_colon.chars().next()?;
        if quote_char != '"' && quote_char != '\'' {
            return None;
        }
        let inner = &after_colon[1..];
        let end = inner.find(quote_char)?;
        Some(inner[..end].to_string())
    }

    /// Classify query type based on operation name.
    /// Classify a GraphQL query into its operation type for routing.
    ///
    /// This is the first phase of query execution. It determines which handler
    /// to invoke based on the query structure and conventions:
    ///
    /// - **Introspection** (`__schema`, `__type`) → Uses pre-built responses (zero-cost)
    /// - **Federation** (`_service`, `_entities`) → Fed-specific logic
    /// - **Relay node** (`node(id: "...")`) → Global ID lookup
    /// - **Mutations** (`mutation { ... }`) → Write operations
    /// - **Aggregates** (root field ends with `_aggregate`) → Analytics queries
    /// - **Windows** (root field ends with `_window`) → Time-series queries
    /// - **Regular** (default) → Standard field selections
    ///
    /// # Performance Notes
    ///
    /// - Introspection and federation use cheap text scans (no parsing)
    /// - Other queries require full GraphQL parsing
    /// - Classification result is used to route to specialized handlers
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Parse` if the query string is malformed GraphQL.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Regular query
    /// let query_type = executor.classify_query("{ users { id } }")?;
    /// assert_eq!(query_type, QueryType::Regular);
    ///
    /// // Mutation
    /// let query_type = executor.classify_query("mutation { createUser(...) { id } }")?;
    /// // Routes to execute_mutation_query()
    ///
    /// // Introspection (uses pre-built response)
    /// let query_type = executor.classify_query("{ __schema { types { name } } }")?;
    /// // Routes to introspection.schema_response
    /// ```
    fn classify_query(&self, query: &str) -> Result<QueryType> {
        // Check for introspection queries first (highest priority).
        // These use a cheap text scan to avoid parsing queries that only
        // need the built-in introspection response.
        if let Some(introspection_type) = self.detect_introspection(query) {
            return Ok(introspection_type);
        }

        // Check for federation queries (higher priority than regular queries).
        // Also a text scan — federation queries bypass normal execution.
        if let Some(federation_type) = self.detect_federation(query) {
            return Ok(federation_type);
        }

        // Parse the query to extract the root field name and operation type.
        let parsed = parse_query(query).map_err(|e| FraiseQLError::Parse {
            message:  e.to_string(),
            location: "query".to_string(),
        })?;

        let root_field = &parsed.root_field;

        // Relay global node lookup: root field is exactly "node" on a query operation.
        if parsed.operation_type == "query" && root_field == "node" {
            return Ok(QueryType::NodeQuery);
        }

        // Mutations are routed by operation type
        if parsed.operation_type == "mutation" {
            return Ok(QueryType::Mutation(root_field.clone()));
        }

        // Check if it's an aggregate query (ends with _aggregate)
        if root_field.ends_with("_aggregate") {
            return Ok(QueryType::Aggregate(root_field.clone()));
        }

        // Check if it's a window query (ends with _window)
        if root_field.ends_with("_window") {
            return Ok(QueryType::Window(root_field.clone()));
        }

        // Otherwise, it's a regular query
        Ok(QueryType::Regular)
    }

    /// Detect if a query is an introspection query.
    ///
    /// Returns `Some(QueryType)` for introspection queries, `None` otherwise.
    fn detect_introspection(&self, query: &str) -> Option<QueryType> {
        let query_trimmed = query.trim();

        // Check for __schema query
        if query_trimmed.contains("__schema") {
            return Some(QueryType::IntrospectionSchema);
        }

        // Check for __type(name: "...") query
        if query_trimmed.contains("__type") {
            // Extract the type name from __type(name: "TypeName")
            if let Some(type_name) = self.extract_type_argument(query_trimmed) {
                return Some(QueryType::IntrospectionType(type_name));
            }
            // If no type name found, return schema introspection as fallback
            return Some(QueryType::IntrospectionSchema);
        }

        None
    }

    /// Detect if a query is a federation query (_service or _entities).
    ///
    /// Returns `Some(QueryType)` for federation queries, `None` otherwise.
    fn detect_federation(&self, query: &str) -> Option<QueryType> {
        let query_trimmed = query.trim();

        // Check for _service query
        if query_trimmed.contains("_service") {
            return Some(QueryType::Federation("_service".to_string()));
        }

        // Check for _entities query
        if query_trimmed.contains("_entities") {
            return Some(QueryType::Federation("_entities".to_string()));
        }

        None
    }

    /// Extract the type name argument from `__type(name: "TypeName")`.
    fn extract_type_argument(&self, query: &str) -> Option<String> {
        // Find __type(name: "..." pattern
        // Supports: __type(name: "User"), __type(name:"User"), __type(name: 'User')
        let type_pos = query.find("__type")?;
        let after_type = &query[type_pos + 6..];

        // Find the opening parenthesis
        let paren_pos = after_type.find('(')?;
        let after_paren = &after_type[paren_pos + 1..];

        // Find name: and extract the value
        let name_pos = after_paren.find("name")?;
        let after_name = &after_paren[name_pos + 4..].trim_start();

        // Skip colon
        let after_colon = if let Some(stripped) = after_name.strip_prefix(':') {
            stripped.trim_start()
        } else {
            after_name
        };

        // Extract string value (either "..." or '...')
        let quote_char = after_colon.chars().next()?;
        if quote_char != '"' && quote_char != '\'' {
            return None;
        }

        let after_quote = &after_colon[1..];
        let end_quote = after_quote.find(quote_char)?;
        Some(after_quote[..end_quote].to_string())
    }

    /// Execute an aggregate query dispatch.
    async fn execute_aggregate_dispatch(
        &self,
        query_name: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // Extract table name from query name (e.g., "sales_aggregate" -> "tf_sales")
        let table_name =
            query_name.strip_suffix("_aggregate").ok_or_else(|| FraiseQLError::Validation {
                message: format!("Invalid aggregate query name: {}", query_name),
                path:    None,
            })?;

        let fact_table_name = format!("tf_{}", table_name);

        // Get fact table metadata from schema
        let metadata_json = self.schema.get_fact_table(&fact_table_name).ok_or_else(|| {
            let known: Vec<&str> = self.schema.list_fact_tables();
            let suggestion = suggest_similar(&fact_table_name, &known);
            let base = format!("Fact table '{}' not found in schema", fact_table_name);
            let message = match suggestion.as_slice() {
                [s] => format!("{base}. Did you mean '{s}'?"),
                _ => base,
            };
            FraiseQLError::Validation {
                message,
                path: Some(format!("fact_tables.{}", fact_table_name)),
            }
        })?;

        // Parse metadata into FactTableMetadata
        let metadata: crate::compiler::fact_table::FactTableMetadata =
            serde_json::from_value(metadata_json.clone())?;

        // Parse query variables into aggregate query JSON
        let empty_json = serde_json::json!({});
        let query_json = variables.unwrap_or(&empty_json);

        // Execute aggregate query
        self.execute_aggregate_query(query_json, query_name, &metadata).await
    }

    /// Execute a window query dispatch.
    async fn execute_window_dispatch(
        &self,
        query_name: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // Extract table name from query name (e.g., "sales_window" -> "tf_sales")
        let table_name =
            query_name.strip_suffix("_window").ok_or_else(|| FraiseQLError::Validation {
                message: format!("Invalid window query name: {}", query_name),
                path:    None,
            })?;

        let fact_table_name = format!("tf_{}", table_name);

        // Get fact table metadata from schema
        let metadata_json = self.schema.get_fact_table(&fact_table_name).ok_or_else(|| {
            let known: Vec<&str> = self.schema.list_fact_tables();
            let suggestion = suggest_similar(&fact_table_name, &known);
            let base = format!("Fact table '{}' not found in schema", fact_table_name);
            let message = match suggestion.as_slice() {
                [s] => format!("{base}. Did you mean '{s}'?"),
                _ => base,
            };
            FraiseQLError::Validation {
                message,
                path: Some(format!("fact_tables.{}", fact_table_name)),
            }
        })?;

        // Parse metadata into FactTableMetadata
        let metadata: crate::compiler::fact_table::FactTableMetadata =
            serde_json::from_value(metadata_json.clone())?;

        // Parse query variables into window query JSON
        let empty_json = serde_json::json!({});
        let query_json = variables.unwrap_or(&empty_json);

        // Execute window query
        self.execute_window_query(query_json, query_name, &metadata).await
    }

    /// Execute a federation query (_service or _entities).
    async fn execute_federation_query(
        &self,
        query_name: &str,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        match query_name {
            "_service" => self.execute_service_query().await,
            "_entities" => self.execute_entities_query(query, variables).await,
            _ => Err(FraiseQLError::Validation {
                message: format!("Unknown federation query: {}", query_name),
                path:    None,
            }),
        }
    }

    /// Execute _service query returning federation SDL.
    async fn execute_service_query(&self) -> Result<String> {
        // Get federation metadata from schema
        let fed_metadata =
            self.schema.federation_metadata().ok_or_else(|| FraiseQLError::Validation {
                message: "Federation not enabled in schema".to_string(),
                path:    None,
            })?;

        // Generate SDL with federation directives
        let raw_schema = self.schema.raw_schema();
        let sdl = crate::federation::generate_service_sdl(&raw_schema, &fed_metadata);

        // Return federation response format
        let response = serde_json::json!({
            "data": {
                "_service": {
                    "sdl": sdl
                }
            }
        });

        Ok(serde_json::to_string(&response)?)
    }

    /// Execute _entities query resolving federation entities.
    async fn execute_entities_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // Get federation metadata from schema
        let fed_metadata =
            self.schema.federation_metadata().ok_or_else(|| FraiseQLError::Validation {
                message: "Federation not enabled in schema".to_string(),
                path:    None,
            })?;

        // Extract representations from variables
        let representations_value =
            variables.and_then(|v| v.get("representations")).ok_or_else(|| {
                FraiseQLError::Validation {
                    message: "_entities query requires 'representations' variable".to_string(),
                    path:    None,
                }
            })?;

        // Parse representations
        let representations =
            crate::federation::parse_representations(representations_value, &fed_metadata)
                .map_err(|e| FraiseQLError::Validation {
                    message: format!("Failed to parse representations: {}", e),
                    path:    None,
                })?;

        // Validate representations
        crate::federation::validate_representations(&representations, &fed_metadata).map_err(
            |errors| FraiseQLError::Validation {
                message: format!("Invalid representations: {}", errors.join("; ")),
                path:    None,
            },
        )?;

        // Create federation resolver
        let fed_resolver = crate::federation::FederationResolver::new(fed_metadata);

        // Extract actual field selection from GraphQL query AST
        let selection = match crate::federation::selection_parser::parse_field_selection(query) {
            Ok(sel) if !sel.fields.is_empty() => {
                // Ensure __typename is always selected
                let mut fields = sel.fields;
                if !fields.contains(&"__typename".to_string()) {
                    fields.push("__typename".to_string());
                }
                crate::federation::FieldSelection::new(fields)
            },
            _ => {
                // Fallback to wildcard if parsing fails or no fields extracted
                crate::federation::FieldSelection::new(vec![
                    "__typename".to_string(),
                    "*".to_string(), // Wildcard for all fields (will be expanded by resolver)
                ])
            },
        };

        // Extract or create trace context for federation operations
        // Note: Trace context should ideally be passed from HTTP headers via ExecutionContext,
        // but for now we create a new context for tracing federation operations.
        // The trace context could be injected through the query variables or a request-scoped store
        // in future versions to correlate with the incoming HTTP trace headers.
        let trace_context = crate::federation::FederationTraceContext::new();

        // Batch load entities from database with tracing support
        let entities = crate::federation::batch_load_entities_with_tracing(
            &representations,
            &fed_resolver,
            Arc::clone(&self.adapter),
            &selection,
            Some(trace_context),
        )
        .await?;

        // Return federation response format
        let response = serde_json::json!({
            "data": {
                "_entities": entities
            }
        });

        Ok(serde_json::to_string(&response)?)
    }

    /// Execute a window query.
    ///
    /// # Arguments
    ///
    /// * `query_json` - JSON representation of the window query
    /// * `query_name` - GraphQL field name (e.g., "sales_window")
    /// * `metadata` - Fact table metadata
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query parsing fails
    /// - Execution plan generation fails
    /// - SQL generation fails
    /// - Database execution fails
    /// - Result projection fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let query_json = json!({
    ///     "table": "tf_sales",
    ///     "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
    ///     "windows": [{
    ///         "function": {"type": "row_number"},
    ///         "alias": "rank",
    ///         "partitionBy": [{"type": "dimension", "path": "category"}],
    ///         "orderBy": [{"field": "revenue", "direction": "DESC"}]
    ///     }]
    /// });
    ///
    /// let metadata = /* fact table metadata */;
    /// let result = executor.execute_window_query(&query_json, "sales_window", &metadata).await?;
    /// ```
    pub async fn execute_window_query(
        &self,
        query_json: &serde_json::Value,
        query_name: &str,
        metadata: &crate::compiler::fact_table::FactTableMetadata,
    ) -> Result<String> {
        // 1. Parse JSON query into WindowRequest
        let request = super::WindowQueryParser::parse(query_json, metadata)?;

        // 2. Generate execution plan (validates semantic names against metadata)
        let plan =
            crate::compiler::window_functions::WindowPlanner::plan(request, metadata.clone())?;

        // 3. Generate SQL
        let sql_generator = super::WindowSqlGenerator::new(self.adapter.database_type());
        let sql = sql_generator.generate(&plan)?;

        // 4. Execute SQL
        let rows = self.adapter.execute_raw_query(&sql.complete_sql).await?;

        // 5. Project results
        let projected = super::WindowProjector::project(rows, &plan)?;

        // 6. Wrap in GraphQL data envelope
        let response = super::WindowProjector::wrap_in_data_envelope(projected, query_name);

        // 7. Serialize to JSON string
        Ok(serde_json::to_string(&response)?)
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

    /// Execute an aggregate query.
    ///
    /// # Arguments
    ///
    /// * `query_json` - JSON representation of the aggregate query
    /// * `query_name` - GraphQL field name (e.g., "sales_aggregate")
    /// * `metadata` - Fact table metadata
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query parsing fails
    /// - Execution plan generation fails
    /// - SQL generation fails
    /// - Database execution fails
    /// - Result projection fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let query_json = json!({
    ///     "table": "tf_sales",
    ///     "groupBy": { "category": true },
    ///     "aggregates": [{"count": {}}]
    /// });
    ///
    /// let metadata = /* fact table metadata */;
    /// let result = executor.execute_aggregate_query(&query_json, "sales_aggregate", &metadata).await?;
    /// ```
    pub async fn execute_aggregate_query(
        &self,
        query_json: &serde_json::Value,
        query_name: &str,
        metadata: &crate::compiler::fact_table::FactTableMetadata,
    ) -> Result<String> {
        // 1. Parse JSON query into AggregationRequest
        let request = super::AggregateQueryParser::parse(query_json, metadata)?;

        // 2. Generate execution plan
        let plan =
            crate::compiler::aggregation::AggregationPlanner::plan(request, metadata.clone())?;

        // 3. Generate SQL
        let sql_generator = super::AggregationSqlGenerator::new(self.adapter.database_type());
        let sql = sql_generator.generate(&plan)?;

        // 4. Execute SQL
        let rows = self.adapter.execute_raw_query(&sql.complete_sql).await?;

        // 5. Project results
        let projected = super::AggregationProjector::project(rows, &plan)?;

        // 6. Wrap in GraphQL data envelope
        let response = super::AggregationProjector::wrap_in_data_envelope(projected, query_name);

        // 7. Serialize to JSON string
        Ok(serde_json::to_string(&response)?)
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

/// Resolve a single injected parameter value from the security context.
///
/// Returns `FraiseQLError::Validation` if the source claim is required but absent.
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
/// ```rust,ignore
/// // Executor does this:
/// let user_id = resolve_inject_value("userId", "jwt:sub", &security_ctx)?;
/// let tenant_id = resolve_inject_value("tenantId", "jwt:tenant_id", &security_ctx)?;
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

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::{
        db::{types::JsonbValue, where_clause::WhereClause},
        runtime::JsonbOptimizationOptions,
        schema::{AutoParams, CompiledSchema, QueryDefinition},
    };

    /// Mock database adapter for testing.
    struct MockAdapter {
        mock_results: Vec<JsonbValue>,
    }

    impl MockAdapter {
        fn new(mock_results: Vec<JsonbValue>) -> Self {
            Self { mock_results }
        }
    }

    #[async_trait]
    impl DatabaseAdapter for MockAdapter {
        async fn execute_with_projection(
            &self,
            view: &str,
            _projection: Option<&crate::schema::SqlProjectionHint>,
            where_clause: Option<&WhereClause>,
            limit: Option<u32>,
        ) -> Result<Vec<JsonbValue>> {
            // Fall back to standard query for tests
            self.execute_where_query(view, where_clause, limit, None).await
        }

        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(self.mock_results.clone())
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::PostgreSQL
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics {
                total_connections:  1,
                active_connections: 0,
                idle_connections:   1,
                waiting_requests:   0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            // Mock implementation: return empty results
            Ok(vec![])
        }

        async fn execute_function_call(
            &self,
            _function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

    }

    fn test_schema() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    Vec::new(),
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams::default(),
            deprecation:  None,
            jsonb_column: "data".to_string(),
            relay: false,
            relay_cursor_column: None,
            relay_cursor_type: Default::default(),
            inject_params:     Default::default(),
            cache_ttl_seconds:   None,
            additional_views: vec![],
            requires_role:       None,
        });
        schema
    }

    fn mock_user_results() -> Vec<JsonbValue> {
        vec![
            JsonbValue::new(serde_json::json!({"id": "1", "name": "Alice"})),
            JsonbValue::new(serde_json::json!({"id": "2", "name": "Bob"})),
        ]
    }

    #[tokio::test]
    async fn test_executor_new() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        assert_eq!(executor.schema().queries.len(), 1);
    }

    #[tokio::test]
    async fn test_execute_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let query = "{ users { id name } }";
        let result = executor.execute(query, None).await.unwrap();

        assert!(result.contains("\"data\""));
        assert!(result.contains("\"users\""));
        assert!(result.contains("\"id\""));
        assert!(result.contains("\"name\""));
    }

    #[tokio::test]
    async fn test_execute_json() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let query = "{ users { id name } }";
        let result = executor.execute_json(query, None).await.unwrap();

        assert!(result.get("data").is_some());
        assert!(result["data"].get("users").is_some());
    }

    #[tokio::test]
    async fn test_executor_with_config() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let config = RuntimeConfig {
            cache_query_plans:    false,
            max_query_depth:      5,
            max_query_complexity: 500,
            enable_tracing:       true,
            field_filter:         None,
            rls_policy:           None,
            query_timeout_ms:     30_000,
            jsonb_optimization:   JsonbOptimizationOptions::default(),
        };

        let executor = Executor::with_config(schema, adapter, config);

        assert!(!executor.config().cache_query_plans);
        assert_eq!(executor.config().max_query_depth, 5);
        assert!(executor.config().enable_tracing);
    }

    #[tokio::test]
    async fn test_introspection_schema_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"{ __schema { queryType { name } } }";
        let result = executor.execute(query, None).await.unwrap();

        assert!(result.contains("__schema"));
        assert!(result.contains("Query"));
    }

    #[tokio::test]
    async fn test_introspection_type_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ __type(name: "Int") { kind name } }"#;
        let result = executor.execute(query, None).await.unwrap();

        assert!(result.contains("__type"));
        assert!(result.contains("Int"));
    }

    #[tokio::test]
    async fn test_introspection_unknown_type() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ __type(name: "UnknownType") { kind name } }"#;
        let result = executor.execute(query, None).await.unwrap();

        // Unknown type returns null
        assert!(result.contains("null"));
    }

    #[test]
    fn test_detect_introspection_schema() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"{ __schema { types { name } } }";
        let query_type = executor.classify_query(query).unwrap();
        assert_eq!(query_type, QueryType::IntrospectionSchema);
    }

    #[test]
    fn test_detect_introspection_type() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ __type(name: "User") { fields { name } } }"#;
        let query_type = executor.classify_query(query).unwrap();
        assert_eq!(query_type, QueryType::IntrospectionType("User".to_string()));
    }

    #[test]
    fn test_classify_node_query_inline_id() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ node(id: "VXNlcjoxMjM=") { ... on User { name } } }"#;
        let query_type = executor.classify_query(query).unwrap();
        assert_eq!(query_type, QueryType::NodeQuery);
    }

    #[test]
    fn test_classify_node_query_with_variable() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"query GetNode($id: ID!) { node(id: $id) { id } }";
        let query_type = executor.classify_query(query).unwrap();
        assert_eq!(query_type, QueryType::NodeQuery);
    }

    #[test]
    fn test_classify_node_query_rejects_substring_match() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // "nodeCounts" contains "node(" as a substring — must NOT match
        let query = r#"{ nodeCounts(id: "x") { total } }"#;
        let query_type = executor.classify_query(query).unwrap();
        assert_eq!(query_type, QueryType::Regular);
    }

    #[test]
    fn test_extract_type_argument() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // Double quotes
        let query1 = r#"{ __type(name: "User") { name } }"#;
        assert_eq!(executor.extract_type_argument(query1), Some("User".to_string()));

        // Single quotes
        let query2 = r"{ __type(name: 'Product') { name } }";
        assert_eq!(executor.extract_type_argument(query2), Some("Product".to_string()));

        // No space after colon
        let query3 = r#"{ __type(name:"Query") { name } }"#;
        assert_eq!(executor.extract_type_argument(query3), Some("Query".to_string()));
    }

    // ==================== ExecutionContext Tests ====================

    #[test]
    fn test_execution_context_creation() {
        let ctx = ExecutionContext::new("query-123".to_string());
        assert_eq!(ctx.query_id(), "query-123");
        assert!(!ctx.is_cancelled());
    }

    #[test]
    fn test_execution_context_cancellation_token() {
        let ctx = ExecutionContext::new("query-456".to_string());
        let token = ctx.cancellation_token();
        assert!(!token.is_cancelled());

        // Cancel the token
        token.cancel();
        assert!(token.is_cancelled());
        assert!(ctx.is_cancelled());
    }

    #[tokio::test]
    async fn test_execute_with_context_success() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let ctx = ExecutionContext::new("test-query-1".to_string());
        let query = r"{ __schema { queryType { name } } }";

        let result = executor.execute_with_context(query, None, &ctx).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("__schema"));
    }

    #[tokio::test]
    async fn test_execute_with_context_already_cancelled() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let ctx = ExecutionContext::new("test-query-2".to_string());
        let token = ctx.cancellation_token().clone();

        // Cancel before execution
        token.cancel();

        let query = r"{ __schema { queryType { name } } }";
        let result = executor.execute_with_context(query, None, &ctx).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            FraiseQLError::Cancelled { query_id, reason } => {
                assert_eq!(query_id, "test-query-2");
                assert!(reason.contains("before execution"));
            },
            e => panic!("Expected Cancelled error, got: {}", e),
        }
    }

    #[tokio::test]
    async fn test_execute_with_context_cancelled_during_execution() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let ctx = ExecutionContext::new("test-query-3".to_string());
        let token = ctx.cancellation_token().clone();

        // Spawn a task to cancel after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            token.cancel();
        });

        let query = r"{ __schema { queryType { name } } }";
        let result = executor.execute_with_context(query, None, &ctx).await;

        // Depending on timing, may succeed or be cancelled (both are acceptable)
        // But if cancelled, it should be our error
        if let Err(FraiseQLError::Cancelled { query_id, .. }) = result {
            assert_eq!(query_id, "test-query-3");
        }
    }

    #[test]
    fn test_execution_context_clone() {
        let ctx = ExecutionContext::new("query-clone".to_string());
        let ctx_clone = ctx.clone();

        assert_eq!(ctx.query_id(), ctx_clone.query_id());
        assert!(!ctx_clone.is_cancelled());

        // Cancel original
        ctx.cancellation_token().cancel();

        // Clone should also see cancellation (same token)
        assert!(ctx_clone.is_cancelled());
    }

    #[test]
    fn test_error_cancelled_constructor() {
        let err = FraiseQLError::cancelled("query-001", "user requested cancellation");

        assert!(err.to_string().contains("Query cancelled"));
        assert_eq!(err.status_code(), 408);
        assert_eq!(err.error_code(), "CANCELLED");
        assert!(err.is_retryable());
        assert!(err.is_server_error());
    }

    // ========================================================================

    // ========================================================================

    #[test]
    fn test_jsonb_strategy_in_runtime_config() {
        // Verify that RuntimeConfig includes JSONB optimization options
        let config = RuntimeConfig {
            cache_query_plans:    false,
            max_query_depth:      5,
            max_query_complexity: 500,
            enable_tracing:       true,
            field_filter:         None,
            rls_policy:           None,
            query_timeout_ms:     30_000,
            jsonb_optimization:   JsonbOptimizationOptions::default(),
        };

        assert_eq!(config.jsonb_optimization.default_strategy, JsonbStrategy::Project);
        assert_eq!(config.jsonb_optimization.auto_threshold_percent, 80);
    }

    #[test]
    fn test_jsonb_strategy_custom_config() {
        // Verify custom JSONB strategy options in config
        let custom_options = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 50,
        };

        let config = RuntimeConfig {
            cache_query_plans:    false,
            max_query_depth:      5,
            max_query_complexity: 500,
            enable_tracing:       true,
            field_filter:         None,
            rls_policy:           None,
            query_timeout_ms:     30_000,
            jsonb_optimization:   custom_options,
        };

        assert_eq!(config.jsonb_optimization.default_strategy, JsonbStrategy::Stream);
        assert_eq!(config.jsonb_optimization.auto_threshold_percent, 50);
    }

    // =========================================================================
    // resolve_inject_value unit tests
    // =========================================================================

    fn make_security_ctx(
        user_id: &str,
        tenant_id: Option<&str>,
        extra: &[(&str, serde_json::Value)],
    ) -> SecurityContext {
        use chrono::Utc;
        let now = Utc::now();
        SecurityContext {
            user_id:          user_id.to_string(),
            roles:            vec![],
            tenant_id:        tenant_id.map(str::to_string),
            scopes:           vec![],
            attributes:       extra.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect(),
            request_id:       "test-req".to_string(),
            ip_address:       None,
            authenticated_at: now,
            expires_at:       now + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        }
    }

    #[test]
    fn test_resolve_inject_sub_maps_to_user_id() {
        let ctx = make_security_ctx("user-42", None, &[]);
        let source = InjectedParamSource::Jwt("sub".to_string());
        let result = resolve_inject_value("user_id", &source, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("user-42".to_string()));
    }

    #[test]
    fn test_resolve_inject_tenant_id_claim() {
        let ctx = make_security_ctx("user-1", Some("tenant-abc"), &[]);
        let source = InjectedParamSource::Jwt("tenant_id".to_string());
        let result = resolve_inject_value("tenant_id", &source, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("tenant-abc".to_string()));
    }

    #[test]
    fn test_resolve_inject_org_id_alias() {
        let ctx = make_security_ctx("user-1", Some("org-xyz"), &[]);
        let source = InjectedParamSource::Jwt("org_id".to_string());
        let result = resolve_inject_value("org_id", &source, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("org-xyz".to_string()));
    }

    #[test]
    fn test_resolve_inject_custom_attribute() {
        let ctx = make_security_ctx(
            "user-1",
            None,
            &[("department", serde_json::json!("engineering"))],
        );
        let source = InjectedParamSource::Jwt("department".to_string());
        let result = resolve_inject_value("dept", &source, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("engineering".to_string()));
    }

    #[test]
    fn test_resolve_inject_missing_claim_returns_error() {
        let ctx = make_security_ctx("user-1", None, &[]);
        let source = InjectedParamSource::Jwt("org_id".to_string());
        let err = resolve_inject_value("org_id", &source, &ctx).unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
        let msg = err.to_string();
        assert!(msg.contains("org_id"), "Error should mention claim name");
    }

    #[test]
    fn test_resolve_inject_missing_tenant_id_returns_error() {
        let ctx = make_security_ctx("user-1", None, &[]);
        let source = InjectedParamSource::Jwt("tenant_id".to_string());
        let err = resolve_inject_value("tenant_id", &source, &ctx).unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
    }

    #[tokio::test]
    async fn test_query_with_inject_rejects_unauthenticated() {
        use indexmap::IndexMap;

        let mut schema = test_schema();
        // Add a query that requires inject
        let mut inject_params = IndexMap::new();
        inject_params.insert("org_id".to_string(), InjectedParamSource::Jwt("org_id".to_string()));
        schema.queries.push(QueryDefinition {
            name:                "org_items".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           Vec::new(),
            sql_source:          Some("v_org_items".to_string()),
            description:         None,
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   Default::default(),
            inject_params,
            cache_ttl_seconds:   None,
            additional_views: vec![],
            requires_role:       None,
        });
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // Execute without security context — should fail with Validation error
        let result = executor.execute("{ org_items { id } }", None).await;
        assert!(result.is_err(), "Expected Err for unauthenticated inject query");
        let err = result.unwrap_err();
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "Expected Validation error, got: {err:?}"
        );
    }

    // =========================================================================
    // null_masked_fields tests
    // =========================================================================

    #[test]
    fn test_null_masked_fields_object() {
        let mut value = serde_json::json!({"id": 1, "email": "alice@example.com", "name": "Alice"});
        null_masked_fields(&mut value, &["email".to_string()]);
        assert_eq!(value, serde_json::json!({"id": 1, "email": null, "name": "Alice"}));
    }

    #[test]
    fn test_null_masked_fields_array() {
        let mut value = serde_json::json!([
            {"id": 1, "email": "a@b.com", "salary": 100_000},
            {"id": 2, "email": "c@d.com", "salary": 120_000},
        ]);
        null_masked_fields(&mut value, &["email".to_string(), "salary".to_string()]);
        assert_eq!(
            value,
            serde_json::json!([
                {"id": 1, "email": null, "salary": null},
                {"id": 2, "email": null, "salary": null},
            ])
        );
    }

    #[test]
    fn test_null_masked_fields_no_masked() {
        let mut value = serde_json::json!({"id": 1, "name": "Alice"});
        let original = value.clone();
        null_masked_fields(&mut value, &[]);
        assert_eq!(value, original);
    }

    #[test]
    fn test_plan_query_regular() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let plan = executor.plan_query("{ users { id name } }", None).unwrap();
        assert_eq!(plan.query_type, "regular");
        assert!(plan.sql.contains("v_user"));
        assert_eq!(plan.views_accessed, vec!["v_user"]);
        assert!(plan.estimated_cost > 0);
    }

    #[test]
    fn test_plan_query_introspection() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let plan = executor
            .plan_query("{ __schema { types { name } } }", None)
            .unwrap();
        assert_eq!(plan.query_type, "introspection");
        assert!(plan.sql.is_empty());
        assert!(plan.views_accessed.is_empty());
    }

    #[test]
    fn test_plan_query_empty_rejected() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let result = executor.plan_query("", None);
        assert!(result.is_err());
    }

    // ── Regression tests for issue #53 ──────────────────────────────────────
    //
    // The executor must fall back to operation.table when mutation_def.sql_source
    // is None.  Before the fix, the "has no sql_source configured" error was
    // returned unconditionally whenever sql_source was absent (e.g. when a schema
    // was compiled via the core Rust codegen path rather than the CLI converter).

    /// A mutation compiled without an explicit sql_source (only operation.table set)
    /// must NOT return a "has no sql_source configured" error.  Instead it should
    /// fall back to operation.table and attempt to call the SQL function, which in
    /// this test returns "function returned no rows" (the mock adapter is empty) —
    /// proving the executor reached the function-call stage (issue #53 regression).
    #[tokio::test]
    async fn test_mutation_falls_back_to_operation_table_when_sql_source_none() {
        use crate::schema::{MutationDefinition, MutationOperation};

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            name:       "createUser".to_string(),
            return_type: "User".to_string(),
            // sql_source deliberately absent — simulates codegen path before the fix.
            sql_source: None,
            operation: MutationOperation::Insert {
                table: "fn_create_user".to_string(),
            },
            ..MutationDefinition::new("createUser", "User")
        });

        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let err = executor
            .execute("mutation { createUser { id } }", None)
            .await
            .unwrap_err();

        let msg = err.to_string();
        // Must NOT be the "missing sql_source" error — the fallback must have fired.
        assert!(
            !msg.contains("has no sql_source configured"),
            "executor still failed on missing sql_source instead of using operation.table: {msg}"
        );
        // Must be the downstream "no rows" error — proving the SQL call was attempted.
        assert!(
            msg.contains("function returned no rows") || msg.contains("no rows"),
            "expected 'no rows' error after fallback, got: {msg}"
        );
    }

    /// When both sql_source and operation.table are absent the executor must still
    /// return a clear validation error (not panic or silently succeed).
    #[tokio::test]
    async fn test_mutation_errors_when_both_sql_source_and_table_absent() {
        use crate::schema::{MutationDefinition, MutationOperation};

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            name:       "deleteUser".to_string(),
            return_type: "User".to_string(),
            sql_source: None,
            // Custom operation has no table — no fallback available.
            operation: MutationOperation::Custom,
            ..MutationDefinition::new("deleteUser", "User")
        });

        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let err = executor
            .execute("mutation { deleteUser { id } }", None)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("has no sql_source configured"),
            "expected sql_source error, got: {err}"
        );
    }
}
