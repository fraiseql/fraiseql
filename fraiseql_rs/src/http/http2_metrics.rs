//! HTTP/2 Metrics and Observability (Phase 18.5)
//!
//! Prometheus metrics for HTTP/2 multiplexing, stream management,
//! and connection pool efficiency monitoring.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// HTTP/2 Metrics for production observability
///
/// Tracks:
/// - Stream lifecycle (opened, closed, active)
/// - Multiplexing efficiency (streams per connection)
/// - Flow control events (backpressure)
/// - Connection protocol distribution (HTTP/1.1 vs HTTP/2)
#[derive(Debug)]
pub struct Http2Metrics {
    /// Total HTTP/2 streams opened (counter)
    streams_opened_total: Arc<AtomicU64>,

    /// Total HTTP/2 streams closed (counter)
    streams_closed_total: Arc<AtomicU64>,

    /// Currently active HTTP/2 streams (gauge)
    streams_active_current: Arc<AtomicU64>,

    /// Peak number of active streams (gauge)
    streams_peak: Arc<AtomicU64>,

    /// Total HTTP/2 connections established (counter)
    h2_connections_total: Arc<AtomicU64>,

    /// Total HTTP/1.1 connections (for comparison)
    h1_connections_total: Arc<AtomicU64>,

    /// Flow control backpressure events (counter)
    flow_control_events: Arc<AtomicU64>,

    /// Frames sent total by type (h2 specific)
    frames_sent_data: Arc<AtomicU64>,
    frames_sent_headers: Arc<AtomicU64>,
    frames_sent_settings: Arc<AtomicU64>,
    frames_sent_window_update: Arc<AtomicU64>,
    frames_sent_goaway: Arc<AtomicU64>,

    /// Frames received total by type
    frames_received_data: Arc<AtomicU64>,
    frames_received_headers: Arc<AtomicU64>,
    frames_received_settings: Arc<AtomicU64>,
    frames_received_window_update: Arc<AtomicU64>,
    frames_received_goaway: Arc<AtomicU64>,

    /// Connection pool metrics
    pool_connections_active: Arc<AtomicU64>,
    pool_connections_idle: Arc<AtomicU64>,
    pool_wait_events: Arc<AtomicU64>,
    pool_wait_ms_total: Arc<AtomicU64>,

    /// Flow window statistics (bytes)
    flow_window_total_bytes: Arc<AtomicU64>,
    flow_window_exhausted_events: Arc<AtomicU64>,
}

impl Http2Metrics {
    /// Create new HTTP/2 metrics
    #[must_use]
    pub fn new() -> Self {
        Self {
            streams_opened_total: Arc::new(AtomicU64::new(0)),
            streams_closed_total: Arc::new(AtomicU64::new(0)),
            streams_active_current: Arc::new(AtomicU64::new(0)),
            streams_peak: Arc::new(AtomicU64::new(0)),
            h2_connections_total: Arc::new(AtomicU64::new(0)),
            h1_connections_total: Arc::new(AtomicU64::new(0)),
            flow_control_events: Arc::new(AtomicU64::new(0)),
            frames_sent_data: Arc::new(AtomicU64::new(0)),
            frames_sent_headers: Arc::new(AtomicU64::new(0)),
            frames_sent_settings: Arc::new(AtomicU64::new(0)),
            frames_sent_window_update: Arc::new(AtomicU64::new(0)),
            frames_sent_goaway: Arc::new(AtomicU64::new(0)),
            frames_received_data: Arc::new(AtomicU64::new(0)),
            frames_received_headers: Arc::new(AtomicU64::new(0)),
            frames_received_settings: Arc::new(AtomicU64::new(0)),
            frames_received_window_update: Arc::new(AtomicU64::new(0)),
            frames_received_goaway: Arc::new(AtomicU64::new(0)),
            pool_connections_active: Arc::new(AtomicU64::new(0)),
            pool_connections_idle: Arc::new(AtomicU64::new(0)),
            pool_wait_events: Arc::new(AtomicU64::new(0)),
            pool_wait_ms_total: Arc::new(AtomicU64::new(0)),
            flow_window_total_bytes: Arc::new(AtomicU64::new(0)),
            flow_window_exhausted_events: Arc::new(AtomicU64::new(0)),
        }
    }

    // ===== Stream Lifecycle Metrics =====

    /// Record a new HTTP/2 stream opened
    pub fn record_stream_opened(&self) {
        self.streams_opened_total.fetch_add(1, Ordering::Relaxed);
        let current = self
            .streams_active_current
            .fetch_add(1, Ordering::Relaxed) + 1;

        // Update peak if necessary
        let mut peak = self.streams_peak.load(Ordering::Relaxed);
        while current > peak {
            match self.streams_peak.compare_exchange(
                peak,
                current,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => peak = actual,
            }
        }
    }

    /// Record an HTTP/2 stream closed
    pub fn record_stream_closed(&self) {
        self.streams_closed_total.fetch_add(1, Ordering::Relaxed);
        self.streams_active_current
            .fetch_sub(1, Ordering::Relaxed);
    }

    /// Get current active streams
    #[must_use]
    pub fn streams_active(&self) -> u64 {
        self.streams_active_current.load(Ordering::Relaxed)
    }

    /// Get peak streams seen
    #[must_use]
    pub fn streams_peak(&self) -> u64 {
        self.streams_peak.load(Ordering::Relaxed)
    }

    /// Get total streams opened
    #[must_use]
    pub fn streams_opened_total(&self) -> u64 {
        self.streams_opened_total.load(Ordering::Relaxed)
    }

    // ===== Connection Metrics =====

    /// Record new HTTP/2 connection
    pub fn record_h2_connection(&self) {
        self.h2_connections_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record new HTTP/1.1 connection
    pub fn record_h1_connection(&self) {
        self.h1_connections_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Calculate multiplexing factor (streams per HTTP/2 connection)
    #[must_use]
    pub fn multiplexing_factor(&self) -> f64 {
        let h2_conns = self.h2_connections_total.load(Ordering::Relaxed);
        if h2_conns == 0 {
            return 0.0;
        }

        self.streams_opened_total.load(Ordering::Relaxed) as f64 / h2_conns as f64
    }

    // ===== Flow Control Metrics =====

    /// Record a flow control backpressure event
    pub fn record_flow_control_event(&self) {
        self.flow_control_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Record flow window exhausted (streams waiting for window)
    pub fn record_window_exhausted(&self) {
        self.flow_window_exhausted_events
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Update total flow window bytes in use
    pub fn set_flow_window_bytes(&self, bytes: u64) {
        self.flow_window_total_bytes
            .store(bytes, Ordering::Relaxed);
    }

    // ===== Frame Metrics =====

    /// Record sent DATA frame
    pub fn record_frame_sent_data(&self) {
        self.frames_sent_data.fetch_add(1, Ordering::Relaxed);
    }

    /// Record sent HEADERS frame
    pub fn record_frame_sent_headers(&self) {
        self.frames_sent_headers.fetch_add(1, Ordering::Relaxed);
    }

    /// Record sent SETTINGS frame
    pub fn record_frame_sent_settings(&self) {
        self.frames_sent_settings.fetch_add(1, Ordering::Relaxed);
    }

    /// Record sent WINDOW_UPDATE frame
    pub fn record_frame_sent_window_update(&self) {
        self.frames_sent_window_update
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record sent GOAWAY frame
    pub fn record_frame_sent_goaway(&self) {
        self.frames_sent_goaway.fetch_add(1, Ordering::Relaxed);
    }

    /// Record received DATA frame
    pub fn record_frame_received_data(&self) {
        self.frames_received_data.fetch_add(1, Ordering::Relaxed);
    }

    /// Record received HEADERS frame
    pub fn record_frame_received_headers(&self) {
        self.frames_received_headers.fetch_add(1, Ordering::Relaxed);
    }

    // ===== Connection Pool Metrics =====

    /// Update active connection count
    pub fn set_pool_active_connections(&self, count: u64) {
        self.pool_connections_active.store(count, Ordering::Relaxed);
    }

    /// Update idle connection count
    pub fn set_pool_idle_connections(&self, count: u64) {
        self.pool_connections_idle.store(count, Ordering::Relaxed);
    }

    /// Record a pool wait event (acquiring connection from pool)
    pub fn record_pool_wait(&self, wait_ms: u64) {
        self.pool_wait_events.fetch_add(1, Ordering::Relaxed);
        self.pool_wait_ms_total
            .fetch_add(wait_ms, Ordering::Relaxed);
    }

    /// Get average pool wait time in ms
    #[must_use]
    pub fn avg_pool_wait_ms(&self) -> f64 {
        let events = self.pool_wait_events.load(Ordering::Relaxed);
        if events == 0 {
            return 0.0;
        }

        self.pool_wait_ms_total.load(Ordering::Relaxed) as f64 / events as f64
    }

    // ===== Snapshot for Prometheus Export =====

    /// Get all metrics as a snapshot for reporting
    #[must_use]
    pub fn snapshot(&self) -> Http2MetricsSnapshot {
        Http2MetricsSnapshot {
            streams_opened_total: self.streams_opened_total.load(Ordering::Relaxed),
            streams_closed_total: self.streams_closed_total.load(Ordering::Relaxed),
            streams_active_current: self.streams_active_current.load(Ordering::Relaxed),
            streams_peak: self.streams_peak.load(Ordering::Relaxed),
            h2_connections_total: self.h2_connections_total.load(Ordering::Relaxed),
            h1_connections_total: self.h1_connections_total.load(Ordering::Relaxed),
            flow_control_events: self.flow_control_events.load(Ordering::Relaxed),
            frames_sent_data: self.frames_sent_data.load(Ordering::Relaxed),
            frames_sent_headers: self.frames_sent_headers.load(Ordering::Relaxed),
            frames_sent_settings: self.frames_sent_settings.load(Ordering::Relaxed),
            frames_sent_window_update: self.frames_sent_window_update.load(Ordering::Relaxed),
            frames_sent_goaway: self.frames_sent_goaway.load(Ordering::Relaxed),
            frames_received_data: self.frames_received_data.load(Ordering::Relaxed),
            frames_received_headers: self.frames_received_headers.load(Ordering::Relaxed),
            frames_received_settings: self.frames_received_settings.load(Ordering::Relaxed),
            frames_received_window_update: self.frames_received_window_update.load(Ordering::Relaxed),
            frames_received_goaway: self.frames_received_goaway.load(Ordering::Relaxed),
            pool_connections_active: self.pool_connections_active.load(Ordering::Relaxed),
            pool_connections_idle: self.pool_connections_idle.load(Ordering::Relaxed),
            pool_wait_events: self.pool_wait_events.load(Ordering::Relaxed),
            avg_pool_wait_ms: self.avg_pool_wait_ms(),
            flow_window_total_bytes: self.flow_window_total_bytes.load(Ordering::Relaxed),
            flow_window_exhausted_events: self
                .flow_window_exhausted_events
                .load(Ordering::Relaxed),
            multiplexing_factor: self.multiplexing_factor(),
        }
    }

    /// Clear all metrics (for testing)
    pub fn reset(&self) {
        self.streams_opened_total.store(0, Ordering::Relaxed);
        self.streams_closed_total.store(0, Ordering::Relaxed);
        self.streams_active_current.store(0, Ordering::Relaxed);
        self.streams_peak.store(0, Ordering::Relaxed);
        self.h2_connections_total.store(0, Ordering::Relaxed);
        self.h1_connections_total.store(0, Ordering::Relaxed);
        self.flow_control_events.store(0, Ordering::Relaxed);
        self.frames_sent_data.store(0, Ordering::Relaxed);
        self.frames_sent_headers.store(0, Ordering::Relaxed);
        self.frames_sent_settings.store(0, Ordering::Relaxed);
        self.frames_sent_window_update.store(0, Ordering::Relaxed);
        self.frames_sent_goaway.store(0, Ordering::Relaxed);
        self.frames_received_data.store(0, Ordering::Relaxed);
        self.frames_received_headers.store(0, Ordering::Relaxed);
        self.frames_received_settings.store(0, Ordering::Relaxed);
        self.frames_received_window_update.store(0, Ordering::Relaxed);
        self.frames_received_goaway.store(0, Ordering::Relaxed);
        self.pool_connections_active.store(0, Ordering::Relaxed);
        self.pool_connections_idle.store(0, Ordering::Relaxed);
        self.pool_wait_events.store(0, Ordering::Relaxed);
        self.pool_wait_ms_total.store(0, Ordering::Relaxed);
        self.flow_window_total_bytes.store(0, Ordering::Relaxed);
        self.flow_window_exhausted_events
            .store(0, Ordering::Relaxed);
    }
}

impl Default for Http2Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of HTTP/2 metrics for reporting
#[derive(Debug, Clone)]
pub struct Http2MetricsSnapshot {
    pub streams_opened_total: u64,
    pub streams_closed_total: u64,
    pub streams_active_current: u64,
    pub streams_peak: u64,
    pub h2_connections_total: u64,
    pub h1_connections_total: u64,
    pub flow_control_events: u64,
    pub frames_sent_data: u64,
    pub frames_sent_headers: u64,
    pub frames_sent_settings: u64,
    pub frames_sent_window_update: u64,
    pub frames_sent_goaway: u64,
    pub frames_received_data: u64,
    pub frames_received_headers: u64,
    pub frames_received_settings: u64,
    pub frames_received_window_update: u64,
    pub frames_received_goaway: u64,
    pub pool_connections_active: u64,
    pub pool_connections_idle: u64,
    pub pool_wait_events: u64,
    pub avg_pool_wait_ms: f64,
    pub flow_window_total_bytes: u64,
    pub flow_window_exhausted_events: u64,
    pub multiplexing_factor: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_lifecycle() {
        let metrics = Http2Metrics::new();

        // Open 5 streams
        for _ in 0..5 {
            metrics.record_stream_opened();
        }

        assert_eq!(metrics.streams_opened_total(), 5);
        assert_eq!(metrics.streams_active(), 5);
        assert_eq!(metrics.streams_peak(), 5);

        // Close 2 streams
        for _ in 0..2 {
            metrics.record_stream_closed();
        }

        assert_eq!(metrics.streams_active(), 3);
    }

    #[test]
    fn test_peak_tracking() {
        let metrics = Http2Metrics::new();

        metrics.record_stream_opened();
        metrics.record_stream_opened();
        let peak1 = metrics.streams_peak();

        metrics.record_stream_opened();
        metrics.record_stream_opened();
        metrics.record_stream_opened();
        let peak2 = metrics.streams_peak();

        assert!(peak2 > peak1);
    }

    #[test]
    fn test_multiplexing_factor() {
        let metrics = Http2Metrics::new();

        // 100 streams over 10 connections
        for _ in 0..10 {
            metrics.record_h2_connection();
        }
        for _ in 0..100 {
            metrics.record_stream_opened();
        }

        let factor = metrics.multiplexing_factor();
        assert!((factor - 10.0).abs() < 0.01); // Should be ~10
    }

    #[test]
    fn test_frame_counting() {
        let metrics = Http2Metrics::new();

        for _ in 0..5 {
            metrics.record_frame_sent_data();
        }
        for _ in 0..3 {
            metrics.record_frame_received_headers();
        }

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.frames_sent_data, 5);
        assert_eq!(snapshot.frames_received_headers, 3);
    }

    #[test]
    fn test_pool_wait_average() {
        let metrics = Http2Metrics::new();

        metrics.record_pool_wait(10);
        metrics.record_pool_wait(20);
        metrics.record_pool_wait(30);

        let avg = metrics.avg_pool_wait_ms();
        assert!((avg - 20.0).abs() < 0.01); // Should be 20
    }

    #[test]
    fn test_reset() {
        let metrics = Http2Metrics::new();

        metrics.record_stream_opened();
        metrics.record_h2_connection();
        metrics.record_flow_control_event();

        let snapshot1 = metrics.snapshot();
        assert!(snapshot1.streams_opened_total > 0);

        metrics.reset();

        let snapshot2 = metrics.snapshot();
        assert_eq!(snapshot2.streams_opened_total, 0);
        assert_eq!(snapshot2.h2_connections_total, 0);
        assert_eq!(snapshot2.flow_control_events, 0);
    }
}
