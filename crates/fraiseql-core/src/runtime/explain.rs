//! Query explain plan generation.

use serde::Serialize;

/// Explanation of a query's execution plan, returned by the explain endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct ExplainPlan {
    /// The SQL query that would be executed.
    pub sql: String,

    /// Parameter bindings (name → value).
    pub parameters: Vec<(String, serde_json::Value)>,

    /// Estimated cost (from the planner heuristic).
    pub estimated_cost: usize,

    /// Views/tables that would be accessed.
    pub views_accessed: Vec<String>,

    /// Classification of the query ("regular", "mutation", "aggregate", "window", etc.).
    pub query_type: String,
}

/// Result of `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` run against a view
/// with real parameter values.
///
/// Returned by [`Executor::explain`](crate::runtime::Executor::explain) and
/// served by `POST /api/v1/admin/explain`.
#[derive(Debug, Clone, Serialize)]
pub struct ExplainResult {
    /// The GraphQL query name that was explained (e.g., `"users"`).
    pub query_name: String,

    /// The database view the query reads from (e.g., `"v_user"`).
    pub sql_source: String,

    /// The SQL statement passed to `EXPLAIN ANALYZE` (SELECT with WHERE/LIMIT).
    pub generated_sql: String,

    /// The bound parameter values (in positional order, matching `$1`, `$2`, …).
    pub parameters: Vec<serde_json::Value>,

    /// Raw JSON output from `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)`.
    pub explain_output: serde_json::Value,
}
