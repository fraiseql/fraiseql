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
//! let schema = CompiledSchema::from_json(schema_json, false)?;
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
/// Argument-name validation for a root field (GraphQL § 5.4.1).
mod argument_validation;
pub mod cascade;
mod executor;
pub mod executor_adapter;
mod explain;
pub mod field_filter;
pub mod input_validator;
pub mod jsonb_strategy;
mod matcher;
pub mod mutation_result;
pub(crate) mod native_columns;
pub mod partial_period;
mod planner;
pub(crate) mod projection;
pub mod query_tracing;
pub mod relay;
pub mod sql_logger;
pub mod subscription;
pub mod tenant_enforcer;
/// Variable-definition validation for the executed operation (GraphQL § 5.8.3).
mod variable_validation;
pub mod window;
mod window_parser;
mod window_projector;

use std::sync::Arc;

pub use aggregate_parser::AggregateQueryParser;
pub use aggregate_projector::AggregationProjector;
pub use aggregation::{AggregationSqlGenerator, ParameterizedAggregationSql};
pub use argument_validation::validate_argument_names;
pub use executor::{
    Executor,
    JsonRowStream,
    pipeline::{extract_root_field_names, is_multi_root, multi_root_queries_total},
    // Exported so the admin SQL console's RLS preview (#962) resolves an
    // identity's session variables with the *same* function that sets them on a
    // real query. A preview computed by a second implementation is a preview of
    // that implementation.
    security::resolve_session_variables,
};
pub use executor_adapter::ExecutorAdapter;
pub use explain::{ExplainPlan, ExplainResult};
pub use field_filter::{FieldAccessResult, can_access_field, classify_field_access, filter_fields};
pub use jsonb_strategy::{JsonbOptimizationOptions, JsonbStrategy};
pub use matcher::{QueryMatch, QueryMatcher, suggest_similar};
pub use planner::{ExecutionPlan, QueryPlanner};
pub use projection::{
    FieldMapping, ProjectionMapper, ResultProjector, project_entity, project_nested_lists,
    stamp_nested_typenames,
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
pub use variable_validation::{collect_variable_references, validate_variable_uses};

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

use crate::security::{
    Authorizer, FieldAuthorizer, FieldFilter, FieldFilterConfig, QueryValidatorConfig, RLSPolicy,
};

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
/// | `query_validation` | `None` | Derived from compiled `[validation]` limits (#379) |
/// | `enable_tracing` | `false` | Emit `OpenTelemetry` spans for each query |
/// | `query_timeout_ms` | `30 000` | Hard limit; 0 disables the timeout |
/// | `field_filter` | `None` | No field-level access control |
/// | `rls_policy` | `None` | No row-level security |
/// | `authorizer` | `None` | No operation-level authorization |
///
/// # Example
///
/// ```
/// use fraiseql_core::runtime::RuntimeConfig;
/// use fraiseql_core::security::FieldFilterConfig;
///
/// let config = RuntimeConfig {
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
///
/// `Clone` is derived (the trait-object fields are behind `Arc`) so a rebuild —
/// a hot-reload, a per-tenant executor — can start from the live config and
/// re-apply only what the new compiled schema owns, via
/// [`with_compiled_schema`](Self::with_compiled_schema).
#[derive(Clone)]
pub struct RuntimeConfig {
    /// Enable query plan caching.
    pub cache_query_plans: bool,

    /// Maximum number of rows a top-level `first`/`last`/`limit` argument may
    /// request, guarding against unbounded-pagination denial of service (#421):
    /// the top-level row count is the one knob that sizes the database result set
    /// and the serialized response. A request exceeding this is rejected with a
    /// [`crate::FraiseQLError::Validation`]. `None` disables the ceiling. Default
    /// `Some(1000)`.
    pub max_page_size: Option<u32>,

    /// Enable performance tracing.
    pub enable_tracing: bool,

    /// Optional field filter for access control.
    /// When set, validates that users have required scopes to access fields.
    pub field_filter: Option<FieldFilter>,

    /// Optional row-level security (RLS) policy.
    /// When set, evaluates access rules based on `SecurityContext` to determine
    /// what rows a user can access (e.g., tenant isolation, owner-based access).
    pub rls_policy: Option<Arc<dyn RLSPolicy>>,

    /// Optional dynamic field-level authorizer.
    ///
    /// When set, fields marked policy-gated in the compiled schema
    /// ([`FieldDefinition::authorize`](crate::schema::FieldDefinition)) are passed to
    /// this authorizer per row, which returns an allow/deny decision based on the
    /// principal, the parent row, and the field arguments. Composes as a logical AND
    /// with the static `requires_scope` gate and is fail-closed (any error denies).
    /// See [`FieldAuthorizer`].
    pub field_authorizer: Option<Arc<dyn FieldAuthorizer>>,

    /// Optional dynamic operation-level authorizer.
    ///
    /// When set, every operation (query, mutation, subscription) is passed to this
    /// authorizer before dispatch, which returns an allow/deny decision based on the
    /// principal (or `None` when anonymous), the operation kind and name, and the
    /// request input. Composes as a logical AND with the static `requires_role` gate
    /// and is fail-closed (any error or raise denies with HTTP 403 / `FORBIDDEN`).
    /// See [`Authorizer`].
    pub authorizer: Option<Arc<dyn Authorizer>>,

    /// Query timeout in milliseconds (0 = no timeout).
    pub query_timeout_ms: u64,

    /// JSONB field optimization strategy options
    pub jsonb_optimization: JsonbOptimizationOptions,

    /// Optional query validation config.
    ///
    /// When `Some`, `QueryValidator` runs at the start of every
    /// `Executor::execute()` call, before any parsing or SQL dispatch.
    /// This provides `DoS` protection for direct `fraiseql-core` embedders that
    /// do not route through `fraiseql-server` (which already runs `RequestValidator`
    /// at the HTTP layer). Enforces: query size, depth, complexity, and alias count
    /// (alias amplification protection).
    ///
    /// When `None` (the default), the executor derives the gate from the
    /// compiled schema's declared `[validation]` limits at construction (#379),
    /// so an operator's declared bound binds on every transport that reaches
    /// the executor. An embedder that must disable validation despite declared
    /// schema limits can install an explicit all-`usize::MAX` config.
    pub query_validation: Option<QueryValidatorConfig>,

    /// Hard ceiling on a single operation's estimated cost (#379).
    ///
    /// **Schema-derived** — recomputed from the compiled
    /// `[security.cost_budget] per_request_max` on every
    /// [`with_compiled_schema`](Self::with_compiled_schema), never set by the
    /// caller. Enforced in the executor's dispatch alongside GATE-1, so the
    /// operator's declared ceiling binds on every transport that executes a
    /// GraphQL document — not only on `/graphql`.
    pub max_operation_cost: Option<u64>,

    /// Emit structured `tracing` events for every successfully-executed mutation.
    ///
    /// When `true`, a `tracing::info!` event with target `"fraiseql::mutation_audit"` is
    /// emitted at the end of every successful `execute_mutation_query_with_security()` call.
    /// The event carries fields: `mutation_name`, `entity_type`, `operation`, `tenant_id`.
    ///
    /// **Zero-cost when disabled**: the guard `if !self.config.audit_mutations { return }`
    /// short-circuits before any string formatting or allocation occurs.
    ///
    /// Set to `true` when `audit_logging_enabled = true` in the compiled schema's
    /// `[security.enterprise]` section (threaded through `Server::new()` at startup).
    pub audit_mutations: bool,

    /// Global switch for the Change-Spine change-log outbox write (default `true`).
    ///
    /// When `true`, every successful state-changing mutation writes one
    /// `core.tb_entity_change_log` row in-transaction (the framework owns the
    /// write). Set `false` to disable the outbox **globally** — e.g. for an
    /// application that does not consume the Change Spine — so no mutation pays
    /// the write. The per-mutation
    /// [`MutationDefinition.changelog`](crate::schema::MutationDefinition) flag
    /// composes as a logical AND on top of this: a row is written only when the
    /// global switch is on **and** the mutation is not individually opted out.
    ///
    /// Sourced from `[changelog] enabled` in `fraiseql.toml`, overridable at
    /// runtime by `FRAISEQL_CHANGELOG_ENABLED`.
    pub changelog_enabled: bool,

    /// Validate-bind-without-commit mode for mutations (default `false`).
    ///
    /// When `true`, every state-changing mutation is executed inside a database
    /// transaction that is **rolled back** instead of committed: the function
    /// binds and runs (so constraints, triggers, and the `mutation_response`
    /// shape are all validated) but no writes persist and no change-log row is
    /// emitted. Powers the `fraiseql query --dry-run` CLI smoke check and the
    /// `doctor --runtime` mutation probes.
    ///
    /// Currently honoured only by the PostgreSQL adapter; other adapters return
    /// a `Validation` error from `execute_function_call_dry_run` rather than
    /// silently committing. Queries are unaffected (they never commit).
    pub dry_run_mutations: bool,

    /// Response-size guards for the typed cascade surface (graphql-cascade
    /// `16_security`). A cascade mutation returning more affected entities than
    /// [`CascadeLimits::max_updated_entities`] is truncated with `truncated`
    /// metadata; one exceeding [`CascadeLimits::max_response_size_mb`] is
    /// rejected. Same DoS-guard family as [`max_page_size`](Self::max_page_size).
    pub cascade_limits: CascadeLimits,
}

/// Response-size limits for the typed cascade surface, per the graphql-cascade
/// spec's security requirements (`specification/16_security.md`).
///
/// Defaults are the spec's: depth 3, 500 affected entities, 5 `MiB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CascadeLimits {
    /// Maximum relationship depth a cascade may traverse (spec default `3`).
    pub max_depth:            usize,
    /// Maximum number of affected entities (`updated` + `deleted`) before the
    /// cascade is truncated and flagged `truncated` in its metadata (spec
    /// default `500`).
    pub max_updated_entities: usize,
    /// Maximum serialized cascade size in `MiB` before the mutation is rejected
    /// (spec default `5`). A value of `0` **disables** the size ceiling — set it
    /// only to intentionally lift the limit, never as an "unbounded" value reached
    /// by accident.
    pub max_response_size_mb: usize,
}

impl Default for CascadeLimits {
    fn default() -> Self {
        Self {
            max_depth:            3,
            max_updated_entities: 500,
            max_response_size_mb: 5,
        }
    }
}

impl std::fmt::Debug for RuntimeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeConfig")
            .field("cache_query_plans", &self.cache_query_plans)
            .field("max_page_size", &self.max_page_size)
            .field("enable_tracing", &self.enable_tracing)
            .field("field_filter", &self.field_filter.is_some())
            .field("rls_policy", &self.rls_policy.is_some())
            .field("field_authorizer", &self.field_authorizer.is_some())
            .field("authorizer", &self.authorizer.is_some())
            .field("query_timeout_ms", &self.query_timeout_ms)
            .field("jsonb_optimization", &self.jsonb_optimization)
            .field("query_validation", &self.query_validation)
            .field("max_operation_cost", &self.max_operation_cost)
            .field("audit_mutations", &self.audit_mutations)
            .field("changelog_enabled", &self.changelog_enabled)
            .field("dry_run_mutations", &self.dry_run_mutations)
            .field("cascade_limits", &self.cascade_limits)
            .finish()
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            cache_query_plans:  true,
            max_page_size:      Some(1000),
            enable_tracing:     false,
            field_filter:       None,
            rls_policy:         None,
            field_authorizer:   None,
            authorizer:         None,
            query_timeout_ms:   30_000, // 30 second default timeout
            jsonb_optimization: JsonbOptimizationOptions::default(),
            query_validation:   None,
            max_operation_cost: None,
            audit_mutations:    false,
            changelog_enabled:  true,
            dry_run_mutations:  false,
            cascade_limits:     CascadeLimits::default(),
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
    #[must_use = "builder method returns modified builder"]
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
    #[must_use = "builder method returns modified builder"]
    pub fn with_rls_policy(mut self, policy: Arc<dyn RLSPolicy>) -> Self {
        self.rls_policy = Some(policy);
        self
    }

    /// Configure a dynamic field-level authorizer.
    ///
    /// When set, fields marked policy-gated in the compiled schema
    /// ([`FieldDefinition::authorize`](crate::schema::FieldDefinition)) are evaluated
    /// per row by this authorizer. The decision composes as a logical AND with the
    /// static `requires_scope` gate and is fail-closed (any error or raise denies
    /// with HTTP 403 / `FORBIDDEN`). Parallel to [`with_rls_policy`](Self::with_rls_policy).
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::runtime::RuntimeConfig;
    /// use fraiseql_core::security::{
    ///     FieldAuthorizer, FieldAuthzRequest, FieldAuthzDecision,
    /// };
    /// use fraiseql_core::error::Result;
    /// use std::sync::Arc;
    ///
    /// struct AllowAll;
    /// impl FieldAuthorizer for AllowAll {
    ///     fn authorize_field(&self, _req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
    ///         Ok(FieldAuthzDecision::Allow)
    ///     }
    /// }
    ///
    /// let config = RuntimeConfig::default().with_field_authorizer(Arc::new(AllowAll));
    /// ```
    #[must_use = "builder method returns modified builder"]
    pub fn with_field_authorizer(mut self, authorizer: Arc<dyn FieldAuthorizer>) -> Self {
        self.field_authorizer = Some(authorizer);
        self
    }

    /// Override the cascade response-size limits (graphql-cascade `16_security`).
    #[must_use = "builder method returns modified builder"]
    pub const fn with_cascade_limits(mut self, limits: CascadeLimits) -> Self {
        self.cascade_limits = limits;
        self
    }

    /// Configure a dynamic operation-level authorizer.
    ///
    /// When set, every operation (query, mutation, subscription) is passed to this
    /// authorizer before dispatch. The decision composes as a logical AND with the
    /// static `requires_role` gate and is fail-closed (any error or raise denies with
    /// HTTP 403 / `FORBIDDEN`). Parallel to
    /// [`with_field_authorizer`](Self::with_field_authorizer) and
    /// [`with_rls_policy`](Self::with_rls_policy).
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::runtime::RuntimeConfig;
    /// use fraiseql_core::security::{Authorizer, AuthzRequest, AuthzDecision};
    /// use fraiseql_core::error::Result;
    /// use std::sync::Arc;
    ///
    /// struct AllowAll;
    /// impl Authorizer for AllowAll {
    ///     fn authorize(&self, _req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
    ///         Ok(AuthzDecision::Allow)
    ///     }
    /// }
    ///
    /// let config = RuntimeConfig::default().with_authorizer(Arc::new(AllowAll));
    /// ```
    #[must_use = "builder method returns modified builder"]
    pub fn with_authorizer(mut self, authorizer: Arc<dyn Authorizer>) -> Self {
        self.authorizer = Some(authorizer);
        self
    }

    /// Build a [`RuntimeConfig`] from a compiled schema, applying every
    /// schema-derived runtime setting that an executor must honor.
    ///
    /// This is the **single seam** every server entry point routes through so
    /// the config can never drift by constructor (H16): `Server::new`,
    /// `with_relay_pagination`, and `with_flight_service` previously built the
    /// executor with [`RuntimeConfig::default`], silently dropping the
    /// compiled audit-logging flag, the #421 page-size ceiling, and the
    /// change-log toggle, and skipping the schema-format-version check.
    ///
    /// Applied in order:
    /// 1. **Schema-format-version validation** — a legacy schema (no version) warns; an
    ///    incompatible future version is rejected. Coupling the check into the constructor means a
    ///    caller cannot obtain a config while skipping the validation.
    /// 2. **Audit logging** — `audit_mutations` from the compiled `[security.enterprise]
    ///    audit_logging_enabled`.
    /// 3. **Page-size ceiling (#421)** — `FRAISEQL_MAX_PAGE_SIZE` overrides the compiled
    ///    `[validation] max_page_size`, which overrides the default.
    /// 4. **Change-log outbox toggle** — `FRAISEQL_CHANGELOG_ENABLED` overrides the compiled
    ///    `[changelog] write_enabled` (default `true`).
    ///
    /// # Errors
    ///
    /// Returns the validation message when the schema's `schema_format_version`
    /// is incompatible with this runtime.
    pub fn from_compiled_schema(schema: &crate::schema::CompiledSchema) -> Result<Self, String> {
        Self::default().with_compiled_schema(schema)
    }

    /// Re-apply every schema-derived setting on top of an existing config.
    ///
    /// [`from_compiled_schema`](Self::from_compiled_schema) is this method over
    /// [`Self::default`]. The split exists for **rebuilds**: a hot-reload must
    /// re-derive the settings the new compiled schema owns while preserving the
    /// ones the embedder installed programmatically (authorizers, RLS policy,
    /// field filter, validator). Reconstructing from `default()` on reload is
    /// what silently reverted audit logging, the page-size ceiling and the
    /// change-log toggle on every `SIGUSR1` (#750).
    ///
    /// # Errors
    ///
    /// Returns the validation message when the schema's `schema_format_version`
    /// is incompatible with this runtime.
    pub fn with_compiled_schema(
        self,
        schema: &crate::schema::CompiledSchema,
    ) -> Result<Self, String> {
        if schema.schema_format_version.is_none() {
            tracing::warn!(
                "Loaded schema has no schema_format_version (pre-v2.1 format). \
                 Re-compile with the current fraiseql-cli for version compatibility checking."
            );
        }
        schema.validate_format_version()?;

        // Audit logging: security.enterprise.audit_logging_enabled.
        let audit_mutations = schema
            .security
            .as_ref()
            .and_then(|s| s.enterprise.as_ref())
            .is_some_and(|e| e.audit_logging_enabled);
        if audit_mutations {
            tracing::info!("Mutation audit logging enabled (target: fraiseql::mutation_audit)");
        }

        // #421: FRAISEQL_MAX_PAGE_SIZE > compiled [validation] max_page_size > default.
        let max_page_size = page_size_precedence(
            std::env::var("FRAISEQL_MAX_PAGE_SIZE").ok().as_deref(),
            schema.validation_config.as_ref().and_then(|v| v.max_page_size),
        );

        // Change-Spine outbox write toggle (default on): FRAISEQL_CHANGELOG_ENABLED
        // overrides the compiled [changelog] write_enabled.
        let changelog_enabled = std::env::var("FRAISEQL_CHANGELOG_ENABLED")
            .ok()
            .map(|v| {
                !matches!(v.trim().to_ascii_lowercase().as_str(), "false" | "0" | "no" | "off")
            })
            .or_else(|| schema.changelog.as_ref().map(|c| c.write_enabled))
            .unwrap_or(true);
        if !changelog_enabled {
            tracing::info!(
                "Change-log outbox write disabled (FRAISEQL_CHANGELOG_ENABLED / [changelog] write_enabled)"
            );
        }

        // Exhaustive destructuring, deliberately without `..`: adding a field to
        // `RuntimeConfig` is a compile error here until it is classified as
        // schema-derived (recomputed above, so the binding is discarded) or
        // caller-owned (carried through unchanged). A `..Self::default()` tail
        // would instead make a new schema-derived field silently revert to its
        // default on every construction and every reload — the H16 drift this
        // seam exists to prevent, one level down.
        // Declaration order throughout (clippy::inconsistent_struct_constructor).
        // `_`-bound fields are the schema-derived ones, recomputed above; every
        // other field is caller-owned and carried through unchanged.
        // #379: the per-request cost ceiling is declared in the compiled
        // [security.cost_budget] and owned by the schema.
        let max_operation_cost = schema
            .security
            .as_ref()
            .and_then(|s| s.cost_budget.as_ref())
            .and_then(|c| c.per_request_max);

        let Self {
            cache_query_plans,
            max_page_size: _, // schema-derived
            enable_tracing,
            field_filter,
            rls_policy,
            field_authorizer,
            authorizer,
            query_timeout_ms,
            jsonb_optimization,
            query_validation,
            max_operation_cost: _, // schema-derived
            audit_mutations: _,    // schema-derived
            changelog_enabled: _,  // schema-derived
            dry_run_mutations,
            cascade_limits,
        } = self;

        Ok(Self {
            cache_query_plans,
            max_page_size,
            enable_tracing,
            field_filter,
            rls_policy,
            field_authorizer,
            authorizer,
            query_timeout_ms,
            jsonb_optimization,
            query_validation,
            max_operation_cost,
            audit_mutations,
            changelog_enabled,
            dry_run_mutations,
            cascade_limits,
        })
    }
}

/// Resolve the top-level page-size ceiling (#421) by precedence.
///
/// `env` is the raw `FRAISEQL_MAX_PAGE_SIZE` value when set (a positive integer,
/// or `"0"`/`"none"` to disable the ceiling). It overrides `compiled` (the
/// `[validation] max_page_size` from the compiled schema), which overrides the
/// runtime default (1000). Returns `None` only when explicitly disabled.
#[must_use]
pub fn page_size_precedence(env: Option<&str>, compiled: Option<u32>) -> Option<u32> {
    if let Some(raw) = env {
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("none") || trimmed == "0" {
            return None;
        }
        if let Ok(n) = trimmed.parse::<u32>() {
            return Some(n);
        }
        // Unparseable env value: ignore it and fall through to compiled/default.
    }
    compiled.or(RuntimeConfig::default().max_page_size)
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
mod tests;
