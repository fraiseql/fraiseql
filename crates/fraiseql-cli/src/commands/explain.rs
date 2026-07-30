//! Explain command - show query complexity analysis
//!
//! Usage: fraiseql explain `<query>` `[--json]`
//!
//! # What this command does not do
//!
//! It does **not** show compiled SQL. It used to publish a `sql` field, documented as
//! "Compiled SQL representation (if available)", whose value was a hard-coded
//! `SELECT data FROM v_table LIMIT 1000;` — a relation name that appears nowhere else in the
//! codebase — with the query text and metrics pasted into a comment header. The command takes
//! no `--schema` argument, so it could not have produced real SQL even in principle. An
//! adopter (or an agent consuming `--show-output-schema explain`) who took that as the SQL
//! FraiseQL would run got `relation "v_table" does not exist` from psql (#868 item 2).
//!
//! The field is gone rather than faked. Showing real SQL needs the `--schema` argument the old
//! module doc already promised plus the `QueryPlanner`; until that exists, this command
//! reports only what it actually computes: depth, complexity score, alias count and warnings.

use anyhow::Result;
use fraiseql_core::graphql::{DEFAULT_MAX_ALIASES, complexity::RequestValidator, parse_query};
use serde::Serialize;

use crate::output::CommandResult;

/// Response with execution plan and complexity info
#[derive(Debug, Serialize)]
pub struct ExplainResponse {
    /// The analyzed query string
    pub query:          String,
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
///
/// # Errors
///
/// Returns an error if the query cannot be parsed or if complexity analysis
/// fails. Also propagates errors from JSON serialization of the response.
pub fn run(query: &str) -> Result<CommandResult> {
    // Parse the query to validate syntax
    let _parsed = parse_query(query)?;

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

    let has_warnings = !warnings.is_empty();

    let response = ExplainResponse {
        query:          query.to_string(),
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
