//! Query executor - main runtime execution engine.
//!
//! # Async Cancellation Strategy
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

use std::{sync::Arc, time::Duration};

use super::{
    ExecutionContext, QueryMatcher, QueryPlanner, ResultProjector, RuntimeConfig, filter_fields,
};
#[cfg(test)]
use crate::db::types::{DatabaseType, PoolMetrics};
use crate::{
    db::{WhereClause, projection_generator::PostgresProjectionGenerator, traits::DatabaseAdapter},
    error::{FraiseQLError, Result},
    graphql::parse_query,
    schema::{CompiledSchema, IntrospectionResponses, SecurityConfig, SqlProjectionHint},
    security::{FieldAccessError, SecurityContext},
};

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
            matcher,
            planner,
            config,
            introspection,
        }
    }

    /// Execute a GraphQL query.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Query variables (optional)
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query is malformed
    /// - Query references undefined operations
    /// - Database execution fails
    /// - Result projection fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let query = r#"query { users { id name } }"#;
    /// let result = executor.execute(query, None).await?;
    /// println!("{}", result);
    /// ```
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

    /// Execute a GraphQL query with row-level security (RLS) context.
    ///
    /// This method applies RLS filtering based on the user's SecurityContext
    /// before executing the query. If an RLS policy is configured in RuntimeConfig,
    /// it will be evaluated to determine what rows the user can access.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Query variables (optional)
    /// * `security_context` - User's security context (authentication + permissions)
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string, or error if access denied by RLS
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
    /// Filters the projection fields based on the user's security context and field scope
    /// requirements. Returns only fields that the user is authorized to access.
    ///
    /// # Arguments
    ///
    /// * `return_type` - The GraphQL return type name (e.g., "User", "Post")
    /// * `projection_fields` - The originally requested field names
    /// * `security_context` - The user's security context with roles
    ///
    /// # Returns
    ///
    /// Filtered list of accessible field names in the same order as requested
    fn apply_field_rbac_filtering(
        &self,
        return_type: &str,
        projection_fields: Vec<String>,
        security_context: &SecurityContext,
    ) -> Result<Vec<String>> {
        // Try to extract security config from compiled schema
        if let Some(ref security_json) = self.schema.security {
            // Deserialize security config
            let security_config: SecurityConfig = serde_json::from_value(security_json.clone())
                .map_err(|_| FraiseQLError::Validation {
                    message: "Invalid security configuration in compiled schema".to_string(),
                    path:    Some("schema.security".to_string()),
                })?;

            // Find the type in the schema
            if let Some(type_def) = self.schema.types.iter().find(|t| t.name == return_type) {
                // Filter fields based on user roles and scope requirements
                let accessible_fields =
                    filter_fields(security_context, &security_config, &type_def.fields);

                // Map back to field names, preserving order from projection_fields
                let accessible_names: std::collections::HashSet<String> =
                    accessible_fields.iter().map(|f| f.name.clone()).collect();

                let filtered: Vec<String> = projection_fields
                    .into_iter()
                    .filter(|name| accessible_names.contains(name))
                    .collect();

                return Ok(filtered);
            }
        }

        // If no security config or type not found, return all projection fields (no filtering)
        Ok(projection_fields)
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
        let projection_hint = if !plan.projection_fields.is_empty() {
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
            None
        };

        // 7. Execute query with RLS WHERE clause filter
        // The database adapter handles composition of RLS filter with user filters
        // and generates the final SQL with both constraints applied
        let results = self
            .adapter
            .execute_with_projection(
                sql_source,
                projection_hint.as_ref(),
                rls_where_clause.as_ref(),
                None,
            )
            .await?;

        // 8. Apply field-level RBAC filtering
        // Filter projection fields based on user roles and field scope requirements
        let filtered_projection_fields = self.apply_field_rbac_filtering(
            &query_match.query_def.return_type,
            plan.projection_fields,
            security_context,
        )?;

        // 9. Project results to accessible fields only
        let projector = ResultProjector::new(filtered_projection_fields);
        let projected = projector.project_results(&results, query_match.query_def.returns_list)?;

        // 10. Wrap in GraphQL data envelope
        let response =
            ResultProjector::wrap_in_data_envelope(projected, &query_match.query_def.name);

        // 11. Serialize to JSON string
        Ok(serde_json::to_string(&response)?)
    }

    async fn execute_regular_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // 1. Match query to compiled template
        let query_match = self.matcher.match_query(query, variables)?;

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
        // This reduces payload by 40-55% by projecting only requested fields at the database level
        let projection_hint = if !plan.projection_fields.is_empty() {
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

    /// Classify query type based on operation name.
    fn classify_query(&self, query: &str) -> Result<QueryType> {
        // Check for introspection queries first (highest priority)
        if let Some(introspection_type) = self.detect_introspection(query) {
            return Ok(introspection_type);
        }

        // Check for federation queries (higher priority than regular queries)
        if let Some(federation_type) = self.detect_federation(query) {
            return Ok(federation_type);
        }

        // Parse the query to extract the root field name
        let parsed = parse_query(query).map_err(|e| FraiseQLError::Parse {
            message:  e.to_string(),
            location: "query".to_string(),
        })?;

        let root_field = &parsed.root_field;

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
            FraiseQLError::Validation {
                message: format!("Fact table '{}' not found in schema", fact_table_name),
                path:    Some(format!("fact_tables.{}", fact_table_name)),
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
            FraiseQLError::Validation {
                message: format!("Fact table '{}' not found in schema", fact_table_name),
                path:    Some(format!("fact_tables.{}", fact_table_name)),
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

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::{
        db::{types::JsonbValue, where_clause::WhereClause},
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
}
