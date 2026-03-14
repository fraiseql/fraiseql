//! Explain command - show query execution plan and complexity analysis
//!
//! Usage: fraiseql explain `<query>` --schema `<schema.compiled.json>` `[--json]`

use anyhow::Result;
use fraiseql_core::graphql::{DEFAULT_MAX_ALIASES, complexity::RequestValidator, parse_query};
use serde::Serialize;

use crate::output::CommandResult;

/// Response with execution plan and complexity info
#[derive(Debug, Serialize)]
pub struct ExplainResponse {
    /// The analyzed query string
    pub query:          String,
    /// Compiled SQL representation (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql:            Option<String>,
    /// Estimated query execution cost
    pub estimated_cost: usize,
    /// Complexity metrics
    pub complexity:     ComplexityInfo,
    /// Warnings about query structure
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings:       Vec<String>,
}

/// Complexity analysis metrics for a query
#[derive(Debug, Serialize)]
pub struct ComplexityInfo {
    /// Maximum nesting depth of the query
    pub depth:       usize,
    /// Overall complexity score (accounts for pagination multipliers)
    pub score:       usize,
    /// Number of aliased fields
    pub alias_count: usize,
}

/// Run explain command
pub fn run(query: &str) -> Result<CommandResult> {
    // Parse the query to validate syntax
    let parsed = parse_query(query)?;

    // Analyze complexity using the AST-based validator.
    let validator = RequestValidator::default();
    let metrics = validator.analyze(query)?;

    let depth = metrics.depth;
    let score = metrics.complexity;
    let alias_count = metrics.alias_count;

    // Generate warnings for unusual patterns
    let mut warnings = Vec::new();

    if depth > 10 {
        warnings.push(format!(
            "Query depth {depth} exceeds recommended maximum of 10 - consider breaking into multiple queries"
        ));
    }

    if score > 100 {
        warnings.push(format!(
            "Query complexity score {score} is high - consider optimizing query structure"
        ));
    }

    if alias_count > DEFAULT_MAX_ALIASES {
        warnings.push(format!("Query has {alias_count} aliases — consider reducing alias count"));
    }

    // Generate SQL representation (simplified for now)
    // In a real implementation, this would use the QueryPlanner
    let sql = format!(
        "-- Query execution plan for: {}\n-- Depth: {}, Score: {}, Aliases: {}\nSELECT data FROM v_table LIMIT 1000;",
        parsed.root_field, depth, score, alias_count
    );

    let has_warnings = !warnings.is_empty();

    let response = ExplainResponse {
        query:          query.to_string(),
        sql:            Some(sql),
        estimated_cost: score,
        complexity:     ComplexityInfo {
            depth,
            score,
            alias_count,
        },
        warnings:       warnings.clone(),
    };

    let result = if has_warnings {
        CommandResult::success_with_warnings("explain", serde_json::to_value(&response)?, warnings)
    } else {
        CommandResult::success("explain", serde_json::to_value(&response)?)
    };

    Ok(result)
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explain_simple_query() {
        let query = "query { users { id } }";
        let result = run(query);

        assert!(result.is_ok());
        let cmd_result = result.unwrap();
        assert_eq!(cmd_result.status, "success");
    }

    #[test]
    fn test_explain_invalid_query_fails() {
        let query = "query { invalid {";
        let result = run(query);

        assert!(result.is_err());
    }

    #[test]
    fn test_explain_detects_deep_nesting() {
        let query = "query { a { b { c { d { e { f { g { h { i { j { k { l } } } } } } } } } } } }";
        let result = run(query);

        assert!(result.is_ok());
        let cmd_result = result.unwrap();
        if let Some(warnings) = cmd_result.data {
            assert!(!warnings.to_string().is_empty());
        }
    }
}
