//! Metrics and observability for fraiseql-wire
//!
//! This module provides comprehensive metrics collection for production observability:
//!
//! - **Query metrics**: submissions, completions, latencies, row/byte counts
//! - **Error metrics**: error counts by category and phase
//! - **Stream metrics**: row processing, filtering, deserialization per-type
//! - **Connection metrics**: creation, authentication, state transitions
//! - **Channel metrics**: send latency (backpressure indicator)
//!
//! # Usage
//!
//! Metrics are recorded automatically throughout the query execution pipeline.
//! The `metrics` crate provides a framework-agnostic API that can be used with
//! various exporters (Prometheus, OpenTelemetry, DataDog, etc.).
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_wire::metrics;
//!
//! // Metrics are recorded automatically during query execution
//! let client = fraiseql_wire::FraiseClient::connect("postgres://...").await?;
//! let mut stream = client.query::<serde_json::Value>("users").execute().await?;
//!
//! // Metrics are now being collected:
//! // - fraiseql_queries_total{entity="users", ...}
//! // - fraiseql_query_startup_duration_ms{entity="users"}
//! // - fraiseql_rows_processed_total{entity="users", status="ok"}
//! // - fraiseql_query_total_duration_ms{entity="users"}
//! ```

pub mod counters;
pub mod histograms;
pub mod labels;

pub use counters::*;
pub use histograms::*;
pub use labels::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_labels_exported() {
        // Verify public API
        let _entity = labels::ENTITY;
        let _error = labels::ERROR_CATEGORY;
        let _type_name = labels::TYPE_NAME;
    }

    #[test]
    fn test_counters_exported() {
        // Verify counters are callable (won't panic)
        counters::query_submitted("test", true, false, true);
        counters::query_success("test");
    }

    #[test]
    fn test_histograms_exported() {
        // Verify histograms are callable (won't panic)
        histograms::query_startup_duration("test", 100);
        histograms::chunk_processing_duration("test", 50);
    }
}
