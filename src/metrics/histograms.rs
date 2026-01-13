//! Histogram metrics for fraiseql-wire
//!
//! Histograms track distributions of values:
//! - Query timing (startup, total duration)
//! - Chunk processing latency
//! - JSON parsing time
//! - Deserialization time
//! - Row and bytes distributions

use metrics::histogram;
use crate::metrics::labels;

/// Record query startup duration (from submit to first DataRow)
pub fn query_startup_duration(entity: &str, duration_ms: u64) {
    histogram!(
        "fraiseql_query_startup_duration_ms",
        labels::ENTITY => entity.to_string(),
    )
    .record(duration_ms as f64);
}

/// Record total query execution time
pub fn query_total_duration(entity: &str, duration_ms: u64) {
    histogram!(
        "fraiseql_query_total_duration_ms",
        labels::ENTITY => entity.to_string(),
    )
    .record(duration_ms as f64);
}

/// Record distribution of row counts per query
pub fn query_rows_processed(entity: &str, count: u64) {
    histogram!(
        "fraiseql_query_rows_processed",
        labels::ENTITY => entity.to_string(),
    )
    .record(count as f64);
}

/// Record distribution of bytes received per query
pub fn query_bytes_received(entity: &str, bytes: u64) {
    histogram!(
        "fraiseql_query_bytes_received",
        labels::ENTITY => entity.to_string(),
    )
    .record(bytes as f64);
}

/// Record chunk processing duration
pub fn chunk_processing_duration(entity: &str, duration_ms: u64) {
    histogram!(
        "fraiseql_chunk_processing_duration_ms",
        labels::ENTITY => entity.to_string(),
    )
    .record(duration_ms as f64);
}

/// Record chunk size (rows per chunk)
pub fn chunk_size(entity: &str, rows: u64) {
    histogram!(
        "fraiseql_chunk_size_rows",
        labels::ENTITY => entity.to_string(),
    )
    .record(rows as f64);
}

/// Record JSON parsing duration
pub fn json_parse_duration(entity: &str, duration_ms: u64) {
    histogram!(
        "fraiseql_json_parse_duration_ms",
        labels::ENTITY => entity.to_string(),
    )
    .record(duration_ms as f64);
}

/// Record Rust filter execution duration
pub fn filter_duration(entity: &str, duration_ms: u64) {
    histogram!(
        "fraiseql_filter_duration_ms",
        labels::ENTITY => entity.to_string(),
    )
    .record(duration_ms as f64);
}

/// Record deserialization duration
pub fn deserialization_duration(entity: &str, type_name: &str, duration_ms: u64) {
    histogram!(
        "fraiseql_deserialization_duration_ms",
        labels::ENTITY => entity.to_string(),
        labels::TYPE_NAME => type_name.to_string(),
    )
    .record(duration_ms as f64);
}

/// Record channel send latency (measures backpressure)
pub fn channel_send_latency(entity: &str, duration_ms: u64) {
    histogram!(
        "fraiseql_channel_send_latency_ms",
        labels::ENTITY => entity.to_string(),
    )
    .record(duration_ms as f64);
}

/// Record authentication duration
pub fn auth_duration(mechanism: &str, duration_ms: u64) {
    histogram!(
        "fraiseql_auth_duration_ms",
        labels::MECHANISM => mechanism.to_string(),
    )
    .record(duration_ms as f64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_startup_duration() {
        query_startup_duration("test_entity", 100);
        query_startup_duration("test_entity", 250);
        query_startup_duration("test_entity", 50);
    }

    #[test]
    fn test_query_total_duration() {
        query_total_duration("test_entity", 1000);
        query_total_duration("test_entity", 500);
    }

    #[test]
    fn test_query_rows_processed() {
        query_rows_processed("test_entity", 100);
        query_rows_processed("test_entity", 5000);
        query_rows_processed("test_entity", 1);
    }

    #[test]
    fn test_chunk_processing_duration() {
        chunk_processing_duration("test_entity", 10);
        chunk_processing_duration("test_entity", 25);
    }

    #[test]
    fn test_chunk_size() {
        chunk_size("test_entity", 256);
        chunk_size("test_entity", 128);
        chunk_size("test_entity", 42);
    }

    #[test]
    fn test_deserialization_duration() {
        deserialization_duration("test_entity", "User", 5);
        deserialization_duration("test_entity", "Project", 8);
    }

    #[test]
    fn test_auth_duration() {
        auth_duration(crate::metrics::labels::MECHANISM_SCRAM, 150);
        auth_duration(crate::metrics::labels::MECHANISM_CLEARTEXT, 10);
    }
}
