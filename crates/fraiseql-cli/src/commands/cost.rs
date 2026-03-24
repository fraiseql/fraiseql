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
/// Returns an error if the operation fails.
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

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_simple_query() {
        let query = "query { users { id } }";
        let result = run(query);

        assert!(result.is_ok());
        let cmd_result = result.unwrap();
        assert_eq!(cmd_result.status, "success");
    }

    #[test]
    fn test_cost_invalid_query_fails() {
        let query = "query { invalid {";
        let result = run(query);

        assert!(result.is_err());
    }

    #[test]
    fn test_cost_provides_score() {
        let query = "query { users { id name } }";
        let result = run(query);

        assert!(result.is_ok());
        let cmd_result = result.unwrap();
        if let Some(data) = cmd_result.data {
            assert!(data["complexity_score"].is_number());
        }
    }

    #[test]
    fn test_cost_more_fields_higher_score() {
        let few_fields = run("query { users { id } }").unwrap();
        let many_fields = run("query { users { id name email phone address } }").unwrap();

        let few_score = few_fields
            .data
            .as_ref()
            .and_then(|d| d["complexity_score"].as_u64())
            .unwrap_or(0);
        let many_score = many_fields
            .data
            .as_ref()
            .and_then(|d| d["complexity_score"].as_u64())
            .unwrap_or(0);

        assert!(many_score >= few_score);
    }

    #[test]
    fn test_cost_nested_has_higher_score() {
        let shallow = run("query { users { id } }").unwrap();
        let deep = run("query { users { posts { comments { author } } } }").unwrap();

        let shallow_score =
            shallow.data.as_ref().and_then(|d| d["complexity_score"].as_u64()).unwrap_or(0);
        let deep_score =
            deep.data.as_ref().and_then(|d| d["complexity_score"].as_u64()).unwrap_or(0);

        assert!(deep_score > shallow_score);
    }
}
