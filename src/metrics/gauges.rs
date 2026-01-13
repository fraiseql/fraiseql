//! Gauge metrics for fraiseql-wire
//!
//! Gauges track instantaneous values that can increase or decrease:
//! - Current chunk size in bytes
//! - Stream buffered items count
//! - Real-time monitoring of stream health

use metrics::gauge;
use crate::metrics::labels;

/// Record current chunk size
///
/// Called after chunk size adjustments to reflect the current size
/// used for buffering rows from Postgres.
pub fn current_chunk_size(entity: &str, bytes: usize) {
    gauge!(
        "fraiseql_chunk_size_bytes",
        labels::ENTITY => entity.to_string(),
    )
    .set(bytes as f64);
}

/// Record stream buffered items count
///
/// Tracks how many rows are currently buffered in the async channel.
/// High values indicate consumer is slow relative to producer.
pub fn stream_buffered_items(entity: &str, count: usize) {
    gauge!(
        "fraiseql_stream_buffered_items",
        labels::ENTITY => entity.to_string(),
    )
    .set(count as f64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_chunk_size() {
        current_chunk_size("test_entity", 256);
        current_chunk_size("test_entity", 512);
        current_chunk_size("test_entity", 128);
    }

    #[test]
    fn test_stream_buffered_items() {
        stream_buffered_items("test_entity", 0);
        stream_buffered_items("test_entity", 50);
        stream_buffered_items("test_entity", 256);
    }
}
