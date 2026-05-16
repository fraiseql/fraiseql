//! Query performance statistics types.
//!
//! Provides a database-agnostic representation of query execution statistics
//! surfaced by backends such as PostgreSQL (`pg_stat_statements`), MySQL
//! (`performance_schema`), and SQL Server (`sys.dm_exec_query_stats`).

use serde::{Deserialize, Serialize};

/// A single entry from the database's query performance statistics.
///
/// Each entry represents aggregated execution metrics for one normalized
/// query (identified by `query_id`). The exact semantics of each field
/// depend on the underlying database, but all backends normalize into
/// this common shape.
///
/// Database-specific fields (e.g., PostgreSQL's `shared_blks_hit`) are
/// available in `database_specific` as a free-form JSON value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct QueryStatEntry {
    /// Opaque identifier for the normalized query (e.g., PG `queryid`).
    pub query_id: String,

    /// The normalized query text (parameters replaced with placeholders).
    pub query_text: String,

    /// Total number of times this query has been executed.
    pub calls: u64,

    /// Total execution time across all calls, in milliseconds.
    pub total_exec_time_ms: f64,

    /// Mean execution time per call, in milliseconds.
    pub mean_exec_time_ms: f64,

    /// Minimum execution time observed, in milliseconds.
    pub min_exec_time_ms: f64,

    /// Maximum execution time observed, in milliseconds.
    pub max_exec_time_ms: f64,

    /// Total rows returned across all calls.
    pub rows_returned: u64,

    /// Cache/buffer hit ratio (0.0–1.0), if available from the backend.
    pub cache_hit_ratio: Option<f64>,

    /// Additional backend-specific statistics as free-form JSON.
    pub database_specific: serde_json::Value,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test assertions — panics are the intended failure mode
mod tests;
