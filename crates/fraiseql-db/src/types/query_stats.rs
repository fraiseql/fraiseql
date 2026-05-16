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
mod tests {
    use super::*;

    #[test]
    fn serializes_to_expected_json_fields() {
        let entry = QueryStatEntry {
            query_id:           "12345".to_string(),
            query_text:         "SELECT * FROM users WHERE id = $1".to_string(),
            calls:              42,
            total_exec_time_ms: 100.5,
            mean_exec_time_ms:  2.39,
            min_exec_time_ms:   0.8,
            max_exec_time_ms:   15.2,
            rows_returned:      42,
            cache_hit_ratio:    Some(0.95),
            database_specific:  serde_json::json!({"shared_blks_hit": 1024}),
        };

        let json = serde_json::to_value(&entry).unwrap();

        assert_eq!(json["query_id"], "12345");
        assert_eq!(json["query_text"], "SELECT * FROM users WHERE id = $1");
        assert_eq!(json["calls"], 42);
        assert_eq!(json["total_exec_time_ms"], 100.5);
        assert_eq!(json["mean_exec_time_ms"], 2.39);
        assert_eq!(json["min_exec_time_ms"], 0.8);
        assert_eq!(json["max_exec_time_ms"], 15.2);
        assert_eq!(json["rows_returned"], 42);
        assert_eq!(json["cache_hit_ratio"], 0.95);
        assert_eq!(json["database_specific"]["shared_blks_hit"], 1024);
    }

    #[test]
    fn deserializes_from_json() {
        let json = serde_json::json!({
            "query_id": "abc",
            "query_text": "SELECT 1",
            "calls": 1,
            "total_exec_time_ms": 0.1,
            "mean_exec_time_ms": 0.1,
            "min_exec_time_ms": 0.1,
            "max_exec_time_ms": 0.1,
            "rows_returned": 0,
            "cache_hit_ratio": null,
            "database_specific": {}
        });

        let entry: QueryStatEntry = serde_json::from_value(json).unwrap();

        assert_eq!(entry.query_id, "abc");
        assert_eq!(entry.calls, 1);
        assert!(entry.cache_hit_ratio.is_none());
    }
}
