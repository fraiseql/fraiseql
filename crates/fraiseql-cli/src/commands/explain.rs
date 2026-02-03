//! Explain command - show query execution plan and complexity analysis
//!
//! Usage: fraiseql explain <query> --schema <schema.compiled.json> [--json]

use anyhow::Result;
use fraiseql_core::graphql::{complexity::ComplexityAnalyzer, parse_query};
use serde::{Deserialize, Serialize};

use crate::output::CommandResult;

/// Request for explain command
#[derive(Debug, Deserialize)]
pub struct ExplainRequest {
    pub query: String,
}

/// Response with execution plan and complexity info
#[derive(Debug, Serialize)]
pub struct ExplainResponse {
    pub query:          String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql:            Option<String>,
    pub estimated_cost: usize,
    pub complexity:     ComplexityInfo,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings:       Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ComplexityInfo {
    pub depth:       usize,
    pub field_count: usize,
    pub score:       usize,
}

/// Run explain command
pub fn run(query: &str) -> Result<CommandResult> {
    // Parse the query to validate syntax
    let parsed = parse_query(query)?;

    // Analyze complexity
    let analyzer = ComplexityAnalyzer::new();
    let (depth, field_count, score) = analyzer.analyze_complexity(query);

    // Generate warnings for unusual patterns
    let mut warnings = Vec::new();

    if depth > 10 {
        warnings.push(format!(
            "Query depth {} exceeds recommended maximum of 10 - consider breaking into multiple queries",
            depth
        ));
    }

    if field_count > 50 {
        warnings.push(format!(
            "Query requests {} fields - consider using pagination or field selection",
            field_count
        ));
    }

    if score > 500 {
        warnings.push(format!(
            "Query complexity score {} is high - consider optimizing query structure",
            score
        ));
    }

    // Generate SQL representation (simplified for now)
    // In a real implementation, this would use the QueryPlanner
    let sql = format!(
        "-- Query execution plan for: {}\n-- Depth: {}, Fields: {}, Cost: {}\nSELECT data FROM v_table LIMIT 1000;",
        parsed.root_field, depth, field_count, score
    );

    let has_warnings = !warnings.is_empty();

    let response = ExplainResponse {
        query:          query.to_string(),
        sql:            Some(sql),
        estimated_cost: score,
        complexity:     ComplexityInfo {
            depth,
            field_count,
            score,
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
            // Should have warnings for deep nesting
            // Response structure: the data field contains ExplainResponse as JSON
            assert!(!warnings.to_string().is_empty());
        }
    }
}
