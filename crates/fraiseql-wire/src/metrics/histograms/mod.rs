//! Histogram metrics for fraiseql-wire
//!
//! Histograms track distributions of values:
//! - Query timing (startup, total duration)
//! - Chunk processing latency
//! - JSON parsing time
//! - Deserialization time
//! - Row and bytes distributions

use crate::metrics::labels;
use metrics::histogram;

/// Record query startup duration (from submit to first `DataRow`)
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

/// Record channel occupancy (number of items buffered in MPSC channel)
///
/// This metric shows how many rows are currently waiting in the channel,
/// which is a direct indicator of backpressure:
/// - Low values (< 10): Consumer is fast relative to producer
/// - Medium values (50-200): Balanced flow
/// - High values (> 240): Consumer is slow, producer is waiting
pub fn channel_occupancy(entity: &str, items_buffered: u64) {
    histogram!(
        "fraiseql_channel_occupancy_rows",
        labels::ENTITY => entity.to_string(),
    )
    .record(items_buffered as f64);
}

/// Record pause duration (time stream was paused)
///
/// This metric tracks how long streams are paused for, helping identify
/// if pause/resume is being used for backpressure control or diagnostics.
pub fn stream_pause_duration(entity: &str, duration_ms: u64) {
    histogram!(
        "fraiseql_stream_pause_duration_ms",
        labels::ENTITY => entity.to_string(),
    )
    .record(duration_ms as f64);
}

#[cfg(test)]
mod tests;
