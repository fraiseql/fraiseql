//! Cost command - lightweight complexity scoring for queries
//!
//! Usage: fraiseql cost `<query>` `[--json]`

use anyhow::Result;
use fraiseql_core::graphql::{complexity::RequestValidator, parse_query};
use serde::Serialize;

use crate::output::CommandResult;

/// Response with cost estimation
#[derive(Debug, Serialize)]
pub struct CostResponse {
    /// The GraphQL query being analyzed
    pub query:            String,
    /// Complexity score based on query depth and breadth (pagination-aware)
    pub complexity_score: usize,
    /// Estimated execution cost
    pub estimated_cost:   usize,
    /// Maximum query depth
    pub depth:            usize,
    /// Number of aliased fields
    pub alias_count:      usize,
}

/// Run cost command (minimal complexity analysis)
///
/// # Errors
///
/// Returns an error if the query cannot be parsed or if complexity analysis
/// fails. Also propagates errors from JSON serialization of the response.
pub fn run(query: &str) -> Result<CommandResult> {
    // Validate query syntax
    let _parsed = parse_query(query)?;

    // AST-based complexity analysis
    let validator = RequestValidator::default();
    let metrics = validator.analyze(query)?;

    let response = CostResponse {
        query:            query.to_string(),
        complexity_score: metrics.complexity,
        estimated_cost:   metrics.depth * 25, // Rough cost estimation
        depth:            metrics.depth,
        alias_count:      metrics.alias_count,
    };

    Ok(CommandResult::success("cost", serde_json::to_value(&response)?))
}
