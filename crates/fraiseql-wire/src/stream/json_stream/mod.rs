//! JSON stream implementation

use crate::protocol::BackendMessage;
use crate::{Result, WireError};
use bytes::Bytes;
use futures::stream::Stream;
use serde_json::Value;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, Notify};

// Lightweight state machine constants
// Used for fast state tracking without full Mutex overhead
pub const STATE_RUNNING: u8 = 0;
pub const STATE_PAUSED: u8 = 1;
pub const STATE_COMPLETED: u8 = 2;
pub const STATE_FAILED: u8 = 3;

/// Stream state machine
///
/// Tracks the current state of the JSON stream.
/// Streams start in Running state and can transition to Paused
/// or terminal states (Completed, Failed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
    #[must_use]
    pub const fn zero() -> Self {
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
    _cancel_tx: mpsc::Sender<()>,  // Dropped when stream is dropped
    entity: String,                // Entity name for metrics
    rows_yielded: Arc<AtomicU64>,  // Counter of items yielded to consumer
    rows_filtered: Arc<AtomicU64>, // Counter of items filtered
    max_memory: Option<usize>,     // Optional memory limit in bytes
    soft_limit_fail_threshold: Option<f32>, // Fail at threshold % (0.0-1.0)

    // Lightweight state tracking (cheap AtomicU8)
    // Used for fast state checks on all queries
    // Values: 0=Running, 1=Paused, 2=Complete, 3=Error
    state_atomic: Arc<AtomicU8>,

    // Pause/resume state machine, eagerly allocated (audit H43)
    pause_resume: PauseResumeState,

    // Sampling counter for metrics recording (sample 1 in N polls)
    poll_count: AtomicU64, // Counter for sampling metrics
}

/// Pause/resume state, eagerly allocated in [`JsonStream::new`].
///
/// Every handle here is an `Arc` so the same instances can be cloned to the
/// background reader at spawn time *and* retained by the stream. The previous
/// design allocated this lazily on the first `pause()` call — but by then the
/// reader had already captured `None` clones, so pause/resume never reached it
/// and the pause-timeout/occupancy metrics were permanently dead (audit H43).
pub struct PauseResumeState {
    state: Arc<Mutex<StreamState>>,     // Current stream state
    resume_signal: Arc<Notify>,         // Wakes the reader when resumed
    paused_occupancy: Arc<AtomicUsize>, // Buffered rows captured at pause time
    pause_timeout_ms: Arc<AtomicU64>, // Auto-resume timeout in ms (0 = none), read live by the reader
}

impl JsonStream {
    /// Create new JSON stream
    pub(crate) fn new(
        receiver: mpsc::Receiver<Result<Value>>,
        cancel_tx: mpsc::Sender<()>,
        entity: String,
        max_memory: Option<usize>,
        _soft_limit_warn_threshold: Option<f32>,
        soft_limit_fail_threshold: Option<f32>,
    ) -> Self {
        Self {
            receiver,
            _cancel_tx: cancel_tx,
            entity,
            rows_yielded: Arc::new(AtomicU64::new(0)),
            rows_filtered: Arc::new(AtomicU64::new(0)),
            max_memory,
            soft_limit_fail_threshold,

            // Initialize lightweight atomic state
            state_atomic: Arc::new(AtomicU8::new(STATE_RUNNING)),

            // Pause/resume infrastructure is allocated eagerly so the background
            // reader receives live handles at spawn time (audit H43).
            pause_resume: PauseResumeState {
                state: Arc::new(Mutex::new(StreamState::Running)),
                resume_signal: Arc::new(Notify::new()),
                paused_occupancy: Arc::new(AtomicUsize::new(0)),
                pause_timeout_ms: Arc::new(AtomicU64::new(0)),
            },

            // Initialize sampling counter
            poll_count: AtomicU64::new(0),
        }
    }

    /// Get current stream state
    ///
    /// Returns the current state of the stream (Running, Paused, Completed, or Failed).
    /// This is a synchronous getter that doesn't require awaiting.
    ///
    /// Note: This is a best-effort snapshot that may return slightly stale state
    /// due to the non-blocking nature of atomic reads.
    pub fn state_snapshot(&self) -> StreamState {
        // Read from lightweight atomic state (fast path, no locks)
        match self.state_atomic.load(Ordering::Acquire) {
            STATE_RUNNING => StreamState::Running,
            STATE_PAUSED => StreamState::Paused,
            STATE_COMPLETED => StreamState::Completed,
            STATE_FAILED => StreamState::Failed,
            _ => {
                // Unknown state - fall back to checking if channel is closed
                if self.receiver.is_closed() {
                    StreamState::Completed
                } else {
                    StreamState::Running
                }
            }
        }
    }

    /// Get buffered rows when paused
    ///
    /// Returns the number of rows buffered in the channel when the stream was paused.
    /// Only meaningful when stream is in Paused state.
    pub fn paused_occupancy(&self) -> usize {
        self.pause_resume.paused_occupancy.load(Ordering::Relaxed)
    }

    /// Set timeout for pause (auto-resume after duration)
    ///
    /// When a stream is paused, the background task will automatically resume
    /// after the specified duration expires, even if `resume()` is not called.
    ///
    /// # Arguments
    ///
    /// * `duration` - How long to stay paused before auto-resuming
    ///
    /// # Examples
    ///
    /// ```text
    /// // Requires: a JsonStream instance from execute_query.
    /// // Note: set_pause_timeout is on JsonStream, not QueryStream.
    /// // Use FraiseClient::execute_query (internal) to obtain a JsonStream directly.
    /// use std::time::Duration;
    /// stream.set_pause_timeout(Duration::from_secs(5));
    /// stream.pause().await?;  // Will auto-resume after 5 seconds
    /// ```
    pub fn set_pause_timeout(&mut self, duration: Duration) {
        // Stored as a shared atomic (ms) so the already-spawned reader picks up
        // the value live; a zero-millisecond request is clamped to 1 ms so it is
        // never confused with "no timeout" (audit H43).
        let ms = u64::try_from(duration.as_millis())
            .unwrap_or(u64::MAX)
            .max(1);
        self.pause_resume
            .pause_timeout_ms
            .store(ms, Ordering::Relaxed);
        tracing::debug!("pause timeout set to {:?}", duration);
    }

    /// Clear pause timeout (no auto-resume)
    pub fn clear_pause_timeout(&mut self) {
        self.pause_resume
            .pause_timeout_ms
            .store(0, Ordering::Relaxed);
        tracing::debug!("pause timeout cleared");
    }

    /// Pause the stream
    ///
    /// Suspends the background task from reading more data from Postgres.
    /// The connection remains open and can be resumed later.
    /// Buffered rows are preserved and can be consumed normally.
    ///
    /// This method is idempotent: calling `pause()` on an already-paused stream is a no-op.
    ///
    /// Returns an error if the stream has already completed or failed.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Protocol`] if the stream is in a terminal state (completed or failed).
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: live Postgres streaming connection.
    /// # async fn example(client: fraiseql_wire::FraiseClient) -> fraiseql_wire::Result<()> {
    /// let mut stream = client.query::<serde_json::Value>("entity").execute().await?;
    /// stream.pause().await?;
    /// // Background task stops reading
    /// // Consumer can still poll for remaining buffered items
    /// # Ok(())
    /// # }
    /// ```
    pub async fn pause(&mut self) -> Result<()> {
        let entity = self.entity.clone();

        // Capture the buffered-row count at the moment of pause so
        // `paused_occupancy()` reflects reality (it was never recorded before —
        // audit H43).
        let occupancy = self.receiver.len();

        // Update lightweight atomic state first (fast path)
        self.state_atomic_set_paused();

        let pr = &self.pause_resume;
        let mut state = pr.state.lock().await;

        match *state {
            StreamState::Running => {
                pr.paused_occupancy.store(occupancy, Ordering::Relaxed);
                // Update state
                *state = StreamState::Paused;

                // Record metric
                crate::metrics::counters::stream_paused(&entity);
                Ok(())
            }
            StreamState::Paused => {
                // Idempotent: already paused
                Ok(())
            }
            StreamState::Completed | StreamState::Failed => {
                // Cannot pause a terminal stream
                Err(WireError::Protocol(
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
    /// This method is idempotent: calling `resume()` before `pause()` or on an
    /// already-running stream is a no-op.
    ///
    /// Returns an error if the stream has already completed or failed.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Protocol`] if the stream is in a terminal state (completed or failed).
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: live Postgres streaming connection.
    /// # async fn example(client: fraiseql_wire::FraiseClient) -> fraiseql_wire::Result<()> {
    /// let mut stream = client.query::<serde_json::Value>("entity").execute().await?;
    /// stream.resume().await?;
    /// // Background task resumes reading
    /// // Consumer can poll for more items
    /// # Ok(())
    /// # }
    /// ```
    pub async fn resume(&mut self) -> Result<()> {
        // Update lightweight atomic state first (fast path)
        let current = self.state_atomic_get();
        let entity = self.entity.clone();

        // Note: Set to RUNNING to reflect resumed state
        if current == STATE_PAUSED {
            // Only update atomic if currently paused
            self.state_atomic.store(STATE_RUNNING, Ordering::Release);
        }

        let pr = &self.pause_resume;
        let mut state = pr.state.lock().await;

        match *state {
            StreamState::Paused => {
                // Wake the background reader, which is parked on this signal.
                pr.resume_signal.notify_one();
                // Update state
                *state = StreamState::Running;

                // Record metric
                crate::metrics::counters::stream_resumed(&entity);
                Ok(())
            }
            StreamState::Running => {
                // Idempotent: already running (or resume before pause)
                Ok(())
            }
            StreamState::Completed | StreamState::Failed => {
                // Cannot resume a terminal stream
                Err(WireError::Protocol(
                    "cannot resume a completed or failed stream".to_string(),
                ))
            }
        }
    }

    /// Pause the stream with a diagnostic reason
    ///
    /// Like `pause()`, but logs the provided reason for diagnostic purposes.
    /// This helps track why streams are being paused (e.g., "backpressure",
    /// "maintenance", "rate limit").
    ///
    /// # Arguments
    ///
    /// * `reason` - Optional reason for pausing (logged at debug level)
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Protocol`] if the stream is in a terminal state (completed or failed).
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: live Postgres streaming connection.
    /// # async fn example(client: fraiseql_wire::FraiseClient) -> fraiseql_wire::Result<()> {
    /// let mut stream = client.query::<serde_json::Value>("entity").execute().await?;
    /// stream.pause_with_reason("backpressure: consumer busy").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn pause_with_reason(&mut self, reason: &str) -> Result<()> {
        tracing::debug!("pausing stream: {}", reason);
        self.pause().await
    }

    /// Clone the shared stream-state handle for the background reader.
    pub(crate) fn clone_state(&self) -> Arc<Mutex<StreamState>> {
        Arc::clone(&self.pause_resume.state)
    }

    /// Clone the resume signal the background reader parks on while paused.
    pub(crate) fn clone_resume_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.pause_resume.resume_signal)
    }

    /// Clone the live pause-timeout handle (ms, 0 = none) for the background reader.
    pub(crate) fn clone_pause_timeout(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.pause_resume.pause_timeout_ms)
    }

    // =========================================================================
    // Lightweight state machine methods
    // =========================================================================

    /// Clone atomic state for passing to background task
    pub(crate) fn clone_state_atomic(&self) -> Arc<AtomicU8> {
        Arc::clone(&self.state_atomic)
    }

    /// Get current state from atomic (fast path, no locks)
    pub(crate) fn state_atomic_get(&self) -> u8 {
        self.state_atomic.load(Ordering::Acquire)
    }

    /// Set state to paused using atomic
    pub(crate) fn state_atomic_set_paused(&self) {
        self.state_atomic.store(STATE_PAUSED, Ordering::Release);
    }

    /// Set state to completed using atomic
    pub(crate) fn state_atomic_set_completed(&self) {
        self.state_atomic.store(STATE_COMPLETED, Ordering::Release);
    }

    /// Set state to failed using atomic
    pub(crate) fn state_atomic_set_failed(&self) {
        self.state_atomic.store(STATE_FAILED, Ordering::Release);
    }

    /// Record that one row was filtered out by a downstream Rust predicate.
    ///
    /// Called by [`crate::stream::QueryStream`] when its predicate rejects a row,
    /// so `stats().total_rows_filtered` reflects reality (audit L-wire-stats).
    pub(crate) fn record_filtered(&self) {
        self.rows_filtered.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current stream statistics
    ///
    /// Returns a snapshot of stream state without consuming any items.
    /// This can be called at any time to monitor progress.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: live Postgres streaming connection.
    /// # async fn example(client: fraiseql_wire::FraiseClient) -> fraiseql_wire::Result<()> {
    /// let stream = client.query::<serde_json::Value>("entity").execute().await?;
    /// let stats = stream.stats();
    /// println!("Buffered: {}, Yielded: {}", stats.items_buffered, stats.total_rows_yielded);
    /// # Ok(())
    /// # }
    /// ```
    pub fn stats(&self) -> StreamStats {
        let items_buffered = self.receiver.len();
        let estimated_memory = items_buffered * 2048; // Conservative: 2KB per item
        let total_rows_yielded = self.rows_yielded.load(Ordering::Relaxed);
        let total_rows_filtered = self.rows_filtered.load(Ordering::Relaxed);

        StreamStats {
            items_buffered,
            estimated_memory,
            total_rows_yielded,
            total_rows_filtered,
        }
    }
}

impl Stream for JsonStream {
    type Item = Result<Value>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Sample metrics: record 1 in every 1000 polls to avoid hot path overhead
        // For 100K rows, this records ~100 times instead of 100K times
        let poll_idx = self.poll_count.fetch_add(1, Ordering::Relaxed);
        if poll_idx.is_multiple_of(1000) {
            let occupancy = self.receiver.len() as u64;
            crate::metrics::histograms::channel_occupancy(&self.entity, occupancy);
            crate::metrics::gauges::stream_buffered_items(&self.entity, occupancy as usize);
        }

        // Check memory limit BEFORE receiving (pre-enqueue strategy)
        // This stops consuming when buffer reaches limit
        if let Some(limit) = self.max_memory {
            let items_buffered = self.receiver.len();
            let estimated_memory = items_buffered * 2048; // Conservative: 2KB per item

            // Check soft limit thresholds first (warn before fail)
            if let Some(fail_threshold) = self.soft_limit_fail_threshold {
                let threshold_bytes = (limit as f32 * fail_threshold) as usize;
                if estimated_memory > threshold_bytes {
                    // Record metric for memory limit exceeded
                    crate::metrics::counters::memory_limit_exceeded(&self.entity);
                    self.state_atomic_set_failed();
                    return Poll::Ready(Some(Err(WireError::MemoryLimitExceeded {
                        limit,
                        estimated_memory,
                    })));
                }
            } else if estimated_memory > limit {
                // Hard limit (no soft limits configured)
                crate::metrics::counters::memory_limit_exceeded(&self.entity);
                self.state_atomic_set_failed();
                return Poll::Ready(Some(Err(WireError::MemoryLimitExceeded {
                    limit,
                    estimated_memory,
                })));
            }

            // Note: Warn threshold would be handled by instrumentation/logging layer
            // This is for application-level monitoring, not a hard error
        }

        match self.receiver.poll_recv(cx) {
            Poll::Ready(Some(Ok(value))) => {
                // Count rows handed to the consumer so `stats().total_rows_yielded`
                // is truthful (audit L-wire-stats — it was never incremented).
                self.rows_yielded.fetch_add(1, Ordering::Relaxed);
                Poll::Ready(Some(Ok(value)))
            }
            Poll::Ready(Some(Err(e))) => {
                // Stream encountered an error
                self.state_atomic_set_failed();
                Poll::Ready(Some(Err(e)))
            }
            Poll::Ready(None) => {
                // Stream completed normally
                self.state_atomic_set_completed();
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Extract JSON bytes from `DataRow` message
///
/// # Errors
///
/// Returns [`WireError::Protocol`] if the message is not a `DataRow`, the row does not
/// have exactly one field, or the field value is null.
pub fn extract_json_bytes(msg: &BackendMessage) -> Result<Bytes> {
    match msg {
        BackendMessage::DataRow(fields) => match fields.as_slice() {
            [only] => only
                .clone()
                .ok_or_else(|| WireError::Protocol("null data field".into())),
            _ => Err(WireError::Protocol(format!(
                "expected 1 field, got {}",
                fields.len()
            ))),
        },
        _ => Err(WireError::Protocol("expected DataRow".into())),
    }
}

/// Parse JSON bytes into Value
///
/// # Errors
///
/// Returns [`WireError`] if the bytes are not valid JSON.
pub fn parse_json(data: Bytes) -> Result<Value> {
    let value: Value = serde_json::from_slice(&data)?;
    Ok(value)
}

#[cfg(test)]
mod tests;
