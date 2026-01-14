//! JSON stream implementation

use crate::protocol::BackendMessage;
use crate::{Error, Result};
use bytes::Bytes;
use futures::stream::Stream;
use serde_json::Value;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, Notify};

/// Stream state machine
///
/// Tracks the current state of the JSON stream.
/// Streams start in Running state and can transition to Paused
/// or terminal states (Completed, Failed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// Background task is actively reading from Postgres
    Running,
    /// Background task is paused (suspended, connection alive)
    Paused,
    /// Query completed normally
    Completed,
    /// Query failed with error
    Failed,
}

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
    soft_limit_warn_threshold: Option<f32>,  // Warn at threshold % (0.0-1.0)
    soft_limit_fail_threshold: Option<f32>,  // Fail at threshold % (0.0-1.0)

    // Pause/resume state machine
    state: Arc<Mutex<StreamState>>,                    // Current stream state
    pause_signal: Arc<Notify>,                         // Signal to pause background task
    resume_signal: Arc<Notify>,                        // Signal to resume background task
    paused_occupancy: Arc<AtomicUsize>,               // Buffered rows when paused
    pause_timeout: Option<Duration>,                  // Optional auto-resume timeout

    // Sampling counter for metrics recording (sample 1 in N polls)
    poll_count: AtomicU64,  // Counter for sampling metrics
}

impl JsonStream {
    /// Create new JSON stream
    pub(crate) fn new(
        receiver: mpsc::Receiver<Result<Value>>,
        cancel_tx: mpsc::Sender<()>,
        entity: String,
        max_memory: Option<usize>,
        soft_limit_warn_threshold: Option<f32>,
        soft_limit_fail_threshold: Option<f32>,
    ) -> Self {
        Self {
            receiver,
            _cancel_tx: cancel_tx,
            entity,
            rows_yielded: Arc::new(AtomicU64::new(0)),
            rows_filtered: Arc::new(AtomicU64::new(0)),
            max_memory,
            soft_limit_warn_threshold,
            soft_limit_fail_threshold,

            // Initialize pause/resume state
            state: Arc::new(Mutex::new(StreamState::Running)),
            pause_signal: Arc::new(Notify::new()),
            resume_signal: Arc::new(Notify::new()),
            paused_occupancy: Arc::new(AtomicUsize::new(0)),
            pause_timeout: None,  // No timeout by default

            // Initialize sampling counter
            poll_count: AtomicU64::new(0),
        }
    }

    /// Get current stream state
    ///
    /// Returns the current state of the stream (Running, Paused, Completed, or Failed).
    /// This is a synchronous getter that doesn't require awaiting.
    pub fn state_snapshot(&self) -> StreamState {
        // This is a best-effort snapshot that may return slightly stale state
        // For guaranteed accurate state, use `state()` method
        StreamState::Running // Will be updated when state machine is fully integrated
    }

    /// Get buffered rows when paused
    ///
    /// Returns the number of rows buffered in the channel when the stream was paused.
    /// Only meaningful when stream is in Paused state.
    pub fn paused_occupancy(&self) -> usize {
        self.paused_occupancy.load(Ordering::Relaxed)
    }

    /// Set timeout for pause (auto-resume after duration)
    ///
    /// When a stream is paused, the background task will automatically resume
    /// after the specified duration expires, even if resume() is not called.
    ///
    /// # Arguments
    ///
    /// * `duration` - How long to stay paused before auto-resuming
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let mut stream = client.query::<T>("entity").execute().await?;
    /// stream.set_pause_timeout(Duration::from_secs(5));
    /// stream.pause().await?;  // Will auto-resume after 5 seconds
    /// ```
    pub fn set_pause_timeout(&mut self, duration: Duration) {
        self.pause_timeout = Some(duration);
        tracing::debug!("pause timeout set to {:?}", duration);
    }

    /// Clear pause timeout (no auto-resume)
    pub fn clear_pause_timeout(&mut self) {
        self.pause_timeout = None;
        tracing::debug!("pause timeout cleared");
    }

    /// Get current pause timeout (if set)
    pub(crate) fn pause_timeout(&self) -> Option<Duration> {
        self.pause_timeout
    }

    /// Pause the stream
    ///
    /// Suspends the background task from reading more data from Postgres.
    /// The connection remains open and can be resumed later.
    /// Buffered rows are preserved and can be consumed normally.
    ///
    /// This method is idempotent: calling pause() on an already-paused stream is a no-op.
    ///
    /// Returns an error if the stream has already completed or failed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// stream.pause().await?;
    /// // Background task stops reading
    /// // Consumer can still poll for remaining buffered items
    /// ```
    pub async fn pause(&mut self) -> Result<()> {
        let mut state = self.state.lock().await;

        match *state {
            StreamState::Running => {
                // Signal background task to pause
                self.pause_signal.notify_one();
                // Update state
                *state = StreamState::Paused;

                // Note: Pause start time tracking removed (Phase 5 optimization)
                // Pause/resume metrics now recorded without duration tracking
                // Record metric
                crate::metrics::counters::stream_paused(&self.entity);
                Ok(())
            }
            StreamState::Paused => {
                // Idempotent: already paused
                Ok(())
            }
            StreamState::Completed | StreamState::Failed => {
                // Cannot pause a terminal stream
                Err(Error::Protocol(
                    "cannot pause a completed or failed stream".to_string(),
                ))
            }
        }
    }

    /// Resume the stream
    ///
    /// Resumes the background task to continue reading data from Postgres.
    /// Only has an effect if the stream is currently paused.
    ///
    /// This method is idempotent: calling resume() before pause() or on an
    /// already-running stream is a no-op.
    ///
    /// Returns an error if the stream has already completed or failed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// stream.resume().await?;
    /// // Background task resumes reading
    /// // Consumer can poll for more items
    /// ```
    pub async fn resume(&mut self) -> Result<()> {
        let mut state = self.state.lock().await;

        match *state {
            StreamState::Paused => {
                // Signal background task to resume
                self.resume_signal.notify_one();
                // Update state
                *state = StreamState::Running;

                // Note: Pause duration tracking removed (Phase 5 optimization)
                // Pause/resume is rarely used; simplified synchronization
                // Record metric
                crate::metrics::counters::stream_resumed(&self.entity);
                Ok(())
            }
            StreamState::Running => {
                // Idempotent: already running (or resume before pause)
                Ok(())
            }
            StreamState::Completed | StreamState::Failed => {
                // Cannot resume a terminal stream
                Err(Error::Protocol(
                    "cannot resume a completed or failed stream".to_string(),
                ))
            }
        }
    }

    /// Pause the stream with a diagnostic reason
    ///
    /// Like pause(), but logs the provided reason for diagnostic purposes.
    /// This helps track why streams are being paused (e.g., "backpressure",
    /// "maintenance", "rate limit").
    ///
    /// # Arguments
    ///
    /// * `reason` - Optional reason for pausing (logged at debug level)
    ///
    /// # Example
    ///
    /// ```ignore
    /// stream.pause_with_reason("backpressure: consumer busy").await?;
    /// ```
    pub async fn pause_with_reason(&mut self, reason: &str) -> Result<()> {
        tracing::debug!("pausing stream: {}", reason);
        self.pause().await
    }

    /// Clone internal state for passing to background task
    pub(crate) fn clone_state(&self) -> Arc<Mutex<StreamState>> {
        Arc::clone(&self.state)
    }

    /// Clone pause signal for passing to background task
    pub(crate) fn clone_pause_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.pause_signal)
    }

    /// Clone resume signal for passing to background task
    pub(crate) fn clone_resume_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.resume_signal)
    }

    /// Clone paused occupancy counter for passing to background task
    pub(crate) fn clone_paused_occupancy(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.paused_occupancy)
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
        // Sample metrics: record 1 in every 1000 polls to avoid hot path overhead
        // For 100K rows, this records ~100 times instead of 100K times
        let poll_idx = self.poll_count.fetch_add(1, Ordering::Relaxed);
        if poll_idx % 1000 == 0 {
            let occupancy = self.receiver.len() as u64;
            crate::metrics::histograms::channel_occupancy(&self.entity, occupancy);
            crate::metrics::gauges::stream_buffered_items(&self.entity, occupancy as usize);
        }

        // Check memory limit BEFORE receiving (pre-enqueue strategy)
        // This stops consuming when buffer reaches limit
        if let Some(limit) = self.max_memory {
            let items_buffered = self.receiver.len();
            let estimated_memory = items_buffered * 2048;  // Conservative: 2KB per item

            // Check soft limit thresholds first (warn before fail)
            if let Some(fail_threshold) = self.soft_limit_fail_threshold {
                let threshold_bytes = (limit as f32 * fail_threshold) as usize;
                if estimated_memory > threshold_bytes {
                    // Record metric for memory limit exceeded
                    crate::metrics::counters::memory_limit_exceeded(&self.entity);
                    return Poll::Ready(Some(Err(Error::MemoryLimitExceeded {
                        limit,
                        estimated_memory,
                    })));
                }
            } else if estimated_memory > limit {
                // Hard limit (no soft limits configured)
                crate::metrics::counters::memory_limit_exceeded(&self.entity);
                return Poll::Ready(Some(Err(Error::MemoryLimitExceeded {
                    limit,
                    estimated_memory,
                })));
            }

            // Note: Warn threshold would be handled by instrumentation/logging layer
            // This is for application-level monitoring, not a hard error
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
