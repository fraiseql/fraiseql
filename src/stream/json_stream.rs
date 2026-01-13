//! JSON stream implementation

use crate::protocol::BackendMessage;
use crate::{Error, Result};
use bytes::Bytes;
use futures::stream::Stream;
use serde_json::Value;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

/// Stream statistics snapshot
///
/// Provides a read-only view of current stream state without consuming items.
/// All values are point-in-time measurements.
#[derive(Debug, Clone)]
pub struct StreamStats {
    /// Number of items currently buffered in channel (0-256)
    pub items_buffered: usize,
    /// Estimated memory used by buffered items in bytes
    pub estimated_memory: usize,
    /// Total rows yielded to consumer so far
    pub total_rows_yielded: u64,
    /// Total rows filtered out by Rust predicates
    pub total_rows_filtered: u64,
}

impl StreamStats {
    /// Create zero-valued stats
    ///
    /// Useful for testing and initialization.
    pub fn zero() -> Self {
        Self {
            items_buffered: 0,
            estimated_memory: 0,
            total_rows_yielded: 0,
            total_rows_filtered: 0,
        }
    }
}

/// JSON value stream
pub struct JsonStream {
    receiver: mpsc::Receiver<Result<Value>>,
    _cancel_tx: mpsc::Sender<()>, // Dropped when stream is dropped
    entity: String,  // Entity name for metrics
    rows_yielded: Arc<AtomicU64>,  // Counter of items yielded to consumer
    rows_filtered: Arc<AtomicU64>,  // Counter of items filtered
    max_memory: Option<usize>,  // Optional memory limit in bytes
}

impl JsonStream {
    /// Create new JSON stream
    pub(crate) fn new(
        receiver: mpsc::Receiver<Result<Value>>,
        cancel_tx: mpsc::Sender<()>,
        entity: String,
        max_memory: Option<usize>,
    ) -> Self {
        Self {
            receiver,
            _cancel_tx: cancel_tx,
            entity,
            rows_yielded: Arc::new(AtomicU64::new(0)),
            rows_filtered: Arc::new(AtomicU64::new(0)),
            max_memory,
        }
    }

    /// Get current stream statistics
    ///
    /// Returns a snapshot of stream state without consuming any items.
    /// This can be called at any time to monitor progress.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let stats = stream.stats();
    /// println!("Buffered: {}, Yielded: {}", stats.items_buffered, stats.total_rows_yielded);
    /// ```
    pub fn stats(&self) -> StreamStats {
        let items_buffered = self.receiver.len();
        let estimated_memory = items_buffered * 2048;  // Conservative: 2KB per item
        let total_rows_yielded = self.rows_yielded.load(Ordering::Relaxed);
        let total_rows_filtered = self.rows_filtered.load(Ordering::Relaxed);

        StreamStats {
            items_buffered,
            estimated_memory,
            total_rows_yielded,
            total_rows_filtered,
        }
    }

    /// Increment rows yielded counter (called from FilteredStream)
    pub(crate) fn increment_rows_yielded(&self, count: u64) {
        self.rows_yielded.fetch_add(count, Ordering::Relaxed);
    }

    /// Increment rows filtered counter (called from FilteredStream)
    pub(crate) fn increment_rows_filtered(&self, count: u64) {
        self.rows_filtered.fetch_add(count, Ordering::Relaxed);
    }

    /// Clone the yielded counter for passing to background task
    pub(crate) fn clone_rows_yielded(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.rows_yielded)
    }

    /// Clone the filtered counter for passing to background task
    pub(crate) fn clone_rows_filtered(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.rows_filtered)
    }
}

impl Stream for JsonStream {
    type Item = Result<Value>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Record channel occupancy before polling
        let occupancy = self.receiver.len() as u64;
        crate::metrics::histograms::channel_occupancy(&self.entity, occupancy);

        // Check memory limit BEFORE receiving (pre-enqueue strategy)
        // This stops consuming when buffer reaches limit
        if let Some(limit) = self.max_memory {
            let items_buffered = self.receiver.len();
            let estimated_memory = items_buffered * 2048;  // Conservative: 2KB per item

            if estimated_memory > limit {
                // Record metric for memory limit exceeded
                crate::metrics::counters::memory_limit_exceeded(&self.entity);
                return Poll::Ready(Some(Err(Error::MemoryLimitExceeded {
                    limit,
                    current: estimated_memory,
                })));
            }
        }

        self.receiver.poll_recv(cx)
    }
}

/// Extract JSON bytes from DataRow message
pub fn extract_json_bytes(msg: &BackendMessage) -> Result<Bytes> {
    match msg {
        BackendMessage::DataRow(fields) => {
            if fields.len() != 1 {
                return Err(Error::Protocol(format!(
                    "expected 1 field, got {}",
                    fields.len()
                )));
            }

            let field = &fields[0];
            field
                .clone()
                .ok_or_else(|| Error::Protocol("null data field".into()))
        }
        _ => Err(Error::Protocol("expected DataRow".into())),
    }
}

/// Parse JSON bytes into Value
pub fn parse_json(data: Bytes) -> Result<Value> {
    let value: Value = serde_json::from_slice(&data)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_bytes() {
        let data = Bytes::from_static(b"{\"key\":\"value\"}");
        let msg = BackendMessage::DataRow(vec![Some(data.clone())]);

        let extracted = extract_json_bytes(&msg).unwrap();
        assert_eq!(extracted, data);
    }

    #[test]
    fn test_extract_null_field() {
        let msg = BackendMessage::DataRow(vec![None]);
        assert!(extract_json_bytes(&msg).is_err());
    }

    #[test]
    fn test_parse_json() {
        let data = Bytes::from_static(b"{\"key\":\"value\"}");
        let value = parse_json(data).unwrap();

        assert_eq!(value["key"], "value");
    }

    #[test]
    fn test_parse_invalid_json() {
        let data = Bytes::from_static(b"not json");
        assert!(parse_json(data).is_err());
    }

    #[test]
    fn test_stream_stats_creation() {
        let stats = StreamStats::zero();
        assert_eq!(stats.items_buffered, 0);
        assert_eq!(stats.estimated_memory, 0);
        assert_eq!(stats.total_rows_yielded, 0);
        assert_eq!(stats.total_rows_filtered, 0);
    }

    #[test]
    fn test_stream_stats_memory_estimation() {
        let stats = StreamStats {
            items_buffered: 100,
            estimated_memory: 100 * 2048,
            total_rows_yielded: 100,
            total_rows_filtered: 10,
        };

        // 100 items * 2KB per item = 200KB
        assert_eq!(stats.estimated_memory, 204800);
    }

    #[test]
    fn test_stream_stats_clone() {
        let stats = StreamStats {
            items_buffered: 50,
            estimated_memory: 100000,
            total_rows_yielded: 500,
            total_rows_filtered: 50,
        };

        let cloned = stats.clone();
        assert_eq!(cloned.items_buffered, stats.items_buffered);
        assert_eq!(cloned.estimated_memory, stats.estimated_memory);
        assert_eq!(cloned.total_rows_yielded, stats.total_rows_yielded);
        assert_eq!(cloned.total_rows_filtered, stats.total_rows_filtered);
    }
}
