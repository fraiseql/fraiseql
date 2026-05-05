//! Query intelligence API endpoints.
//!
//! Provides endpoints for:
//! - Explaining query execution plans with complexity metrics
//! - Validating GraphQL query syntax
//! - Retrieving query statistics and performance data

use axum::{Json, extract::State};
use fraiseql_core::{db::traits::DatabaseAdapter, graphql::DEFAULT_MAX_ALIASES};
use serde::{Deserialize, Serialize};

use crate::{
    routes::{
        api::types::{ApiError, ApiResponse},
        graphql::AppState,
    },
    validation::RequestValidator,
};

/// Request to explain a query.
#[derive(Debug, Deserialize)]
pub struct ExplainRequest {
    /// GraphQL query string to analyze
    pub query:     String,
    /// Optional GraphQL variables
    #[serde(default)]
    pub variables: Option<serde_json::Value>,
}

/// Response from explain endpoint.
#[derive(Debug, Serialize)]
pub struct ExplainResponse {
    /// Original query that was analyzed
    pub query:          String,
    /// Generated SQL equivalent (if available)
    pub sql:            Option<String>,
    /// Complexity metrics for the query
    pub complexity:     ComplexityInfo,
    /// Warning messages for potential issues
    pub warnings:       Vec<String>,
    /// Estimated cost to execute the query
    pub estimated_cost: usize,
    /// Views/tables that would be accessed
    pub views_accessed: Vec<String>,
    /// Query type classification
    pub query_type:     String,
    /// Database-level EXPLAIN output (only when `debug.database_explain` is enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_plan:  Option<serde_json::Value>,
}

/// Complexity information for a query.
///
/// All metrics are computed via AST walking (not character scanning), so
/// operation names, arguments, and directives are never miscounted as fields.
#[derive(Debug, Serialize, Clone, Copy)]
pub struct ComplexityInfo {
    /// Maximum selection-set nesting depth.
    pub depth:       usize,
    /// Complexity score (accounts for pagination multipliers on list fields).
    pub complexity:  usize,
    /// Number of aliased fields (alias amplification indicator).
    pub alias_count: usize,
}

/// Request to validate a query.
#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    /// GraphQL query string to validate
    pub query: String,
}

/// Response from validate endpoint.
#[derive(Debug, Serialize)]
pub struct ValidateResponse {
    /// Whether the query is syntactically valid
    pub valid:  bool,
    /// List of validation errors (if any)
    pub errors: Vec<String>,
}

/// Response from stats endpoint.
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    /// Total number of queries executed
    pub total_queries:      usize,
    /// Number of successful query executions
    pub successful_queries: usize,
    /// Number of failed query executions
    pub failed_queries:     usize,
    /// Average latency in milliseconds
    pub average_latency_ms: f64,
}

/// Explain query execution plan and complexity.
///
/// Analyzes a GraphQL query using AST-based validation and returns:
/// - SQL equivalent
/// - Complexity metrics (depth, complexity score, alias count)
/// - Warnings for potential performance issues
/// - Estimated cost to execute
///
/// # Errors
///
/// Returns `ApiError` with a validation error if the query string is empty.
pub async fn explain_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    Json(req): Json<ExplainRequest>,
) -> Result<Json<ApiResponse<ExplainResponse>>, ApiError> {
    // Validate query is not empty
    if req.query.trim().is_empty() {
        return Err(ApiError::validation_error("Query cannot be empty"));
    }

    // Compute AST-based complexity metrics.
    let validator = RequestValidator::default();
    let metrics = validator
        .analyze(&req.query)
        .map_err(|e| ApiError::validation_error(format!("Query parse error: {e}")))?;

    let complexity = ComplexityInfo {
        depth:       metrics.depth,
        complexity:  metrics.complexity,
        alias_count: metrics.alias_count,
    };

    // Generate warnings for high complexity
    let warnings = generate_warnings(&complexity);

    // Use the real QueryPlanner via Executor::plan_query
    let executor = state.executor();
    let (sql, estimated_cost, views_accessed, query_type, database_plan) =
        match executor.plan_query(&req.query, req.variables.as_ref()) {
            Ok(plan) => {
                // Optionally run DB-level EXPLAIN when debug.database_explain is enabled
                let db_plan =
                    if is_db_explain_enabled(state.debug_config.as_ref()) && !plan.sql.is_empty() {
                        executor
                            .adapter()
                            .explain_query(&plan.sql, &[])
                            .await
                            .inspect_err(|e| tracing::warn!(error = %e, "EXPLAIN query failed"))
                            .ok()
                    } else {
                        None
                    };

                (
                    if plan.sql.is_empty() {
                        None
                    } else {
                        Some(plan.sql)
                    },
                    plan.estimated_cost,
                    plan.views_accessed,
                    plan.query_type,
                    db_plan,
                )
            },
            Err(_) => {
                // Fall back to heuristic cost if the query can't be planned
                // (e.g. schema mismatch, parse errors that passed basic validation)
                (None, estimate_cost(&complexity), Vec::new(), "unknown".to_string(), None)
            },
        };

    let response = ExplainResponse {
        query: req.query,
        sql,
        complexity,
        warnings,
        estimated_cost,
        views_accessed,
        query_type,
        database_plan,
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   response,
    }))
}

/// Validate GraphQL query syntax.
///
/// Performs full AST-based validation on a GraphQL query.
/// Returns a list of any errors found.
///
/// # Errors
///
/// This handler always succeeds; validation errors are reported inside the response body.
pub async fn validate_handler<A: DatabaseAdapter>(
    State(_state): State<AppState<A>>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ApiResponse<ValidateResponse>>, ApiError> {
    if req.query.trim().is_empty() {
        return Ok(Json(ApiResponse {
            status: "success".to_string(),
            data:   ValidateResponse {
                valid:  false,
                errors: vec!["Query cannot be empty".to_string()],
            },
        }));
    }

    // Full AST parse: reports real syntax errors, not brace-matching heuristics.
    let (valid, errors) = match graphql_parser::parse_query::<String>(&req.query) {
        Ok(_) => (true, vec![]),
        Err(e) => (false, vec![e.to_string()]),
    };

    let response = ValidateResponse { valid, errors };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   response,
    }))
}

/// Get query execution statistics.
///
/// Returns aggregated metrics from query executions, read from the in-process
/// atomic counters that the GraphQL handler updates on every request:
/// - Total queries executed
/// - Successful vs failed counts
/// - Average latency in milliseconds (computed from cumulative microseconds)
///
/// Counters reset to zero on server restart (they are not persisted).
///
/// # Errors
///
/// This handler is infallible.
pub async fn stats_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Json<ApiResponse<StatsResponse>>, ApiError> {
    // Get metrics from the metrics collector using atomic operations
    let total_queries = state.metrics.queries_total.load(std::sync::atomic::Ordering::Relaxed);
    let successful_queries =
        state.metrics.queries_success.load(std::sync::atomic::Ordering::Relaxed);
    let failed_queries = state.metrics.queries_error.load(std::sync::atomic::Ordering::Relaxed);
    let total_duration_us =
        state.metrics.queries_duration_us.load(std::sync::atomic::Ordering::Relaxed);

    // Calculate average latency in milliseconds
    #[allow(clippy::cast_precision_loss)] // Reason: precision loss is acceptable for metrics/statistics
    let average_latency_ms = if total_queries > 0 {
        (total_duration_us as f64 / total_queries as f64) / 1000.0
    } else {
        0.0
    };

    #[allow(clippy::cast_possible_truncation)] // Reason: AtomicU64 counters fit in usize on 64-bit targets; saturating is acceptable for
    // display stats
    let response = StatsResponse {
        total_queries: total_queries as usize,
        successful_queries: successful_queries as usize,
        failed_queries: failed_queries as usize,
        average_latency_ms,
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   response,
    }))
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Generate warnings based on AST-based complexity metrics.
pub(crate) fn generate_warnings(complexity: &ComplexityInfo) -> Vec<String> {
    let mut warnings = vec![];

    if complexity.depth > 10 {
        warnings.push(format!(
            "Query nesting depth is {} (threshold: 10). Consider using aliases or fragments.",
            complexity.depth
        ));
    }

    if complexity.complexity > 100 {
        warnings.push(format!(
            "Query complexity score is {} (threshold: 100). This may take longer to execute.",
            complexity.complexity
        ));
    }

    if complexity.alias_count > DEFAULT_MAX_ALIASES {
        warnings.push(format!(
            "Query has {} aliases (threshold: {DEFAULT_MAX_ALIASES}). High alias counts may indicate amplification.",
            complexity.alias_count
        ));
    }

    warnings
}

/// Estimate execution cost based on AST-based complexity metrics.
pub(crate) const fn estimate_cost(complexity: &ComplexityInfo) -> usize {
    let base_cost = 50;
    let depth_cost = complexity.depth.saturating_mul(10);
    let complexity_cost = complexity.complexity.saturating_mul(5);

    base_cost + depth_cost + complexity_cost
}

/// Check whether DB-level EXPLAIN is enabled in the debug configuration.
pub(crate) fn is_db_explain_enabled(debug_config: Option<&fraiseql_core::schema::DebugConfig>) -> bool {
    debug_config.is_some_and(|c| c.enabled && c.database_explain)
}

