//! Query intelligence API endpoints.
//!
//! Provides endpoints for:
//! - Explaining query execution plans with complexity metrics
//! - Validating GraphQL query syntax
//! - Retrieving query statistics and performance data

use axum::{
    extract::State,
    Json,
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};
use crate::routes::api::types::{ApiResponse, ApiError};
use crate::routes::graphql::AppState;

/// Request to explain a query.
#[derive(Debug, Deserialize)]
pub struct ExplainRequest {
    /// GraphQL query string to analyze
    pub query: String,
}

/// Response from explain endpoint.
#[derive(Debug, Serialize)]
pub struct ExplainResponse {
    /// Original query that was analyzed
    pub query: String,
    /// Generated SQL equivalent (if available)
    pub sql: Option<String>,
    /// Complexity metrics for the query
    pub complexity: ComplexityInfo,
    /// Warning messages for potential issues
    pub warnings: Vec<String>,
    /// Estimated cost to execute the query
    pub estimated_cost: usize,
}

/// Complexity information for a query.
#[derive(Debug, Serialize, Clone, Copy)]
pub struct ComplexityInfo {
    /// Maximum nesting depth of the query
    pub depth: usize,
    /// Total number of fields requested
    pub field_count: usize,
    /// Combined complexity score (depth × field_count)
    pub score: usize,
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
    pub valid: bool,
    /// List of validation errors (if any)
    pub errors: Vec<String>,
}

/// Response from stats endpoint.
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    /// Total number of queries executed
    pub total_queries: usize,
    /// Number of successful query executions
    pub successful_queries: usize,
    /// Number of failed query executions
    pub failed_queries: usize,
    /// Average latency in milliseconds
    pub average_latency_ms: f64,
}

/// Explain query execution plan and complexity.
///
/// Analyzes a GraphQL query and returns:
/// - SQL equivalent
/// - Complexity metrics (depth, field count, score)
/// - Warnings for potential performance issues
/// - Estimated cost to execute
pub async fn explain_handler<A: DatabaseAdapter>(
    State(_state): State<AppState<A>>,
    Json(req): Json<ExplainRequest>,
) -> Result<Json<ApiResponse<ExplainResponse>>, ApiError> {
    // Validate query is not empty
    if req.query.trim().is_empty() {
        return Err(ApiError::validation_error("Query cannot be empty"));
    }

    // Calculate complexity metrics
    let complexity = calculate_complexity(&req.query);

    // Generate warnings for high complexity
    let warnings = generate_warnings(&complexity);

    // Generate mock SQL (in real implementation, would use QueryPlanner)
    let sql = generate_mock_sql(&req.query);

    let response = ExplainResponse {
        query: req.query,
        sql,
        complexity,
        warnings,
        estimated_cost: estimate_cost(&complexity),
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data: response,
    }))
}

/// Validate GraphQL query syntax.
///
/// Performs basic syntax validation on a GraphQL query.
/// Returns a list of any errors found.
pub async fn validate_handler<A: DatabaseAdapter>(
    State(_state): State<AppState<A>>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ApiResponse<ValidateResponse>>, ApiError> {
    if req.query.trim().is_empty() {
        return Ok(Json(ApiResponse {
            status: "success".to_string(),
            data: ValidateResponse {
                valid: false,
                errors: vec!["Query cannot be empty".to_string()],
            },
        }));
    }

    // Basic syntax check
    let errors = validate_query_syntax(&req.query);
    let valid = errors.is_empty();

    let response = ValidateResponse {
        valid,
        errors,
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data: response,
    }))
}

/// Get query statistics.
///
/// Returns aggregated statistics about query execution performance.
/// Currently returns placeholder data; in production would be populated
/// from metrics collection during query execution.
pub async fn stats_handler<A: DatabaseAdapter>(
    State(_state): State<AppState<A>>,
) -> Result<Json<ApiResponse<StatsResponse>>, ApiError> {
    // In a real implementation, this would:
    // 1. Query the metrics collector in AppState
    // 2. Calculate statistics from observed query executions
    // 3. Return real-time or aggregated metrics

    let response = StatsResponse {
        total_queries: 0,
        successful_queries: 0,
        failed_queries: 0,
        average_latency_ms: 0.0,
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data: response,
    }))
}

// Helper functions

/// Calculate complexity metrics for a GraphQL query.
fn calculate_complexity(query: &str) -> ComplexityInfo {
    let depth = calculate_depth(query);
    let field_count = count_fields(query);
    let score = depth.saturating_mul(field_count);

    ComplexityInfo {
        depth,
        field_count,
        score,
    }
}

/// Calculate maximum nesting depth of braces in a query.
fn calculate_depth(query: &str) -> usize {
    let mut max_depth = 0;
    let mut current_depth = 0;

    for ch in query.chars() {
        match ch {
            '{' => {
                current_depth += 1;
                max_depth = max_depth.max(current_depth);
            }
            '}' => {
                if current_depth > 0 {
                    current_depth -= 1;
                }
            }
            _ => {}
        }
    }

    max_depth
}

/// Count approximate number of fields in a GraphQL query.
/// This is a simple heuristic that counts commas and newlines within braces.
fn count_fields(query: &str) -> usize {
    let mut count = 1; // Start with at least one field
    let mut in_braces = 0;

    for ch in query.chars() {
        match ch {
            '{' => in_braces += 1,
            '}' => {
                if in_braces > 0 {
                    in_braces -= 1;
                }
            }
            ',' => {
                if in_braces > 0 {
                    count += 1;
                }
            }
            '\n' if in_braces > 0 => {
                // Rough approximation: each line in field list is a field
                if !query.contains(',') {
                    count += 1;
                }
            }
            _ => {}
        }
    }

    count.max(1)
}

/// Generate warnings based on complexity metrics.
fn generate_warnings(complexity: &ComplexityInfo) -> Vec<String> {
    let mut warnings = vec![];

    // Warn about deep nesting
    if complexity.depth > 10 {
        warnings.push(format!(
            "Query nesting depth is {} (threshold: 10). Consider using aliases or fragments.",
            complexity.depth
        ));
    }

    // Warn about high complexity score
    if complexity.score > 500 {
        warnings.push(format!(
            "Query complexity score is {} (threshold: 500). This may take longer to execute.",
            complexity.score
        ));
    }

    // Warn about many fields
    if complexity.field_count > 50 {
        warnings.push(format!(
            "Query requests {} fields (threshold: 50). Consider requesting only necessary fields.",
            complexity.field_count
        ));
    }

    warnings
}

/// Estimate execution cost based on complexity.
fn estimate_cost(complexity: &ComplexityInfo) -> usize {
    // Simple cost model: base cost + scaling factor
    let base_cost = 50;
    let depth_cost = complexity.depth.saturating_mul(10);
    let field_cost = complexity.field_count.saturating_mul(5);

    base_cost + depth_cost + field_cost
}

/// Generate mock SQL from a GraphQL query.
/// In a real implementation, this would use fraiseql-core's QueryPlanner.
fn generate_mock_sql(_query: &str) -> Option<String> {
    // Placeholder: In production, would call:
    // let planner = fraiseql_core::runtime::planner::QueryPlanner::new(true);
    // let plan = planner.plan(&parsed)?;
    // Some(plan.sql)

    Some("SELECT * FROM generated_view".to_string())
}

/// Validate GraphQL query syntax.
/// Returns list of syntax errors found.
fn validate_query_syntax(query: &str) -> Vec<String> {
    let mut errors = vec![];

    // Check for basic structure
    if !query.contains('{') || !query.contains('}') {
        errors.push("Query must contain opening and closing braces".to_string());
    }

    // Check brace matching
    let open_braces = query.matches('{').count();
    let close_braces = query.matches('}').count();
    if open_braces != close_braces {
        errors.push(format!(
            "Mismatched braces: {} opening, {} closing",
            open_braces, close_braces
        ));
    }

    // Check for at least query/mutation/subscription keyword
    let has_operation = query.contains("query")
        || query.contains("mutation")
        || query.contains("subscription");

    if !has_operation {
        errors.push("Query must contain query, mutation, or subscription operation".to_string());
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_depth_simple() {
        let depth = calculate_depth("query { users { id } }");
        assert_eq!(depth, 2);
    }

    #[test]
    fn test_calculate_depth_nested() {
        let depth = calculate_depth("query { users { posts { comments { text } } } }");
        assert_eq!(depth, 4);
    }

    #[test]
    fn test_count_fields_single() {
        let count = count_fields("query { users { id } }");
        assert!(count >= 1);
    }

    #[test]
    fn test_generate_warnings_deep() {
        let complexity = ComplexityInfo {
            depth: 15,
            field_count: 5,
            score: 75,
        };
        let warnings = generate_warnings(&complexity);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("depth"));
    }

    #[test]
    fn test_generate_warnings_high_score() {
        let complexity = ComplexityInfo {
            depth: 3,
            field_count: 200,
            score: 600,
        };
        let warnings = generate_warnings(&complexity);
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("complexity")));
    }

    #[test]
    fn test_estimate_cost() {
        let complexity = ComplexityInfo {
            depth: 2,
            field_count: 3,
            score: 6,
        };
        let cost = estimate_cost(&complexity);
        assert!(cost > 0);
    }

    #[test]
    fn test_validate_empty_query() {
        let errors = validate_query_syntax("");
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_validate_mismatched_braces() {
        let errors = validate_query_syntax("query { users { id }");
        assert!(!errors.is_empty());
        assert!(errors[0].contains("Mismatched"));
    }

    #[test]
    fn test_validate_valid_query() {
        let errors = validate_query_syntax("query { users { id } }");
        assert!(errors.is_empty());
    }
}
