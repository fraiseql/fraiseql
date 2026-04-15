//! Runtime query executor - executes compiled queries.
//!
//! # Architecture
//!
//! The runtime loads a `CompiledSchema` and executes incoming GraphQL queries by:
//! 1. Parsing the GraphQL query
//! 2. Matching it to a compiled query template
//! 3. Binding variables
//! 4. Executing the pre-compiled SQL
//! 5. Projecting JSONB results to GraphQL response
//!
//! # Key Concepts
//!
//! - **Zero runtime compilation**: All SQL is pre-compiled
//! - **Pattern matching**: Match incoming query structure to templates
//! - **Variable binding**: Safe parameter substitution
//! - **Result projection**: JSONB → GraphQL JSON transformation
//!
//! # Example
//!
//! ```no_run
//! // Requires: a compiled schema file and a live PostgreSQL database.
//! // See: tests/integration/ for runnable examples.
//! use fraiseql_core::runtime::Executor;
//! use fraiseql_core::schema::CompiledSchema;
//! use fraiseql_core::db::postgres::PostgresAdapter;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let schema_json = r#"{"types":[],"queries":[]}"#;
//! // Load compiled schema
//! let schema = CompiledSchema::from_json(schema_json)?;
//!
//! // Create executor with a concrete adapter implementation
//! let adapter = Arc::new(PostgresAdapter::new("postgresql://localhost/mydb").await?);
//! let executor = Executor::new(schema, adapter);
//!
//! // Execute GraphQL query
//! let query = r#"query { users { id name } }"#;
//! let result = executor.execute(query, None).await?;
//!
//! println!("{}", result);
//! # Ok(())
//! # }
//! ```

mod aggregate_parser;
mod aggregate_projector;
pub mod aggregation;
pub mod cascade;
mod executor;
pub mod executor_adapter;
mod explain;
pub mod field_filter;
pub mod input_validator;
pub mod jsonb_strategy;
mod matcher;
pub mod mutation_result;
pub mod mutation_result_v2;
mod planner;
mod projection;
pub mod query_tracing;
pub mod relay;
pub mod sql_logger;
pub mod subscription;
pub mod tenant_enforcer;
pub mod window;
mod window_parser;
mod window_projector;

use std::sync::Arc;

pub use aggregate_parser::AggregateQueryParser;
pub use aggregate_projector::AggregationProjector;
pub use aggregation::{AggregationSqlGenerator, ParameterizedAggregationSql};
pub use executor::{
    Executor,
    pipeline::{extract_root_field_names, is_multi_root, multi_root_queries_total},
};
pub use executor_adapter::ExecutorAdapter;
pub use explain::{ExplainPlan, ExplainResult};
pub use field_filter::{FieldAccessResult, can_access_field, classify_field_access, filter_fields};
pub use jsonb_strategy::{JsonbOptimizationOptions, JsonbStrategy};
pub use matcher::{QueryMatch, QueryMatcher, suggest_similar};
pub use planner::{ExecutionPlan, QueryPlanner};
pub use projection::{
    FieldMapping, ProjectionMapper, ResultProjector, build_field_mappings_from_type,
};
pub use query_tracing::{
    QueryExecutionTrace, QueryPhaseSpan, QueryTraceBuilder, create_phase_span, create_query_span,
};
pub use sql_logger::{SqlOperation, SqlQueryLog, SqlQueryLogBuilder, create_sql_span};
pub use subscription::{
    ActiveSubscription, DeliveryResult, KafkaAdapter, KafkaConfig, KafkaMessage, SubscriptionError,
    SubscriptionEvent, SubscriptionId, SubscriptionManager, SubscriptionOperation,
    SubscriptionPayload, TransportAdapter, TransportManager, WebhookAdapter, WebhookConfig,
    WebhookPayload, extract_rls_conditions, protocol,
};
pub use tenant_enforcer::TenantEnforcer;

/// Result of a bulk REST operation (collection-level PATCH/DELETE).
#[derive(Debug, Clone)]
pub struct BulkResult {
    /// Number of rows affected.
    pub affected_rows: u64,
    /// Entities returned when `Prefer: return=representation` is set.
    pub entities:      Option<Vec<serde_json::Value>>,
}
pub use window::{WindowSql, WindowSqlGenerator};
pub use window_parser::WindowQueryParser;
pub use window_projector::WindowProjector;

use crate::security::{FieldFilter, FieldFilterConfig, QueryValidatorConfig, RLSPolicy};

/// Runtime configuration for the FraiseQL query executor.
///
/// Controls safety limits, security policies, and performance tuning. All settings
/// have production-safe defaults and can be overridden via the builder-style methods.
///
/// # Defaults
///
/// | Field | Default | Notes |
/// |-------|---------|-------|
/// | `cache_query_plans` | `true` | Caches parsed query plans for repeated queries |
/// | `max_query_depth` | `10` | Prevents stack overflow on recursive GraphQL |
/// | `max_query_complexity` | `1000` | Rough cost model; tune per workload |
/// | `enable_tracing` | `false` | Emit `OpenTelemetry` spans for each query |
/// | `query_timeout_ms` | `30 000` | Hard limit; 0 disables the timeout |
/// | `field_filter` | `None` | No field-level access control |
/// | `rls_policy` | `None` | No row-level security |
///
/// # Example
///
/// ```
/// use fraiseql_core::runtime::RuntimeConfig;
/// use fraiseql_core::security::FieldFilterConfig;
///
/// let config = RuntimeConfig {
///     max_query_depth: 5,
///     max_query_complexity: 500,
///     enable_tracing: true,
///     query_timeout_ms: 5_000,
///     ..RuntimeConfig::default()
/// }
/// .with_field_filter(
///     FieldFilterConfig::new()
///         .protect_field("User", "salary")
///         .protect_field("User", "ssn"),
/// );
/// ```
pub struct RuntimeConfig {
    /// Enable query plan caching.
    pub cache_query_plans: bool,

    /// Maximum query depth (prevents deeply nested queries).
    pub max_query_depth: usize,

    /// Maximum query complexity score.
    pub max_query_complexity: usize,

    /// Enable performance tracing.
    pub enable_tracing: bool,

    /// Optional field filter for access control.
    /// When set, validates that users have required scopes to access fields.
    pub field_filter: Option<FieldFilter>,

    /// Optional row-level security (RLS) policy.
    /// When set, evaluates access rules based on `SecurityContext` to determine
    /// what rows a user can access (e.g., tenant isolation, owner-based access).
    pub rls_policy: Option<Arc<dyn RLSPolicy>>,

    /// Query timeout in milliseconds (0 = no timeout).
    pub query_timeout_ms: u64,

    /// JSONB field optimization strategy options
    pub jsonb_optimization: JsonbOptimizationOptions,

    /// Optional query validation config.
    ///
    /// When `Some`, `QueryValidator::validate()` runs at the start of every
    /// `Executor::execute()` call, before any parsing or SQL dispatch.
    /// This provides `DoS` protection for direct `fraiseql-core` embedders that
    /// do not route through `fraiseql-server` (which already runs `RequestValidator`
    /// at the HTTP layer). Enforces: query size, depth, complexity, and alias count
    /// (alias amplification protection).
    ///
    /// Set `None` to disable (default) — useful when the caller applies
    /// validation at a higher layer, or when `fraiseql-server` is in use.
    pub query_validation: Option<QueryValidatorConfig>,
}

impl std::fmt::Debug for RuntimeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeConfig")
            .field("cache_query_plans", &self.cache_query_plans)
            .field("max_query_depth", &self.max_query_depth)
            .field("max_query_complexity", &self.max_query_complexity)
            .field("enable_tracing", &self.enable_tracing)
            .field("field_filter", &self.field_filter.is_some())
            .field("rls_policy", &self.rls_policy.is_some())
            .field("query_timeout_ms", &self.query_timeout_ms)
            .field("jsonb_optimization", &self.jsonb_optimization)
            .field("query_validation", &self.query_validation)
            .finish()
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            cache_query_plans:    true,
            max_query_depth:      10,
            max_query_complexity: 1000,
            enable_tracing:       false,
            field_filter:         None,
            rls_policy:           None,
            query_timeout_ms:     30_000, // 30 second default timeout
            jsonb_optimization:   JsonbOptimizationOptions::default(),
            query_validation:     None,
        }
    }
}

impl RuntimeConfig {
    /// Create a new runtime config with a field filter.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::runtime::RuntimeConfig;
    /// use fraiseql_core::security::FieldFilterConfig;
    ///
    /// let config = RuntimeConfig::default()
    ///     .with_field_filter(
    ///         FieldFilterConfig::new()
    ///             .protect_field("User", "salary")
    ///             .protect_field("User", "ssn")
    ///     );
    /// ```
    #[must_use]
    pub fn with_field_filter(mut self, config: FieldFilterConfig) -> Self {
        self.field_filter = Some(FieldFilter::new(config));
        self
    }

    /// Configure row-level security (RLS) policy for access control.
    ///
    /// When set, the executor will evaluate the RLS policy before executing queries,
    /// applying WHERE clause filters based on the user's `SecurityContext`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::runtime::RuntimeConfig;
    /// use fraiseql_core::security::DefaultRLSPolicy;
    /// use std::sync::Arc;
    ///
    /// let config = RuntimeConfig::default()
    ///     .with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    /// ```
    #[must_use]
    pub fn with_rls_policy(mut self, policy: Arc<dyn RLSPolicy>) -> Self {
        self.rls_policy = Some(policy);
        self
    }
}

/// Execution context for query cancellation support.
///
/// This struct provides a mechanism for gracefully cancelling long-running queries
/// via cancellation tokens, enabling proper cleanup and error reporting when:
/// - A client connection closes
/// - A user explicitly cancels a query
/// - A system shutdown is initiated
///
/// # Example
///
/// ```no_run
/// // Requires: a running tokio runtime and an Executor with a live database adapter.
/// // See: tests/integration/ for runnable examples.
/// use fraiseql_core::runtime::ExecutionContext;
/// use std::time::Duration;
///
/// let ctx = ExecutionContext::new("query-123".to_string());
///
/// // Spawn a task that cancels after 5 seconds
/// let cancel_token = ctx.cancellation_token().clone();
/// tokio::spawn(async move {
///     tokio::time::sleep(Duration::from_secs(5)).await;
///     cancel_token.cancel();
/// });
///
/// // Execute query with cancellation support
/// // let result = executor.execute_with_context(query, None, &ctx).await;
/// ```
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Unique identifier for tracking the query execution
    query_id: String,

    /// Cancellation token for gracefully stopping the query
    /// When cancelled, ongoing query execution should stop and return a Cancelled error
    token: tokio_util::sync::CancellationToken,
}

impl ExecutionContext {
    /// Create a new execution context with a cancellation token.
    ///
    /// # Arguments
    ///
    /// * `query_id` - Unique identifier for this query execution
    ///
    /// # Example
    ///
    /// ```rust
    /// # use fraiseql_core::runtime::ExecutionContext;
    /// let ctx = ExecutionContext::new("user-query-001".to_string());
    /// assert_eq!(ctx.query_id(), "user-query-001");
    /// ```
    #[must_use]
    pub fn new(query_id: String) -> Self {
        Self {
            query_id,
            token: tokio_util::sync::CancellationToken::new(),
        }
    }

    /// Get the query ID.
    #[must_use]
    pub fn query_id(&self) -> &str {
        &self.query_id
    }

    /// Get a reference to the cancellation token.
    ///
    /// The returned token can be used to:
    /// - Clone and pass to background tasks
    /// - Check if cancellation was requested
    /// - Propagate cancellation through the call stack
    #[must_use]
    pub const fn cancellation_token(&self) -> &tokio_util::sync::CancellationToken {
        &self.token
    }

    /// Check if cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RuntimeConfig::default();
        assert!(config.cache_query_plans);
        assert_eq!(config.max_query_depth, 10);
        assert_eq!(config.max_query_complexity, 1000);
        assert!(!config.enable_tracing);
    }
}
