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
